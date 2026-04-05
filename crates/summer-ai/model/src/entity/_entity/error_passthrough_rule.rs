//! AI 错误透传规则表实体
//! 控制上游错误如何透传或改写给客户端

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "error_passthrough_rule")]
pub struct Model {
    /// 规则ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 规则名称
    pub name: String,
    /// 是否启用
    pub enabled: bool,
    /// 优先级（越小越先匹配）
    pub priority: i32,
    /// 限定渠道类型（0=全部）
    pub channel_type: i16,
    /// 限定供应商编码（空=全部）
    pub vendor_code: String,
    /// 匹配错误码列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub error_codes: serde_json::Value,
    /// 匹配关键词列表（JSON 数组）
    #[sea_orm(column_type = "JsonBinary")]
    pub keywords: serde_json::Value,
    /// 匹配模式：any/all
    pub match_mode: String,
    /// 是否透传上游状态码
    pub passthrough_status_code: bool,
    /// 自定义返回状态码
    pub response_status_code: i32,
    /// 是否透传上游响应体
    pub passthrough_body: bool,
    /// 自定义响应体
    #[sea_orm(column_type = "Text")]
    pub custom_body: String,
    /// 是否跳过监控系统记录
    pub skip_monitoring: bool,
    /// 规则说明
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
