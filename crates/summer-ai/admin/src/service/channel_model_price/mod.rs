use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::channel_model_price::req::{
    ChannelModelPriceQuery, CreateChannelModelPriceReq, UpdateChannelModelPriceReq,
};
use crate::router::channel_model_price::res::{
    ChannelModelPriceDetailRes, ChannelModelPriceRes, ChannelModelPriceVersionRes,
};
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel_model_price::{self};
use summer_ai_model::entity::channel_model_price_version::{self, ChannelModelPriceVersionStatus};

#[derive(Clone, Service)]
pub struct ChannelModelPriceService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelModelPriceService {
    pub async fn list_prices(
        &self,
        query: ChannelModelPriceQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelModelPriceRes>> {
        let page = channel_model_price::Entity::find()
            .filter(query)
            .order_by_desc(channel_model_price::Column::UpdateTime)
            .order_by_desc(channel_model_price::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道模型价格失败")?;

        Ok(page.map(ChannelModelPriceRes::from_model))
    }

    pub async fn get_price_detail(&self, id: i64) -> ApiResult<ChannelModelPriceDetailRes> {
        let price = channel_model_price::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询渠道模型价格失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道模型价格不存在".to_string()))?;

        let versions = channel_model_price_version::Entity::find()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .order_by_desc(channel_model_price_version::Column::VersionNo)
            .all(&self.db)
            .await
            .context("查询渠道模型价格版本失败")?;

        Ok(ChannelModelPriceDetailRes {
            price: ChannelModelPriceRes::from_model(price),
            versions: versions
                .into_iter()
                .map(ChannelModelPriceVersionRes::from_model)
                .collect(),
        })
    }

    pub async fn create_price(
        &self,
        req: CreateChannelModelPriceReq,
        operator: &str,
    ) -> ApiResult<ChannelModelPriceRes> {
        self.ensure_channel_exists(req.channel_id).await?;
        let reference_id = generate_reference_id();
        let price_config = req.price_config.clone();

        let txn = self.db.begin().await.context("开启事务失败")?;
        let price = req
            .into_active_model(operator, reference_id.clone())
            .insert(&txn)
            .await
            .context("创建渠道模型价格失败")?;

        let now = chrono::Utc::now().fixed_offset();
        channel_model_price_version::ActiveModel {
            channel_model_price_id: Set(price.id),
            channel_id: Set(price.channel_id),
            model_name: Set(price.model_name.clone()),
            version_no: Set(1),
            reference_id: Set(reference_id),
            price_config: Set(price_config),
            effective_start_at: Set(now),
            effective_end_at: Set(None),
            status: Set(ChannelModelPriceVersionStatus::Effective),
            create_time: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .context("创建价格版本失败")?;

        txn.commit().await.context("提交事务失败")?;
        Ok(ChannelModelPriceRes::from_model(price))
    }

    pub async fn update_price(
        &self,
        id: i64,
        req: UpdateChannelModelPriceReq,
        operator: &str,
    ) -> ApiResult<ChannelModelPriceRes> {
        let txn = self.db.begin().await.context("开启事务失败")?;

        let price = channel_model_price::Entity::find_by_id(id)
            .one(&txn)
            .await
            .context("查询渠道模型价格失败")?
            .ok_or_else(|| ApiErrors::NotFound("渠道模型价格不存在".to_string()))?;

        let price_config_changed = req.price_config.is_some();
        let next_price_config = req
            .price_config
            .clone()
            .unwrap_or_else(|| price.price_config.clone());

        let mut active: channel_model_price::ActiveModel = price.clone().into();
        let next_reference_id = if price_config_changed {
            let reference_id = generate_reference_id();
            active.reference_id = Set(reference_id.clone());
            reference_id
        } else {
            price.reference_id.clone()
        };

        req.apply_to(&mut active, operator);
        let updated = active.update(&txn).await.context("更新渠道模型价格失败")?;

        if price_config_changed {
            let now = chrono::Utc::now().fixed_offset();
            let current_versions = channel_model_price_version::Entity::find()
                .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
                .filter(
                    channel_model_price_version::Column::Status
                        .eq(ChannelModelPriceVersionStatus::Effective),
                )
                .all(&txn)
                .await
                .context("查询价格版本失败")?;

            let mut max_version_no = 0_i32;
            for version in current_versions {
                max_version_no = max_version_no.max(version.version_no);
                let mut active_version: channel_model_price_version::ActiveModel = version.into();
                active_version.status = Set(ChannelModelPriceVersionStatus::Archived);
                active_version.effective_end_at = Set(Some(now));
                active_version
                    .update(&txn)
                    .await
                    .context("归档价格版本失败")?;
            }

            channel_model_price_version::ActiveModel {
                channel_model_price_id: Set(id),
                channel_id: Set(updated.channel_id),
                model_name: Set(updated.model_name.clone()),
                version_no: Set(max_version_no + 1),
                reference_id: Set(next_reference_id),
                price_config: Set(next_price_config),
                effective_start_at: Set(now),
                effective_end_at: Set(None),
                status: Set(ChannelModelPriceVersionStatus::Effective),
                create_time: Set(now),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .context("创建新价格版本失败")?;
        }

        txn.commit().await.context("提交事务失败")?;
        Ok(ChannelModelPriceRes::from_model(updated))
    }

    pub async fn delete_price(&self, id: i64) -> ApiResult<()> {
        let txn = self.db.begin().await.context("开启事务失败")?;

        channel_model_price_version::Entity::delete_many()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .exec(&txn)
            .await
            .context("删除价格版本失败")?;

        channel_model_price::Entity::delete_by_id(id)
            .exec(&txn)
            .await
            .context("删除渠道模型价格失败")?;

        txn.commit().await.context("提交事务失败")?;
        Ok(())
    }

    async fn ensure_channel_exists(&self, channel_id: i64) -> ApiResult<()> {
        let exists = channel::Entity::find_by_id(channel_id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询渠道失败")?
            .is_some();

        if exists {
            Ok(())
        } else {
            Err(ApiErrors::NotFound("渠道不存在".to_string()))
        }
    }
}

fn generate_reference_id() -> String {
    let now = chrono::Utc::now();
    format!(
        "cmp_{}",
        now.timestamp_nanos_opt()
            .unwrap_or_else(|| now.timestamp_micros())
    )
}
