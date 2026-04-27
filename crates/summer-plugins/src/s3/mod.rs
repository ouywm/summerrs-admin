//! S3 对象存储插件：基于 aws-sdk-s3，兼容 RustFS / MinIO

pub mod config;

pub use config::S3Config;

use aws_credential_types::Credentials;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use std::time::Duration;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};

pub struct S3Plugin;

#[async_trait]
impl Plugin for S3Plugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app.get_config::<S3Config>().expect("S3 插件配置加载失败");

        // 1. 构建 AWS credentials
        let credentials =
            Credentials::new(&config.access_key, &config.secret_key, None, None, "rustfs");

        // 2. 构建 AWS SDK config（指定自定义 endpoint）
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .endpoint_url(&config.endpoint);

        // 重试策略
        if config.retry_mode.as_deref() == Some("adaptive") {
            let mut retry = RetryConfig::adaptive();
            if let Some(max) = config.max_attempts {
                retry = retry.with_max_attempts(max);
            }
            loader = loader.retry_config(retry);
        } else if let Some(max) = config.max_attempts {
            loader = loader.retry_config(RetryConfig::standard().with_max_attempts(max));
        }

        // 超时配置
        if config.connect_timeout.is_some()
            || config.operation_timeout.is_some()
            || config.operation_attempt_timeout.is_some()
        {
            let mut tb = TimeoutConfig::builder();
            if let Some(ms) = config.connect_timeout {
                tb = tb.connect_timeout(Duration::from_millis(ms));
            }
            if let Some(ms) = config.operation_timeout {
                tb = tb.operation_timeout(Duration::from_millis(ms));
            }
            if let Some(ms) = config.operation_attempt_timeout {
                tb = tb.operation_attempt_timeout(Duration::from_millis(ms));
            }
            loader = loader.timeout_config(tb.build());
        }

        // 停滞流保护
        if let Some(enabled) = config.stalled_stream_protection {
            use aws_config::stalled_stream_protection::StalledStreamProtectionConfig;
            let ssp = if enabled {
                let mut builder = StalledStreamProtectionConfig::enabled();
                if let Some(secs) = config.stalled_stream_grace_period {
                    builder = builder.grace_period(Duration::from_secs(secs));
                }
                builder.build()
            } else {
                StalledStreamProtectionConfig::disabled()
            };
            loader = loader.stalled_stream_protection(ssp);
        }

        let sdk_config = loader.load().await;

        // 3. 构建 S3 Client（应用 force_path_style 和 checksum 配置）
        let mut s3_builder = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(config.force_path_style);

        if let Some(ref mode) = config.request_checksum {
            use aws_smithy_types::checksum_config::RequestChecksumCalculation;
            s3_builder = s3_builder.request_checksum_calculation(match mode.as_str() {
                "when_required" => RequestChecksumCalculation::WhenRequired,
                _ => RequestChecksumCalculation::WhenSupported,
            });
        }
        if let Some(ref mode) = config.response_checksum {
            use aws_smithy_types::checksum_config::ResponseChecksumValidation;
            s3_builder = s3_builder.response_checksum_validation(match mode.as_str() {
                "when_required" => ResponseChecksumValidation::WhenRequired,
                _ => ResponseChecksumValidation::WhenSupported,
            });
        }

        let raw_client = aws_sdk_s3::Client::from_conf(s3_builder.build());

        tracing::info!(
            "S3Plugin 初始化完成，endpoint: {}, bucket: {}",
            config.endpoint,
            config.bucket,
        );

        // 4. 注册组件：aws_sdk_s3::Client
        app.add_component(raw_client);
    }

    fn name(&self) -> &str {
        "s3"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::presigning::PresigningConfig;
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
    use aws_smithy_types::byte_stream::ByteStream;
    use bytes::Bytes;
    use std::time::Duration;
    use summer::App;
    use summer::plugin::ComponentRegistry;

    /// S3 测试配置（从开发配置文件读取）
    const S3_TEST_CONFIG: &str = include_str!("../../../../config/app-dev.toml");

    /// 每个测试独立构建 S3 客户端，避免跨 tokio runtime 共享连接池导致 DispatchGone
    async fn s3() -> (aws_sdk_s3::Client, S3Config) {
        let mut builder = App::new();
        builder.use_config_str(S3_TEST_CONFIG);
        S3Plugin.build(&mut builder).await;
        let client: aws_sdk_s3::Client = builder.get_expect_component();
        let config: S3Config = builder.get_config::<S3Config>().unwrap();
        (client, config)
    }

    /// 生成唯一测试 key，统一前缀方便清理
    fn test_key(prefix: &str, ext: &str) -> String {
        format!("__test__/{}-{}.{}", prefix, uuid::Uuid::new_v4(), ext)
    }

    // ─── 连通性测试 ──────────────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_s3_list_buckets() {
        let (s3, _) = s3().await;
        let resp = s3
            .list_buckets()
            .send()
            .await
            .expect("list_buckets 失败，请确认 RustFS 已启动");

        let names: Vec<&str> = resp.buckets().iter().filter_map(|b| b.name()).collect();
        println!("可用 buckets: {:?}", names);
        assert!(!names.is_empty(), "至少应有一个 bucket");
    }

    // ─── 单文件 Put / Head / Get / Delete ────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_put_head_get_delete() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let key = test_key("simple", "txt");
        let body = b"hello RustFS from integration test!";

        // PUT
        s3.put_object()
            .bucket(bucket)
            .key(&key)
            .body(ByteStream::from(Bytes::from_static(body)))
            .content_type("text/plain")
            .send()
            .await
            .expect("put_object 失败");

        // HEAD
        let head = s3
            .head_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("head_object 失败");
        assert_eq!(head.content_length().unwrap(), body.len() as i64);

        // GET
        let get_resp = s3
            .get_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("get_object 失败");
        let data = get_resp.body.collect().await.expect("读取 body 失败");
        assert_eq!(data.into_bytes().as_ref(), body);

        // DELETE
        s3.delete_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("delete_object 失败");

        // 验证已删除
        let gone = s3.head_object().bucket(bucket).key(&key).send().await;
        assert!(gone.is_err(), "删除后 head_object 应失败");
    }

    // ─── 分片上传 (Multipart Upload) ─────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_multipart_upload() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let key = test_key("multipart", "bin");
        let chunk_size: usize = 5 * 1024 * 1024; // S3 最小分片 5MB
        let total_parts: i32 = 3;

        // 1. 创建 multipart upload
        let create_resp = s3
            .create_multipart_upload()
            .bucket(bucket)
            .key(&key)
            .content_type(mime::APPLICATION_OCTET_STREAM.as_ref())
            .send()
            .await
            .expect("create_multipart_upload 失败");
        let upload_id = create_resp
            .upload_id()
            .expect("未获取到 upload_id")
            .to_string();

        // 2. 上传分片
        let mut completed_parts = Vec::new();
        for i in 1..=total_parts {
            let data = vec![i as u8; chunk_size];
            let part_resp = s3
                .upload_part()
                .bucket(bucket)
                .key(&key)
                .upload_id(&upload_id)
                .part_number(i)
                .body(ByteStream::from(Bytes::from(data)))
                .send()
                .await
                .unwrap_or_else(|e| panic!("上传分片 {} 失败: {}", i, e));

            completed_parts.push(
                CompletedPart::builder()
                    .e_tag(part_resp.e_tag().unwrap_or_default())
                    .part_number(i)
                    .build(),
            );
        }

        // 3. 完成
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();
        s3.complete_multipart_upload()
            .bucket(bucket)
            .key(&key)
            .upload_id(&upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .expect("complete_multipart_upload 失败");

        // 验证大小
        let head = s3
            .head_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("分片上传后 head_object 失败");
        let expected = (chunk_size * total_parts as usize) as i64;
        assert_eq!(head.content_length().unwrap(), expected);
        println!(
            "分片上传成功: {} 片 × 5MB = {}MB",
            total_parts,
            expected / 1024 / 1024
        );

        // 清理
        s3.delete_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("清理失败");
    }

    // ─── Presigned URL 生成 + 验证 ───────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_presigned_put_and_get() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let key = test_key("presign", "txt");
        let content = "presigned URL upload test";

        // 生成 presigned PUT URL
        let presign_cfg =
            PresigningConfig::expires_in(Duration::from_secs(300)).expect("presign config");
        let presigned_put = s3
            .put_object()
            .bucket(bucket)
            .key(&key)
            .content_type("text/plain")
            .presigned(presign_cfg)
            .await
            .expect("生成 presigned PUT URL 失败");

        let put_url = presigned_put.uri().to_string();
        println!("Presigned PUT URL: {}", put_url);
        assert!(put_url.contains("X-Amz-Signature"), "URL 应包含签名参数");

        // 通过 presigned URL 上传（模拟前端直传）
        let http = reqwest::Client::new();
        let resp = http
            .put(&put_url)
            .header("Content-Type", "text/plain")
            .body(content.to_string())
            .send()
            .await
            .expect("presigned PUT 请求失败");
        assert!(
            resp.status().is_success(),
            "presigned PUT 应成功, 实际: {}",
            resp.status()
        );

        // 生成 presigned GET URL
        let presign_cfg =
            PresigningConfig::expires_in(Duration::from_secs(300)).expect("presign config");
        let presigned_get = s3
            .get_object()
            .bucket(bucket)
            .key(&key)
            .presigned(presign_cfg)
            .await
            .expect("生成 presigned GET URL 失败");

        // 通过 presigned URL 下载验证
        let resp = http
            .get(presigned_get.uri())
            .send()
            .await
            .expect("presigned GET 请求失败");
        assert!(resp.status().is_success());
        let text = resp.text().await.expect("读取响应体失败");
        assert_eq!(text, content, "下载内容应与上传一致");

        // 清理
        s3.delete_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("清理失败");
    }

    // ─── 覆盖上传 ───────────────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_overwrite_object() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let key = test_key("overwrite", "txt");

        // 第一次上传
        s3.put_object()
            .bucket(bucket)
            .key(&key)
            .body(ByteStream::from(Bytes::from_static(b"version 1")))
            .send()
            .await
            .expect("第一次上传失败");

        // 覆盖
        s3.put_object()
            .bucket(bucket)
            .key(&key)
            .body(ByteStream::from(Bytes::from_static(b"version 2")))
            .send()
            .await
            .expect("覆盖上传失败");

        // 验证内容是 v2
        let resp = s3
            .get_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("get_object 失败");
        let data = resp.body.collect().await.expect("读取 body 失败");
        assert_eq!(data.into_bytes().as_ref(), b"version 2");

        // 清理
        s3.delete_object()
            .bucket(bucket)
            .key(&key)
            .send()
            .await
            .expect("清理失败");
    }

    // ─── S3 删除幂等性 ──────────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_delete_nonexistent_is_idempotent() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let key = test_key("nonexistent", "txt");

        let result = s3.delete_object().bucket(bucket).key(&key).send().await;
        assert!(result.is_ok(), "删除不存在的对象应成功（S3 幂等行为）");
    }

    // ─── 上传本地文件（不删除，用于面板验证） ──────────────────────────────

    #[tokio::test]
    #[ignore = "requires S3_ACCESS_KEY, S3_SECRET_KEY, and a local S3-compatible service"]
    async fn test_upload_main_rs_no_cleanup() {
        let (s3, config) = s3().await;
        let bucket = &config.bucket;
        let data = include_bytes!("../../../../crates/app/src/main.rs");

        s3.put_object()
            .bucket(bucket)
            .key("__test__/main.rs")
            .body(ByteStream::from(Bytes::from_static(data)))
            .content_type("text/x-rust; charset=utf-8")
            .send()
            .await
            .expect("上传 main.rs 失败");

        let head = s3
            .head_object()
            .bucket(bucket)
            .key("__test__/main.rs")
            .send()
            .await
            .expect("head_object 失败");

        assert_eq!(head.content_length().unwrap(), data.len() as i64);
        println!(
            "main.rs 已上传到 {}/{}/__test__/main.rs，大小 {} 字节，请在 RustFS 面板查看",
            config.endpoint,
            bucket,
            data.len()
        );
    }
}
