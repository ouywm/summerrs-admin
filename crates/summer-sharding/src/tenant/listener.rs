use std::{sync::Arc, time::Duration};

use futures::future::BoxFuture;
use sea_orm::{
    DatabaseConnection,
    sqlx::{
        Error as SqlxError,
        postgres::{PgListener, PgPool},
    },
};
use tokio::task::JoinHandle;

/// Notification handler invoked whenever a payload is received from PostgreSQL.
pub type TenantMetadataNotificationHandler =
    Arc<dyn Fn(String) -> BoxFuture<'static, ()> + Send + Sync>;

/// Generic trait that exposes a background payload listener.
pub trait TenantMetadataListener: Send + Sync + 'static {
    fn spawn(
        self: Arc<Self>,
        metadata_connection: DatabaseConnection,
        handler: TenantMetadataNotificationHandler,
    ) -> JoinHandle<()>;
}

/// Default channel used by `pg_notify('summer_sharding_tenant_metadata', ..)`.
pub const TENANT_METADATA_CHANNEL: &str = "summer_sharding_tenant_metadata";
const TENANT_METADATA_RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct PgTenantMetadataListener {
    channel: String,
}

impl PgTenantMetadataListener {
    pub fn new(channel: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
        }
    }
}

impl TenantMetadataListener for PgTenantMetadataListener {
    fn spawn(
        self: Arc<Self>,
        metadata_connection: DatabaseConnection,
        handler: TenantMetadataNotificationHandler,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let pool = metadata_connection.get_postgres_connection_pool().clone();
            loop {
                if let Err(error) =
                    run_listener(pool.clone(), self.channel.clone(), handler.clone()).await
                {
                    tracing::warn!(
                        error = %error,
                        channel = %self.channel,
                        "tenant metadata listener disconnected, will reconnect shortly"
                    );
                }
                tokio::time::sleep(TENANT_METADATA_RECONNECT_DELAY).await;
            }
        })
    }
}

async fn run_listener(
    pool: PgPool,
    channel: String,
    handler: TenantMetadataNotificationHandler,
) -> Result<(), SqlxError> {
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen(&channel).await?;

    loop {
        let notification = listener.recv().await?;
        handler(notification.payload().to_string()).await;
    }
}
