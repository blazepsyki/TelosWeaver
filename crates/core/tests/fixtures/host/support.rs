#![allow(dead_code)]
use async_trait::async_trait;
use futures::{FutureExt, StreamExt, stream::BoxStream};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use telos_adapters::{AdapterError, AgentAdapter};
use telos_core::execution::{ExecutionSupervisor, RunAuthorizer, RunPermit};
use telos_schema::{api::*, fixtures::*, *};
use tokio::sync::watch;

pub struct Temporary {
    pub root: PathBuf,
}
impl Temporary {
    pub fn new() -> Self {
        let root =
            std::env::temp_dir().join(telos_core::execution::new_id("telos-host-test").unwrap());
        std::fs::create_dir_all(root.join("project")).unwrap();
        Self { root }
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
pub struct AllowFixture;
#[async_trait]
impl RunAuthorizer for AllowFixture {
    async fn authorize(&self, _: &TaskSpec, _: &SessionRequest) -> Result<RunPermit, ApiError> {
        Ok(RunPermit {
            workspace_id: "ws-fixture".into(),
            context_snapshot_id: "snapshot-fixture".into(),
            policy_revision: "fixture".into(),
            controller_epoch: 1,
        })
    }
}
#[derive(Default)]
pub struct FixtureAdapter {
    pub starts: AtomicUsize,
    runs: Mutex<HashMap<String, (String, watch::Sender<bool>)>>,
}
#[async_trait]
impl AgentAdapter for FixtureAdapter {
    fn id(&self) -> &str {
        "codex"
    }
    async fn detect(&self, _: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        Ok(HashMap::new())
    }
    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.runs.lock().unwrap().insert(
            context.run_id.clone(),
            (task.objective.clone(), watch::channel(false).0),
        );
        Ok(RunStarted {
            run_id: context.run_id.clone(),
            native_session_id: None,
            native_turn_id: None,
        })
    }
    fn events(&self, id: &str, _: u64) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        let id = id.to_owned();
        let (mode, cancel) = self.runs.lock().unwrap().get(&id).unwrap().clone();
        async move {
            let events = match mode.as_str() {
                mode if mode == "hold" || mode.starts_with("hold-") => {
                    let mut cancelled = cancel.subscribe();
                    cancelled.wait_for(|v| *v).await.unwrap();
                    if mode.ends_with("after-ack") {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    let mut events = if mode == "hold" || mode.starts_with("hold-cancelled") {
                        cancelled_run_events(&id)
                    } else {
                        completed_run_events(&id)
                    };
                    if mode == "hold-failed" {
                        let turn = events
                            .iter_mut()
                            .find(|event| event.kind == ProviderEventKind::TurnFinished)
                            .unwrap();
                        turn.payload.insert("status".into(), "failed".into());
                        events
                            .last_mut()
                            .unwrap()
                            .payload
                            .insert("exitCode".into(), 1.into());
                    }
                    let exit = &mut events.last_mut().unwrap().payload;
                    if mode.contains("unknown-exit") {
                        exit.insert("termination".into(), "unknown".into());
                    } else if mode.contains("children-left") {
                        exit.insert("childrenReaped".into(), false.into());
                    } else if mode.contains("children-unknown") {
                        exit.remove("childrenReaped");
                    }
                    events
                }
                "missing" => {
                    let mut e = completed_run_events(&id);
                    e.truncate(2);
                    e
                }
                "gap" => {
                    let mut e = completed_run_events(&id);
                    e[1].sequence += 1;
                    e
                }
                "unknown" => tracking_lost_run_events(&id),
                "start-failed" => start_failed_run_events(&id),
                "persistent" => persistent_process_run_events(&id),
                _ => completed_run_events(&id),
            };
            futures::stream::iter(events.into_iter().map(Ok))
        }
        .into_stream()
        .flatten()
        .boxed()
    }
    async fn send_input(&self, _: &str, _: &str, _: &str) -> Result<(), AdapterError> {
        unreachable!()
    }
    async fn answer_approval(
        &self,
        _: &str,
        _: &str,
        _: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        unreachable!()
    }
    async fn cancel_run(&self, id: &str) -> Result<bool, AdapterError> {
        let cancel = self
            .runs
            .lock()
            .unwrap()
            .get(id)
            .map(|(_, cancel)| cancel.clone());
        if let Some(cancel) = cancel {
            cancel.send_replace(true);
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(true)
        } else {
            Err(AdapterError::UnknownRun { run_id: id.into() })
        }
    }
    async fn inspect_run(&self, _: &str) -> Result<RunInspection, AdapterError> {
        Ok(RunInspection::Unknown)
    }
}
pub fn supervisor() -> (Arc<ExecutionSupervisor>, Arc<FixtureAdapter>) {
    let adapter = Arc::new(FixtureAdapter::default());
    (
        Arc::new(ExecutionSupervisor::new(
            PROJECT_ID.into(),
            HOST_ID.into(),
            vec![adapter.clone()],
            Arc::new(AllowFixture),
        )),
        adapter,
    )
}
pub fn params(mode: &str, key: &str) -> RunsStartParams {
    let mut task = task_spec();
    task.objective = mode.into();
    let session = run_context(&task).session;
    RunsStartParams {
        task,
        session,
        idempotency_key: key.into(),
    }
}
