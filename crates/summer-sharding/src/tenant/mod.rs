mod context;
mod lifecycle;
mod listener;
mod metadata;
mod rewrite;
mod rls;
mod router;
#[cfg(test)]
pub(crate) mod test_support;

pub use context::TenantContext;
pub use lifecycle::TenantLifecycleManager;
pub use listener::{
    PgTenantMetadataListener, TENANT_METADATA_CHANNEL, TenantMetadataListener,
    TenantMetadataNotificationHandler,
};
pub use metadata::{
    SeaOrmTenantMetadataLoader, TenantMetadataApplyOutcome, TenantMetadataEvent,
    TenantMetadataEventKind, TenantMetadataLoader, TenantMetadataRecord, TenantMetadataSchema,
    TenantMetadataStore,
};
pub use rewrite::apply_tenant_rewrite;
pub use rls::TenantRlsManager;
pub use router::{TenantRouteAdjustment, TenantRouter};
