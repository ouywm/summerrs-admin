use serde_json::Value;
use tokio::sync::mpsc;

use crate::enums::TriggerType;

#[derive(Debug, Clone)]
pub struct LocalTrigger {
    pub job_id: i64,
    pub trigger_by: Option<i64>,
    pub params_override: Option<Value>,
    pub trigger_type: TriggerType,
}

pub type LocalTriggerSender = mpsc::UnboundedSender<LocalTrigger>;
pub type LocalTriggerReceiver = mpsc::UnboundedReceiver<LocalTrigger>;

pub fn channel() -> (LocalTriggerSender, LocalTriggerReceiver) {
    mpsc::unbounded_channel()
}
