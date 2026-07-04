use std::sync::Arc;
use crate::features::realtime::OutboxProcessor;

#[derive(Clone)]
pub struct RealtimeState {
    pub processor: Arc<OutboxProcessor>,
}

impl RealtimeState {
    pub fn new(
        processor: Arc<OutboxProcessor>,
    ) -> Self {
        Self {
            processor,
        }
    }
}
