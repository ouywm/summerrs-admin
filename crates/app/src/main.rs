mod router;

use summer::App;
use summer_auth::SummerAuthPlugin;
use summer_job::JobConfigurator;
use summer_job::JobPlugin;
use summer_mail::MailPlugin;
use summer_mcp::McpPlugin;
use summer_plugins::{BackgroundTaskPlugin, Ip2RegionPlugin, LogBatchCollectorPlugin, S3Plugin};
use summer_redis::RedisPlugin;
use summer_sea_orm::SeaOrmPlugin;
use summer_system::plugins::{PermBitmapPlugin, SocketGatewayPlugin};
use summer_web::{WebConfigurator, WebPlugin};
use summer_xxl_job::XxlJobPlugin;

rust_i18n::i18n!("../../locales", fallback = "en");

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_plugin(WebPlugin)
        .add_plugin(SeaOrmPlugin)
        .add_plugin(RedisPlugin)
        .add_plugin(JobPlugin)
        .add_plugin(XxlJobPlugin)
        .add_plugin(MailPlugin)
        .add_plugin(SummerAuthPlugin)
        .add_plugin(PermBitmapPlugin)
        .add_plugin(SocketGatewayPlugin)
        .add_plugin(Ip2RegionPlugin)
        .add_plugin(S3Plugin)
        .add_plugin(BackgroundTaskPlugin)
        .add_plugin(LogBatchCollectorPlugin)
        .add_plugin(McpPlugin)
        .add_jobs(summer_job::handler::auto_jobs());
    summer_system::job::register_xxl_handlers(&mut app);
    app.add_router(router::router()).run().await;
}
