use caelix_api::context::AgentRunManagerTrait;
use caelix_api::cancel::CancellationToken;
use dashmap::DashMap;
use tokio::task::AbortHandle;

struct AgentRunInfo {
    abort_handle: AbortHandle,
    cancel_token: CancellationToken,
}

pub struct AgentRunManager {
    runs: DashMap<String, AgentRunInfo>,
}

impl AgentRunManager {
    pub fn new() -> Self {
        Self {
            runs: DashMap::new(),
        }
    }

    pub fn register(
        &self,
        session_id: String,
        abort_handle: AbortHandle,
        cancel_token: CancellationToken,
    ) {
        if let Some(old) = self.runs.insert(session_id, AgentRunInfo {
            abort_handle: abort_handle.clone(),
            cancel_token: cancel_token.clone(),
        }) {
            old.cancel_token.cancel();
            old.abort_handle.abort();
        }
    }

    pub async fn stop(&self, session_id: &str) -> bool {
        if let Some((_, info)) = self.runs.remove(session_id) {
            info.cancel_token.cancel();
            info.abort_handle.abort();
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, session_id: &str) {
        self.runs.remove(session_id);
    }
}

impl Default for AgentRunManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentRunManagerTrait for AgentRunManager {
    async fn stop_agent(&self, session_id: &str) -> bool {
        self.stop(session_id).await
    }
}
