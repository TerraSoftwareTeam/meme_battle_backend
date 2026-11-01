use crate::features::realtime::RealtimeOutboxWorker;
use std::sync::Arc;

#[derive(Clone)]
pub struct RealtimeState {
    pub outbox_worker: Arc<RealtimeOutboxWorker>,
    /// Backward-compatibility alias for outbox_worker
    pub processor: Arc<RealtimeOutboxWorker>,
}

impl RealtimeState {
    pub fn new(outbox_worker: Arc<RealtimeOutboxWorker>) -> Self {
        Self {
            processor: outbox_worker.clone(),
            outbox_worker,
        }
    }
}
