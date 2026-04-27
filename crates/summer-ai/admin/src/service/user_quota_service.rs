use anyhow::Context;
use bigdecimal::BigDecimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use serde_json::json;
use summer::plugin::Service;
use summer_ai_model::dto::user_quota::{
    AdjustUserQuotaDto, CreateUserQuotaDto, UpdateUserQuotaDto, UserQuotaQueryDto,
};
use summer_ai_model::entity::billing::{transaction, user_quota};
use summer_ai_model::vo::user_quota::UserQuotaVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use uuid::Uuid;

#[derive(Clone, Service)]
pub struct UserQuotaService {
    #[inject(component)]
    db: DbConn,
}

impl UserQuotaService {
    pub async fn list(
        &self,
        query: UserQuotaQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<UserQuotaVo>> {
        let page: Page<user_quota::Model> = user_quota::Entity::find()
            .filter(query)
            .order_by_desc(user_quota::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询用户额度列表失败")?;

        Ok(page.map(UserQuotaVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<UserQuotaVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(UserQuotaVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateUserQuotaDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_unique_user(dto.user_id).await?;

        let quota = dto.quota;
        let remark = dto.remark.clone().unwrap_or_default();
        let group = dto
            .channel_group
            .clone()
            .unwrap_or_else(|| "default".to_string());

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.to_string();
                let reference_no = new_reference_no();
                Box::pin(async move {
                    let model = dto
                        .into_active_model(&operator)
                        .insert(txn)
                        .await
                        .context("创建用户额度失败")
                        .map_err(ApiErrors::Internal)?;

                    if quota != 0 {
                        insert_adjust_transaction(
                            txn,
                            AdjustTransactionInput {
                                user_id: model.user_id,
                                balance_before: 0,
                                balance_after: quota,
                                quota_delta: quota,
                                reference_no: &reference_no,
                                reason: "初始化用户额度",
                                metadata: json!({
                                    "channelGroup": group,
                                    "remark": remark,
                                }),
                            },
                        )
                        .await?;
                    }
                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateUserQuotaDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        let model = self.find_model_by_id(id).await?;

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.to_string();
                let model = model.clone();
                let reference_no = new_reference_no();
                Box::pin(async move {
                    if let Some(new_quota) = dto.quota {
                        ensure_quota_target_valid(new_quota, model.used_quota)
                            .map_err(ApiErrors::BadRequest)?;
                    }

                    let mut active: user_quota::ActiveModel = model.clone().into();
                    dto.apply_to(&mut active, &operator);
                    let updated = active
                        .update(txn)
                        .await
                        .context("更新用户额度失败")
                        .map_err(ApiErrors::Internal)?;

                    let quota_delta = updated.quota - model.quota;
                    if quota_delta != 0 {
                        insert_adjust_transaction(
                            txn,
                            AdjustTransactionInput {
                                user_id: updated.user_id,
                                balance_before: model.quota - model.used_quota,
                                balance_after: updated.quota - updated.used_quota,
                                quota_delta,
                                reference_no: &reference_no,
                                reason: "后台更新用户额度",
                                metadata: json!({
                                    "oldQuota": model.quota,
                                    "newQuota": updated.quota,
                                    "usedQuota": updated.used_quota,
                                }),
                            },
                        )
                        .await?;
                    }

                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    pub async fn adjust(&self, id: i64, dto: AdjustUserQuotaDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;

        self.db
            .transaction::<_, (), ApiErrors>(|txn| {
                let operator = operator.to_string();
                let reference_no = dto.reference_no.clone().unwrap_or_else(new_reference_no);
                let reason = dto
                    .reason
                    .clone()
                    .unwrap_or_else(|| "后台调整用户额度".to_string());
                Box::pin(async move {
                    let model = user_quota::Entity::find_by_id(id)
                        .lock_exclusive()
                        .one(txn)
                        .await
                        .context("查询待调整用户额度失败")
                        .map_err(ApiErrors::Internal)?
                        .ok_or_else(|| ApiErrors::NotFound(format!("用户额度不存在: id={id}")))?;

                    let new_quota =
                        apply_quota_delta(model.quota, model.used_quota, dto.quota_delta)
                            .map_err(ApiErrors::BadRequest)?;

                    let mut active: user_quota::ActiveModel = model.clone().into();
                    active.quota = Set(new_quota);
                    active.update_by = Set(operator);
                    active
                        .update(txn)
                        .await
                        .context("调整用户额度失败")
                        .map_err(ApiErrors::Internal)?;

                    insert_adjust_transaction(
                        txn,
                        AdjustTransactionInput {
                            user_id: model.user_id,
                            balance_before: model.quota - model.used_quota,
                            balance_after: new_quota - model.used_quota,
                            quota_delta: dto.quota_delta,
                            reference_no: &reference_no,
                            reason: &reason,
                            metadata: json!({
                                "oldQuota": model.quota,
                                "newQuota": new_quota,
                                "usedQuota": model.used_quota,
                            }),
                        },
                    )
                    .await?;

                    Ok(())
                })
            })
            .await?;

        Ok(())
    }

    async fn ensure_unique_user(&self, user_id: i64) -> ApiResult<()> {
        let exists = user_quota::Entity::find()
            .filter(user_quota::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .context("检查用户额度唯一性失败")?;

        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "用户额度已存在: user_id={user_id}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<user_quota::Model> {
        user_quota::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询用户额度详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("用户额度不存在: id={id}")))
    }
}

pub fn apply_quota_delta(
    total_quota: i64,
    used_quota: i64,
    quota_delta: i64,
) -> Result<i64, String> {
    let new_total = total_quota + quota_delta;
    if new_total < 0 {
        return Err("调整后总额度不能为负数".to_string());
    }
    if new_total < used_quota {
        return Err("调整后总额度不能低于已使用额度".to_string());
    }
    Ok(new_total)
}

pub fn transaction_direction(quota_delta: i64) -> &'static str {
    if quota_delta >= 0 { "credit" } else { "debit" }
}

pub fn transaction_trade_type() -> &'static str {
    "adjust"
}

fn ensure_quota_target_valid(new_quota: i64, used_quota: i64) -> Result<(), String> {
    if new_quota < 0 {
        return Err("quota 不能为负数".to_string());
    }
    if new_quota < used_quota {
        return Err("quota 不能低于已使用额度".to_string());
    }
    Ok(())
}

fn new_reference_no() -> String {
    format!("uq_{}", Uuid::new_v4().simple())
}

struct AdjustTransactionInput<'a> {
    user_id: i64,
    balance_before: i64,
    balance_after: i64,
    quota_delta: i64,
    reference_no: &'a str,
    reason: &'a str,
    metadata: serde_json::Value,
}

async fn insert_adjust_transaction(
    txn: &sea_orm::DatabaseTransaction,
    input: AdjustTransactionInput<'_>,
) -> ApiResult<()> {
    let mut payload = input.metadata;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("reason".to_string(), serde_json::json!(input.reason));
    }

    transaction::ActiveModel {
        organization_id: Set(0),
        user_id: Set(input.user_id),
        project_id: Set(0),
        order_id: Set(0),
        payment_method_id: Set(0),
        account_type: Set("quota".to_string()),
        direction: Set(transaction_direction(input.quota_delta).to_string()),
        trade_type: Set(transaction_trade_type().to_string()),
        amount: Set(BigDecimal::from(0)),
        currency: Set("USD".to_string()),
        quota_delta: Set(input.quota_delta),
        balance_before: Set(BigDecimal::from(input.balance_before)),
        balance_after: Set(BigDecimal::from(input.balance_after)),
        reference_no: Set(input.reference_no.to_string()),
        status: Set(transaction::TransactionStatus::Succeeded),
        metadata: Set(payload),
        ..Default::default()
    }
    .insert(txn)
    .await
    .context("写入用户额度调整流水失败")
    .map_err(ApiErrors::Internal)?;
    Ok(())
}
