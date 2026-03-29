use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use crate::{
    config::KeyGeneratorConfig,
    error::{Result, ShardingError},
    keygen::KeyGenerator,
};

#[derive(Debug)]
struct SnowflakeState {
    last_timestamp: i64,
    sequence: i64,
}

#[derive(Debug)]
pub struct SnowflakeKeyGenerator {
    epoch_millis: i64,
    worker_id: i64,
    state: Mutex<SnowflakeState>,
}

impl SnowflakeKeyGenerator {
    pub fn from_config(config: &KeyGeneratorConfig) -> Result<Self> {
        let worker_id = config
            .props
            .get("worker_id")
            .and_then(|value| value.as_i64())
            .unwrap_or(1);
        if !(0..=1023).contains(&worker_id) {
            return Err(ShardingError::Config(
                "snowflake worker_id must be within 0..=1023".to_string(),
            ));
        }
        let epoch_millis = config
            .props
            .get("epoch_millis")
            .and_then(|value| value.as_i64())
            .unwrap_or(1_704_067_200_000);
        Ok(Self {
            epoch_millis,
            worker_id,
            state: Mutex::new(SnowflakeState {
                last_timestamp: 0,
                sequence: 0,
            }),
        })
    }

    fn current_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_millis() as i64
    }
}

impl KeyGenerator for SnowflakeKeyGenerator {
    fn next_id(&self) -> i64 {
        let mut state = self.state.lock();
        let mut timestamp = Self::current_millis();
        if timestamp < state.last_timestamp {
            timestamp = state.last_timestamp;
        }

        if timestamp == state.last_timestamp {
            state.sequence = (state.sequence + 1) & 0x0fff;
            if state.sequence == 0 {
                while timestamp <= state.last_timestamp {
                    timestamp = Self::current_millis();
                }
            }
        } else {
            state.sequence = 0;
        }
        state.last_timestamp = timestamp;

        ((timestamp - self.epoch_millis) << 22) | (self.worker_id << 12) | state.sequence
    }

    fn generator_type(&self) -> &str {
        "snowflake"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::{
        config::KeyGeneratorConfig,
        error::ShardingError,
        keygen::{KeyGenerator, SnowflakeKeyGenerator},
    };

    fn config(worker_id: i64) -> KeyGeneratorConfig {
        KeyGeneratorConfig {
            kind: "snowflake".to_string(),
            props: BTreeMap::from([
                ("worker_id".to_string(), json!(worker_id)),
                ("epoch_millis".to_string(), json!(1_700_000_000_000_i64)),
            ]),
        }
    }

    #[test]
    fn snowflake_rejects_out_of_range_worker_id() {
        let error = SnowflakeKeyGenerator::from_config(&config(2048)).expect_err("config error");
        assert!(matches!(error, ShardingError::Config(_)));
    }

    #[test]
    fn snowflake_generates_monotonic_ids() {
        let generator = SnowflakeKeyGenerator::from_config(&config(7)).expect("generator");

        let first = generator.next_id();
        let second = generator.next_id();

        assert!(second > first);
        assert_eq!(generator.generator_type(), "snowflake");
        assert_eq!((first >> 12) & 0x03ff, 7);
    }
}
