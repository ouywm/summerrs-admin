//! 文件处理工具函数

use bytes::Bytes;
use md5::{Digest, Md5};

use crate::error::{ApiErrors, ApiResult};

// ─── 命名策略 ────────────────────────────────────────────────────────────────

/// 文件命名策略
///
/// 控制生成 S3 object key / 文件名的组合方式。
///
/// ```
/// use summer_common::file_util::NamingStrategy;
///
/// // UUID 文件名
/// let name = NamingStrategy::Uuid.generate("jpg");
/// assert!(name.ends_with(".jpg"));
/// assert_eq!(name.len(), 40); // uuid(36) + ".jpg"(4)
///
/// // 按日期分目录
/// let key = NamingStrategy::DatePath.generate("png");
/// assert_eq!(key.split('/').count(), 4); // YYYY/MM/DD/uuid.png
///
/// // 无后缀
/// let name = NamingStrategy::Uuid.generate("");
/// assert!(!name.contains('.'));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingStrategy {
    /// `<uuid>.<suffix>` — 纯 UUID 文件名
    Uuid,
    /// `<timestamp>_<uuid>.<suffix>` — 时间戳前缀，便于按时间排序
    TimestampUuid,
    /// `YYYY/MM/DD/<uuid>.<suffix>` — 按日期分目录
    DatePath,
    /// `YYYY/MM/DD/<timestamp>_<uuid>.<suffix>` — 日期目录 + 时间戳前缀
    DatePathTimestamp,
}

impl NamingStrategy {
    /// 根据策略生成文件名 / object key
    pub fn generate(&self, suffix: &str) -> String {
        let uuid = uuid::Uuid::new_v4();
        let base = Self::join_name(&uuid, suffix);

        match self {
            NamingStrategy::Uuid => base,
            NamingStrategy::TimestampUuid => {
                let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
                format!("{}_{}", ts, base)
            }
            NamingStrategy::DatePath => {
                let date = chrono::Local::now().format("%Y/%m/%d");
                format!("{}/{}", date, base)
            }
            NamingStrategy::DatePathTimestamp => {
                let now = chrono::Local::now();
                let date = now.format("%Y/%m/%d");
                let ts = now.format("%H%M%S");
                let uuid = uuid::Uuid::new_v4();
                let base = Self::join_name(&uuid, suffix);
                format!("{}/{}_{}", date, ts, base)
            }
        }
    }

    /// 拼接 uuid 和后缀
    fn join_name(uuid: &uuid::Uuid, suffix: &str) -> String {
        if suffix.is_empty() {
            uuid.to_string()
        } else {
            format!("{}.{}", uuid, suffix)
        }
    }
}

/// 生成 S3 object key：`YYYY/MM/DD/<uuid>.<suffix>`
///
/// 等价于 `NamingStrategy::DatePath.generate(suffix)`
pub fn generate_object_key(suffix: &str) -> String {
    NamingStrategy::DatePath.generate(suffix)
}

/// 生成存储文件名：`<uuid>.<suffix>`
///
/// 等价于 `NamingStrategy::Uuid.generate(suffix)`
pub fn generate_file_name(suffix: &str) -> String {
    NamingStrategy::Uuid.generate(suffix)
}

/// 生成文件业务编号（对外稳定标识）
///
/// 格式：`F<YYYYMMDDHHMMSS>_<8位随机hex>`
pub fn generate_file_no() -> String {
    let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
    let rand = uuid::Uuid::new_v4().simple().to_string();
    format!("F{}_{}", ts, &rand[..8])
}

// ─── 文件名 / 路径工具 ──────────────────────────────────────────────────────

/// 从文件名提取后缀（小写，不含点号）
///
/// 使用 `std::path::Path::extension()` 提取，对 `.gitignore` 等隐藏文件返回空。
///
/// ```
/// assert_eq!(summer_common::file_util::extract_suffix("photo.JPG"), "jpg");
/// assert_eq!(summer_common::file_util::extract_suffix("Makefile"), "");
/// assert_eq!(summer_common::file_util::extract_suffix(".gitignore"), "");
/// ```
pub fn extract_suffix(file_name: &str) -> String {
    std::path::Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default()
}

/// 从 object key 中提取文件名部分（最后一个 `/` 之后）
pub fn extract_file_name_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// 计算数据的 MD5 摘要（32 位小写 hex）
pub fn compute_md5(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// 校验文件大小和后缀
pub fn validate_file(
    file_size: i64,
    suffix: &str,
    max_file_size: u64,
    allowed_extensions: &[String],
) -> ApiResult<()> {
    if file_size > max_file_size as i64 {
        return Err(ApiErrors::BadRequest(format!(
            "文件大小超过限制：最大 {} MB",
            max_file_size / 1024 / 1024
        )));
    }

    if !allowed_extensions.is_empty()
        && !suffix.is_empty()
        && !allowed_extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(suffix))
    {
        return Err(ApiErrors::BadRequest(format!(
            "不允许的文件类型：{}，允许的类型：{}",
            suffix,
            allowed_extensions.join(", ")
        )));
    }

    Ok(())
}

/// 解析 MIME 类型，`None` 时回退到 `application/octet-stream`
pub fn resolve_mime(content_type: Option<&str>) -> &str {
    content_type.unwrap_or(mime::APPLICATION_OCTET_STREAM.as_ref())
}

/// 根据文件后缀推断 MIME 类型
pub fn resolve_mime_by_suffix(suffix: &str) -> String {
    mime_guess::from_ext(suffix)
        .first_or_octet_stream()
        .to_string()
}

// ─── Multipart 工具函数 ─────────────────────────────────────────────────────

/// 从 multipart 字段中解析出的文件
pub struct UploadedFile {
    pub file_name: String,
    pub content_type: Option<String>,
    pub data: Bytes,
}

fn map_multipart_err(ctx: &str, e: summer_web::axum::extract::multipart::MultipartError) -> ApiErrors {
    let detail = e.body_text();
    let msg = format!("{ctx}: {detail}");

    match e.status() {
        summer_web::axum::http::StatusCode::PAYLOAD_TOO_LARGE => ApiErrors::PayloadTooLarge(msg),
        _ => ApiErrors::BadRequest(msg),
    }
}

/// 从 multipart 请求中读取所有文件字段，跳过非文件字段和空文件
pub async fn read_multipart_files(
    multipart: &mut summer_web::axum::extract::Multipart,
) -> ApiResult<Vec<UploadedFile>> {
    let mut files = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| map_multipart_err("读取上传文件失败", e))?
    {
        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };
        let content_type = field.content_type().map(|s| s.to_string());
        let data: Bytes = field
            .bytes()
            .await
            .map_err(|e| map_multipart_err("读取文件内容失败", e))?;
        if !data.is_empty() {
            files.push(UploadedFile {
                file_name,
                content_type,
                data,
            });
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── NamingStrategy ──────────────────────────────────────────────────────

    #[test]
    fn test_uuid_strategy_with_suffix() {
        let name = NamingStrategy::Uuid.generate("pdf");
        assert!(name.ends_with(".pdf"));
        assert_eq!(name.len(), 40); // uuid(36) + ".pdf"(4)
        assert!(!name.contains('/'), "Uuid 策略不含路径分隔符");
    }

    #[test]
    fn test_uuid_strategy_no_suffix() {
        let name = NamingStrategy::Uuid.generate("");
        assert_eq!(name.len(), 36);
        assert!(!name.contains('.'));
    }

    #[test]
    fn test_generate_file_no_format_and_uniqueness() {
        let a = generate_file_no();
        let b = generate_file_no();

        assert_ne!(a, b, "file_no 应该包含随机部分，避免同秒冲突");
        assert_eq!(a.len(), 24, "格式应为 F + 14位时间戳 + '_' + 8位hex: {a}");
        assert!(a.starts_with('F'));
        assert_eq!(&a[15..16], "_");
        assert!(a[1..15].chars().all(|c| c.is_ascii_digit()));
        assert!(a[16..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
    }

    #[test]
    fn test_date_path_strategy_with_suffix() {
        let key = NamingStrategy::DatePath.generate("jpg");
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 4, "DatePath 应有 4 段: {}", key);
        assert!(parts[3].ends_with(".jpg"), "应以 .jpg 结尾: {}", key);
        assert_eq!(parts[0].len(), 4, "年份 4 位");
        assert_eq!(parts[1].len(), 2, "月份 2 位");
        assert_eq!(parts[2].len(), 2, "日期 2 位");
    }

    #[test]
    fn test_date_path_strategy_no_suffix() {
        let key = NamingStrategy::DatePath.generate("");
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 4);
        assert!(!parts[3].contains('.'), "无后缀不应有点号: {}", key);
    }

    #[test]
    fn test_timestamp_uuid_strategy() {
        let name = NamingStrategy::TimestampUuid.generate("png");
        // 格式：YYYYMMDDHHmmss_uuid.png
        assert!(name.contains('_'), "应包含下划线分隔符: {}", name);
        assert!(name.ends_with(".png"));
        let prefix = name.split('_').next().unwrap();
        assert_eq!(prefix.len(), 14, "时间戳应为 14 位: {}", prefix);
    }

    #[test]
    fn test_date_path_timestamp_strategy() {
        let key = NamingStrategy::DatePathTimestamp.generate("txt");
        // 格式：YYYY/MM/DD/HHmmss_uuid.txt
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 4, "应有 4 段: {}", key);
        let file_part = parts[3];
        assert!(file_part.contains('_'), "文件名应包含下划线: {}", file_part);
        assert!(file_part.ends_with(".txt"));
        let ts = file_part.split('_').next().unwrap();
        assert_eq!(ts.len(), 6, "时间部分应为 6 位(HHmmss): {}", ts);
    }

    #[test]
    fn test_strategy_uniqueness() {
        let key1 = NamingStrategy::DatePath.generate("png");
        let key2 = NamingStrategy::DatePath.generate("png");
        assert_ne!(key1, key2, "每次生成应不同（UUID）");
    }

    // ─── 兼容函数 ───────────────────────────────────────────────────────────

    #[test]
    fn test_generate_object_key_delegates_to_date_path() {
        let key = generate_object_key("jpg");
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 4);
        assert!(parts[3].ends_with(".jpg"));
    }

    #[test]
    fn test_generate_file_name_delegates_to_uuid() {
        let name = generate_file_name("pdf");
        assert!(name.ends_with(".pdf"));
        assert!(!name.contains('/'));
    }

    // ─── extract_suffix ──────────────────────────────────────────────────────

    #[test]
    fn test_extract_suffix_normal() {
        assert_eq!(extract_suffix("photo.JPG"), "jpg");
        assert_eq!(extract_suffix("doc.pdf"), "pdf");
        assert_eq!(extract_suffix("archive.tar.gz"), "gz");
    }

    #[test]
    fn test_extract_suffix_no_ext() {
        assert_eq!(extract_suffix("Makefile"), "");
        assert_eq!(extract_suffix(""), "");
    }

    #[test]
    fn test_extract_suffix_dot_file() {
        // std::path::Path::extension() 对 .gitignore 返回 None
        assert_eq!(extract_suffix(".gitignore"), "");
    }

    #[test]
    fn test_extract_suffix_multiple_dots() {
        assert_eq!(extract_suffix("my.file.name.txt"), "txt");
    }

    // ─── extract_file_name_from_path ─────────────────────────────────────────

    #[test]
    fn test_extract_file_name_from_path() {
        assert_eq!(extract_file_name_from_path("2026/03/06/abc.jpg"), "abc.jpg");
        assert_eq!(extract_file_name_from_path("abc.jpg"), "abc.jpg");
        assert_eq!(extract_file_name_from_path("a/b/c"), "c");
    }

    // ─── compute_md5 ─────────────────────────────────────────────────────────

    #[test]
    fn test_compute_md5() {
        assert_eq!(compute_md5(b"hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn test_compute_md5_empty() {
        assert_eq!(compute_md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_compute_md5_binary() {
        let data = vec![0u8; 1024];
        let md5 = compute_md5(&data);
        assert_eq!(md5.len(), 32, "MD5 hex 应为 32 字符");
    }

    // ─── validate_file ───────────────────────────────────────────────────────

    #[test]
    fn test_validate_file_ok() {
        assert!(validate_file(1024, "jpg", 1048576, &[]).is_ok());
    }

    #[test]
    fn test_validate_file_too_large() {
        let result = validate_file(2_000_000, "jpg", 1_048_576, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_extension_allowed() {
        let exts = vec!["jpg".to_string(), "png".to_string()];
        assert!(validate_file(100, "jpg", 1048576, &exts).is_ok());
        assert!(validate_file(100, "JPG", 1048576, &exts).is_ok());
    }

    #[test]
    fn test_validate_file_extension_blocked() {
        let exts = vec!["jpg".to_string(), "png".to_string()];
        let result = validate_file(100, "exe", 1048576, &exts);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_file_no_extension_no_whitelist() {
        assert!(validate_file(100, "", 1048576, &[]).is_ok());
    }

    #[test]
    fn test_validate_file_no_suffix_with_whitelist() {
        let exts = vec!["jpg".to_string()];
        // 无后缀不校验后缀白名单
        assert!(validate_file(100, "", 1048576, &exts).is_ok());
    }

    // ─── resolve_mime ────────────────────────────────────────────────────────

    #[test]
    fn test_resolve_mime_some() {
        assert_eq!(resolve_mime(Some("image/png")), "image/png");
    }

    #[test]
    fn test_resolve_mime_none() {
        assert_eq!(resolve_mime(None), "application/octet-stream");
    }
}
