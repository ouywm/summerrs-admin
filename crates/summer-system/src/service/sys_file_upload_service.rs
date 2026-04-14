//! 文件上传 / 下载服务

use anyhow::Context;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_smithy_types::byte_stream::ByteStream;
use bytes::Bytes;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use std::collections::HashMap;
use std::time::Duration;
use summer::plugin::Service;
use summer_auth::LoginId;
use summer_common::error::{ApiErrors, ApiResult};
use summer_common::file_util;
use summer_system_model::dto::sys_file::{
    MultipartAbortDto, MultipartCompleteDto, MultipartInitDto, MultipartListPartsDto,
    PresignUploadCallbackDto, PresignUploadDto,
};
use summer_system_model::entity::{sys_file, sys_file_folder};
use summer_system_model::vo::sys_file::{
    BatchUploadVo, FileDownloadUrlVo, FileUploadVo, MultipartInitVo, MultipartListPartsVo,
    PartPresignedUrl, PresignedDownloadVo, PresignedUploadVo, UploadFailureVo, UploadedPartVo,
};

use summer_plugins::s3::config::S3Config;
use summer_sea_orm::DbConn;

/// list_parts 返回的分片信息（内部使用）
struct UploadedPart {
    part_number: i32,
    e_tag: String,
    size: i64,
}

#[derive(Debug, Default)]
struct FolderUpsertCache {
    /// (parent_id, slug) -> folder_id
    by_parent_slug: HashMap<(i64, String), i64>,
}

#[derive(Clone, Service)]
pub struct SysFileUploadService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    s3: aws_sdk_s3::Client,
    #[inject(config)]
    s3_config: S3Config,
}

impl SysFileUploadService {
    fn sanitize_folder_segment(segment: &str) -> ApiResult<(String, String)> {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err(ApiErrors::BadRequest("目录名不能为空".to_string()));
        }
        if segment == "." {
            return Err(ApiErrors::BadRequest("目录名不允许为 '.'".to_string()));
        }
        if segment == ".." {
            return Err(ApiErrors::BadRequest("目录名不允许为 '..'".to_string()));
        }

        // Keep name as-is (trimmed), but ensure DB column limits.
        let name = segment.chars().take(128).collect::<String>();

        // Slug: deterministic + safe, but doesn't try to be fancy.
        let slug = segment
            .replace(['/', '\\'], "-")
            .trim()
            .chars()
            .take(128)
            .collect::<String>();

        if slug.is_empty() {
            return Err(ApiErrors::BadRequest("目录slug不能为空".to_string()));
        }

        Ok((name, slug))
    }

    fn extract_upload_basename(file_name: &str) -> ApiResult<String> {
        let name = file_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_name)
            .trim();
        if name.is_empty() {
            return Err(ApiErrors::BadRequest("文件名不能为空".to_string()));
        }
        if name == "." || name == ".." {
            return Err(ApiErrors::BadRequest("非法文件名".to_string()));
        }
        Ok(name.to_string())
    }

    fn parse_relative_dirs(file_name: &str) -> ApiResult<Vec<String>> {
        // Normalize Windows separators.
        let normalized = file_name.replace('\\', "/");
        let normalized = normalized.trim_matches('/');

        // Browser "fakepath" (regular <input type="file">) should never be treated as folder upload.
        if normalized.as_bytes().len() >= 11 {
            let bytes = normalized.as_bytes();
            if bytes[1] == b':'
                && bytes[2] == b'/'
                && bytes[0].is_ascii_alphabetic()
                && normalized[3..].to_ascii_lowercase().starts_with("fakepath/")
            {
                return Ok(Vec::new());
            }
        }

        let parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() <= 1 {
            return Ok(Vec::new());
        }

        let mut dirs = Vec::with_capacity(parts.len().saturating_sub(1));
        for segment in &parts[..parts.len() - 1] {
            let segment = segment.trim();
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                return Err(ApiErrors::BadRequest("不允许包含 '..' 路径段".to_string()));
            }
            dirs.push(segment.to_string());
        }
        Ok(dirs)
    }

    async fn get_folder_by_id(&self, id: i64) -> ApiResult<sys_file_folder::Model> {
        sys_file_folder::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询目标文件夹失败")?
            .ok_or_else(|| ApiErrors::NotFound("目标文件夹不存在".to_string()))
    }

    async fn get_or_create_child_folder(
        &self,
        parent_id: i64,
        segment: &str,
        visibility: &str,
        cache: &mut FolderUpsertCache,
    ) -> ApiResult<i64> {
        let (name, slug) = Self::sanitize_folder_segment(segment)?;
        if let Some(id) = cache.by_parent_slug.get(&(parent_id, slug.clone())) {
            return Ok(*id);
        }

        if let Some(model) = sys_file_folder::Entity::find()
            .filter(sys_file_folder::Column::ParentId.eq(parent_id))
            .filter(sys_file_folder::Column::Slug.eq(slug.clone()))
            .one(&self.db)
            .await
            .context("查询文件夹失败")?
        {
            cache
                .by_parent_slug
                .insert((parent_id, slug), model.id);
            return Ok(model.id);
        }

        let active = sys_file_folder::ActiveModel {
            parent_id: Set(parent_id),
            name: Set(name),
            slug: Set(slug.clone()),
            visibility: Set(visibility.to_string()),
            sort: Set(0),
            ..Default::default()
        };

        match active.insert(&self.db).await {
            Ok(model) => {
                cache
                    .by_parent_slug
                    .insert((parent_id, slug), model.id);
                Ok(model.id)
            }
            Err(err) => {
                // Most likely a concurrent insert due to the unique constraint; re-query and reuse.
                tracing::warn!(parent_id, %slug, %err, "create folder failed, retrying with select");
                let model = sys_file_folder::Entity::find()
                    .filter(sys_file_folder::Column::ParentId.eq(parent_id))
                    .filter(sys_file_folder::Column::Slug.eq(slug.clone()))
                    .one(&self.db)
                    .await
                    .context("查询文件夹失败")?
                    .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("创建文件夹失败: {}", err)))?;
                cache
                    .by_parent_slug
                    .insert((parent_id, slug), model.id);
                Ok(model.id)
            }
        }
    }

    async fn ensure_folder_path(
        &self,
        base_folder_id: Option<i64>,
        dirs: &[String],
        cache: &mut FolderUpsertCache,
    ) -> ApiResult<Option<i64>> {
        if dirs.is_empty() {
            return Ok(base_folder_id);
        }

        let (mut parent_id, visibility, base_name, base_slug) = if let Some(folder_id) =
            base_folder_id
        {
            let base = self.get_folder_by_id(folder_id).await?;
            (
                base.id,
                base.visibility.clone(),
                Some(base.name),
                Some(base.slug),
            )
        } else {
            (0_i64, "PRIVATE".to_string(), None, None)
        };

        // If the user selected a target folder on UI, the browser's `webkitRelativePath`
        // typically includes the selected directory name as the first segment. Dropping it
        // avoids creating a duplicated root folder under the selected target.
        let mut start_idx = 0usize;
        if base_folder_id.is_some() && !dirs.is_empty() {
            let first = dirs[0].as_str();
            if base_name.as_ref().is_some_and(|n| n.eq_ignore_ascii_case(first))
                || base_slug.as_ref().is_some_and(|s| s.eq_ignore_ascii_case(first))
            {
                start_idx = 1;
            }
        }

        for segment in &dirs[start_idx..] {
            parent_id = self
                .get_or_create_child_folder(parent_id, segment, visibility.as_str(), cache)
                .await?;
        }

        Ok(Some(parent_id))
    }

    /// Resolve a client-provided filename (possibly containing a relative path) into:
    /// - the original file name (basename only)
    /// - the target folder id (optionally auto-created from the relative path)
    pub async fn resolve_upload_target(
        &self,
        folder_id: Option<i64>,
        preserve_path: bool,
        client_file_name: &str,
    ) -> ApiResult<(String, Option<i64>)> {
        let folder_id = match folder_id {
            Some(0) => None,
            other => other,
        };
        let original_name = Self::extract_upload_basename(client_file_name)?;
        if !preserve_path {
            return Ok((original_name, folder_id));
        }

        let dirs = Self::parse_relative_dirs(client_file_name)?;
        let mut cache = FolderUpsertCache::default();
        let folder_id = self
            .ensure_folder_path(folder_id, &dirs, &mut cache)
            .await?;
        Ok((original_name, folder_id))
    }

    fn infer_kind(mime: &str) -> &'static str {
        if mime.starts_with("image/") {
            "IMAGE"
        } else if mime.starts_with("video/") {
            "VIDEO"
        } else if mime.starts_with("audio/") {
            "AUDIO"
        } else if mime == "application/pdf" {
            "DOCUMENT"
        } else {
            "FILE"
        }
    }

    /// 校验文件大小和后缀
    fn validate_file(&self, file_size: i64, suffix: &str) -> ApiResult<()> {
        file_util::validate_file(
            file_size,
            suffix,
            self.s3_config.max_file_size,
            &self.s3_config.allowed_extensions,
        )
    }

    /// 在文件记录里查找是否已经存在相同内容的对象（用于“对象存储层内容去重”）
    async fn find_existing_object(
        &self,
        file_md5: &str,
        file_size: i64,
        bucket: &str,
    ) -> ApiResult<Option<sys_file::Model>> {
        let file_md5 = file_md5.trim();
        if file_md5.is_empty() || file_size <= 0 {
            return Ok(None);
        }

        let model = sys_file::Entity::find()
            .filter(sys_file::Column::FileMd5.eq(file_md5))
            .filter(sys_file::Column::Size.eq(file_size))
            .filter(sys_file::Column::Provider.eq("S3"))
            .filter(sys_file::Column::Bucket.eq(bucket))
            .filter(sys_file::Column::DeletedAt.is_null())
            .order_by_desc(sys_file::Column::Id)
            .one(&self.db)
            .await
            .context("查询去重对象失败")?;

        Ok(model)
    }

    /// 默认 bucket 名称
    fn bucket(&self) -> &str {
        &self.s3_config.bucket
    }

    async fn ensure_bucket_exists(&self, bucket: &str) -> ApiResult<()> {
        let head = self.s3.head_bucket().bucket(bucket).send().await;
        if head.is_ok() {
            return Ok(());
        }

        // bucket 不存在时尝试创建；若已存在/并发创建，错误将被忽略
        let create = self.s3.create_bucket().bucket(bucket).send().await;
        if let Err(e) = create {
            tracing::warn!(bucket, %e, "create_bucket failed (may already exist)");
        }
        Ok(())
    }

    async fn presign_download_url(&self, bucket: &str, object_key: &str) -> ApiResult<FileDownloadUrlVo> {
        let expiry = self.s3_config.presign_expiry;
        let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expiry))
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Presign 配置错误: {}", e)))?;

        let presigned = self
            .s3
            .get_object()
            .bucket(bucket)
            .key(object_key)
            .presigned(presigning_config)
            .await
            .context("生成下载 presigned URL 失败")?;

        let expires_at = chrono::Local::now().naive_local() + chrono::Duration::seconds(expiry as i64);
        Ok(FileDownloadUrlVo {
            url: presigned.uri().to_string(),
            expires_at: Some(expires_at),
        })
    }

    /// 从 S3 分页获取所有已上传分片
    async fn fetch_all_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> ApiResult<Vec<UploadedPart>> {
        let mut parts = Vec::new();
        let mut part_marker: Option<String> = None;
        loop {
            let mut req = self
                .s3
                .list_parts()
                .bucket(bucket)
                .key(key)
                .upload_id(upload_id);

            if let Some(ref marker) = part_marker {
                req = req.part_number_marker(marker.as_str());
            }

            let resp = req.send().await.context("查询已上传分片失败")?;

            for part in resp.parts() {
                parts.push(UploadedPart {
                    part_number: part.part_number().unwrap_or_default(),
                    e_tag: part.e_tag().unwrap_or_default().to_string(),
                    size: part.size().unwrap_or_default(),
                });
            }

            if resp.is_truncated().unwrap_or(false) {
                part_marker = resp.next_part_number_marker().map(|s| s.to_string());
            } else {
                break;
            }
        }
        Ok(parts)
    }

    // ─── 服务端代理上传（单文件） ───────────────────────────────────────────────

    pub async fn upload_file(
        &self,
        original_name: &str,
        content_type: Option<&str>,
        data: Bytes,
        folder_id: Option<i64>,
        login_id: &LoginId,
        _operator: &str,
    ) -> ApiResult<FileUploadVo> {
        let extension = file_util::extract_suffix(original_name);
        let file_size = data.len() as i64;
        let file_md5 = file_util::compute_md5(data.as_ref());

        self.validate_file(file_size, &extension)?;

        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;
        let mime = file_util::resolve_mime(content_type);
        let kind = Self::infer_kind(mime);

        // 对象存储层内容去重：同内容（md5+size）优先复用已存在的 object_key/etag，避免存储重复对象。
        let (object_key, etag) = if let Some(existing) = self
            .find_existing_object(&file_md5, file_size, bucket_name)
            .await?
        {
            // 容错：若 DB 有记录但对象被手动删除等导致 S3 不存在，则回退到重新上传。
            let exists = self
                .s3
                .head_object()
                .bucket(bucket_name)
                .key(&existing.object_key)
                .send()
                .await
                .is_ok();

            if exists {
                (existing.object_key, existing.etag)
            } else {
                let object_key = file_util::generate_object_key(&extension);
                // 上传到 S3
                if file_size as u64 > self.s3_config.multipart_threshold {
                    self.multipart_upload(
                        bucket_name,
                        &object_key,
                        data,
                        mime,
                        self.s3_config.multipart_chunk_size,
                    )
                    .await?;
                } else {
                    let _ = self
                        .s3
                        .put_object()
                        .bucket(bucket_name)
                        .key(&object_key)
                        .body(ByteStream::from(data))
                        .content_type(mime)
                        .send()
                        .await
                        .context("S3 上传失败")?;
                }

                let head = self
                    .s3
                    .head_object()
                    .bucket(bucket_name)
                    .key(&object_key)
                    .send()
                    .await
                    .context("S3 HeadObject 失败")?;
                let etag = head.e_tag().unwrap_or_default().to_string();
                (object_key, etag)
            }
        } else {
            let object_key = file_util::generate_object_key(&extension);
            // 上传到 S3
            if file_size as u64 > self.s3_config.multipart_threshold {
                self.multipart_upload(
                    bucket_name,
                    &object_key,
                    data,
                    mime,
                    self.s3_config.multipart_chunk_size,
                )
                .await?;
            } else {
                let _ = self
                    .s3
                    .put_object()
                    .bucket(bucket_name)
                    .key(&object_key)
                    .body(ByteStream::from(data))
                    .content_type(mime)
                    .send()
                    .await
                    .context("S3 上传失败")?;
            }

            let head = self
                .s3
                .head_object()
                .bucket(bucket_name)
                .key(&object_key)
                .send()
                .await
                .context("S3 HeadObject 失败")?;
            let etag = head.e_tag().unwrap_or_default().to_string();
            (object_key, etag)
        };

        let creator_id: Option<i64> = Some(login_id.user_id);
        let file_no = file_util::generate_file_no();
        let active = sys_file::ActiveModel {
            file_no: Set(file_no),
            provider: Set("S3".to_string()),
            bucket: Set(bucket_name.to_string()),
            object_key: Set(object_key),
            etag: Set(etag),
            original_name: Set(original_name.to_string()),
            display_name: Set(original_name.to_string()),
            extension: Set(extension),
            mime_type: Set(mime.to_string()),
            kind: Set(kind.to_string()),
            size: Set(file_size),
            file_md5: Set(file_md5),
            visibility: Set("PRIVATE".to_string()),
            status: Set("NORMAL".to_string()),
            folder_id: Set(folder_id),
            creator_id: Set(creator_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;
        let download = self
            .presign_download_url(&model.bucket, &model.object_key)
            .await?;

        Ok(FileUploadVo {
            file_id: model.id,
            file_no: model.file_no,
            original_name: model.original_name,
            size: model.size,
            download,
        })
    }

    // ─── S3 分片上传（内部） ─────────────────────────────────────────────────────

    async fn multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        content_type: &str,
        chunk_size: u64,
    ) -> ApiResult<()> {
        let client = &self.s3;

        let create_resp = client
            .create_multipart_upload()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .context("创建分片上传失败")?;

        let upload_id = create_resp
            .upload_id()
            .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("未获取到 upload_id")))?
            .to_string();

        let mut completed_parts = Vec::new();
        let total_len = data.len();
        let mut offset = 0usize;
        let mut part_number = 1i32;

        let result: ApiResult<()> = async {
            while offset < total_len {
                let end = std::cmp::min(offset + chunk_size as usize, total_len);
                let chunk = data.slice(offset..end);

                let upload_resp = client
                    .upload_part()
                    .bucket(bucket)
                    .key(key)
                    .upload_id(&upload_id)
                    .part_number(part_number)
                    .body(ByteStream::from(chunk))
                    .send()
                    .await
                    .context(format!("上传分片 {} 失败", part_number))?;

                let e_tag = upload_resp.e_tag().unwrap_or_default().to_string();
                completed_parts.push(
                    CompletedPart::builder()
                        .e_tag(e_tag)
                        .part_number(part_number)
                        .build(),
                );

                offset = end;
                part_number += 1;
            }

            let completed = CompletedMultipartUpload::builder()
                .set_parts(Some(completed_parts))
                .build();

            client
                .complete_multipart_upload()
                .bucket(bucket)
                .key(key)
                .upload_id(&upload_id)
                .multipart_upload(completed)
                .send()
                .await
                .context("完成分片上传失败")?;

            Ok(())
        }
        .await;

        if result.is_err() {
            let _ = client
                .abort_multipart_upload()
                .bucket(bucket)
                .key(key)
                .upload_id(&upload_id)
                .send()
                .await;
        }

        result
    }

    // ─── 批量上传 ───────────────────────────────────────────────────────────────

    pub async fn batch_upload(
        &self,
        files: Vec<(String, Option<String>, Bytes)>,
        folder_id: Option<i64>,
        preserve_path: bool,
        login_id: &LoginId,
        operator: &str,
    ) -> ApiResult<BatchUploadVo> {
        let folder_id = match folder_id {
            Some(0) => None,
            other => other,
        };

        // 先解析路径并创建需要的文件夹（串行），避免并发创建导致大量唯一索引冲突。
        let mut cache = FolderUpsertCache::default();
        let mut prepared = Vec::new();
        let mut failed = Vec::new();

        for (client_file_name, content_type, data) in files {
            let original_name = match Self::extract_upload_basename(&client_file_name) {
                Ok(v) => v,
                Err(e) => {
                    failed.push(UploadFailureVo {
                        original_name: client_file_name,
                        reason: e.to_string(),
                    });
                    continue;
                }
            };

            let target_folder_id = if preserve_path {
                match Self::parse_relative_dirs(&client_file_name) {
                    Ok(dirs) => match self
                        .ensure_folder_path(folder_id, &dirs, &mut cache)
                        .await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            failed.push(UploadFailureVo {
                                original_name: client_file_name,
                                reason: e.to_string(),
                            });
                            continue;
                        }
                    },
                    Err(e) => {
                        failed.push(UploadFailureVo {
                            original_name: client_file_name,
                            reason: e.to_string(),
                        });
                        continue;
                    }
                }
            } else {
                folder_id
            };

            prepared.push((original_name, content_type, data, target_folder_id));
        }

        let futs: Vec<_> = prepared
            .into_iter()
            .map(|(original_name, content_type, data, target_folder_id)| {
                let login_id = *login_id;
                let operator = operator.to_string();
                async move {
                    let result = self
                        .upload_file(
                            &original_name,
                            content_type.as_deref(),
                            data,
                            target_folder_id,
                            &login_id,
                            &operator,
                        )
                        .await;
                    (original_name, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futs).await;

        let mut success = Vec::new();
        for (original_name, result) in results {
            match result {
                Ok(vo) => success.push(vo),
                Err(e) => failed.push(UploadFailureVo {
                    original_name,
                    reason: e.to_string(),
                }),
            }
        }

        Ok(BatchUploadVo { success, failed })
    }

    // ─── Presigned URL 上传 ─────────────────────────────────────────────────────

    pub async fn generate_presigned_upload(
        &self,
        dto: PresignUploadDto,
        login_id: &LoginId,
        _operator: &str,
    ) -> ApiResult<PresignedUploadVo> {
        let extension = file_util::extract_suffix(&dto.file_name);
        self.validate_file(dto.file_size, &extension)?;

        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;
        let content_type = file_util::resolve_mime_by_suffix(&extension);
        let kind = Self::infer_kind(&content_type);

        // 秒传：若前端提供了 md5，并且服务端已存在同内容对象，则跳过上传，直接复用对象并生成一条新的业务记录。
        if let Some(ref md5) = dto.file_md5 {
            if let Some(existing) = self
                .find_existing_object(md5.as_str(), dto.file_size, bucket_name)
                .await?
            {
                let creator_id: Option<i64> = Some(login_id.user_id);
                let file_no = file_util::generate_file_no();
                let active = sys_file::ActiveModel {
                    file_no: Set(file_no),
                    provider: Set(existing.provider),
                    bucket: Set(existing.bucket),
                    object_key: Set(existing.object_key),
                    etag: Set(existing.etag),
                    original_name: Set(dto.file_name.clone()),
                    display_name: Set(dto.file_name.clone()),
                    extension: Set(extension.clone()),
                    mime_type: Set(content_type.clone()),
                    kind: Set(kind.to_string()),
                    size: Set(dto.file_size),
                    file_md5: Set(md5.clone()),
                    visibility: Set("PRIVATE".to_string()),
                    status: Set("NORMAL".to_string()),
                    creator_id: Set(creator_id),
                    ..Default::default()
                };

                let model = active.insert(&self.db).await.context("保存文件记录失败")?;
                let download = self
                    .presign_download_url(&model.bucket, &model.object_key)
                    .await?;

                return Ok(PresignedUploadVo {
                    fast_uploaded: true,
                    file: Some(FileUploadVo {
                        file_id: model.id,
                        file_no: model.file_no,
                        original_name: model.original_name,
                        size: model.size,
                        download,
                    }),
                    upload_url: None,
                    object_key: None,
                    expires_in: None,
                });
            }
        }

        let object_key = file_util::generate_object_key(&extension);

        let expiry = self.s3_config.presign_expiry;
        let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expiry))
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Presign 配置错误: {}", e)))?;

        let presigned = self
            .s3
            .put_object()
            .bucket(bucket_name)
            .key(&object_key)
            .content_type(content_type)
            .presigned(presigning_config)
            .await
            .context("生成 presigned URL 失败")?;

        Ok(PresignedUploadVo {
            fast_uploaded: false,
            file: None,
            upload_url: Some(presigned.uri().to_string()),
            object_key: Some(object_key),
            expires_in: Some(expiry),
        })
    }

    // ─── Presigned 上传回调 ─────────────────────────────────────────────────────

    pub async fn confirm_presigned_upload(
        &self,
        dto: PresignUploadCallbackDto,
        login_id: &LoginId,
        _operator: &str,
    ) -> ApiResult<FileUploadVo> {
        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;

        let head = self
            .s3
            .head_object()
            .bucket(bucket_name)
            .key(&dto.object_key)
            .send()
            .await
            .context("文件不存在于 S3，请确认上传是否成功")?;

        let file_size = head.content_length().unwrap_or(dto.file_size);
        let extension = file_util::extract_suffix(&dto.original_name);
        let content_type = head
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| file_util::resolve_mime_by_suffix(&extension));
        let kind = Self::infer_kind(&content_type);
        let etag = head.e_tag().unwrap_or_default().to_string();
        let file_md5 = dto.file_md5.clone().unwrap_or_default();

        let creator_id: Option<i64> = Some(login_id.user_id);
        let file_no = file_util::generate_file_no();
        let active = sys_file::ActiveModel {
            file_no: Set(file_no),
            provider: Set("S3".to_string()),
            bucket: Set(bucket_name.to_string()),
            object_key: Set(dto.object_key.clone()),
            etag: Set(etag),
            original_name: Set(dto.original_name.clone()),
            display_name: Set(dto.original_name.clone()),
            extension: Set(extension),
            mime_type: Set(content_type),
            kind: Set(kind.to_string()),
            size: Set(file_size),
            file_md5: Set(file_md5),
            visibility: Set("PRIVATE".to_string()),
            status: Set("NORMAL".to_string()),
            creator_id: Set(creator_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;
        let download = self
            .presign_download_url(&model.bucket, &model.object_key)
            .await?;
        Ok(FileUploadVo {
            file_id: model.id,
            file_no: model.file_no,
            original_name: dto.original_name,
            size: model.size,
            download,
        })
    }

    // ─── 下载 ───────────────────────────────────────────────────────────────────

    pub async fn generate_presigned_download(
        &self,
        file_id: i64,
    ) -> ApiResult<PresignedDownloadVo> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let expiry = self.s3_config.presign_expiry;
        let download = self.presign_download_url(&file.bucket, &file.object_key).await?;

        Ok(PresignedDownloadVo {
            download,
            expires_in: expiry,
        })
    }

    pub async fn download_file(&self, file_id: i64) -> ApiResult<(sys_file::Model, ByteStream)> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }

        let resp = self
            .s3
            .get_object()
            .bucket(&file.bucket)
            .key(&file.object_key)
            .send()
            .await
            .context("从 S3 下载文件失败")?;

        Ok((file, resp.body))
    }

    /// 公开分享链接下载（无需登录）
    pub async fn download_public_file(
        &self,
        token: &str,
    ) -> ApiResult<(sys_file::Model, ByteStream)> {
        if token.trim().is_empty() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }

        let file = sys_file::Entity::find()
            .filter(sys_file::Column::PublicToken.eq(token))
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        if file.deleted_at.is_some() {
            return Err(ApiErrors::NotFound("文件不存在".to_string()));
        }
        if file.visibility != "PUBLIC" {
            return Err(ApiErrors::Forbidden("文件未公开".to_string()));
        }
        if file.status != "NORMAL" {
            return Err(ApiErrors::Forbidden("文件不可用".to_string()));
        }
        if let Some(expires_at) = file.public_url_expires_at {
            let now = chrono::Local::now().naive_local();
            if now > expires_at {
                return Err(ApiErrors::Forbidden("公开链接已过期".to_string()));
            }
        }

        let resp = self
            .s3
            .get_object()
            .bucket(&file.bucket)
            .key(&file.object_key)
            .send()
            .await
            .context("从 S3 下载文件失败")?;

        Ok((file, resp.body))
    }

    // ─── 前端驱动分片上传 ───────────────────────────────────────────────────────

    pub async fn init_multipart_upload(
        &self,
        dto: MultipartInitDto,
        login_id: &LoginId,
        _operator: &str,
    ) -> ApiResult<MultipartInitVo> {
        let extension = file_util::extract_suffix(&dto.file_name);
        self.validate_file(dto.file_size, &extension)?;

        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;
        let content_type = file_util::resolve_mime_by_suffix(&extension);
        let kind = Self::infer_kind(&content_type);

        // 秒传：服务端已存在同内容对象（md5+size）时，直接复用并返回新记录。
        if let Some(existing) = self
            .find_existing_object(&dto.file_md5, dto.file_size, bucket_name)
            .await?
        {
            let creator_id: Option<i64> = Some(login_id.user_id);
            let file_no = file_util::generate_file_no();
            let active = sys_file::ActiveModel {
                file_no: Set(file_no),
                provider: Set(existing.provider),
                bucket: Set(existing.bucket),
                object_key: Set(existing.object_key),
                etag: Set(existing.etag),
                original_name: Set(dto.file_name.clone()),
                display_name: Set(dto.file_name.clone()),
                extension: Set(extension.clone()),
                mime_type: Set(content_type.clone()),
                kind: Set(kind.to_string()),
                size: Set(dto.file_size),
                file_md5: Set(dto.file_md5.clone()),
                visibility: Set("PRIVATE".to_string()),
                status: Set("NORMAL".to_string()),
                creator_id: Set(creator_id),
                ..Default::default()
            };

            let model = active.insert(&self.db).await.context("保存文件记录失败")?;
            let download = self
                .presign_download_url(&model.bucket, &model.object_key)
                .await?;

            return Ok(MultipartInitVo {
                fast_uploaded: true,
                file: Some(FileUploadVo {
                    file_id: model.id,
                    file_no: model.file_no,
                    original_name: model.original_name,
                    size: model.size,
                    download,
                }),
                upload_id: None,
                object_key: None,
                chunk_size: None,
                total_parts: None,
                part_urls: None,
                expires_in: None,
            });
        }

        let object_key = file_util::generate_object_key(&extension);

        let create_resp = self
            .s3
            .create_multipart_upload()
            .bucket(bucket_name)
            .key(&object_key)
            .content_type(content_type)
            .send()
            .await
            .context("创建分片上传失败")?;

        let upload_id = create_resp
            .upload_id()
            .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("未获取到 upload_id")))?
            .to_string();

        let chunk_size = self.s3_config.multipart_chunk_size;
        let total_parts = (dto.file_size as u64).div_ceil(chunk_size) as i32;
        let expiry = self.s3_config.presign_expiry;

        let mut part_urls = Vec::with_capacity(total_parts as usize);
        for part_number in 1..=total_parts {
            let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expiry))
                .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Presign 配置错误: {}", e)))?;

            let presigned = self
                .s3
                .upload_part()
                .bucket(bucket_name)
                .key(&object_key)
                .upload_id(&upload_id)
                .part_number(part_number)
                .presigned(presigning_config)
                .await
                .context(format!("生成分片 {} presigned URL 失败", part_number))?;

            part_urls.push(PartPresignedUrl {
                part_number,
                upload_url: presigned.uri().to_string(),
            });
        }

        Ok(MultipartInitVo {
            fast_uploaded: false,
            file: None,
            upload_id: Some(upload_id),
            object_key: Some(object_key),
            chunk_size: Some(chunk_size),
            total_parts: Some(total_parts),
            part_urls: Some(part_urls),
            expires_in: Some(expiry),
        })
    }

    pub async fn list_uploaded_parts(
        &self,
        dto: MultipartListPartsDto,
    ) -> ApiResult<MultipartListPartsVo> {
        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;
        let chunk_size = self.s3_config.multipart_chunk_size;
        let total_parts = (dto.file_size as u64).div_ceil(chunk_size) as i32;

        let parts = self
            .fetch_all_parts(bucket_name, &dto.object_key, &dto.upload_id)
            .await?;

        let uploaded_parts: Vec<UploadedPartVo> = parts
            .iter()
            .map(|p| UploadedPartVo {
                part_number: p.part_number,
                e_tag: p.e_tag.clone(),
                size: p.size,
            })
            .collect();

        let uploaded_set: std::collections::HashSet<i32> =
            parts.iter().map(|p| p.part_number).collect();

        let expiry = self.s3_config.presign_expiry;
        let mut pending_part_urls = Vec::new();

        for part_number in 1..=total_parts {
            if !uploaded_set.contains(&part_number) {
                let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expiry))
                    .map_err(|e| {
                    ApiErrors::Internal(anyhow::anyhow!("Presign 配置错误: {}", e))
                })?;

                let presigned = self
                    .s3
                    .upload_part()
                    .bucket(bucket_name)
                    .key(&dto.object_key)
                    .upload_id(&dto.upload_id)
                    .part_number(part_number)
                    .presigned(presigning_config)
                    .await
                    .context(format!("生成分片 {} presigned URL 失败", part_number))?;

                pending_part_urls.push(PartPresignedUrl {
                    part_number,
                    upload_url: presigned.uri().to_string(),
                });
            }
        }

        Ok(MultipartListPartsVo {
            uploaded_parts,
            pending_part_urls,
            expires_in: expiry,
        })
    }

    pub async fn complete_multipart_upload(
        &self,
        dto: MultipartCompleteDto,
        login_id: &LoginId,
        _operator: &str,
    ) -> ApiResult<FileUploadVo> {
        let client = &self.s3;
        let bucket_name = self.bucket();
        self.ensure_bucket_exists(bucket_name).await?;

        // 从 S3 获取已上传的分片列表
        let parts = self
            .fetch_all_parts(bucket_name, &dto.object_key, &dto.upload_id)
            .await?;

        if parts.is_empty() {
            return Err(ApiErrors::BadRequest("没有已上传的分片".to_string()));
        }

        // 校验分片完整性
        let chunk_size = self.s3_config.multipart_chunk_size;
        let expected_parts = (dto.file_size as u64).div_ceil(chunk_size) as i32;
        let uploaded_set: std::collections::HashSet<i32> =
            parts.iter().map(|p| p.part_number).collect();

        let missing: Vec<i32> = (1..=expected_parts)
            .filter(|n| !uploaded_set.contains(n))
            .collect();

        if !missing.is_empty() {
            return Err(ApiErrors::IncompleteUpload(format!(
                "分片未上传完整，缺失 {} 个分片: {:?}",
                missing.len(),
                missing
            )));
        }

        let completed_parts: Vec<CompletedPart> = parts
            .iter()
            .map(|p| {
                CompletedPart::builder()
                    .e_tag(&p.e_tag)
                    .part_number(p.part_number)
                    .build()
            })
            .collect();

        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        client
            .complete_multipart_upload()
            .bucket(bucket_name)
            .key(&dto.object_key)
            .upload_id(&dto.upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .context("完成分片上传失败")?;

        let head = client
            .head_object()
            .bucket(bucket_name)
            .key(&dto.object_key)
            .send()
            .await
            .context("文件不存在于 S3，请确认上传是否成功")?;

        let file_size = head.content_length().unwrap_or_default();
        let extension = file_util::extract_suffix(&dto.original_name);
        let content_type = head
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| file_util::resolve_mime_by_suffix(&extension));
        let kind = Self::infer_kind(&content_type);
        let etag = head.e_tag().unwrap_or_default().to_string();
        let file_md5 = dto.file_md5.clone().unwrap_or_default();

        let creator_id: Option<i64> = Some(login_id.user_id);
        let file_no = file_util::generate_file_no();
        let active = sys_file::ActiveModel {
            file_no: Set(file_no),
            provider: Set("S3".to_string()),
            bucket: Set(bucket_name.to_string()),
            object_key: Set(dto.object_key.clone()),
            etag: Set(etag),
            original_name: Set(dto.original_name.clone()),
            display_name: Set(dto.original_name.clone()),
            extension: Set(extension),
            mime_type: Set(content_type),
            kind: Set(kind.to_string()),
            size: Set(file_size),
            file_md5: Set(file_md5),
            visibility: Set("PRIVATE".to_string()),
            status: Set("NORMAL".to_string()),
            creator_id: Set(creator_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;
        let download = self
            .presign_download_url(&model.bucket, &model.object_key)
            .await?;
        Ok(FileUploadVo {
            file_id: model.id,
            file_no: model.file_no,
            original_name: dto.original_name,
            size: model.size,
            download,
        })
    }

    pub async fn abort_multipart_upload(&self, dto: MultipartAbortDto) -> ApiResult<()> {
        self.ensure_bucket_exists(self.bucket()).await?;
        self.s3
            .abort_multipart_upload()
            .bucket(self.bucket())
            .key(&dto.object_key)
            .upload_id(&dto.upload_id)
            .send()
            .await
            .context("取消分片上传失败")?;

        Ok(())
    }
}
