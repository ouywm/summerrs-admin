//! S3 分片上传过期清理

use std::time::SystemTime;

use aws_smithy_types::DateTime;
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

    match cleanup_stale_multipart_uploads(&s3, bucket, &cutoff).await {
        Ok(count) if count > 0 => {
            info!("清理了 {} 个过期分片上传", count);
        }
        Err(e) => {
            error!(%e, "分片上传清理任务失败");
        }
        _ => {}
    }
}

/// 扫描并清理过期的分片上传
async fn cleanup_stale_multipart_uploads(
    s3: &aws_sdk_s3::Client,
    bucket: &str,
    cutoff: &DateTime,
) -> Result<
    u32,
    aws_sdk_s3::error::SdkError<
        aws_sdk_s3::operation::list_multipart_uploads::ListMultipartUploadsError,
    >,
> {
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
                        error!(key, upload_id = uid, %e, "abort 分片上传失败");
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
