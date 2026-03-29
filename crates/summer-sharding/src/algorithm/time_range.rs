use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveTime, TimeZone, Timelike, Weekday};

use crate::{
    config::TableRuleConfig,
    error::{Result, ShardingError},
};

use super::{ShardingAlgorithm, ShardingValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeGranularity {
    Month,
    Week,
    Day,
}

#[derive(Debug, Clone)]
pub struct TimeRangeShardingAlgorithm {
    pub granularity: TimeGranularity,
    pub pre_create_periods: usize,
    pub retention_periods: usize,
}

impl TimeRangeShardingAlgorithm {
    pub fn from_rule(rule: &TableRuleConfig) -> Result<Self> {
        let granularity = match rule
            .algorithm_props
            .get("granularity")
            .and_then(|value| value.as_str())
            .unwrap_or("month")
        {
            "month" => TimeGranularity::Month,
            "week" => TimeGranularity::Week,
            "day" => TimeGranularity::Day,
            other => {
                return Err(ShardingError::Config(format!(
                    "unsupported time_range granularity `{other}`"
                )));
            }
        };
        let pre_create_periods = rule
            .algorithm_props
            .get("pre_create_months")
            .or_else(|| rule.algorithm_props.get("pre_create_periods"))
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as usize;
        let retention_periods = rule
            .algorithm_props
            .get("retention_months")
            .or_else(|| rule.algorithm_props.get("retention_periods"))
            .and_then(|value| value.as_i64())
            .unwrap_or(12) as usize;

        Ok(Self {
            granularity,
            pre_create_periods,
            retention_periods,
        })
    }

    pub fn render_target(&self, pattern: &str, datetime: DateTime<FixedOffset>) -> String {
        pattern
            .replace(
                "${yyyyMM}",
                format!("{:04}{:02}", datetime.year(), datetime.month()).as_str(),
            )
            .replace(
                "${yyyyMMdd}",
                format!(
                    "{:04}{:02}{:02}",
                    datetime.year(),
                    datetime.month(),
                    datetime.day()
                )
                .as_str(),
            )
            .replace(
                "${yyyyww}",
                format!(
                    "{:04}{:02}",
                    datetime.iso_week().year(),
                    datetime.iso_week().week()
                )
                .as_str(),
            )
    }

    pub fn candidate_targets(&self, pattern: &str, now: DateTime<FixedOffset>) -> Vec<String> {
        let mut targets = Vec::new();
        for step in (0..self.retention_periods).rev() {
            targets.push(self.render_target(pattern, self.step_period(now, -(step as i64))));
        }
        for step in 1..=self.pre_create_periods {
            targets.push(self.render_target(pattern, self.step_period(now, step as i64)));
        }
        targets.sort();
        targets.dedup();
        targets
    }

    pub fn history_targets(
        &self,
        pattern: &str,
        now: DateTime<FixedOffset>,
        periods: usize,
    ) -> Vec<String> {
        let mut targets = Vec::new();
        for step in (0..periods).rev() {
            targets.push(self.render_target(pattern, self.step_period(now, -(step as i64))));
        }
        targets.sort();
        targets.dedup();
        targets
    }

    fn step_period(&self, datetime: DateTime<FixedOffset>, steps: i64) -> DateTime<FixedOffset> {
        match self.granularity {
            TimeGranularity::Month => add_months(datetime, steps),
            TimeGranularity::Week => datetime + Duration::weeks(steps),
            TimeGranularity::Day => datetime + Duration::days(steps),
        }
    }

    fn normalize_upper_bound(&self, upper: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        match self.granularity {
            TimeGranularity::Month => {
                if upper.day() == 1
                    && upper.hour() == 0
                    && upper.minute() == 0
                    && upper.second() == 0
                {
                    upper - Duration::seconds(1)
                } else {
                    upper
                }
            }
            TimeGranularity::Week => {
                if upper.weekday() == Weekday::Mon
                    && upper.time() == NaiveTime::from_hms_opt(0, 0, 0).expect("valid")
                {
                    upper - Duration::seconds(1)
                } else {
                    upper
                }
            }
            TimeGranularity::Day => {
                if upper.time() == NaiveTime::from_hms_opt(0, 0, 0).expect("valid") {
                    upper - Duration::seconds(1)
                } else {
                    upper
                }
            }
        }
    }

    fn range_targets(
        &self,
        available_targets: &[String],
        lower: DateTime<FixedOffset>,
        upper: DateTime<FixedOffset>,
    ) -> Vec<String> {
        if available_targets.len() == 1 && contains_time_placeholder(available_targets[0].as_str())
        {
            let pattern = available_targets[0].as_str();
            let mut cursor = lower;
            let mut targets = Vec::new();
            let upper = self.normalize_upper_bound(upper);
            while cursor <= upper {
                targets.push(self.render_target(pattern, cursor));
                cursor = self.step_period(cursor, 1);
            }
            targets.sort();
            targets.dedup();
            return targets;
        }

        available_targets.to_vec()
    }
}

impl ShardingAlgorithm for TimeRangeShardingAlgorithm {
    fn do_sharding(
        &self,
        available_targets: &[String],
        sharding_value: &ShardingValue,
    ) -> Vec<String> {
        let Some(datetime) = sharding_value.as_datetime() else {
            return available_targets.to_vec();
        };

        if let Some(pattern) = available_targets
            .first()
            .filter(|value| contains_time_placeholder(value.as_str()))
        {
            return vec![self.render_target(pattern.as_str(), datetime)];
        }

        let rendered_month = self.render_target("${yyyyMM}", datetime);
        let rendered_day = self.render_target("${yyyyMMdd}", datetime);
        let rendered_week = self.render_target("${yyyyww}", datetime);

        available_targets
            .iter()
            .filter(|target| {
                target.ends_with(rendered_month.as_str())
                    || target.ends_with(rendered_day.as_str())
                    || target.ends_with(rendered_week.as_str())
            })
            .cloned()
            .collect()
    }

    fn do_range_sharding(
        &self,
        available_targets: &[String],
        lower: &ShardingValue,
        upper: &ShardingValue,
    ) -> Vec<String> {
        let Some(lower) = lower.as_datetime() else {
            return available_targets.to_vec();
        };
        let Some(upper) = upper.as_datetime() else {
            return available_targets.to_vec();
        };
        self.range_targets(available_targets, lower, upper)
    }

    fn algorithm_type(&self) -> &str {
        "time_range"
    }
}

fn contains_time_placeholder(value: &str) -> bool {
    value.contains("${yyyyMM}") || value.contains("${yyyyMMdd}") || value.contains("${yyyyww}")
}

fn add_months(datetime: DateTime<FixedOffset>, steps: i64) -> DateTime<FixedOffset> {
    let mut year = datetime.year();
    let mut month = datetime.month() as i64 + steps;
    while month <= 0 {
        month += 12;
        year -= 1;
    }
    while month > 12 {
        month -= 12;
        year += 1;
    }
    let day = datetime.day().min(days_in_month(year, month as u32));
    datetime
        .timezone()
        .with_ymd_and_hms(
            year,
            month as u32,
            day,
            datetime.hour(),
            datetime.minute(),
            datetime.second(),
        )
        .single()
        .expect("valid shifted month")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::{TimeGranularity, TimeRangeShardingAlgorithm};
    use crate::algorithm::{ShardingAlgorithm, ShardingValue};

    #[test]
    fn time_range_routes_month_and_range() {
        let offset = FixedOffset::east_opt(0).expect("offset");
        let algorithm = TimeRangeShardingAlgorithm {
            granularity: TimeGranularity::Month,
            pre_create_periods: 0,
            retention_periods: 12,
        };
        let pattern = vec!["ai.log_${yyyyMM}".to_string()];

        let exact = algorithm.do_sharding(
            &pattern,
            &ShardingValue::DateTime(offset.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap()),
        );
        assert_eq!(exact, vec!["ai.log_202603".to_string()]);

        let range = algorithm.do_range_sharding(
            &pattern,
            &ShardingValue::DateTime(offset.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()),
            &ShardingValue::DateTime(offset.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()),
        );
        assert_eq!(
            range,
            vec!["ai.log_202602".to_string(), "ai.log_202603".to_string()]
        );
    }
}
