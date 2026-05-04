use chrono::NaiveDateTime;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use validator::Validate;

use crate::entity::{sys_job, sys_job_run};
use crate::enums::{
    BlockingStrategy, MisfireStrategy, RetryBackoff, RunState, ScheduleType, ScriptEngine,
    TriggerType,
};

// ---------------------------------------------------------------------------
// 任务定义 DTO
// ---------------------------------------------------------------------------

/// 创建任务
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobDto {
    #[validate(length(min = 1, max = 128, message = "任务名称必须 1-128 个字符"))]
    pub name: String,
    pub group_name: Option<String>,
    pub description: Option<String>,
    #[validate(length(min = 1, max = 128, message = "handler 名称必须 1-128 个字符"))]
    pub handler: String,
    pub schedule_type: ScheduleType,
    pub cron_expr: Option<String>,
    pub interval_ms: Option<i64>,
    pub fire_time: Option<NaiveDateTime>,
    pub params_json: Option<Json>,
    pub script: Option<String>,
    pub script_engine: Option<ScriptEngine>,
    pub enabled: Option<bool>,
    pub blocking: Option<BlockingStrategy>,
    pub misfire: Option<MisfireStrategy>,
    pub timeout_ms: Option<i64>,
    pub retry_max: Option<i32>,
    pub retry_backoff: Option<RetryBackoff>,
    pub unique_key: Option<String>,
    pub tenant_id: Option<i64>,
}

impl CreateJobDto {
    pub fn into_active_model(self, created_by: Option<i64>) -> sys_job::ActiveModel {
        sys_job::ActiveModel {
            id: NotSet,
            tenant_id: Set(self.tenant_id),
            name: Set(self.name),
            group_name: Set(self.group_name.unwrap_or_else(|| "default".to_string())),
            description: Set(self.description.unwrap_or_default()),
            handler: Set(self.handler),
            schedule_type: Set(self.schedule_type),
            cron_expr: Set(self.cron_expr),
            interval_ms: Set(self.interval_ms),
            fire_time: Set(self.fire_time),
            params_json: Set(self.params_json.unwrap_or(serde_json::json!({}))),
            script: Set(self.script),
            script_engine: Set(self.script_engine),
            enabled: Set(self.enabled.unwrap_or(true)),
            blocking: Set(self.blocking.unwrap_or(BlockingStrategy::Serial)),
            misfire: Set(self.misfire.unwrap_or(MisfireStrategy::FireNow)),
            timeout_ms: Set(self.timeout_ms.unwrap_or(0)),
            retry_max: Set(self.retry_max.unwrap_or(0)),
            retry_backoff: Set(self.retry_backoff.unwrap_or(RetryBackoff::Exponential)),
            unique_key: Set(self.unique_key),
            version: Set(0),
            created_by: Set(created_by),
            create_time: NotSet,
            update_time: NotSet,
        }
    }
}

/// 更新任务（所有字段可选；提供的字段才更新）
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJobDto {
    pub name: Option<String>,
    pub group_name: Option<String>,
    pub description: Option<String>,
    pub handler: Option<String>,
    pub schedule_type: Option<ScheduleType>,
    pub cron_expr: Option<Option<String>>,
    pub interval_ms: Option<Option<i64>>,
    pub fire_time: Option<Option<NaiveDateTime>>,
    pub params_json: Option<Json>,
    pub script: Option<Option<String>>,
    pub script_engine: Option<Option<ScriptEngine>>,
    pub enabled: Option<bool>,
    pub blocking: Option<BlockingStrategy>,
    pub misfire: Option<MisfireStrategy>,
    pub timeout_ms: Option<i64>,
    pub retry_max: Option<i32>,
    pub retry_backoff: Option<RetryBackoff>,
    pub unique_key: Option<Option<String>>,
}

impl UpdateJobDto {
    /// 把 dto 中提供的字段应用到 ActiveModel；同时把 version 自增 1（乐观锁 + 触发 reload 事件）。
    pub fn apply_to(self, active: &mut sys_job::ActiveModel, current_version: i64) {
        if let Some(v) = self.name {
            active.name = Set(v);
        }
        if let Some(v) = self.group_name {
            active.group_name = Set(v);
        }
        if let Some(v) = self.description {
            active.description = Set(v);
        }
        if let Some(v) = self.handler {
            active.handler = Set(v);
        }
        if let Some(v) = self.schedule_type {
            active.schedule_type = Set(v);
        }
        if let Some(v) = self.cron_expr {
            active.cron_expr = Set(v);
        }
        if let Some(v) = self.interval_ms {
            active.interval_ms = Set(v);
        }
        if let Some(v) = self.fire_time {
            active.fire_time = Set(v);
        }
        if let Some(v) = self.params_json {
            active.params_json = Set(v);
        }
        if let Some(v) = self.script {
            active.script = Set(v);
        }
        if let Some(v) = self.script_engine {
            active.script_engine = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.blocking {
            active.blocking = Set(v);
        }
        if let Some(v) = self.misfire {
            active.misfire = Set(v);
        }
        if let Some(v) = self.timeout_ms {
            active.timeout_ms = Set(v);
        }
        if let Some(v) = self.retry_max {
            active.retry_max = Set(v);
        }
        if let Some(v) = self.retry_backoff {
            active.retry_backoff = Set(v);
        }
        if let Some(v) = self.unique_key {
            active.unique_key = Set(v);
        }
        active.version = Set(current_version + 1);
    }
}

/// 任务列表查询
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobQueryDto {
    pub name: Option<String>,
    pub group_name: Option<String>,
    pub handler: Option<String>,
    pub schedule_type: Option<ScheduleType>,
    pub enabled: Option<bool>,
    pub tenant_id: Option<i64>,
}

impl From<JobQueryDto> for Condition {
    fn from(q: JobQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(name) = q.name
            && !name.is_empty()
        {
            cond = cond.add(sys_job::Column::Name.contains(name));
        }
        if let Some(group_name) = q.group_name
            && !group_name.is_empty()
        {
            cond = cond.add(sys_job::Column::GroupName.eq(group_name));
        }
        if let Some(handler) = q.handler
            && !handler.is_empty()
        {
            cond = cond.add(sys_job::Column::Handler.eq(handler));
        }
        if let Some(t) = q.schedule_type {
            cond = cond.add(sys_job::Column::ScheduleType.eq(t));
        }
        if let Some(enabled) = q.enabled {
            cond = cond.add(sys_job::Column::Enabled.eq(enabled));
        }
        if let Some(tid) = q.tenant_id {
            cond = cond.add(sys_job::Column::TenantId.eq(tid));
        }
        cond
    }
}

/// 手动触发参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TriggerJobDto {
    /// 覆盖任务默认 params_json（仅本次触发生效，不写库）
    pub params_override: Option<Json>,
}

// ---------------------------------------------------------------------------
// 执行记录查询
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobRunQueryDto {
    pub job_id: Option<i64>,
    pub trace_id: Option<String>,
    pub trigger_type: Option<TriggerType>,
    pub state: Option<RunState>,
    pub instance: Option<String>,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
}

impl From<JobRunQueryDto> for Condition {
    fn from(q: JobRunQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(id) = q.job_id {
            cond = cond.add(sys_job_run::Column::JobId.eq(id));
        }
        if let Some(tid) = q.trace_id
            && !tid.is_empty()
        {
            cond = cond.add(sys_job_run::Column::TraceId.eq(tid));
        }
        if let Some(t) = q.trigger_type {
            cond = cond.add(sys_job_run::Column::TriggerType.eq(t));
        }
        if let Some(s) = q.state {
            cond = cond.add(sys_job_run::Column::State.eq(s));
        }
        if let Some(inst) = q.instance
            && !inst.is_empty()
        {
            cond = cond.add(sys_job_run::Column::Instance.eq(inst));
        }
        if let Some(start) = q.start_time {
            cond = cond.add(sys_job_run::Column::ScheduledAt.gte(start));
        }
        if let Some(end) = q.end_time {
            cond = cond.add(sys_job_run::Column::ScheduledAt.lte(end));
        }
        cond
    }
}

// ---------------------------------------------------------------------------
// VO
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobVo {
    pub id: i64,
    pub tenant_id: Option<i64>,
    pub name: String,
    pub group_name: String,
    pub handler: String,
    pub schedule_type: ScheduleType,
    pub cron_expr: Option<String>,
    pub enabled: bool,
    pub blocking: BlockingStrategy,
    pub create_time: NaiveDateTime,
    pub update_time: NaiveDateTime,
    /// 下次触发时间（enabled=false 或 OneShot 已触发返回 null）
    pub next_fire_at: Option<NaiveDateTime>,
    /// 最近一次执行的状态
    pub last_run_state: Option<RunState>,
    /// 最近一次执行的完成时间
    pub last_run_finished_at: Option<NaiveDateTime>,
}

impl JobVo {
    pub fn from_model_with_runtime(
        m: sys_job::Model,
        next_fire_at: Option<NaiveDateTime>,
        last_run_state: Option<RunState>,
        last_run_finished_at: Option<NaiveDateTime>,
    ) -> Self {
        Self {
            id: m.id,
            tenant_id: m.tenant_id,
            name: m.name,
            group_name: m.group_name,
            handler: m.handler,
            schedule_type: m.schedule_type,
            cron_expr: m.cron_expr,
            enabled: m.enabled,
            blocking: m.blocking,
            create_time: m.create_time,
            update_time: m.update_time,
            next_fire_at,
            last_run_state,
            last_run_finished_at,
        }
    }

    pub fn from_model(m: sys_job::Model) -> Self {
        Self::from_model_with_runtime(m, None, None, None)
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobDetailVo {
    #[serde(flatten)]
    pub model: sys_job::Model,
    /// 下次触发时间（enabled=false 或 OneShot 已触发返回 null）
    pub next_fire_at: Option<NaiveDateTime>,
    /// 最近一次执行的状态
    pub last_run_state: Option<RunState>,
    /// 最近一次执行的完成时间
    pub last_run_finished_at: Option<NaiveDateTime>,
}

impl JobDetailVo {
    pub fn from_model(m: sys_job::Model) -> Self {
        Self {
            model: m,
            next_fire_at: None,
            last_run_state: None,
            last_run_finished_at: None,
        }
    }

    pub fn from_model_with_runtime(
        m: sys_job::Model,
        next_fire_at: Option<NaiveDateTime>,
        last_run_state: Option<RunState>,
        last_run_finished_at: Option<NaiveDateTime>,
    ) -> Self {
        Self {
            model: m,
            next_fire_at,
            last_run_state,
            last_run_finished_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobRunVo {
    #[serde(flatten)]
    pub model: sys_job_run::Model,
}

impl JobRunVo {
    pub fn from_model(m: sys_job_run::Model) -> Self {
        Self { model: m }
    }
}

/// handler registry 列表项（用于网页下拉选择）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HandlerVo {
    pub name: String,
}

// ---------------------------------------------------------------------------
// 任务依赖 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AddDependencyDto {
    pub downstream_id: i64,
    #[serde(default = "default_on_state")]
    pub on_state: crate::enums::DependencyOnState,
}

fn default_on_state() -> crate::enums::DependencyOnState {
    crate::enums::DependencyOnState::Succeeded
}

/// 依赖关系展示（admin 列表用）：附带对端 job 的 name / groupName 方便前端渲染
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DependencyVo {
    pub id: i64,
    pub upstream_id: i64,
    pub upstream_name: String,
    pub downstream_id: i64,
    pub downstream_name: String,
    pub on_state: crate::enums::DependencyOnState,
    pub enabled: bool,
    pub create_time: NaiveDateTime,
}

/// 双向依赖列表（GET /jobs/{id}/dependencies 返回）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobDependencyListVo {
    /// 出向：本 job 作为 upstream
    pub outgoing: Vec<DependencyVo>,
    /// 入向：本 job 作为 downstream
    pub incoming: Vec<DependencyVo>,
}

// ---------------------------------------------------------------------------
// 批量操作 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct BatchToggleDto {
    #[validate(length(min = 1, max = 100, message = "ids 数量必须 1-100"))]
    pub ids: Vec<i64>,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct BatchIdsDto {
    #[validate(length(min = 1, max = 100, message = "ids 数量必须 1-100"))]
    pub ids: Vec<i64>,
}

/// 批量操作结果（部分成功也算 200 返回，前端按 failures 数组处理）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchResultVo {
    pub success_count: usize,
    pub failed_count: usize,
    pub failures: Vec<BatchFailure>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchFailure {
    pub id: i64,
    pub reason: String,
}
