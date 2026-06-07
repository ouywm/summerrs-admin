use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::job::{
    JobBlockingStrategy, JobPastDueStrategy, JobRouterStrategy, JobRunMode, JobScheduleType,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RatchJobVo {
    pub id: u64,
    pub enable: bool,
    pub app_name: String,
    pub namespace: String,
    pub key: Option<String>,
    pub description: String,
    pub schedule_type: JobScheduleType,
    pub cron_value: String,
    pub delay_second: u32,
    pub interval_second: u32,
    pub run_mode: JobRunMode,
    pub handle_name: String,
    pub trigger_param: String,
    pub router_strategy: JobRouterStrategy,
    pub past_due_strategy: JobPastDueStrategy,
    pub blocking_strategy: JobBlockingStrategy,
    pub timeout_second: u32,
    pub try_times: u32,
    pub version_id: u64,
    pub last_modified_millis: u64,
    #[serde(alias = "registerTime")]
    pub create_time: u64,
    pub retry_interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RatchJobTaskVo {
    pub task_id: u64,
    pub job_id: u64,
    pub trigger_time: u64,
    pub instance_addr: String,
    pub trigger_message: String,
    pub status: String,
    pub finish_time: u64,
    pub callback_message: String,
    pub execution_time: u64,
    pub trigger_from: String,
    pub try_times: u32,
    #[serde(default)]
    pub try_logs: Vec<RatchJobTryLogVo>,
    pub retry_interval: u32,
    pub retry_count: u32,
    pub timeout_second: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RatchJobTryLogVo {
    pub execution_time: u64,
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RatchNamespaceVo {
    pub namespace: String,
    pub namespace_desc: String,
}
