use summer::extractor::Component;
use summer_job::cron;
use tracing::warn;

use crate::service::channel::ChannelService;

/// Every 5 minutes, probe auto-disabled channels and recover the healthy ones.
#[cron("0 */5 * * * *")]
async fn recover_auto_disabled_ai_channels(Component(service): Component<ChannelService>) {
    if let Err(error) = service.recover_auto_disabled_channels().await {
        warn!("failed to recover auto-disabled AI channels: {error}");
    }
}
