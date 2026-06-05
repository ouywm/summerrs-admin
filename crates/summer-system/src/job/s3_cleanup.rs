//! S3 分片上传过期清理 —— xxl-job 执行器 handler。
//!
//! 通过 [`crate::job::register_xxl_handlers`] 注册到 summer-xxl-job，由 ratch-job /
//! xxl-job-admin 远程下发调度。调度配置（原 cron `0 0 * * * *`，每小时整点）现在
//! 在 admin 控制台维护，绑定 handler 名 [`HANDLER_NAME`]。

use std::error::Error as StdError;
use std::time::SystemTime;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::list_multipart_uploads::ListMultipartUploadsError;
use aws_sdk_s3::operation::{RequestId, RequestIdExt};
use aws_smithy_types::DateTime;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use summer::async_trait;
use summer::plugin::service::Service;
use summer_plugins::s3::S3Config;
use summer_xxl_job::{AsyncJobHandler, JobContext};
use tracing::{error, info};

/// admin 侧任务绑定的 handler 名。
pub const HANDLER_NAME: &str = "summer_system::s3_multipart_cleanup";

/// 扫描并清理过期的 S3 分片上传碎片。按 `S3Config.multipart_max_age` 判定过期，
/// 超时的 multipart upload 逐个 abort。任务执行时间与 bucket 内未完成分片数量成正比。
#[derive(Clone, Service)]
pub struct S3MultipartCleanupHandler {
    #[inject(component)]
    s3: aws_sdk_s3::Client,
    #[inject(config)]
    config: S3Config,
}

#[async_trait]
impl AsyncJobHandler for S3MultipartCleanupHandler {
    async fn process(&self, ctx: JobContext) -> anyhow::Result<JobContext> {
        let now = DateTime::from(SystemTime::now());
        let cutoff = DateTime::from_secs(now.secs() - self.config.multipart_max_age as i64);
        let bucket = &self.config.bucket;

        info!("开始清理过期分片上传");

        let count = cleanup_stale_multipart_uploads(&self.s3, bucket, &cutoff)
            .await
            .map_err(anyhow::Error::new)?;

        if count > 0 {
            info!("清理了 {} 个过期分片上传", count);
        } else {
            info!("未发现需要清理的过期分片上传");
        }
        Ok(ctx)
    }
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
