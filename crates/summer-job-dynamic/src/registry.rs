use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::context::{JobContext, JobResult};
use crate::dto::CreateJobDto;

pub type HandlerFuture = Pin<Box<dyn Future<Output = JobResult> + Send>>;
pub type HandlerFn = fn(JobContext) -> HandlerFuture;

/// 编译期收集的 handler 注册项。
///
/// `#[job_handler("name")]` 宏展开生成 `inventory::submit!`，把 handler 函数包成
/// 统一签名 `fn(JobContext) -> Pin<Box<dyn Future<Output = JobResult>>>`，并把函数
/// 上的 `///` doc comment 作为 description 一并注册（前端下拉里展示用）。
pub struct JobHandlerEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub call: HandlerFn,
}

inventory::collect!(JobHandlerEntry);

/// 编译期声明的"内置任务" —— 启动期 `SchedulerPlugin::start` 把它们 import 到 DB。
///
/// 已存在（同租户同名）时**不**覆盖，运维改的 cron / 启停以 DB 为准。
/// 给宿主 crate 用 `inventory::submit!` 注册：
///
/// ```rust,ignore
/// use summer_job_dynamic::{BuiltinJob, dto::CreateJobDto};
/// fn my_default() -> CreateJobDto { ... }
/// inventory::submit!(BuiltinJob { dto_factory: my_default });
/// ```
pub struct BuiltinJob {
    pub dto_factory: fn() -> CreateJobDto,
}

inventory::collect!(BuiltinJob);

/// 启动期一次性收集所有 inventory 项的 handler 注册表。
///
/// 调度器按 `sys_job.handler` 字段（字符串）从这张表查函数指针调用。
pub struct HandlerRegistry {
    map: HashMap<&'static str, HandlerFn>,
    descriptions: HashMap<&'static str, &'static str>,
}

impl HandlerRegistry {
    pub fn collect() -> Self {
        let mut map = HashMap::new();
        let mut descriptions = HashMap::new();
        for entry in inventory::iter::<JobHandlerEntry> {
            if map.insert(entry.name, entry.call).is_some() {
                tracing::warn!(
                    handler = entry.name,
                    "duplicate job handler name, last registration wins"
                );
            }
            descriptions.insert(entry.name, entry.description);
        }
        Self { map, descriptions }
    }

    pub fn get(&self, name: &str) -> Option<HandlerFn> {
        self.map.get(name).copied()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn description(&self, name: &str) -> Option<&'static str> {
        self.descriptions.get(name).copied()
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.map.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// 按 handler name 升序返回 `(name, description)` 列表，前端下拉展示用。
    pub fn entries(&self) -> Vec<(&'static str, &'static str)> {
        let mut v: Vec<(&'static str, &'static str)> = self
            .map
            .keys()
            .map(|name| (*name, self.descriptions.get(name).copied().unwrap_or("")))
            .collect();
        v.sort_unstable_by_key(|(n, _)| *n);
        v
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
