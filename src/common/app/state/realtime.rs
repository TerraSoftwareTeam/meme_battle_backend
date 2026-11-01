use crate::features::realtime::OutboxProcessor;
use std::sync::Arc;

#[derive(Clone)]
pub struct RealtimeState {
    pub processor: Arc<OutboxProcessor>,
}

impl RealtimeState {
    pub fn new(processor: Arc<OutboxProcessor>) -> Self {
        Self { processor }
    }
}
