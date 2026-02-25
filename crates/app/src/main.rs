mod router;
mod service;

use spring::App;
use spring_web::WebPlugin;
use spring_sea_orm::SeaOrmPlugin;
use spring_redis::RedisPlugin;
use spring_job::JobPlugin;
use spring_sa_token::SaTokenPlugin;
use spring_web::WebConfigurator;
use spring_job::JobConfigurator;
use spring::auto_config;

#[auto_config(WebConfigurator, JobConfigurator)]
#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(WebPlugin)
        .add_plugin(SeaOrmPlugin)
        .add_plugin(RedisPlugin)
        .add_plugin(JobPlugin)
        .add_plugin(SaTokenPlugin)
        .run()
        .await;
}
