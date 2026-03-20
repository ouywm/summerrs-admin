use schemars::JsonSchema;
use serde::Deserialize;
use summer::config::Configurable;

summer::submit_config_schema!("socket-gateway", SocketGatewayConfig);

#[derive(Debug, Clone, Deserialize, Configurable, JsonSchema)]
#[config_prefix = "socket-gateway"]
pub struct SocketGatewayConfig {
    #[serde(default = "default_redis_prefix")]
    pub redis_prefix: String,
    #[serde(default = "default_session_ttl_seconds")]
    pub session_ttl_seconds: u64,
}

fn default_redis_prefix() -> String {
    "summerrs:socket".to_string()
}

fn default_session_ttl_seconds() -> u64 {
    86400
}
