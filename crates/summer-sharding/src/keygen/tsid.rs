use std::time::{SystemTime, UNIX_EPOCH};

use crate::{config::KeyGeneratorConfig, keygen::KeyGenerator};
use parking_lot::Mutex;

#[derive(Debug)]
struct TsidState {
    last_timestamp: i64,
    counter: i64,
}

#[derive(Debug)]
pub struct TsidKeyGenerator {
    epoch_millis: i64,
    state: Mutex<TsidState>,
}

impl TsidKeyGenerator {
    pub fn from_config(config: &KeyGeneratorConfig) -> Self {
        let epoch_millis = config
            .props
            .get("epoch_millis")
            .and_then(|value| value.as_i64())
            .unwrap_or(1_704_067_200_000);
        Self {
            epoch_millis,
            state: Mutex::new(TsidState {
                last_timestamp: 0,
                counter: rand::random_range(0..=0x1f_ffff),
            }),
        }
    }

    fn current_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_millis() as i64
    }
}

impl KeyGenerator for TsidKeyGenerator {
    fn next_id(&self) -> i64 {
        let mut state = self.state.lock();
        let mut timestamp = Self::current_millis();
        if timestamp < state.last_timestamp {
            timestamp = state.last_timestamp;
        }

        if timestamp == state.last_timestamp {
            state.counter = (state.counter + 1) & 0x1f_ffff;
        } else {
            state.counter = rand::random_range(0..=0x1f_ffff);
        }
        state.last_timestamp = timestamp;

        ((timestamp - self.epoch_millis) << 21) | state.counter
    }

    fn generator_type(&self) -> &str {
        "tsid"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::{
        config::KeyGeneratorConfig,
        keygen::{KeyGenerator, TsidKeyGenerator},
    };

    fn config() -> KeyGeneratorConfig {
        KeyGeneratorConfig {
            kind: "tsid".to_string(),
            props: BTreeMap::from([(
                "epoch_millis".to_string(),
                json!(1_700_000_000_000_i64),
            )]),
        }
    }

    #[test]
    fn tsid_generates_monotonic_ids() {
        let generator = TsidKeyGenerator::from_config(&config());

        let first = generator.next_id();
        let second = generator.next_id();

        assert!(second >= first);
        assert_eq!(generator.generator_type(), "tsid");
        assert!(first > 0);
    }
}
