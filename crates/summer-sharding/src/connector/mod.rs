pub mod connection;
pub mod statement;
pub mod transaction;

use serde::{Deserialize, Serialize};

use crate::Extensions;
use crate::algorithm::ShardingValue;

pub use connection::ShardingConnection;
pub use statement::{StatementContext, analyze_statement};
pub use transaction::{
    PreparedTwoPhaseTransaction, ShardingTransaction, TwoPhaseShardingTransaction,
    TwoPhaseTransactionError,
};

/// SQL 路由 hint，允许调用方显式指定路由目标或行为。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShardingHint {
    /// 强制路由到指定的实际表名
    Table(String),
    /// 强制按指定列值路由
    Value(String, ShardingValue),
    /// 广播到所有分片
    Broadcast,
}

/// 请求级访问上下文，携带调用方的角色/权限信息，供插件使用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShardingAccessContext {
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 类型安全的扩展数据容器，供自定义插件存取请求级上下文。
    #[serde(skip)]
    pub extensions: Extensions,
}

impl ShardingAccessContext {
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    pub fn with_permission(mut self, permission: impl Into<String>) -> Self {
        self.permissions.push(permission.into());
        self
    }

    /// 存入一个类型安全的扩展数据（链式调用）
    pub fn with_extension<T: Clone + Send + Sync + 'static>(mut self, val: T) -> Self {
        self.extensions.insert(val);
        self
    }

    /// 获取扩展数据的不可变引用
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }
}

pub fn with_hint(connection: &ShardingConnection, hint: ShardingHint) -> ShardingConnection {
    connection.with_hint(hint)
}

pub fn with_access_context(
    connection: &ShardingConnection,
    context: ShardingAccessContext,
) -> ShardingConnection {
    connection.with_access_context(context)
}
