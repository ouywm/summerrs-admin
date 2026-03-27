use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::algorithm::ShardingValue;

tokio::task_local! {
    pub static CURRENT_HINT: ShardingHint;
}

tokio::task_local! {
    pub static CURRENT_ACCESS_CONTEXT: ShardingAccessContext;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShardingHint {
    Table(String),
    Value(String, ShardingValue),
    Broadcast,
    Shadow,
    SkipMasking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShardingAccessContext {
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub allow_skip_masking: bool,
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

    pub fn allow_skip_masking(mut self) -> Self {
        self.allow_skip_masking = true;
        self
    }

    pub fn can_skip_masking(&self) -> bool {
        self.allow_skip_masking
            || self.roles.iter().any(|role| {
                matches!(
                    role.to_ascii_lowercase().as_str(),
                    "admin" | "root" | "super_admin"
                )
            })
            || self.permissions.iter().any(|permission| {
                matches!(
                    permission.to_ascii_lowercase().as_str(),
                    "masking:skip" | "data:unmask" | "pii:read:raw"
                )
            })
    }
}

pub async fn with_hint<F, T>(hint: ShardingHint, future: F) -> T
where
    F: Future<Output = T>,
{
    CURRENT_HINT.scope(hint, future).await
}

pub async fn with_access_context<F, T>(context: ShardingAccessContext, future: F) -> T
where
    F: Future<Output = T>,
{
    CURRENT_ACCESS_CONTEXT.scope(context, future).await
}

pub fn current_hint() -> Option<ShardingHint> {
    CURRENT_HINT.try_with(Clone::clone).ok()
}

pub fn current_access_context() -> Option<ShardingAccessContext> {
    CURRENT_ACCESS_CONTEXT.try_with(Clone::clone).ok()
}

pub fn should_skip_masking(hint: Option<&ShardingHint>) -> bool {
    matches!(hint, Some(ShardingHint::SkipMasking))
        || current_access_context().is_some_and(|context| context.can_skip_masking())
}

#[cfg(test)]
mod tests {
    use crate::connector::hint::{ShardingAccessContext, should_skip_masking, with_access_context};

    #[test]
    fn skip_masking_is_disabled_by_default() {
        assert!(!should_skip_masking(None));
    }

    #[tokio::test]
    async fn access_context_can_enable_skip_masking() {
        let allowed =
            with_access_context(ShardingAccessContext::default().with_role("admin"), async {
                should_skip_masking(None)
            })
            .await;
        assert!(allowed);
    }
}
