use std::collections::BTreeSet;

use crate::datasource::{DataSourceHealth, DataSourceRouteState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceDiscovery {
    pub primary: Option<String>,
    pub replicas: Vec<String>,
    pub unhealthy: Vec<String>,
}

impl DataSourceDiscovery {
    pub fn detect(health: &[DataSourceHealth], primary_hint: Option<&str>) -> Self {
        Self::detect_scoped(health, primary_hint, None)
    }

    pub fn detect_scoped(
        health: &[DataSourceHealth],
        primary_hint: Option<&str>,
        allowed: Option<&BTreeSet<String>>,
    ) -> Self {
        let health = health
            .iter()
            .filter(|item| {
                allowed
                    .map(|names| names.contains(item.datasource.as_str()))
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        let unhealthy = health
            .iter()
            .filter(|item| !item.reachable)
            .map(|item| item.datasource.clone())
            .collect::<Vec<_>>();
        let primary = primary_hint
            .and_then(|value| {
                health
                    .iter()
                    .find(|item| item.reachable && item.datasource == value)
                    .map(|item| item.datasource.clone())
            })
            .or_else(|| {
                health
                    .iter()
                    .find(|item| item.reachable)
                    .map(|item| item.datasource.clone())
            });
        let replicas = health
            .iter()
            .filter(|item| item.reachable)
            .filter(|item| Some(item.datasource.as_str()) != primary.as_deref())
            .map(|item| item.datasource.clone())
            .collect();
        Self {
            primary,
            replicas,
            unhealthy,
        }
    }

    pub fn into_route_state(
        &self,
        rule_name: &str,
        configured_primary: &str,
        configured_replicas: &[String],
    ) -> DataSourceRouteState {
        let reachable = self
            .primary
            .iter()
            .cloned()
            .chain(self.replicas.iter().cloned())
            .collect::<BTreeSet<_>>();
        let configured_replica_set = configured_replicas.iter().cloned().collect::<BTreeSet<_>>();

        let write_target = if reachable.contains(configured_primary) {
            Some(configured_primary.to_string())
        } else {
            self.primary.clone()
        };
        let failover_active = write_target
            .as_deref()
            .is_some_and(|target| !target.eq_ignore_ascii_case(configured_primary));

        let mut healthy_replicas = reachable
            .iter()
            .filter(|candidate| configured_replica_set.contains(*candidate))
            .cloned()
            .collect::<Vec<_>>();
        healthy_replicas.sort();

        DataSourceRouteState {
            rule_name: rule_name.to_string(),
            configured_primary: configured_primary.to_string(),
            write_target,
            healthy_replicas,
            unhealthy: self.unhealthy.clone(),
            failover_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::datasource::{DataSourceDiscovery, DataSourceHealth};

    #[test]
    fn discovery_picks_reachable_primary_and_replicas() {
        let health = vec![
            DataSourceHealth {
                datasource: "primary".to_string(),
                reachable: true,
                error: None,
                latency_ms: Some(3),
            },
            DataSourceHealth {
                datasource: "replica".to_string(),
                reachable: true,
                error: None,
                latency_ms: Some(5),
            },
            DataSourceHealth {
                datasource: "down".to_string(),
                reachable: false,
                error: Some("boom".to_string()),
                latency_ms: None,
            },
        ];
        let discovery = DataSourceDiscovery::detect(&health, Some("primary"));
        assert_eq!(discovery.primary.as_deref(), Some("primary"));
        assert_eq!(discovery.replicas, vec!["replica".to_string()]);
        assert_eq!(discovery.unhealthy, vec!["down".to_string()]);
    }

    #[test]
    fn discovery_builds_failover_route_state() {
        let health = vec![
            DataSourceHealth {
                datasource: "primary".to_string(),
                reachable: false,
                error: Some("timeout".to_string()),
                latency_ms: None,
            },
            DataSourceHealth {
                datasource: "replica_a".to_string(),
                reachable: false,
                error: Some("down".to_string()),
                latency_ms: None,
            },
            DataSourceHealth {
                datasource: "replica_b".to_string(),
                reachable: true,
                error: None,
                latency_ms: Some(2),
            },
        ];
        let discovery = DataSourceDiscovery::detect(&health, Some("primary"));
        let state = discovery.into_route_state(
            "ai-rw",
            "primary",
            &["replica_a".to_string(), "replica_b".to_string()],
        );
        assert_eq!(state.write_target.as_deref(), Some("replica_b"));
        assert_eq!(state.healthy_replicas, vec!["replica_b".to_string()]);
        assert!(state.failover_active);
    }

    #[test]
    fn scoped_discovery_never_fails_over_to_out_of_rule_datasource() {
        let health = vec![
            DataSourceHealth {
                datasource: "primary".to_string(),
                reachable: false,
                error: Some("timeout".to_string()),
                latency_ms: None,
            },
            DataSourceHealth {
                datasource: "replica_a".to_string(),
                reachable: false,
                error: Some("down".to_string()),
                latency_ms: None,
            },
            DataSourceHealth {
                datasource: "foreign_primary".to_string(),
                reachable: true,
                error: None,
                latency_ms: Some(1),
            },
        ];
        let discovery = DataSourceDiscovery::detect_scoped(
            &health,
            Some("primary"),
            Some(
                &["primary".to_string(), "replica_a".to_string()]
                    .into_iter()
                    .collect(),
            ),
        );
        let state = discovery.into_route_state("ai-rw", "primary", &["replica_a".to_string()]);
        assert!(state.write_target.is_none());
        assert!(state.healthy_replicas.is_empty());
        assert!(!state.failover_active);
    }
}
