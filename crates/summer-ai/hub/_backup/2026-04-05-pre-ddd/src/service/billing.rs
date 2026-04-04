use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::billing::*;
use summer_ai_model::entity::{order, redemption, subscription_plan, topup};
use summer_ai_model::vo::billing::*;

#[derive(Clone, Service)]
pub struct BillingService {
    #[inject(component)]
    db: DbConn,
}

impl BillingService {
    // ─── SubscriptionPlan ───
    pub async fn list_plans(
        &self,
        query: QuerySubscriptionPlanDto,
        pagination: Pagination,
    ) -> ApiResult<Page<SubscriptionPlanVo>> {
        let page = subscription_plan::Entity::find()
            .filter(query)
            .order_by_asc(subscription_plan::Column::PlanSort)
            .page(&self.db, &pagination)
            .await
            .context("查询套餐列表失败")?;
        Ok(page.map(SubscriptionPlanVo::from_model))
    }
    pub async fn get_plan(&self, id: i64) -> ApiResult<SubscriptionPlanVo> {
        let m = subscription_plan::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询套餐失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("套餐不存在".to_string()))?;
        Ok(SubscriptionPlanVo::from_model(m))
    }
    pub async fn create_plan(
        &self,
        dto: CreateSubscriptionPlanDto,
        operator: &str,
    ) -> ApiResult<SubscriptionPlanVo> {
        let now = chrono::Utc::now().fixed_offset();
        let a = subscription_plan::ActiveModel {
            plan_code: Set(dto.plan_code),
            plan_name: Set(dto.plan_name),
            description: Set(dto.description),
            status: Set(1),
            billing_cycle: Set(dto.billing_cycle),
            currency: Set(dto.currency),
            price: Set(sea_orm::prelude::Decimal::try_from(dto.price).unwrap_or_default()),
            quota: Set(dto.quota),
            features: Set(dto.features),
            limits: Set(dto.limits),
            plan_sort: Set(0),
            create_by: Set(operator.into()),
            update_by: Set(operator.into()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let m = a
            .insert(&self.db)
            .await
            .context("创建套餐失败")
            .map_err(ApiErrors::Internal)?;
        Ok(SubscriptionPlanVo::from_model(m))
    }
    pub async fn update_plan(
        &self,
        id: i64,
        dto: UpdateSubscriptionPlanDto,
        operator: &str,
    ) -> ApiResult<SubscriptionPlanVo> {
        let m = subscription_plan::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("套餐不存在".to_string()))?;
        let mut a: subscription_plan::ActiveModel = m.into();
        if let Some(v) = dto.plan_name {
            a.plan_name = Set(v);
        }
        if let Some(v) = dto.description {
            a.description = Set(v);
        }
        if let Some(v) = dto.price {
            a.price = Set(sea_orm::prelude::Decimal::try_from(v).unwrap_or_default());
        }
        if let Some(v) = dto.quota {
            a.quota = Set(v);
        }
        if let Some(v) = dto.status {
            a.status = Set(v);
        }
        if let Some(v) = dto.features {
            a.features = Set(v);
        }
        if let Some(v) = dto.limits {
            a.limits = Set(v);
        }
        a.update_by = Set(operator.into());
        let u = a
            .update(&self.db)
            .await
            .context("更新套餐失败")
            .map_err(ApiErrors::Internal)?;
        Ok(SubscriptionPlanVo::from_model(u))
    }

    // ─── Order ───
    pub async fn list_orders(
        &self,
        query: QueryOrderDto,
        pagination: Pagination,
    ) -> ApiResult<Page<OrderVo>> {
        let page = order::Entity::find()
            .filter(query)
            .order_by_desc(order::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询订单列表失败")?;
        Ok(page.map(OrderVo::from_model))
    }
    pub async fn get_order(&self, id: i64) -> ApiResult<OrderVo> {
        let m = order::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询订单失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("订单不存在".to_string()))?;
        Ok(OrderVo::from_model(m))
    }

    // ─── Topup ───
    pub async fn list_topups(
        &self,
        query: QueryTopupDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TopupVo>> {
        let page = topup::Entity::find()
            .filter(query)
            .order_by_desc(topup::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询充值列表失败")?;
        Ok(page.map(TopupVo::from_model))
    }

    // ─── Redemption ───
    pub async fn list_redemptions(
        &self,
        query: QueryRedemptionDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RedemptionVo>> {
        let page = redemption::Entity::find()
            .filter(query)
            .order_by_desc(redemption::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询兑换码列表失败")?;
        Ok(page.map(RedemptionVo::from_model))
    }
    pub async fn create_redemption_batch(
        &self,
        dto: CreateRedemptionBatchDto,
        operator: &str,
    ) -> ApiResult<Vec<RedemptionVo>> {
        let now = chrono::Utc::now().fixed_offset();
        let batch_id = if dto.batch_id.is_empty() {
            format!("BATCH{}", now.format("%Y%m%d%H%M%S"))
        } else {
            dto.batch_id
        };
        let mut results = Vec::with_capacity(dto.count);
        for _ in 0..dto.count.min(1000) {
            let code = generate_redemption_code();
            let a = redemption::ActiveModel {
                code: Set(code),
                batch_id: Set(batch_id.clone()),
                quota: Set(dto.quota),
                status: Set(1),
                redeemed_by: Set(0),
                redeemed_at: Set(None),
                expires_at: Set(dto.expires_at),
                create_by: Set(operator.into()),
                create_time: Set(now),
                ..Default::default()
            };
            let m = a
                .insert(&self.db)
                .await
                .context("创建兑换码失败")
                .map_err(ApiErrors::Internal)?;
            results.push(RedemptionVo::from_model(m));
        }
        Ok(results)
    }
}

fn generate_redemption_code() -> String {
    use std::fmt::Write;
    let random: u64 = rand::random();
    let mut buf = String::with_capacity(16);
    let _ = write!(buf, "RDM-{:012X}", random & 0xFFFF_FFFF_FFFF);
    buf
}
