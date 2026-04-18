use std::{collections::BTreeMap, sync::Arc};

use super::{ExecutionOverrides, ShardingConnection};
use crate::connector::{ShardingAccessContext, ShardingHint};

impl ShardingConnection {
    pub(crate) fn execution_overrides(&self) -> ExecutionOverrides {
        ExecutionOverrides {
            hint: self.hint_override.clone(),
            access_context: self.access_context_override.clone(),
            tenant: self.tenant_override.clone(),
            shadow_headers: self.shadow_headers_override.clone(),
        }
    }

    pub fn with_hint(&self, hint: ShardingHint) -> Self {
        let mut clone = self.clone();
        clone.hint_override = Some(hint);
        clone
    }

    pub fn with_tenant_context(&self, tenant: crate::tenant::TenantContext) -> Self {
        let resolved = self.resolve_tenant_context(tenant);
        let mut clone = self.clone();
        clone.tenant_override = Some(resolved);
        clone
    }

    pub fn tenant_context(&self) -> Option<&crate::tenant::TenantContext> {
        self.tenant_override.as_ref()
    }

    pub fn with_access_context(&self, context: ShardingAccessContext) -> Self {
        let mut clone = self.clone();
        clone.access_context_override = Some(context);
        clone
    }

    pub fn with_shadow_headers(&self, headers: BTreeMap<String, String>) -> Self {
        let mut clone = self.clone();
        clone.shadow_headers_override = Some(Arc::new(headers));
        clone
    }

    pub fn resolve_tenant_context(
        &self,
        tenant: crate::tenant::TenantContext,
    ) -> crate::tenant::TenantContext {
        self.inner.tenant_router.resolve_context(tenant)
    }
}
