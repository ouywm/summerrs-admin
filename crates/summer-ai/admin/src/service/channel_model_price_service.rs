use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use summer::plugin::Service;
use summer_ai_model::dto::channel_model_price::{
    ChannelModelPriceQueryDto, CreateChannelModelPriceDto, PriceMutationFingerprint,
    UpdateChannelModelPriceDto,
};
use summer_ai_model::entity::routing::channel;
use summer_ai_model::entity::routing::channel_model_price::{self};
use summer_ai_model::entity::routing::channel_model_price_version::{
    self, ChannelModelPriceVersionStatus,
};
use summer_ai_model::vo::channel_model_price::{
    ChannelModelPriceDetailVo, ChannelModelPriceVersionVo, ChannelModelPriceVo,
};
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use uuid::Uuid;

#[derive(Clone, Service)]
pub struct ChannelModelPriceService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelModelPriceService {
    pub async fn create(&self, dto: CreateChannelModelPriceDto, operator: &str) -> ApiResult<()> {
        dto.validate_runtime_compatibility()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_channel_exists(dto.channel_id).await?;
        self.ensure_unique_price(dto.channel_id, &dto.model_name, None)
            .await?;

        let reference_id = new_reference_id();
        let operator = operator.to_string();
        let now = chrono::Utc::now().fixed_offset();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                let reference_id = reference_id.clone();
                Box::pin(async move {
                    let model = dto
                        .into_active_model(&operator, reference_id.clone())
                        .insert(txn)
                        .await
                        .context("创建渠道模型价格失败")
                        .map_err(ApiErrors::Internal)?;

                    insert_version_snapshot(txn, &model, 1, reference_id, now).await?;

                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateChannelModelPriceDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        let fingerprint = PriceMutationFingerprint::from_model(&model);
        let rotate_version = dto.touches_price_fields(&fingerprint);

        if rotate_version {
            dto.validate_runtime_compatibility(&fingerprint)
                .map_err(ApiErrors::BadRequest)?;
        }

        let operator = operator.to_string();
        let now = chrono::Utc::now().fixed_offset();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.clone();
                let model = model.clone();
                Box::pin(async move {
                    let next_reference_id = if rotate_version {
                        Some(new_reference_id())
                    } else {
                        None
                    };

                    let next_version = if rotate_version {
                        let current_version_nos = channel_model_price_version::Entity::find()
                            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
                            .all(txn)
                            .await
                            .context("查询价格版本失败")
                            .map_err(ApiErrors::Internal)?
                            .into_iter()
                            .map(|row| row.version_no)
                            .collect::<Vec<_>>();
                        Some(next_version_no(&current_version_nos))
                    } else {
                        None
                    };

                    if rotate_version {
                        channel_model_price_version::Entity::update_many()
                            .col_expr(
                                channel_model_price_version::Column::Status,
                                sea_orm::sea_query::Expr::value(
                                    ChannelModelPriceVersionStatus::Archived,
                                ),
                            )
                            .col_expr(
                                channel_model_price_version::Column::EffectiveEndAt,
                                sea_orm::sea_query::Expr::value(Some(now)),
                            )
                            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
                            .filter(
                                channel_model_price_version::Column::Status
                                    .eq(ChannelModelPriceVersionStatus::Effective),
                            )
                            .exec(txn)
                            .await
                            .context("归档旧价格版本失败")
                            .map_err(ApiErrors::Internal)?;
                    }

                    let mut active: channel_model_price::ActiveModel = model.into();
                    dto.apply_to(&mut active, &operator, next_reference_id.clone());
                    let updated = active
                        .update(txn)
                        .await
                        .context("更新渠道模型价格失败")
                        .map_err(ApiErrors::Internal)?;

                    if let (Some(version_no), Some(reference_id)) =
                        (next_version, next_reference_id)
                    {
                        insert_version_snapshot(txn, &updated, version_no, reference_id, now)
                            .await?;
                    }

                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _model = self.find_model_by_id(id).await?;
        let now = chrono::Utc::now().fixed_offset();

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                Box::pin(async move {
                    channel_model_price_version::Entity::update_many()
                        .col_expr(
                            channel_model_price_version::Column::Status,
                            sea_orm::sea_query::Expr::value(
                                ChannelModelPriceVersionStatus::Archived,
                            ),
                        )
                        .col_expr(
                            channel_model_price_version::Column::EffectiveEndAt,
                            sea_orm::sea_query::Expr::value(Some(now)),
                        )
                        .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
                        .filter(
                            channel_model_price_version::Column::Status
                                .eq(ChannelModelPriceVersionStatus::Effective),
                        )
                        .exec(txn)
                        .await
                        .context("归档价格版本失败")
                        .map_err(ApiErrors::Internal)?;

                    channel_model_price::Entity::delete_by_id(id)
                        .exec(txn)
                        .await
                        .context("删除渠道模型价格失败")
                        .map_err(ApiErrors::Internal)?;

                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    pub async fn list(
        &self,
        query: ChannelModelPriceQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelModelPriceVo>> {
        let page: Page<channel_model_price::Model> = channel_model_price::Entity::find()
            .filter(query)
            .order_by_asc(channel_model_price::Column::ChannelId)
            .order_by_asc(channel_model_price::Column::ModelName)
            .order_by_desc(channel_model_price::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道模型价格列表失败")?;

        Ok(page.map(ChannelModelPriceVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<ChannelModelPriceDetailVo> {
        let model = self.find_model_by_id(id).await?;
        let latest_version = channel_model_price_version::Entity::find()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .order_by_desc(channel_model_price_version::Column::VersionNo)
            .one(&self.db)
            .await
            .context("查询价格版本失败")?;

        Ok(ChannelModelPriceDetailVo {
            base: ChannelModelPriceVo::from_model(model),
            current_version_no: latest_version.map(|version| version.version_no),
        })
    }

    pub async fn list_versions(&self, id: i64) -> ApiResult<Vec<ChannelModelPriceVersionVo>> {
        let _model = self.find_model_by_id(id).await?;
        let versions = channel_model_price_version::Entity::find()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .order_by_desc(channel_model_price_version::Column::VersionNo)
            .all(&self.db)
            .await
            .context("查询价格版本列表失败")?;

        Ok(versions
            .into_iter()
            .map(ChannelModelPriceVersionVo::from_model)
            .collect())
    }

    async fn ensure_channel_exists(&self, channel_id: i64) -> ApiResult<()> {
        let exists = channel::Entity::find_by_id(channel_id)
            .one(&self.db)
            .await
            .context("查询渠道失败")?;

        if exists.is_none() {
            return Err(ApiErrors::NotFound(format!("渠道不存在: id={channel_id}")));
        }
        Ok(())
    }

    async fn ensure_unique_price(
        &self,
        channel_id: i64,
        model_name: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = channel_model_price::Entity::find()
            .filter(channel_model_price::Column::ChannelId.eq(channel_id))
            .filter(channel_model_price::Column::ModelName.eq(model_name));

        if let Some(exclude_id) = exclude_id {
            query = query.filter(channel_model_price::Column::Id.ne(exclude_id));
        }

        let exists = query
            .one(&self.db)
            .await
            .context("检查渠道模型价格唯一性失败")?;

        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "渠道模型价格已存在: channel_id={channel_id}, model_name={model_name}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<channel_model_price::Model> {
        channel_model_price::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询渠道模型价格详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("渠道模型价格不存在: id={id}")))
    }
}

fn new_reference_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn next_version_no(existing: &[i32]) -> i32 {
    existing.iter().copied().max().unwrap_or(0) + 1
}

async fn insert_version_snapshot(
    txn: &sea_orm::DatabaseTransaction,
    model: &channel_model_price::Model,
    version_no: i32,
    reference_id: String,
    effective_start_at: chrono::DateTime<chrono::FixedOffset>,
) -> ApiResult<()> {
    channel_model_price_version::ActiveModel {
        channel_model_price_id: Set(model.id),
        channel_id: Set(model.channel_id),
        model_name: Set(model.model_name.clone()),
        version_no: Set(version_no),
        reference_id: Set(reference_id),
        price_config: Set(model.price_config.clone()),
        effective_start_at: Set(effective_start_at),
        effective_end_at: Set(None),
        status: Set(ChannelModelPriceVersionStatus::Effective),
        ..Default::default()
    }
    .insert(txn)
    .await
    .context("创建价格版本失败")
    .map_err(ApiErrors::Internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_version_no_starts_from_one() {
        assert_eq!(next_version_no(&[]), 1);
    }

    #[test]
    fn next_version_no_increments_max_version() {
        assert_eq!(next_version_no(&[1, 2, 4]), 5);
    }

    #[test]
    fn new_reference_id_uses_fixed_length_uuid_without_dashes() {
        let value = new_reference_id();
        assert_eq!(value.len(), 32);
        assert!(value.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
