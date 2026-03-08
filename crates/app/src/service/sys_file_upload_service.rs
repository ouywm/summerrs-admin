//! 文件上传 / 下载服务

use anyhow::Context;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_smithy_types::byte_stream::ByteStream;
use bytes::Bytes;
use common::error::{ApiErrors, ApiResult};
use common::file_util;
use model::dto::sys_file::{
    MultipartAbortDto, MultipartCompleteDto, MultipartInitDto, MultipartListPartsDto,
    PresignUploadCallbackDto, PresignUploadDto,
};
use model::entity::sys_file;
use model::vo::sys_file::{
    BatchUploadVo, FileUploadVo, MultipartInitVo, MultipartListPartsVo, PartPresignedUrl,
    PresignedDownloadVo, PresignedUploadVo, UploadFailureVo, UploadedPartVo,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use std::time::Duration;
use summer::plugin::Service;
use summer_sa_token::StpUtil;

use crate::plugin::s3::config::S3Config;
use crate::plugin::sea_orm::DbConn;

/// list_parts 返回的分片信息（内部使用）
struct UploadedPart {
    part_number: i32,
    e_tag: String,
    size: i64,
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
    /// 从 JWT payload 获取操作人昵称
    async fn get_operator_name(&self, login_id: &str) -> ApiResult<String> {
        let token = StpUtil::get_token_by_login_id(login_id)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let extra = StpUtil::get_extra_data(&token)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("{e}")))?;
        let name = extra
            .and_then(|v| {
                v.get("nick_name")
                    .and_then(|n| n.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| ApiErrors::Internal(anyhow::anyhow!("无法获取操作人昵称")))?;
        Ok(name)
    }

    /// 秒传检查：MD5 + bucket 命中已有文件时，复用 S3 key 创建新 DB 记录
    async fn try_fast_upload(
        &self,
        file_md5: &str,
        original_name: &str,
        suffix: &str,
        login_id: &str,
    ) -> ApiResult<Option<FileUploadVo>> {
        let bucket_name = self.bucket();

        let existing = sys_file::Entity::find()
            .filter(sys_file::Column::FileMd5.eq(file_md5))
            .filter(sys_file::Column::Bucket.eq(bucket_name))
            .one(&self.db)
            .await
            .context("秒传查询失败")?;

        let Some(existing) = existing else {
            return Ok(None);
        };

        let operator = self.get_operator_name(login_id).await.unwrap_or_default();
        let upload_by_id: Option<i64> = login_id.parse().ok();
        let file_name = file_util::generate_file_name(suffix);

        let active = sys_file::ActiveModel {
            file_name: Set(file_name),
            original_name: Set(original_name.to_string()),
            file_path: Set(existing.file_path.clone()),
            file_size: Set(existing.file_size),
            file_suffix: Set(suffix.to_string()),
            mime_type: Set(existing.mime_type.clone()),
            bucket: Set(bucket_name.to_string()),
            file_md5: Set(file_md5.to_string()),
            upload_by: Set(operator),
            upload_by_id: Set(upload_by_id),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .context("保存秒传文件记录失败")?;

        let url = self.s3_config.file_url(&model.file_path);
        Ok(Some(FileUploadVo {
            file_id: model.id,
            original_name: model.original_name,
            url,
            file_size: model.file_size,
        }))
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

    /// 默认 bucket 名称
    fn bucket(&self) -> &str {
        &self.s3_config.bucket
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
        login_id: &str,
    ) -> ApiResult<FileUploadVo> {
        let suffix = file_util::extract_suffix(original_name);
        let file_size = data.len() as i64;

        self.validate_file(file_size, &suffix)?;

        let file_md5 = file_util::compute_md5(&data);

        // 秒传检查
        if let Some(vo) = self
            .try_fast_upload(&file_md5, original_name, &suffix, login_id)
            .await?
        {
            return Ok(vo);
        }

        let bucket_name = self.bucket();
        let object_key = file_util::generate_object_key(&suffix);
        let file_name = file_util::generate_file_name(&suffix);
        let mime = file_util::resolve_mime(content_type);

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
            self.s3
                .put_object()
                .bucket(bucket_name)
                .key(&object_key)
                .body(ByteStream::from(data))
                .content_type(mime)
                .send()
                .await
                .context("S3 上传失败")?;
        }

        let operator = self.get_operator_name(login_id).await.unwrap_or_default();
        let upload_by_id: Option<i64> = login_id.parse().ok();

        let url = self.s3_config.file_url(&object_key);
        let active = sys_file::ActiveModel {
            file_name: Set(file_name),
            original_name: Set(original_name.to_string()),
            file_path: Set(object_key),
            file_size: Set(file_size),
            file_suffix: Set(suffix),
            mime_type: Set(mime.to_string()),
            bucket: Set(bucket_name.to_string()),
            file_md5: Set(file_md5),
            upload_by: Set(operator),
            upload_by_id: Set(upload_by_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;

        Ok(FileUploadVo {
            file_id: model.id,
            original_name: model.original_name,
            url,
            file_size: model.file_size,
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
        login_id: &str,
    ) -> ApiResult<BatchUploadVo> {
        let futs: Vec<_> = files
            .into_iter()
            .map(|(original_name, content_type, data)| {
                let login_id = login_id.to_string();
                async move {
                    let result = self
                        .upload_file(&original_name, content_type.as_deref(), data, &login_id)
                        .await;
                    (original_name, result)
                }
            })
            .collect();

        let results = futures::future::join_all(futs).await;

        let mut success = Vec::new();
        let mut failed = Vec::new();
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
        login_id: &str,
    ) -> ApiResult<PresignedUploadVo> {
        let suffix = file_util::extract_suffix(&dto.file_name);
        self.validate_file(dto.file_size, &suffix)?;

        // 秒传检查（前端传入 file_md5 时触发）
        if let Some(ref file_md5) = dto.file_md5 {
            if let Some(vo) = self
                .try_fast_upload(file_md5, &dto.file_name, &suffix, login_id)
                .await?
            {
                return Ok(PresignedUploadVo {
                    fast_uploaded: true,
                    file: Some(vo),
                    upload_url: None,
                    file_path: None,
                    expires_in: None,
                });
            }
        }

        let bucket_name = self.bucket();
        let object_key = file_util::generate_object_key(&suffix);
        let content_type = file_util::resolve_mime_by_suffix(&suffix);

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
            file_path: Some(object_key),
            expires_in: Some(expiry),
        })
    }

    // ─── Presigned 上传回调 ─────────────────────────────────────────────────────

    pub async fn confirm_presigned_upload(
        &self,
        dto: PresignUploadCallbackDto,
        login_id: &str,
    ) -> ApiResult<FileUploadVo> {
        let bucket_name = self.bucket();

        let head = self
            .s3
            .head_object()
            .bucket(bucket_name)
            .key(&dto.file_path)
            .send()
            .await
            .context("文件不存在于 S3，请确认上传是否成功")?;

        let file_size = head.content_length().unwrap_or(dto.file_size);
        let suffix = file_util::extract_suffix(&dto.original_name);
        let content_type = head
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| file_util::resolve_mime_by_suffix(&suffix));

        let file_name = file_util::extract_file_name_from_path(&dto.file_path).to_string();

        let operator = self.get_operator_name(login_id).await.unwrap_or_default();
        let upload_by_id: Option<i64> = login_id.parse().ok();

        let active = sys_file::ActiveModel {
            file_name: Set(file_name),
            original_name: Set(dto.original_name.clone()),
            file_path: Set(dto.file_path.clone()),
            file_size: Set(file_size),
            file_suffix: Set(suffix),
            mime_type: Set(content_type),
            bucket: Set(bucket_name.to_string()),
            file_md5: Set(dto.file_md5.unwrap_or_default()),
            upload_by: Set(operator),
            upload_by_id: Set(upload_by_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;

        let url = self.s3_config.file_url(&dto.file_path);
        Ok(FileUploadVo {
            file_id: model.id,
            original_name: dto.original_name,
            url,
            file_size: model.file_size,
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
        let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expiry))
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Presign 配置错误: {}", e)))?;

        let presigned = self
            .s3
            .get_object()
            .bucket(&file.bucket)
            .key(&file.file_path)
            .presigned(presigning_config)
            .await
            .context("生成下载 presigned URL 失败")?;

        Ok(PresignedDownloadVo {
            download_url: presigned.uri().to_string(),
            expires_in: expiry,
        })
    }

    pub async fn download_file(&self, file_id: i64) -> ApiResult<(sys_file::Model, ByteStream)> {
        let file = sys_file::Entity::find_by_id(file_id)
            .one(&self.db)
            .await
            .context("查询文件失败")?
            .ok_or_else(|| ApiErrors::NotFound("文件不存在".to_string()))?;

        let resp = self
            .s3
            .get_object()
            .bucket(&file.bucket)
            .key(&file.file_path)
            .send()
            .await
            .context("从 S3 下载文件失败")?;

        Ok((file, resp.body))
    }

    // ─── 前端驱动分片上传 ───────────────────────────────────────────────────────

    pub async fn init_multipart_upload(
        &self,
        dto: MultipartInitDto,
        login_id: &str,
    ) -> ApiResult<MultipartInitVo> {
        let suffix = file_util::extract_suffix(&dto.file_name);
        self.validate_file(dto.file_size, &suffix)?;

        // 秒传检查
        if let Some(vo) = self
            .try_fast_upload(&dto.file_md5, &dto.file_name, &suffix, login_id)
            .await?
        {
            return Ok(MultipartInitVo {
                fast_uploaded: true,
                file: Some(vo),
                upload_id: None,
                file_path: None,
                chunk_size: None,
                total_parts: None,
                part_urls: None,
                expires_in: None,
            });
        }

        let bucket_name = self.bucket();
        let object_key = file_util::generate_object_key(&suffix);
        let content_type = file_util::resolve_mime_by_suffix(&suffix);

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
        let total_parts = ((dto.file_size as u64 + chunk_size - 1) / chunk_size) as i32;
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
            file_path: Some(object_key),
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
        let chunk_size = self.s3_config.multipart_chunk_size;
        let total_parts = ((dto.file_size as u64 + chunk_size - 1) / chunk_size) as i32;

        let parts = self
            .fetch_all_parts(bucket_name, &dto.file_path, &dto.upload_id)
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
                    .key(&dto.file_path)
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
        login_id: &str,
    ) -> ApiResult<FileUploadVo> {
        let client = &self.s3;
        let bucket_name = self.bucket();

        // 从 S3 获取已上传的分片列表
        let parts = self
            .fetch_all_parts(bucket_name, &dto.file_path, &dto.upload_id)
            .await?;

        if parts.is_empty() {
            return Err(ApiErrors::BadRequest("没有已上传的分片".to_string()));
        }

        // 校验分片完整性
        let chunk_size = self.s3_config.multipart_chunk_size;
        let expected_parts = ((dto.file_size as u64 + chunk_size - 1) / chunk_size) as i32;
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
            .key(&dto.file_path)
            .upload_id(&dto.upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .context("完成分片上传失败")?;

        let head = client
            .head_object()
            .bucket(bucket_name)
            .key(&dto.file_path)
            .send()
            .await
            .context("文件不存在于 S3，请确认上传是否成功")?;

        let file_size = head.content_length().unwrap_or_default();
        let suffix = file_util::extract_suffix(&dto.original_name);
        let content_type = head
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| file_util::resolve_mime_by_suffix(&suffix));

        let file_name = file_util::extract_file_name_from_path(&dto.file_path).to_string();

        let operator = self.get_operator_name(login_id).await.unwrap_or_default();
        let upload_by_id: Option<i64> = login_id.parse().ok();

        let active = sys_file::ActiveModel {
            file_name: Set(file_name),
            original_name: Set(dto.original_name.clone()),
            file_path: Set(dto.file_path.clone()),
            file_size: Set(file_size),
            file_suffix: Set(suffix),
            mime_type: Set(content_type),
            bucket: Set(bucket_name.to_string()),
            file_md5: Set(dto.file_md5.unwrap_or_default()),
            upload_by: Set(operator),
            upload_by_id: Set(upload_by_id),
            ..Default::default()
        };

        let model = active.insert(&self.db).await.context("保存文件记录失败")?;

        let url = self.s3_config.file_url(&dto.file_path);
        Ok(FileUploadVo {
            file_id: model.id,
            original_name: dto.original_name,
            url,
            file_size: model.file_size,
        })
    }

    pub async fn abort_multipart_upload(&self, dto: MultipartAbortDto) -> ApiResult<()> {
        self.s3
            .abort_multipart_upload()
            .bucket(self.bucket())
            .key(&dto.file_path)
            .upload_id(&dto.upload_id)
            .send()
            .await
            .context("取消分片上传失败")?;

        Ok(())
    }
}
