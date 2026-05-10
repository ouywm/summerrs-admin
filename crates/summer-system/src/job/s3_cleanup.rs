//! S3 分片上传过期清理 —— 动态调度任务。
//!
//! 通过 `#[job_handler]` 注册到 `summer-job-dynamic` registry，DB 里 cron 表达式
//! 由 `default_dto()` 在启动期 import（已存在则保留 DB 配置）。

use std::error::Error as StdError;
use std::time::SystemTime;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::list_multipart_uploads::ListMultipartUploadsError;
use aws_sdk_s3::operation::{RequestId, RequestIdExt};
use aws_smithy_types::DateTime;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use summer_admin_macros::job_handler;
use summer_job_dynamic::dto::CreateJobDto;
use summer_job_dynamic::enums::ScheduleType;
use summer_job_dynamic::{JobContext, JobError, JobResult};
use summer_plugins::s3::S3Config;
use tracing::{error, info};

pub const HANDLER_NAME: &str = "summer_system::s3_multipart_cleanup";

/// 内置任务默认配置：每小时整点扫描清理过期分片上传。启动期 import 到 DB；
/// 已存在记录时**不**覆盖（运维改的 cron / 启停以 DB 为准）。
fn default_dto() -> CreateJobDto {
    CreateJobDto {
        name: "s3-multipart-cleanup".to_string(),
        group_name: Some("system".to_string()),
        description: Some("扫描并清理过期的 S3 分片上传碎片".to_string()),
        handler: HANDLER_NAME.to_string(),
        schedule_type: ScheduleType::Cron,
        cron_expr: Some("0 0 * * * *".to_string()),
        interval_ms: None,
        fire_time: None,
        params_json: None,
        enabled: Some(true),
        timeout_ms: Some(0),
        retry_max: Some(0),
        tenant_id: None,
    }
}

inventory::submit!(summer_job_dynamic::BuiltinJob {
    dto_factory: default_dto,
});

/// 扫描并清理过期的 S3 分片上传碎片。按 `S3Config.multipart_max_age` 判定过期，
/// 超时的 multipart upload 逐个 abort。任务执行时间与 bucket 内未完成分片数量成正比。
#[job_handler("summer_system::s3_multipart_cleanup")]
async fn s3_multipart_cleanup(ctx: JobContext) -> JobResult {
    let s3: aws_sdk_s3::Client = ctx.component();
    let config = ctx.config::<S3Config>()?;

    let now = DateTime::from(SystemTime::now());
    let cutoff = DateTime::from_secs(now.secs() - config.multipart_max_age as i64);
    let bucket = &config.bucket;

    info!("开始清理过期分片上传");

    let count = cleanup_stale_multipart_uploads(&s3, bucket, &cutoff)
        .await
        .map_err(|e| JobError::Handler(anyhow::Error::new(e)))?;

    if count > 0 {
        info!("清理了 {} 个过期分片上传", count);
    } else {
        info!("未发现需要清理的过期分片上传");
    }
    Ok(serde_json::json!({"aborted": count}))
}

async fn cleanup_stale_multipart_uploads(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    cutoff: &DateTime,
) -> Result<u32, SdkError<ListMultipartUploadsError>> {
    let mut aborted = 0u32;
    let mut key_marker: Option<String> = None;
    let mut upload_id_marker: Option<String> = None;

    loop {
        let mut req = s3.list_multipart_uploads().bucket(bucket);
        if let Some(ref km) = key_marker {
            req = req.key_marker(km);
        }
        if let Some(ref uim) = upload_id_marker {
            req = req.upload_id_marker(uim);
        }

        let resp = req.send().await?;

        for upload in resp.uploads() {
            let is_stale = upload.initiated().map(|t| t < cutoff).unwrap_or(false);

            if is_stale {
                let key = upload.key().unwrap_or_default();
                let uid = upload.upload_id().unwrap_or_default();
                match s3
                    .abort_multipart_upload()
                    .bucket(bucket)
                    .key(key)
                    .upload_id(uid)
                    .send()
                    .await
                {
                    Ok(_) => {
                        aborted += 1;
                    }
                    Err(e) => {
                        let service_error = e.as_service_error();
                        error!(
                            key,
                            upload_id = uid,
                            sdk_error = %e,
                            sdk_error_debug = ?e,
                            http_status = ?e.raw_response().map(|raw| raw.status().as_u16()),
                            aws_error_code = ?service_error.and_then(|err| err.code()),
                            aws_error_message = ?service_error.and_then(|err| err.message()),
                            request_id = ?e.request_id(),
                            extended_request_id = ?e.extended_request_id(),
                            source_error = ?service_error.and_then(|err| err.source().map(ToString::to_string)),
                            "abort 分片上传失败"
                        );
                    }
                }
            }
        }

        if resp.is_truncated() == Some(true) {
            key_marker = resp.next_key_marker().map(|s| s.to_string());
            upload_id_marker = resp.next_upload_id_marker().map(|s| s.to_string());
        } else {
            break;
        }
    }

    Ok(aborted)
}
