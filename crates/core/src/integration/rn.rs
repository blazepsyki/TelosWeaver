//! RN Fork를 Core가 띄우고 연결합니다 (Spec v1.5.1 §12.3·§12.5, 배정표 §6 "RN Core 연결", PH0-RN-07).
//!
//! - 운영자 정책 `rn.executable`의 `telos-rn daemon`을 공용 프로세스 런타임으로 띄웁니다(Windows Job Object라
//!   Core가 끝나면 함께 끝남). 상태 폴더는 Core 프로젝트별 상태 폴더 아래 `rn`이며 `TELOS_RN_HOME`·
//!   `TELOS_RN_HOST_ID`·`TELOS_RN_CODEX_BIN`을 넘깁니다. 같은 폴더에 이미 RN이 돌고 있으면 연결만 합니다.
//! - 2초마다 `health.get`으로 확인합니다. 끊기거나 Core가 띄운 RN이 끝나면 `rn`을 Degraded로 두고, 1·2·4초 뒤
//!   최대 3회 다시 띄웁니다. 60초 이상 정상이면 횟수를 되돌립니다. 그래도 안 되면 Degraded로 남습니다.
//! - RN 상태는 실행 감독과 무관합니다: Managed Run의 추적·취소·결과 보존은 RN 없이도 그대로입니다 (§12.3).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use futures::StreamExt;
use telos_adapters::runtime::{
    CommandSpec, OutputEvent, OutputLimits, ProcessHandle, ProcessRuntime, StdinMode,
};
use telos_rn_client::{ClientOptions, RnClient};
use telos_schema::api::{ComponentHealth, HealthStatus};
use tokio::sync::watch;

/// RN이 RPC를 열기까지 기다리는 시간.
const START_WAIT: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(2);
const RESTARTS: u32 = 3;
/// 이만큼 정상이면 재시작 횟수를 되돌립니다.
const STABLE: Duration = Duration::from_secs(60);
const STOP_GRACE: Duration = Duration::from_secs(3);

/// RN을 띄우는 조건.
#[derive(Debug, Clone)]
pub struct RnLaunch {
    pub executable: PathBuf,
    /// RN 상태 폴더(`TELOS_RN_HOME`).
    pub home: PathBuf,
    pub host_id: String,
    /// Core가 확장에서 찾은 Codex CLI(DEC-06). 없으면 Codex 세션은 Hook으로만 받습니다.
    pub codex_bin: Option<PathBuf>,
}

pub struct RnService {
    launch: Option<RnLaunch>,
    runtime: Arc<dyn ProcessRuntime>,
    state: Mutex<State>,
    stop: watch::Sender<bool>,
}

struct State {
    status: HealthStatus,
    detail: String,
    client: Option<Arc<RnClient>>,
    /// Core가 띄운 RN. 이미 돌고 있던 RN에 연결했으면 없습니다.
    child: Option<Arc<dyn ProcessHandle>>,
    restarts: u32,
    up_since: Option<Instant>,
}

impl RnService {
    /// 정책에 `rn` 절이 없습니다: RN을 쓰지 않고 그렇게 알립니다.
    pub fn not_configured(runtime: Arc<dyn ProcessRuntime>) -> Arc<Self> {
        Arc::new(Self {
            launch: None,
            runtime,
            state: Mutex::new(State {
                status: HealthStatus::Degraded,
                detail: "not configured (the operator policy has no rn section)".into(),
                client: None,
                child: None,
                restarts: 0,
                up_since: None,
            }),
            stop: watch::channel(false).0,
        })
    }

    /// RN을 띄우거나(이미 있으면 연결) 그 결과를 상태에 남기고, 감시를 시작합니다.
    pub async fn start(launch: RnLaunch, runtime: Arc<dyn ProcessRuntime>) -> Arc<Self> {
        let service = Arc::new(Self {
            launch: Some(launch),
            runtime,
            state: Mutex::new(State {
                status: HealthStatus::Degraded,
                detail: "starting".into(),
                client: None,
                child: None,
                restarts: 0,
                up_since: None,
            }),
            stop: watch::channel(false).0,
        });
        if let Err(why) = service.bring_up().await {
            service.down(&format!("not started: {why}"));
        }
        tokio::spawn(service.clone().watch());
        service
    }

    /// `core.health`의 `rn` 구성 요소.
    pub fn component(&self) -> ComponentHealth {
        let s = self.state.lock().unwrap();
        ComponentHealth {
            name: "rn".into(),
            status: s.status,
            detail: Some(s.detail.clone()),
        }
    }

    /// 연결된 RN Client. 연결되지 않았으면 없습니다.
    pub fn client(&self) -> Option<Arc<RnClient>> {
        self.state.lock().unwrap().client.clone()
    }

    /// Core가 띄운 RN 프로세스의 PID.
    pub fn owned_pid(&self) -> Option<u32> {
        self.state
            .lock()
            .unwrap()
            .child
            .as_ref()
            .map(|c| c.identity().pid)
    }

    /// 감시를 멈추고 Core가 띄운 RN을 정지합니다. 이미 돌고 있던 RN은 그대로 둡니다.
    pub async fn shutdown(&self) {
        self.stop.send_replace(true);
        let child = {
            let mut s = self.state.lock().unwrap();
            s.client = None;
            s.status = HealthStatus::Degraded;
            s.detail = "stopped with Core".into();
            s.child.take()
        };
        if let Some(child) = child {
            let _ = child.terminate(STOP_GRACE).await;
        }
    }

    fn down(&self, why: &str) {
        let mut s = self.state.lock().unwrap();
        s.status = HealthStatus::Degraded;
        s.detail = why.into();
        s.client = None;
        s.up_since = None;
    }

    async fn bring_up(&self) -> Result<(), String> {
        let launch = self.launch.as_ref().ok_or("not configured")?;
        std::fs::create_dir_all(&launch.home)
            .map_err(|e| format!("{}: {e}", launch.home.display()))?;
        let options = ClientOptions::default();
        // Already running for this folder (another Core, or one left from before): connect only.
        if let Ok(client) = RnClient::connect(&launch.home, &options).await {
            return self.up(client, None).await;
        }
        let spec = CommandSpec {
            program: launch.executable.clone(),
            args: vec!["daemon".into()],
            cwd: launch.home.clone(),
            env: vec![
                (
                    "TELOS_RN_HOME".into(),
                    Some(launch.home.clone().into_os_string()),
                ),
                (
                    "TELOS_RN_HOST_ID".into(),
                    Some(launch.host_id.clone().into()),
                ),
                (
                    "TELOS_RN_CODEX_BIN".into(),
                    launch.codex_bin.clone().map(PathBuf::into_os_string),
                ),
            ],
            stdin: StdinMode::Null,
            limits: OutputLimits {
                max_line_bytes: 64 * 1024,
                max_buffered_bytes: 1024 * 1024,
            },
        };
        let child = self.runtime.spawn(spec).await.map_err(|e| e.to_string())?;
        // Its log goes to its folder, as when it starts itself; read so it never blocks on a full pipe.
        if let Ok(mut output) = child.take_output() {
            let log = launch.home.join("daemon.log");
            tokio::spawn(async move {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)
                    .ok();
                while let Some(event) = output.next().await {
                    if let (Some(f), OutputEvent::Line { data, .. }) = (file.as_mut(), event) {
                        let _ = f.write_all(&data).and_then(|_| f.write_all(b"\n"));
                    }
                }
            });
        }
        let pid = child.identity().pid;
        let deadline = Instant::now() + START_WAIT;
        loop {
            match RnClient::connect(&launch.home, &options).await {
                Ok(client) if client.info().pid == pid => {
                    return self.up(client, Some(child)).await;
                }
                // Another RN took the folder first: ours has nothing to do.
                Ok(client) => {
                    let _ = child.terminate(STOP_GRACE).await;
                    return self.up(client, None).await;
                }
                Err(_) => {}
            }
            if let Ok(Ok(status)) = tokio::time::timeout(Duration::ZERO, child.wait()).await {
                return Err(format!(
                    "telos-rn exited ({:?}, code {:?}) before opening its RPC",
                    status.cause, status.code
                ));
            }
            if Instant::now() > deadline {
                let _ = child.terminate(STOP_GRACE).await;
                return Err(format!(
                    "telos-rn didn't open its RPC within {}s",
                    START_WAIT.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn up(
        &self,
        client: RnClient,
        child: Option<Arc<dyn ProcessHandle>>,
    ) -> Result<(), String> {
        let health = client.health().await.map_err(|e| e.to_string())?;
        let owned = child.is_some();
        let detail = format!(
            "rpc {}, fork {}, pid {} ({})",
            health.rpc_version,
            health.fork_commit,
            client.info().pid,
            if owned {
                "started by Core"
            } else {
                "already running"
            }
        );
        let mut s = self.state.lock().unwrap();
        s.status = HealthStatus::Ok;
        s.detail = detail;
        s.client = Some(Arc::new(client));
        s.child = child;
        s.up_since = Some(Instant::now());
        Ok(())
    }

    async fn watch(self: Arc<Self>) {
        let mut stop = self.stop.subscribe();
        loop {
            tokio::select! {
                _ = stop.changed() => return,
                _ = tokio::time::sleep(CHECK_EVERY) => {}
            }
            if *stop.borrow() {
                return;
            }
            let (client, child) = {
                let s = self.state.lock().unwrap();
                (s.client.clone(), s.child.clone())
            };
            let exited = match &child {
                Some(c) => matches!(
                    tokio::time::timeout(Duration::ZERO, c.wait()).await,
                    Ok(Ok(_))
                ),
                None => false,
            };
            let healthy = match &client {
                Some(c) if !c.is_closed() && !exited => c.health().await.is_ok(),
                _ => false,
            };
            if healthy {
                let mut s = self.state.lock().unwrap();
                if s.up_since.is_some_and(|t| t.elapsed() >= STABLE) {
                    s.restarts = 0;
                }
                continue;
            }
            // Down: restart it, unless it has been restarted as often as allowed.
            let restarts = {
                let mut s = self.state.lock().unwrap();
                s.child = None;
                s.restarts += 1;
                s.restarts
            };
            if restarts > RESTARTS {
                if let Some(c) = child {
                    let _ = c.terminate(STOP_GRACE).await;
                }
                if client.is_some()
                    || !self
                        .component()
                        .detail
                        .unwrap_or_default()
                        .contains("gave up")
                {
                    self.down(&format!("stopped; gave up after {RESTARTS} restarts"));
                }
                continue;
            }
            if let Some(c) = child {
                let _ = c.terminate(STOP_GRACE).await;
            }
            self.down(&format!("stopped; restarting ({restarts}/{RESTARTS})"));
            tokio::select! {
                _ = stop.changed() => return,
                _ = tokio::time::sleep(Duration::from_secs(1 << (restarts - 1))) => {}
            }
            if *stop.borrow() {
                return;
            }
            if let Err(why) = self.bring_up().await {
                let gave_up = restarts >= RESTARTS;
                self.down(&if gave_up {
                    format!("stopped; gave up after {RESTARTS} restarts (last: {why})")
                } else {
                    format!("stopped; restart {restarts}/{RESTARTS} failed: {why}")
                });
            }
        }
    }
}
