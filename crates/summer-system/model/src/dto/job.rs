use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobScheduleType {
    Cron,
    Interval,
    Delay,
    #[serde(alias = "")]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobRunMode {
    #[default]
    Bean,
    GlueGroovy,
    GlueShell,
    GluePython,
    GluePhp,
    GlueNodejs,
    GluePowershell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobRouterStrategy {
    First,
    Last,
    RoundRobin,
    Random,
    ConsistentHash,
    ShardingBroadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobPastDueStrategy {
    Default,
    Ignore,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobBlockingStrategy {
    SerialExecution,
    DiscardLater,
    CoverEarly,
    Other,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobDto {
    #[validate(length(min = 1, max = 128, message = "应用名称长度必须在1-128之间"))]
    pub app_name: String,
    #[validate(length(min = 1, max = 128, message = "命名空间长度必须在1-128之间"))]
    pub namespace: String,
    #[validate(length(max = 128, message = "任务唯一标识长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[validate(length(max = 256, message = "任务处理器名称长度不能超过256"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_type: Option<JobScheduleType>,
    #[validate(length(max = 128, message = "Cron表达式长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_mode: Option<JobRunMode>,
    #[validate(length(max = 500, message = "任务描述长度不能超过500"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[validate(length(max = 4000, message = "触发参数长度不能超过4000"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_strategy: Option<JobRouterStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub past_due_strategy: Option<JobPastDueStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_strategy: Option<JobBlockingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub try_times: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
}

impl CreateJobDto {
    pub fn validate_semantics(&self) -> Result<(), String> {
        validate_schedule(
            self.schedule_type,
            self.cron_value.as_deref(),
            self.delay_second,
            self.interval_second,
        )?;
        validate_run_mode_for_create(self.run_mode, self.handle_name.as_deref())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJobDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[validate(length(max = 128, message = "应用名称长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[validate(length(max = 128, message = "命名空间长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[validate(length(max = 128, message = "任务唯一标识长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[validate(length(max = 256, message = "任务处理器名称长度不能超过256"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_type: Option<JobScheduleType>,
    #[validate(length(max = 128, message = "Cron表达式长度不能超过128"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_mode: Option<JobRunMode>,
    #[validate(length(max = 500, message = "任务描述长度不能超过500"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[validate(length(max = 4000, message = "触发参数长度不能超过4000"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_strategy: Option<JobRouterStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub past_due_strategy: Option<JobPastDueStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_strategy: Option<JobBlockingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub try_times: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
}

impl UpdateJobDto {
    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    pub fn enable_request(id: u64, enable: bool) -> Self {
        Self {
            id: Some(id),
            app_name: None,
            namespace: None,
            key: None,
            handle_name: None,
            schedule_type: None,
            cron_value: None,
            delay_second: None,
            interval_second: None,
            run_mode: None,
            description: None,
            trigger_param: None,
            router_strategy: None,
            past_due_strategy: None,
            blocking_strategy: None,
            timeout_second: None,
            try_times: None,
            retry_interval: None,
            enable: Some(enable),
        }
    }

    pub fn validate_semantics(&self) -> Result<(), String> {
        if let Some(id) = self.id
            && id == 0
        {
            return Err("任务ID必须大于0".to_string());
        }
        validate_schedule(
            self.schedule_type,
            self.cron_value.as_deref(),
            self.delay_second,
            self.interval_second,
        )?;
        validate_run_mode_for_update(self.run_mode, self.handle_name.as_deref())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct JobListQueryDto {
    pub namespace: Option<String>,
    pub app_name: Option<String>,
    pub like_description: Option<String>,
    pub like_handle_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct JobTaskQueryDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<u64>,
}

impl JobTaskQueryDto {
    pub fn with_job_id(mut self, job_id: u64) -> Self {
        self.job_id = Some(job_id);
        self
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct JobKeyQueryDto {
    #[validate(length(min = 1, max = 128, message = "命名空间长度必须在1-128之间"))]
    pub namespace: String,
    #[validate(length(min = 1, max = 128, message = "应用名称长度必须在1-128之间"))]
    pub app_name: String,
    #[validate(length(min = 1, max = 128, message = "任务唯一标识长度必须在1-128之间"))]
    pub key: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveJobRequest {
    pub id: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TriggerJobDto {
    #[validate(length(max = 4000, message = "触发参数长度不能超过4000"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_param: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TriggerJobRequest {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_param: Option<String>,
}

fn validate_schedule(
    schedule_type: Option<JobScheduleType>,
    cron_value: Option<&str>,
    delay_second: Option<u32>,
    interval_second: Option<u32>,
) -> Result<(), String> {
    match schedule_type {
        Some(JobScheduleType::Cron) if cron_value.unwrap_or_default().trim().is_empty() => {
            Err("CRON调度必须填写Cron表达式".to_string())
        }
        Some(JobScheduleType::Interval) if interval_second.unwrap_or_default() == 0 => {
            Err("INTERVAL调度必须填写大于0的间隔秒数".to_string())
        }
        Some(JobScheduleType::Delay) if delay_second.unwrap_or_default() == 0 => {
            Err("DELAY调度必须填写大于0的延迟秒数".to_string())
        }
        _ => Ok(()),
    }
}

fn validate_run_mode_for_create(
    run_mode: Option<JobRunMode>,
    handle_name: Option<&str>,
) -> Result<(), String> {
    if run_mode.unwrap_or_default() == JobRunMode::Bean
        && handle_name.unwrap_or_default().trim().is_empty()
    {
        return Err("BEAN运行模式必须填写任务处理器名称".to_string());
    }
    Ok(())
}

fn validate_run_mode_for_update(
    run_mode: Option<JobRunMode>,
    handle_name: Option<&str>,
) -> Result<(), String> {
    if run_mode == Some(JobRunMode::Bean) && handle_name.unwrap_or_default().trim().is_empty() {
        return Err("BEAN运行模式必须填写任务处理器名称".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_create() -> CreateJobDto {
        CreateJobDto {
            app_name: "summerrs-admin-executor".to_string(),
            namespace: "xxl".to_string(),
            key: None,
            handle_name: Some("demo_handler".to_string()),
            schedule_type: None,
            cron_value: None,
            delay_second: None,
            interval_second: None,
            run_mode: Some(JobRunMode::Bean),
            description: None,
            trigger_param: None,
            router_strategy: None,
            past_due_strategy: None,
            blocking_strategy: None,
            timeout_second: None,
            try_times: None,
            retry_interval: None,
            enable: None,
        }
    }

    #[test]
    fn validates_cron_requires_cron_value() {
        let mut dto = base_create();
        dto.schedule_type = Some(JobScheduleType::Cron);
        dto.cron_value = None;
        assert!(dto.validate_semantics().is_err());

        dto.cron_value = Some("0/15 * * * * *".to_string());
        assert!(dto.validate_semantics().is_ok());
    }

    #[test]
    fn validates_bean_requires_handle_name() {
        let mut dto = base_create();
        dto.handle_name = Some(" ".to_string());
        assert!(dto.validate_semantics().is_err());
    }

    #[test]
    fn enable_request_serializes_only_id_and_enable() {
        let dto = UpdateJobDto::enable_request(9, false);
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value, serde_json::json!({"id": 9, "enable": false}));
    }

    #[test]
    fn trigger_request_serializes_id_and_optional_trigger_param() {
        let dto = TriggerJobRequest {
            id: 9,
            trigger_param: Some("manual".to_string()),
        };
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"id": 9, "triggerParam": "manual"})
        );

        let dto = TriggerJobRequest {
            id: 9,
            trigger_param: None,
        };
        let value = serde_json::to_value(dto).unwrap();
        assert_eq!(value, serde_json::json!({"id": 9}));
    }

    #[test]
    fn update_allows_partial_payload_without_run_mode() {
        let dto = UpdateJobDto {
            id: Some(9),
            app_name: None,
            namespace: None,
            key: None,
            handle_name: None,
            schedule_type: None,
            cron_value: None,
            delay_second: None,
            interval_second: None,
            run_mode: None,
            description: Some("只改描述".to_string()),
            trigger_param: None,
            router_strategy: None,
            past_due_strategy: None,
            blocking_strategy: None,
            timeout_second: None,
            try_times: None,
            retry_interval: None,
            enable: None,
        };
        assert!(dto.validate_semantics().is_ok());
        assert!(
            UpdateJobDto::enable_request(9, false)
                .validate_semantics()
                .is_ok()
        );
    }

    #[test]
    fn update_requires_handle_name_when_switching_to_bean() {
        let dto = UpdateJobDto {
            id: Some(9),
            app_name: None,
            namespace: None,
            key: None,
            handle_name: None,
            schedule_type: None,
            cron_value: None,
            delay_second: None,
            interval_second: None,
            run_mode: Some(JobRunMode::Bean),
            description: None,
            trigger_param: None,
            router_strategy: None,
            past_due_strategy: None,
            blocking_strategy: None,
            timeout_second: None,
            try_times: None,
            retry_interval: None,
            enable: None,
        };
        assert!(dto.validate_semantics().is_err());
    }
}
