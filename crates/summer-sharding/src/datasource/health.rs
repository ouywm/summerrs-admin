#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceHealth {
    pub datasource: String,
    pub reachable: bool,
    pub error: Option<String>,
    pub latency_ms: Option<u128>,
}

#[cfg(test)]
mod tests {
    use super::DataSourceHealth;

    #[test]
    fn datasource_health_clone_and_eq_preserve_all_fields() {
        let health = DataSourceHealth {
            datasource: "ds_ai".to_string(),
            reachable: false,
            error: Some("timeout".to_string()),
            latency_ms: Some(128),
        };

        let cloned = health.clone();

        assert_eq!(cloned, health);
        assert_eq!(cloned.datasource, "ds_ai");
        assert!(!cloned.reachable);
    }
}
