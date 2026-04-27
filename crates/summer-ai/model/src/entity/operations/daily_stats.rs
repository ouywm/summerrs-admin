use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "daily_stats")]
pub struct Model {
    /// 统计ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 统计日期
    pub stats_date: Date,
    /// 用户ID（0=全局汇总）
    pub user_id: i64,
    /// 项目ID（0=全局汇总）
    pub project_id: i64,
    /// 渠道ID（0=全局汇总）
    pub channel_id: i64,
    /// 账号ID（0=全局汇总）
    pub account_id: i64,
    /// 标准化模型名（空字符串=全局汇总）
    pub model_name: String,
    /// 请求总数
    pub request_count: i64,
    /// 成功次数
    pub success_count: i64,
    /// 失败次数
    pub fail_count: i64,
    /// 输入 Token 总数
    pub prompt_tokens: i64,
    /// 输出 Token 总数
    pub completion_tokens: i64,
    /// 总 Token 数
    pub total_tokens: i64,
    /// 缓存命中 Token 总数
    pub cached_tokens: i64,
    /// 推理 Token 总数
    pub reasoning_tokens: i64,
    /// 消耗配额总计
    pub quota: i64,
    /// 成本金额总计
    #[sea_orm(column_type = "Decimal(Some((20, 10)))")]
    pub cost_total: BigDecimal,
    /// 平均总耗时（毫秒）
    pub avg_elapsed_time: i32,
    /// 平均首 token 时间（毫秒）
    pub avg_first_token_time: i32,
    /// 记录创建时间
    pub create_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Utc::now().fixed_offset();
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
