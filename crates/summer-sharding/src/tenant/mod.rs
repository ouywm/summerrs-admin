mod context;
mod lifecycle;
mod listener;
mod metadata;
mod rewrite;
mod rls;
mod router;

pub use context::{CURRENT_TENANT, TenantContext, current_tenant, with_tenant};
pub use lifecycle::TenantLifecycleManager;
pub use listener::{
    PgTenantMetadataListener, TENANT_METADATA_CHANNEL, TenantMetadataListener,
    TenantMetadataNotificationHandler,
};
pub use metadata::{
    TenantMetadataApplyOutcome, TenantMetadataEvent, TenantMetadataEventKind, TenantMetadataRecord,
    TenantMetadataStore,
};
pub use rewrite::apply_tenant_rewrite;
pub use rls::TenantRlsManager;
pub use router::{TenantRouteAdjustment, TenantRouter};
