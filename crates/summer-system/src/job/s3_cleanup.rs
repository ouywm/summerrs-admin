//! S3 分片上传过期清理

use std::{error::Error as StdError, time::SystemTime};

use aws_sdk_s3::{
    error::SdkError,
    operation::{RequestId, RequestIdExt, list_multipart_uploads::ListMultipartUploadsError},
};
use aws_smithy_types::{DateTime, error::metadata::ProvideErrorMetadata};
use summer::extractor::{Component, Config};
use summer_job::cron;
use tracing::{error, info};

use summer_plugins::s3::S3Config;

/// 每小时整点执行：扫描并清理过期的分片上传碎片
#[cron("0 0 * * * *")]
async fn s3_multipart_cleanup(
    Component(s3): Component<aws_sdk_s3::Client>,
    Config(config): Config<S3Config>,
) {
    let now = DateTime::from(SystemTime::now());
    let cutoff = DateTime::from_secs(now.secs() - config.multipart_max_age as i64);
    let bucket = &config.bucket;

    info!("开始清理过期分片上传");

    match cleanup_stale_multipart_uploads(&s3, bucket, &cutoff).await {
        Ok(count) if count > 0 => {
            info!("清理了 {} 个过期分片上传", count);
        }
        Ok(_) => {
            info!("未发现需要清理的过期分片上传");
        }
        Err(e) => {
            let service_error = e.as_service_error();
            error!(
                sdk_error = %e,
                sdk_error_debug = ?e,
                http_status = ?e.raw_response().map(|raw| raw.status().as_u16()),
                aws_error_code = ?service_error.and_then(|err| err.code()),
                aws_error_message = ?service_error.and_then(|err| err.message()),
                request_id = ?e.request_id(),
                extended_request_id = ?e.extended_request_id(),
                source_error = ?service_error.and_then(|err| err.source().map(ToString::to_string)),
                "分片上传清理任务失败"
            );
        }
    }
}

/// 扫描并清理过期的分片上传
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
