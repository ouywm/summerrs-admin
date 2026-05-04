//! Rhai 脚本任务 handler —— 让用户在网页直接写脚本，不重新编译就能加任务。
//!
//! 用法（在 admin 创建任务时）：
//! - `handler` 字段填 `script::rhai`
//! - `script_engine` 字段填 `rhai`
//! - `script` 字段填脚本源码
//! - `params_json` 任意 JSON，脚本里通过 `params` 全局变量访问（自动转 rhai Dynamic）
//!
//! 脚本能用的全局：
//! - `params` —— 参数（rhai Dynamic，相当于 JSON 解析后的对象）
//! - `log_info(msg)` / `log_warn(msg)` —— 写到 tracing 输出（job_id / run_id 自动带）
//! - 标准 rhai 算术 / 字符串 / Map / Array
//!
//! 安全：rhai 默认沙盒（无文件 / 无网络 / 无 process），适合让运维写定时清理 / 报表逻辑。
//! 后续如需扩展能力（HTTP / DB），通过 `Engine::register_fn` 显式开放即可。
//!
//! 错误：编译错误 / 运行时错误 → `JobError::Failed`。
//! 超时：rhai 是同步执行，依赖 worker 的 `tokio::time::timeout` 强杀（非 cooperative）。

use rhai::{Dynamic, Engine, Scope};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::context::{JobContext, JobError, JobResult};
use crate::registry::{HandlerFuture, JobHandlerEntry};

// 注册到全局 registry —— 在 crate 内部直接 submit，不通过 `#[job_handler]` 宏
// （宏路径硬编码 `::summer_job_dynamic`，self-crate 走不通）。
inventory::submit! {
    JobHandlerEntry {
        name: "script::rhai",
        call: rhai_handler_entry,
    }
}

fn rhai_handler_entry(ctx: JobContext) -> HandlerFuture {
    Box::pin(run_rhai(ctx))
}

async fn run_rhai(ctx: JobContext) -> JobResult {
    let Some(source) = ctx.script.clone() else {
        return Err(JobError::InvalidParams(
            "Rhai 脚本任务缺失 script 字段".into(),
        ));
    };
    if source.trim().is_empty() {
        return Err(JobError::InvalidParams("Rhai 脚本不能为空".into()));
    }

    let job_id = ctx.job_id;
    let run_id = ctx.run_id;
    let trace_id = ctx.trace_id.clone();
    let params = ctx.params.clone();

    // rhai Engine 不是 Send（默认）；用 spawn_blocking 跑同步脚本，避免阻塞 tokio runtime。
    // 脚本要被 timeout / cancel 强杀只能靠 worker 外层的 tokio::time::timeout（rhai 1.x 还没有
    // cooperative cancel 钩子，这是已知限制）。
    let result =
        tokio::task::spawn_blocking(move || run_sync(source, params, job_id, run_id, trace_id))
            .await
            .map_err(|join_err| {
                JobError::Handler(anyhow::anyhow!("rhai 脚本 task panicked: {join_err}"))
            })??;
    Ok(result)
}

fn run_sync(
    source: String,
    params: Value,
    job_id: i64,
    run_id: i64,
    trace_id: String,
) -> Result<Value, JobError> {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(64, 64);
    engine.set_max_call_levels(64);
    engine.set_max_string_size(1_000_000);
    engine.set_max_array_size(10_000);
    engine.set_max_map_size(10_000);
    engine.set_max_operations(1_000_000);

    // 注册 log 函数（每次调用都附 job_id / run_id 上下文）
    let trace_for_info = trace_id.clone();
    engine.register_fn("log_info", move |msg: &str| {
        tracing::info!(job_id, run_id, trace_id = %trace_for_info, "rhai: {msg}");
    });
    let trace_for_warn = trace_id.clone();
    engine.register_fn("log_warn", move |msg: &str| {
        tracing::warn!(job_id, run_id, trace_id = %trace_for_warn, "rhai: {msg}");
    });

    let mut scope = Scope::new();
    scope.push("params", json_to_dynamic(&params));

    let value: Dynamic = engine
        .eval_with_scope(&mut scope, &source)
        .map_err(|e| JobError::Handler(anyhow::anyhow!("rhai 脚本执行失败: {e}")))?;

    Ok(dynamic_to_json(value))
}

/// 把 serde_json::Value 转成 rhai Dynamic。
fn json_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into()
            } else if let Some(f) = n.as_f64() {
                f.into()
            } else {
                Dynamic::UNIT
            }
        }
        Value::String(s) => s.clone().into(),
        Value::Array(arr) => arr
            .iter()
            .map(json_to_dynamic)
            .collect::<Vec<Dynamic>>()
            .into(),
        Value::Object(obj) => {
            let mut map = rhai::Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_dynamic(v));
            }
            map.into()
        }
    }
}

/// rhai 返回值转回 serde_json::Value。
fn dynamic_to_json(value: Dynamic) -> Value {
    if value.is_unit() {
        return Value::Null;
    }
    if let Ok(b) = value.as_bool() {
        return Value::Bool(b);
    }
    if let Ok(i) = value.as_int() {
        return Value::Number(i.into());
    }
    if let Ok(f) = value.as_float() {
        return serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if value.is_string() {
        return Value::String(value.into_string().unwrap_or_default());
    }
    if value.is_array() {
        let arr = value.into_array().unwrap_or_default();
        return Value::Array(arr.into_iter().map(dynamic_to_json).collect());
    }
    if value.is_map() {
        let map = value.cast::<rhai::Map>();
        let mut obj = serde_json::Map::with_capacity(map.len());
        for (k, v) in map {
            obj.insert(k.to_string(), dynamic_to_json(v));
        }
        return Value::Object(obj);
    }
    Value::String(format!("{value}"))
}

// ---------------------------------------------------------------------------
// dryrun（编辑器试运行）：同步执行，不写 DB，捕获 log_info / log_warn 输出
// ---------------------------------------------------------------------------

/// dryrun 结果：传给前端展示用
#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DryrunResult {
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub logs: Vec<String>,
}

const DRYRUN_DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DRYRUN_MAX_TIMEOUT_MS: u64 = 30_000;

/// 同步试运行 rhai 脚本。
///
/// - 不写任何 DB 记录
/// - 捕获 `log_info(msg)` / `log_warn(msg)` 调用到 `logs` 数组返回
/// - 超过 `timeout_ms` 后立即返回 `error="timeout"`（rhai 1.x 没 cooperative cancel，
///   只能靠总执行时间窗截断；超时时脚本仍可能在 spawn_blocking 线程上多跑一会儿）
/// - `timeout_ms` 截断到 [100, 30000]，None → 5000
pub async fn dryrun(script: String, params: Value, timeout_ms: Option<u64>) -> DryrunResult {
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DRYRUN_DEFAULT_TIMEOUT_MS)
            .clamp(100, DRYRUN_MAX_TIMEOUT_MS),
    );
    let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let logs_clone = logs.clone();

    let started = Instant::now();
    let join = tokio::task::spawn_blocking(move || run_with_capture(script, params, logs_clone));

    match tokio::time::timeout(timeout, join).await {
        Ok(Ok(Ok(value))) => DryrunResult {
            ok: true,
            result: Some(value),
            error: None,
            duration_ms: started.elapsed().as_millis() as u64,
            logs: logs.lock().map(|l| l.clone()).unwrap_or_default(),
        },
        Ok(Ok(Err(err))) => DryrunResult {
            ok: false,
            result: None,
            error: Some(err.to_string()),
            duration_ms: started.elapsed().as_millis() as u64,
            logs: logs.lock().map(|l| l.clone()).unwrap_or_default(),
        },
        Ok(Err(join_err)) => DryrunResult {
            ok: false,
            result: None,
            error: Some(format!("rhai 脚本任务异常: {join_err}")),
            duration_ms: started.elapsed().as_millis() as u64,
            logs: logs.lock().map(|l| l.clone()).unwrap_or_default(),
        },
        Err(_) => DryrunResult {
            ok: false,
            result: None,
            error: Some(format!("脚本执行超时（{} ms）", timeout.as_millis())),
            duration_ms: timeout.as_millis() as u64,
            logs: logs.lock().map(|l| l.clone()).unwrap_or_default(),
        },
    }
}

fn run_with_capture(
    source: String,
    params: Value,
    logs: Arc<Mutex<Vec<String>>>,
) -> Result<Value, JobError> {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(64, 64);
    engine.set_max_call_levels(64);
    engine.set_max_string_size(1_000_000);
    engine.set_max_array_size(10_000);
    engine.set_max_map_size(10_000);
    engine.set_max_operations(1_000_000);

    let logs_for_info = logs.clone();
    engine.register_fn("log_info", move |msg: &str| {
        if let Ok(mut buf) = logs_for_info.lock() {
            buf.push(format!("[INFO] {msg}"));
        }
    });
    let logs_for_warn = logs.clone();
    engine.register_fn("log_warn", move |msg: &str| {
        if let Ok(mut buf) = logs_for_warn.lock() {
            buf.push(format!("[WARN] {msg}"));
        }
    });

    let mut scope = Scope::new();
    scope.push("params", json_to_dynamic(&params));

    let value: Dynamic = engine
        .eval_with_scope(&mut scope, &source)
        .map_err(|e| JobError::Handler(anyhow::anyhow!("rhai 脚本执行失败: {e}")))?;

    Ok(dynamic_to_json(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rhai_addition_returns_int() {
        let v = run_sync("40 + 2".into(), Value::Null, 1, 1, "t".into()).unwrap();
        assert_eq!(v, Value::Number(42.into()));
    }

    #[test]
    fn rhai_can_read_params_object() {
        let params = serde_json::json!({"name": "world"});
        let script = r#"`hello, ${params.name}`"#;
        let v = run_sync(script.into(), params, 1, 1, "t".into()).unwrap();
        assert_eq!(v, Value::String("hello, world".into()));
    }

    #[test]
    fn rhai_returns_map_as_json_object() {
        let script = r#"#{count: 3, name: "ok"}"#;
        let v = run_sync(script.into(), Value::Null, 1, 1, "t".into()).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.get("count").and_then(|x| x.as_i64()), Some(3));
        assert_eq!(obj.get("name").and_then(|x| x.as_str()), Some("ok"));
    }

    #[test]
    fn rhai_compile_error_returns_failed() {
        let v = run_sync("let x = ;".into(), Value::Null, 1, 1, "t".into());
        assert!(v.is_err());
    }
}
