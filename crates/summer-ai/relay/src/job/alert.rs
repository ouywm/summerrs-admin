use summer::extractor::Component;
use summer_job::cron;

use crate::service::alert::DailyStatsAlertService;

#[cron("0 15 0 * * *")]
async fn scan_ai_daily_stats_alerts(Component(service): Component<DailyStatsAlertService>) {
    if let Err(error) = service.scan_yesterday().await {
        tracing::warn!("failed to scan ai daily stats alerts: {error}");
    }
}
