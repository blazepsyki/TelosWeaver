//! Codex App Server adapter, one owned process and one turn per Run (Spec §7, PH0-CODEX).
use crate::{
    AdapterError, AgentAdapter, CliBinaryRule, ProviderCliRule,
    runtime::{CommandSpec, OutputLimits, ProcessRuntime, StdinMode},
};
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use telos_schema::*;
use tokio::sync::watch;

mod protocol;
mod rpc;
mod run;
use rpc::{RpcClient, error};
use run::RunEntry;

pub const PROVIDER_ID: &str = "codex";
pub fn cli_rule(target_platform: &str) -> Option<ProviderCliRule> {
    let platform = match target_platform {
        "win32-x64" => "windows-x86_64",
        "win32-arm64" => "windows-aarch64",
        "linux-x64" => "linux-x86_64",
        "linux-arm64" => "linux-aarch64",
        "darwin-x64" => "macos-x86_64",
        "darwin-arm64" => "macos-aarch64",
        _ => return None,
    };
    let binary = if target_platform.starts_with("win32-") {
        "codex.exe"
    } else {
        "codex"
    };
    Some(ProviderCliRule {
        extension_id: "openai.chatgpt".into(),
        binary: CliBinaryRule::InExtension {
            relative_path: PathBuf::from("bin").join(platform).join(binary),
        },
        binary_name: binary.into(),
        version_args: vec!["--version".into()],
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexSandbox {
    ReadOnly,
    WorkspaceWrite,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexApprovals {
    Deny,
    Prompt,
}
/// Run settings must come from the Core's trusted Workspace, Snapshot and Policy services.
#[derive(Debug, Clone)]
pub struct CodexLaunch {
    pub cwd: PathBuf,
    pub prompt: String,
    pub model: String,
    pub effort: String,
    pub sandbox: CodexSandbox,
    pub approvals: CodexApprovals,
    pub run_timeout: Duration,
}
impl CodexLaunch {
    pub fn read_only(cwd: PathBuf, prompt: String, model: String) -> Self {
        Self {
            cwd,
            prompt,
            model,
            effort: "low".into(),
            sandbox: CodexSandbox::ReadOnly,
            approvals: CodexApprovals::Deny,
            run_timeout: Duration::from_secs(120),
        }
    }
}
pub trait CodexLaunchResolver: Send + Sync {
    fn resolve(&self, task: &TaskSpec, context: &RunContext) -> Result<CodexLaunch, AdapterError>;
    /// Allow requires a trusted check of this exact native action and immutable Run context.
    fn authorize_approval(
        &self,
        _task: &TaskSpec,
        _context: &RunContext,
        _request: &Value,
    ) -> Result<(), AdapterError> {
        Err(AdapterError::UnsupportedCapability {
            capability: capability_keys::ANSWER_APPROVAL.into(),
            reason: "a trusted native approval policy is not configured".into(),
        })
    }
}
/// VS Code list visibility (Spec §7.6, DEC-08). Hidden must fail if no native means exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CodexVscodeListing {
    #[default]
    Listed,
    Hidden,
}

pub struct CodexConfig {
    pub executable: PathBuf,
    pub cli_version: String,
    pub host_id: String,
    pub runtime: Arc<dyn ProcessRuntime>,
    pub launcher: Arc<dyn CodexLaunchResolver>,
    pub request_timeout: Duration,
    pub cancel_grace: Duration,
}
struct StartSlot {
    fingerprint: Value,
    result: watch::Sender<Option<Result<RunStarted, AdapterError>>>,
}
#[derive(Default)]
struct Registry {
    slots: HashMap<String, Arc<StartSlot>>,
    run_keys: HashMap<String, String>,
    runs: HashMap<String, Arc<RunEntry>>,
    owned_threads: HashMap<String, bool>,
}
struct Inner {
    config: Option<CodexConfig>,
    registry: Mutex<Registry>,
    vscode_listing: CodexVscodeListing,
}
pub struct CodexAdapter {
    inner: Arc<Inner>,
}
impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                config: None,
                registry: Mutex::new(Registry::default()),
                vscode_listing: CodexVscodeListing::default(),
            }),
        }
    }
}
impl CodexAdapter {
    pub fn new(config: CodexConfig) -> Self {
        Self::with_vscode_listing(config, CodexVscodeListing::default())
    }
    /// Explicit listing configuration without breaking existing Core configuration literals (§7.6).
    pub fn with_vscode_listing(config: CodexConfig, vscode_listing: CodexVscodeListing) -> Self {
        Self {
            inner: Arc::new(Inner {
                config: Some(config),
                registry: Mutex::new(Registry::default()),
                vscode_listing,
            }),
        }
    }
    fn config(&self) -> Result<&CodexConfig, AdapterError> {
        self.inner
            .config
            .as_ref()
            .ok_or_else(|| AdapterError::UnsupportedCapability {
                capability: capability_keys::MANAGED_RUN.into(),
                reason: "Codex executable and trusted launch resolver are not configured".into(),
            })
    }
    fn run(&self, id: &str) -> Result<Arc<RunEntry>, AdapterError> {
        self.inner
            .registry
            .lock()
            .unwrap()
            .runs
            .get(id)
            .cloned()
            .ok_or_else(|| AdapterError::UnknownRun { run_id: id.into() })
    }
    /// Local catalog only; this method never starts a generation.
    pub async fn models(&self, cwd: PathBuf) -> Result<Value, AdapterError> {
        let config = self.config()?;
        let client = connect(config, cwd).await?;
        let result = client
            .request("model/list", json!({"includeHidden":false}))
            .await;
        let _ = client.close(config.cancel_grace).await;
        result
    }
    /// Only call for an authorized thread owned by the caller; never an active user session.
    pub async fn archive(&self, thread_id: &str, cwd: PathBuf) -> Result<(), AdapterError> {
        if thread_id.is_empty() || thread_id.len() > 256 {
            return Err(error(FailureKind::Configuration, "invalid thread ID"));
        }
        match self
            .inner
            .registry
            .lock()
            .unwrap()
            .owned_threads
            .get(thread_id)
        {
            Some(true) => return Ok(()),
            Some(false) => {}
            None => {
                return Err(error(
                    FailureKind::Configuration,
                    "only threads created by this Adapter may be archived",
                ));
            }
        }
        let config = self.config()?;
        let client = connect(config, cwd).await?;
        let result = client
            .request("thread/archive", json!({"threadId":thread_id}))
            .await
            .map(|_| ());
        let _ = client.close(config.cancel_grace).await;
        if result.is_ok() {
            self.inner
                .registry
                .lock()
                .unwrap()
                .owned_threads
                .insert(thread_id.into(), true);
        }
        result
    }
    pub fn owned_thread_ids(&self) -> Vec<String> {
        self.inner
            .registry
            .lock()
            .unwrap()
            .owned_threads
            .iter()
            .filter(|(_, archived)| !**archived)
            .map(|(id, _)| id.clone())
            .collect()
    }
}
#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &str {
        PROVIDER_ID
    }
    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        let config = self.config()?;
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "win32-x64",
            ("windows", "aarch64") => "win32-arm64",
            ("linux", "x86_64") => "linux-x64",
            ("linux", "aarch64") => "linux-arm64",
            ("macos", "x86_64") => "darwin-x64",
            ("macos", "aarch64") => "darwin-arm64",
            _ => "unsupported",
        };
        if cli_rule(target).is_none() {
            return Err(AdapterError::UnsupportedCapability {
                capability: capability_keys::MANAGED_RUN.into(),
                reason: "no Codex CLI rule for this execution platform".into(),
            });
        }
        if config.host_id != host_id {
            return Err(error(
                FailureKind::Configuration,
                "Codex belongs to another Host",
            ));
        }
        let mut capabilities: HashMap<String, Capability> = [
            capability_keys::MANAGED_RUN,
            capability_keys::EVENT_STREAM,
            capability_keys::CANCEL,
            capability_keys::NATIVE_RESUME,
            capability_keys::NATIVE_FORK,
            capability_keys::SEND_INPUT,
            capability_keys::ANSWER_APPROVAL,
        ]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                Capability {
                    state: if target=="win32-x64"&&config.cli_version=="codex-cli 0.164.0-alpha.11.1"&&matches!(name,capability_keys::MANAGED_RUN|capability_keys::EVENT_STREAM|capability_keys::CANCEL|capability_keys::NATIVE_RESUME|capability_keys::NATIVE_FORK){CapabilityState::Supported}else{CapabilityState::Unverified},
                    provider_version: config.cli_version.clone(),
                    host_id: host_id.into(),
                    checked_at: timestamp(),
                    evidence_level: if target=="win32-x64"&&config.cli_version=="codex-cli 0.164.0-alpha.11.1"&&matches!(name,capability_keys::MANAGED_RUN|capability_keys::EVENT_STREAM|capability_keys::CANCEL|capability_keys::NATIVE_RESUME|capability_keys::NATIVE_FORK){EvidenceLevel::EnvironmentTested}else{EvidenceLevel::MockTested},
                    reason: Some("Windows x64 0.164.0-alpha.11.1 basic execution/resume/fork/cancel have local evidence; other versions, steering and approval need environment checks".into()),
                },
            )
        })
        .collect();
        // §7.6, DEC-08: source/list checks are version/Host-specific; UI opening is separate evidence.
        let listing_verified = target == "win32-x64"
            && config.cli_version == "codex-cli 0.164.0-alpha.11.1"
            && config
                .executable
                .components()
                .any(|part| part.as_os_str() == "openai.chatgpt-26.1021.22027-win32-x64");
        capabilities.insert(capability_keys::VSCODE_LISTING.into(), Capability {
            state: match self.inner.vscode_listing {
                CodexVscodeListing::Hidden => CapabilityState::Unsupported,
                CodexVscodeListing::Listed if listing_verified => CapabilityState::Supported,
                CodexVscodeListing::Listed => CapabilityState::Unverified,
            },
            provider_version: config.cli_version.clone(),
            host_id: host_id.into(),
            checked_at: timestamp(),
            evidence_level: if listing_verified { EvidenceLevel::EnvironmentTested } else { EvidenceLevel::MockTested },
            reason: Some("Windows x64 extension 26.1021.22027 / CLI 0.164.0-alpha.11.1: persisted vscode threads are in the default DB list; hidden has no verified native creation mechanism and is rejected; UI listing/opening user-confirmed on 2026-10-25, live refresh unverified".into()),
        });
        Ok(capabilities)
    }
    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError> {
        self.config()?;
        if task.agent.provider != PROVIDER_ID
            || task.id != context.task_id
            || task.revision != context.task_revision
            || task.workspace.host_id != context.host_id
            || context.host_id != self.config()?.host_id
        {
            return Err(error(
                FailureKind::Configuration,
                "Task, Run and Host do not match",
            ));
        }
        if context.idempotency_key.is_empty()
            || context.idempotency_key.len() > 256
            || context.run_id.is_empty()
        {
            return Err(error(
                FailureKind::Configuration,
                "invalid execution identity",
            ));
        }
        let fingerprint = serde_json::to_value((task, context))
            .map_err(|_| error(FailureKind::Configuration, "invalid Task or Run"))?;
        let (slot, created) = {
            let mut registry = self.inner.registry.lock().unwrap();
            if let Some(slot) = registry.slots.get(&context.idempotency_key) {
                if slot.fingerprint != fingerprint {
                    return Err(AdapterError::IdempotencyConflict {
                        idempotency_key: context.idempotency_key.clone(),
                    });
                }
                (slot.clone(), false)
            } else {
                if registry.run_keys.contains_key(&context.run_id) {
                    return Err(AdapterError::IdempotencyConflict {
                        idempotency_key: context.idempotency_key.clone(),
                    });
                }
                if registry.slots.len() >= 128 {
                    return Err(error(
                        FailureKind::ResourceLimit,
                        "Codex Run registry is full",
                    ));
                }
                let slot = Arc::new(StartSlot {
                    fingerprint,
                    result: watch::channel(None).0,
                });
                registry
                    .run_keys
                    .insert(context.run_id.clone(), context.idempotency_key.clone());
                registry
                    .slots
                    .insert(context.idempotency_key.clone(), slot.clone());
                (slot, true)
            }
        };
        if created {
            let inner = self.inner.clone();
            let task = task.clone();
            let context = context.clone();
            let slot = slot.clone();
            tokio::spawn(async move {
                let result = start_once(inner, task, context).await;
                slot.result.send_replace(Some(result));
            });
        }
        let mut receiver = slot.result.subscribe();
        receiver
            .wait_for(Option::is_some)
            .await
            .map_err(|_| error(FailureKind::Unknown, "Codex start result was lost"))?
            .as_ref()
            .unwrap()
            .clone()
    }
    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        match self.run(run_id) {
            Ok(run) => run.subscribe(after_sequence),
            Err(error) => futures::stream::once(async move { Err(error) }).boxed(),
        }
    }
    async fn send_input(
        &self,
        run_id: &str,
        input_id: &str,
        text: &str,
    ) -> Result<(), AdapterError> {
        self.run(run_id)?.send_input(input_id, text).await
    }
    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        self.run(run_id)?
            .answer_approval(request_id, decision)
            .await
    }
    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError> {
        Ok(self.run(run_id)?.cancel(self.config()?.cancel_grace))
    }
    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError> {
        Ok(self.run(run_id)?.inspect())
    }
}
async fn connect(config: &CodexConfig, cwd: PathBuf) -> Result<Arc<RpcClient>, AdapterError> {
    let cwd = cwd.canonicalize().map_err(|_| {
        error(
            FailureKind::Configuration,
            "Workspace directory is unavailable",
        )
    })?;
    if !cwd.is_dir() || cwd.to_str().is_none() {
        return Err(error(
            FailureKind::Configuration,
            "Workspace directory is invalid",
        ));
    }
    let client = RpcClient::spawn(
        config.runtime.as_ref(),
        CommandSpec {
            program: config.executable.clone(),
            args: vec!["app-server".into(), "--stdio".into()],
            cwd,
            env: vec![],
            stdin: StdinMode::Piped,
            limits: OutputLimits {
                max_line_bytes: 1024 * 1024,
                max_buffered_bytes: 4 * 1024 * 1024,
            },
        },
        config.request_timeout,
    )
    .await?;
    if let Err(failure) = client.initialize().await {
        let _ = client.close(Duration::ZERO).await;
        return Err(failure);
    }
    Ok(client)
}
async fn start_once(
    inner: Arc<Inner>,
    task: TaskSpec,
    context: RunContext,
) -> Result<RunStarted, AdapterError> {
    let config = inner.config.as_ref().unwrap();
    if inner.vscode_listing == CodexVscodeListing::Hidden {
        return Err(AdapterError::UnsupportedCapability {
            capability: capability_keys::VSCODE_LISTING.into(),
            reason: "this App Server classifies threads as vscode even when threadSource is omitted; hidden creation has no verified native mechanism".into(),
        });
    }
    let launch = config.launcher.resolve(&task, &context)?;
    if launch.model.is_empty()
        || launch.effort.is_empty()
        || launch.prompt.len() > 256 * 1024
        || !launch.cwd.is_absolute()
        || launch.run_timeout.is_zero()
    {
        return Err(error(
            FailureKind::Configuration,
            "invalid Codex launch settings",
        ));
    }
    let client = connect(config, launch.cwd.clone()).await?;
    let mut created_thread = None;
    let result=async {
        let mut overrides=client.isolated_config().await?;
        overrides["sandbox_workspace_write.writable_roots"]=json!([]);
        overrides["sandbox_workspace_write.network_access"]=json!(false);
        overrides["sandbox_workspace_write.exclude_tmpdir_env_var"]=json!(true);
        overrides["sandbox_workspace_write.exclude_slash_tmp"]=json!(true);
        let sandbox=match launch.sandbox {CodexSandbox::ReadOnly=>"read-only",CodexSandbox::WorkspaceWrite=>"workspace-write"};
        let mut params=json!({"model":launch.model,"cwd":launch.cwd,"approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":sandbox,"config":overrides,"baseInstructions":"Complete only the requested task within the provided workspace. Do not access unrelated projects or sessions.","developerInstructions":"Do not delegate or use external MCP services. Follow the supplied permission policy.","serviceTier":"default","runtimeWorkspaceRoots":[launch.cwd]});
        let method=match context.session.mode {
            SessionMode::Fresh|SessionMode::Capsule=>{params["allowProviderModelFallback"]=json!(false);params["historyMode"]=json!("legacy");"thread/start"},
            SessionMode::Resume|SessionMode::NativeFork=>{
                let source=context.session.source_session_id.as_ref().filter(|id|!id.is_empty()&&id.len()<=256).ok_or_else(||error(FailureKind::Configuration,"source thread ID is required"))?;
                if context.session.mode==SessionMode::Resume&&context.session.source_turn_id.is_some(){return Err(AdapterError::UnsupportedCapability {capability:capability_keys::NATIVE_RESUME.into(),reason:"resume at a specific turn is not implemented".into()});}
                params["threadId"]=json!(source);params["excludeTurns"]=json!(true);
                params.as_object_mut().unwrap().remove("runtimeWorkspaceRoots");
                if context.session.mode==SessionMode::NativeFork {if let Some(turn)=&context.session.source_turn_id {params["lastTurnId"]=json!(turn);} "thread/fork"}else{"thread/resume"}
            }
        };
        // §7.6, DEC-08: preserve sessions; threadSource is analytics, not a proven visibility switch.
        if method != "thread/resume" {
            params["ephemeral"] = json!(false);
            params["threadSource"] = json!("vscode");
        }
        let response=client.request(method,params).await?;
        let thread=response.pointer("/thread/id").and_then(Value::as_str).filter(|id|!id.is_empty()).ok_or_else(||error(FailureKind::ProtocolParse,"Codex did not return a thread ID"))?.to_owned();
        if context.session.mode==SessionMode::NativeFork&&context.session.source_session_id.as_deref()==Some(&thread){return Err(error(FailureKind::ProtocolParse,"Codex fork reused the source thread ID"));}
        if context.session.mode==SessionMode::Resume&&context.session.source_session_id.as_deref()!=Some(&thread){return Err(error(FailureKind::ProtocolParse,"Codex resume changed the thread ID"));}
        if context.session.mode!=SessionMode::Resume {inner.registry.lock().unwrap().owned_threads.insert(thread.clone(),false);created_thread=Some(thread.clone());}
        if !matches!(response.pointer("/thread/source").and_then(Value::as_str), Some("cli" | "vscode")) {
            return Err(AdapterError::UnsupportedCapability {
                capability: capability_keys::VSCODE_LISTING.into(),
                reason: "App Server did not return a thread source included in the default interactive list".into(),
            });
        }
        let response=client.request("turn/start",json!({"threadId":thread,"input":[{"type":"text","text":launch.prompt}],"model":launch.model,"effort":launch.effort,"approvalPolicy":"on-request","approvalsReviewer":"user"})).await?;
        let turn=response.pointer("/turn/id").and_then(Value::as_str).filter(|id|!id.is_empty()).ok_or_else(||error(FailureKind::ProtocolParse,"Codex did not return a turn ID"))?.to_owned();
        let events=client.take_events()?;
        let launcher=config.launcher.clone();let approval_task=task.clone();let approval_context=context.clone();
        let approval_check=Arc::new(move|request:&Value|launcher.authorize_approval(&approval_task,&approval_context,request));
        let run=RunEntry::new(context.run_id.clone(),client.clone(),thread,turn,launch.clone(),config.cli_version.clone(),approval_check);
        let started=run.started.clone();inner.registry.lock().unwrap().runs.insert(context.run_id.clone(),run.clone());
        let grace=config.cancel_grace;tokio::spawn(async move {run.supervise(events,grace).await;});
        Ok(started)
    }.await;
    if result.is_err() {
        if let Some(thread) = created_thread
            && client
                .request("thread/archive", json!({"threadId":thread}))
                .await
                .is_ok()
        {
            inner
                .registry
                .lock()
                .unwrap()
                .owned_threads
                .insert(thread, true);
        }
        let _ = client.close(Duration::ZERO).await;
    }
    result
}
fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let z = (seconds / 86400) as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds % 86400 / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests;
