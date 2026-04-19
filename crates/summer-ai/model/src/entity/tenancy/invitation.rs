//! AI 邀请表（组织/团队/项目成员邀请）
//! 对应 sql/ai/invitation.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待接受 2=已接受 3=已过期 4=已撤销
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum InvitationStatus {
    /// 待接受
    #[sea_orm(num_value = 1)]
    PendingAcceptance = 1,
    /// 已接受
    #[sea_orm(num_value = 2)]
    Accepted = 2,
    /// 已过期
    #[sea_orm(num_value = 3)]
    Expired = 3,
    /// 已撤销
    #[sea_orm(num_value = 4)]
    Revoked = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "invitation")]
pub struct Model {
    /// 邀请ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属组织ID
    pub organization_id: i64,
    /// 目标团队ID（可为空）
    pub team_id: Option<i64>,
    /// 目标项目ID（可为空）
    pub project_id: Option<i64>,
    /// 邀请发起人用户ID
    pub inviter_user_id: i64,
    /// 被邀请用户ID（已存在用户时使用）
    pub invitee_user_id: i64,
    /// 被邀请邮箱
    pub invitee_email: String,
    /// 目标类型：organization/team/project
    pub target_type: String,
    /// 加入后的角色编码
    pub role_code: String,
    /// 邀请链接令牌哈希
    pub invite_token_hash: String,
    /// 状态：1=待接受 2=已接受 3=已过期 4=已撤销
    pub status: InvitationStatus,
    /// 来源：manual/sso/scim/import
    pub source: String,
    /// 过期时间
    pub expires_at: DateTimeWithTimeZone,
    /// 接受邀请的用户ID
    pub accepted_by: i64,
    /// 接受时间
    pub accepted_time: Option<DateTimeWithTimeZone>,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
