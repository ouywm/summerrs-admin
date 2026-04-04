use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::channel_model_price::{
    CreateChannelModelPriceDto, QueryChannelModelPriceDto, UpdateChannelModelPriceDto,
};
use summer_ai_model::entity::channel_model_price::{self, PriceStatus};
use summer_ai_model::entity::channel_model_price_version::{self, PriceVersionStatus};
use summer_ai_model::vo::channel_model_price::{
    ChannelModelPriceDetailVo, ChannelModelPriceVersionVo, ChannelModelPriceVo,
};

#[derive(Clone, Service)]
pub struct ChannelModelPriceService {
    #[inject(component)]
    db: DbConn,
}

impl ChannelModelPriceService {
    /// 分页查询价格列表
    pub async fn list_prices(
        &self,
        query: QueryChannelModelPriceDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ChannelModelPriceVo>> {
        let page = channel_model_price::Entity::find()
            .filter(query)
            .order_by_desc(channel_model_price::Column::UpdateTime)
            .order_by_desc(channel_model_price::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询渠道模型价格失败")?;

        Ok(page.map(ChannelModelPriceVo::from_model))
    }

    /// 获取价格详情（含版本历史）
    pub async fn get_price_detail(&self, id: i64) -> ApiResult<ChannelModelPriceDetailVo> {
        let price = channel_model_price::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询价格失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("价格记录不存在".to_string()))?;

        let versions = channel_model_price_version::Entity::find()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .order_by_desc(channel_model_price_version::Column::VersionNo)
            .all(&self.db)
            .await
            .context("查询价格版本历史失败")
            .map_err(ApiErrors::Internal)?;

        Ok(ChannelModelPriceDetailVo {
            price: ChannelModelPriceVo::from_model(price),
            versions: versions
                .into_iter()
                .map(ChannelModelPriceVersionVo::from_model)
                .collect(),
        })
    }

    /// 创建价格（同时创建首个版本记录）
    pub async fn create_price(
        &self,
        dto: CreateChannelModelPriceDto,
        operator: &str,
    ) -> ApiResult<ChannelModelPriceVo> {
        let reference_id = generate_reference_id();
        let price_config = dto.price_config.clone();

        let txn = self
            .db
            .begin()
            .await
            .context("开启事务失败")
            .map_err(ApiErrors::Internal)?;

        let price = dto
            .into_active_model(operator, reference_id.clone())
            .insert(&txn)
            .await
            .context("创建渠道模型价格失败")
            .map_err(ApiErrors::Internal)?;

        // 创建首个版本记录
        let now = chrono::Utc::now().fixed_offset();
        let version = channel_model_price_version::ActiveModel {
            channel_model_price_id: Set(price.id),
            channel_id: Set(price.channel_id),
            model_name: Set(price.model_name.clone()),
            version_no: Set(1),
            reference_id: Set(reference_id),
            price_config: Set(price_config),
            effective_start_at: Set(now),
            effective_end_at: Set(None),
            status: Set(PriceVersionStatus::Active),
            create_time: Set(now),
            ..Default::default()
        };
        version
            .insert(&txn)
            .await
            .context("创建价格版本记录失败")
            .map_err(ApiErrors::Internal)?;

        txn.commit()
            .await
            .context("提交事务失败")
            .map_err(ApiErrors::Internal)?;

        Ok(ChannelModelPriceVo::from_model(price))
    }

    /// 更新价格（自动创建新版本 + 归档旧版本）
    pub async fn update_price(
        &self,
        id: i64,
        dto: UpdateChannelModelPriceDto,
        operator: &str,
    ) -> ApiResult<ChannelModelPriceVo> {
        let txn = self
            .db
            .begin()
            .await
            .context("开启事务失败")
            .map_err(ApiErrors::Internal)?;

        let price = channel_model_price::Entity::find_by_id(id)
            .one(&txn)
            .await
            .context("查询价格失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("价格记录不存在".to_string()))?;

        let price_config_changed = dto.price_config.is_some();
        let new_price_config = dto
            .price_config
            .clone()
            .unwrap_or_else(|| price.price_config.clone());

        let mut active: channel_model_price::ActiveModel = price.clone().into();
        let new_reference_id = if price_config_changed {
            let rid = generate_reference_id();
            active.reference_id = Set(rid.clone());
            rid
        } else {
            price.reference_id.clone()
        };

        dto.apply_to(&mut active, operator);

        let updated = active
            .update(&txn)
            .await
            .context("更新价格失败")
            .map_err(ApiErrors::Internal)?;

        // 如果 price_config 变更，创建新版本
        if price_config_changed {
            let now = chrono::Utc::now().fixed_offset();

            // 归档当前生效版本
            let current_versions = channel_model_price_version::Entity::find()
                .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
                .filter(channel_model_price_version::Column::Status.eq(PriceVersionStatus::Active))
                .all(&txn)
                .await
                .context("查询当前版本失败")
                .map_err(ApiErrors::Internal)?;

            let mut max_version_no = 0_i32;
            for ver in current_versions {
                max_version_no = max_version_no.max(ver.version_no);
                let mut ver_active: channel_model_price_version::ActiveModel = ver.into();
                ver_active.status = Set(PriceVersionStatus::Archived);
                ver_active.effective_end_at = Set(Some(now));
                ver_active
                    .update(&txn)
                    .await
                    .context("归档旧版本失败")
                    .map_err(ApiErrors::Internal)?;
            }

            // 创建新版本
            let new_version = channel_model_price_version::ActiveModel {
                channel_model_price_id: Set(id),
                channel_id: Set(updated.channel_id),
                model_name: Set(updated.model_name.clone()),
                version_no: Set(max_version_no + 1),
                reference_id: Set(new_reference_id),
                price_config: Set(new_price_config),
                effective_start_at: Set(now),
                effective_end_at: Set(None),
                status: Set(PriceVersionStatus::Active),
                create_time: Set(now),
                ..Default::default()
            };
            new_version
                .insert(&txn)
                .await
                .context("创建新版本失败")
                .map_err(ApiErrors::Internal)?;
        }

        txn.commit()
            .await
            .context("提交事务失败")
            .map_err(ApiErrors::Internal)?;

        Ok(ChannelModelPriceVo::from_model(updated))
    }

    /// 删除价格（级联删除版本）
    pub async fn delete_price(&self, id: i64) -> ApiResult<()> {
        let txn = self
            .db
            .begin()
            .await
            .context("开启事务失败")
            .map_err(ApiErrors::Internal)?;

        channel_model_price_version::Entity::delete_many()
            .filter(channel_model_price_version::Column::ChannelModelPriceId.eq(id))
            .exec(&txn)
            .await
            .context("删除价格版本失败")
            .map_err(ApiErrors::Internal)?;

        channel_model_price::Entity::delete_by_id(id)
            .exec(&txn)
            .await
            .context("删除价格记录失败")
            .map_err(ApiErrors::Internal)?;

        txn.commit()
            .await
            .context("提交事务失败")
            .map_err(ApiErrors::Internal)?;

        Ok(())
    }

    /// 查询某渠道某模型的当前生效价格
    pub async fn get_effective_price(
        &self,
        channel_id: i64,
        model_name: &str,
    ) -> ApiResult<Option<channel_model_price::Model>> {
        let price = channel_model_price::Entity::find()
            .filter(channel_model_price::Column::ChannelId.eq(channel_id))
            .filter(channel_model_price::Column::ModelName.eq(model_name))
            .filter(channel_model_price::Column::Status.eq(PriceStatus::Active))
            .one(&self.db)
            .await
            .context("查询生效价格失败")
            .map_err(ApiErrors::Internal)?;
        Ok(price)
    }
}

fn generate_reference_id() -> String {
    use std::fmt::Write;
    let now = chrono::Utc::now();
    let random: u32 = rand::random();
    let mut buf = String::with_capacity(24);
    let _ = write!(buf, "PRC{}{:08X}", now.format("%Y%m%d%H%M%S"), random);
    buf
}
