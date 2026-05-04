pub mod plugin;

pub use plugin::SummerAiAgentPlugin;

pub fn agent_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
