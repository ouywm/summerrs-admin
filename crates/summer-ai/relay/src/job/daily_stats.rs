use summer::extractor::Component;
use summer_job::cron;

use crate::service::daily_stats::DailyStatsService;

#[cron("0 5 0 * * *")]
async fn aggregate_ai_daily_stats(Component(service): Component<DailyStatsService>) {
    if let Err(error) = service.aggregate_yesterday().await {
        tracing::warn!("failed to aggregate ai daily stats: {error}");
    }
}
