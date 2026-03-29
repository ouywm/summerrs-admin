use serde::{Deserialize, Serialize};

use crate::algorithm::ShardingValue;

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

pub fn with_hint(
    connection: &crate::ShardingConnection,
    hint: ShardingHint,
) -> crate::ShardingConnection {
    connection.with_hint(hint)
}

pub fn with_access_context(
    connection: &crate::ShardingConnection,
    context: ShardingAccessContext,
) -> crate::ShardingConnection {
    connection.with_access_context(context)
}

pub fn should_skip_masking(
    hint: Option<&ShardingHint>,
    access_context: Option<&ShardingAccessContext>,
) -> bool {
    matches!(hint, Some(ShardingHint::SkipMasking))
        || access_context.is_some_and(ShardingAccessContext::can_skip_masking)
}

#[cfg(test)]
mod tests {
    use crate::connector::hint::{ShardingAccessContext, should_skip_masking};

    #[test]
    fn skip_masking_is_disabled_by_default() {
        assert!(!should_skip_masking(None, None));
    }

    #[test]
    fn access_context_can_enable_skip_masking() {
        let context = ShardingAccessContext::default().with_role("admin");
        assert!(should_skip_masking(None, Some(&context)));
    }
}
