use std::time::Duration;

use super::ShardingConnection;
use crate::error::{Result, ShardingError};

impl ShardingConnection {
    pub async fn reload_tenant_metadata(
        &self,
        metadata_connection: &sea_orm::DatabaseConnection,
    ) -> Result<()> {
        let loader = self.inner.metadata_loader.get().ok_or_else(|| {
            ShardingError::Config(
                "tenant metadata loader is not configured; register an Arc<dyn TenantMetadataLoader> before reloading metadata".to_string(),
            )
        })?;
        self.inner
            .tenant_metadata
            .replace_with_loader(metadata_connection, loader.as_ref())
            .await?;
        self.inner
            .pool
            .sync_tenant_datasources(self.inner.tenant_metadata.as_ref())
            .await?;
        Ok(())
    }

    pub async fn apply_tenant_metadata_notification(
        &self,
        metadata_connection: &sea_orm::DatabaseConnection,
        payload: &str,
    ) -> Result<()> {
        let outcome = self
            .inner
            .tenant_metadata
            .apply_notification_payload(payload)?;
        if outcome == crate::tenant::TenantMetadataApplyOutcome::ReloadRequired {
            self.reload_tenant_metadata(metadata_connection).await?;
        } else {
            self.inner
                .pool
                .sync_tenant_datasources(self.inner.tenant_metadata.as_ref())
                .await?;
        }
        Ok(())
    }

    pub fn spawn_tenant_metadata_polling(
        &self,
        metadata_connection: sea_orm::DatabaseConnection,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let connection = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let _ = connection
                    .reload_tenant_metadata(&metadata_connection)
                    .await;
            }
        })
    }
}
