//! S3 对象存储配置

use serde::Deserialize;
use summer::config::Configurable;

fn default_force_path_style() -> bool {
    true
}

fn default_bucket() -> String {
    "public".to_string()
}

fn default_max_file_size() -> u64 {
    524_288_000 // 500MB
}

fn default_presign_expiry() -> u64 {
    3600
}

fn default_multipart_threshold() -> u64 {
    10_485_760 // 10MB
}

fn default_multipart_chunk_size() -> u64 {
    10_485_760 // 10MB
}

fn default_multipart_max_age() -> u64 {
    86400 // 24 小时
}

#[derive(Debug, Configurable, Clone, Deserialize)]
#[config_prefix = "s3"]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default = "default_bucket")]
    pub bucket: String,

    // ── 连接与传输 ──
    /// 路径风格 URL（MinIO/RustFS 必须 true）
    #[serde(default = "default_force_path_style")]
    pub force_path_style: bool,
    /// 连接超时（毫秒）
    pub connect_timeout: Option<u64>,
    /// 单次操作超时（毫秒）
    pub operation_timeout: Option<u64>,
    /// 单次尝试超时（毫秒）
    pub operation_attempt_timeout: Option<u64>,

    // ── 重试策略 ──
    /// 最大重试次数（SDK 默认 3）
    pub max_attempts: Option<u32>,
    /// 重试模式："standard" / "adaptive"
    pub retry_mode: Option<String>,

    // ── 流保护 ──
    /// 是否启用停滞流检测（SDK 默认启用）
    pub stalled_stream_protection: Option<bool>,
    /// 停滞容忍时间（秒，SDK 默认 20s）
    pub stalled_stream_grace_period: Option<u64>,

    // ── 校验 ──
    /// 请求校验："when_supported" / "when_required"
    pub request_checksum: Option<String>,
    /// 响应校验："when_supported" / "when_required"
    pub response_checksum: Option<String>,

    // ── CDN / 自定义域名 ──
    /// 自定义域名（如 "https://static.oywm.top"），设置后 public_url 使用此域名替代 endpoint
    pub custom_domain: Option<String>,

    // ── 业务层 ──
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default = "default_presign_expiry")]
    pub presign_expiry: u64,
    #[serde(default = "default_multipart_threshold")]
    pub multipart_threshold: u64,
    #[serde(default = "default_multipart_chunk_size")]
    pub multipart_chunk_size: u64,
    #[serde(default)]
    pub allowed_extensions: Vec<String>,

    // ── S3 清理 ──
    /// 分片上传最大存活时间（秒），超过后被 cron 清理，默认 86400（24 小时）
    #[serde(default = "default_multipart_max_age")]
    pub multipart_max_age: u64,
}

impl S3Config {
    /// 根据 S3 key 构建公开访问 URL
    /// 优先使用 custom_domain，否则用 endpoint/bucket 拼接
    pub fn file_url(&self, file_path: &str) -> String {
        if let Some(ref domain) = self.custom_domain {
            format!("{}/{}", domain.trim_end_matches('/'), file_path)
        } else {
            format!(
                "{}/{}/{}",
                self.endpoint.trim_end_matches('/'),
                self.bucket,
                file_path
            )
        }
    }
}
