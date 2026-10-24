//! Daemon 종료 시 활성 Run 정리 (Spec §7.3, HOST 인계 §4-3).
//!
//! API 연결 종료는 Run 취소가 아닙니다. Daemon 자체가 끝날 때만 활성 Run을 취소하고
//! 각 Run의 종료 이벤트(`process_exited` 등)가 실제로 도착했는지 확인합니다.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use telos_adapters::{AdapterError, AgentAdapter};
use telos_schema::{
    ApprovalAnswer, Capability, ProviderEvent, RunContext, RunInspection, RunStarted, TaskSpec,
};

/// 시작한 Run을 기억하는 Adapter 래퍼입니다. 다른 동작은 그대로 위임합니다.
pub struct TrackedAdapter {
    inner: Arc<dyn AgentAdapter>,
    runs: Mutex<Vec<String>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub cancelled: usize,
    /// 제한 시간 안에 종료 이벤트를 확인하지 못한 Run. 성공으로 보지 않습니다.
    pub unconfirmed: Vec<String>,
}

impl TrackedAdapter {
    pub fn new(inner: Arc<dyn AgentAdapter>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            runs: Mutex::new(Vec::new()),
        })
    }

    /// 아직 끝나지 않은 Run을 취소하고 종료 이벤트를 기다립니다.
    pub async fn shutdown(&self, timeout: Duration) -> ShutdownReport {
        let runs = self.runs.lock().unwrap().clone();
        let mut report = ShutdownReport::default();
        let mut waits = HashMap::new();
        for run_id in runs {
            if self.inner.inspect_run(&run_id).await != Ok(RunInspection::Running) {
                continue;
            }
            if self.inner.cancel_run(&run_id).await == Ok(true) {
                report.cancelled += 1;
            }
            waits.insert(run_id.clone(), self.inner.events(&run_id, 0));
        }
        for (run_id, stream) in waits {
            let ended = tokio::time::timeout(timeout, stream.for_each(|_| async {})).await;
            if ended.is_err()
                || self.inner.inspect_run(&run_id).await != Ok(RunInspection::Finished)
            {
                report.unconfirmed.push(run_id);
            }
        }
        report
    }
}

#[async_trait]
impl AgentAdapter for TrackedAdapter {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn detect(&self, host_id: &str) -> Result<HashMap<String, Capability>, AdapterError> {
        self.inner.detect(host_id).await
    }

    async fn start_run(
        &self,
        task: &TaskSpec,
        context: &RunContext,
    ) -> Result<RunStarted, AdapterError> {
        let started = self.inner.start_run(task, context).await?;
        let mut runs = self.runs.lock().unwrap();
        if !runs.contains(&started.run_id) {
            runs.push(started.run_id.clone());
        }
        Ok(started)
    }

    fn events(
        &self,
        run_id: &str,
        after_sequence: u64,
    ) -> BoxStream<'static, Result<ProviderEvent, AdapterError>> {
        self.inner.events(run_id, after_sequence)
    }

    async fn send_input(
        &self,
        run_id: &str,
        input_id: &str,
        text: &str,
    ) -> Result<(), AdapterError> {
        self.inner.send_input(run_id, input_id, text).await
    }

    async fn answer_approval(
        &self,
        run_id: &str,
        request_id: &str,
        decision: ApprovalAnswer,
    ) -> Result<(), AdapterError> {
        self.inner
            .answer_approval(run_id, request_id, decision)
            .await
    }

    async fn cancel_run(&self, run_id: &str) -> Result<bool, AdapterError> {
        self.inner.cancel_run(run_id).await
    }

    async fn inspect_run(&self, run_id: &str) -> Result<RunInspection, AdapterError> {
        self.inner.inspect_run(run_id).await
    }
}
