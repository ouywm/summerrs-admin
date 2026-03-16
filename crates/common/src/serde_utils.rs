/// NaiveDateTime 序列化为 "YYYY-MM-DD HH:mm:ss" 格式
pub mod datetime_format {
    use chrono::NaiveDateTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }

    pub fn serialize_option<S>(
        date: &Option<NaiveDateTime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(date) => serializer.serialize_some(&date.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }
}

/// NaiveDate 序列化为 "YYYY-MM-DD" 格式
pub mod date_format {
    use chrono::NaiveDate;
    use serde::{self, Serializer};

    const FORMAT: &str = "%Y-%m-%d";

    pub fn serialize<S>(date: &NaiveDate, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&date.format(FORMAT).to_string())
    }

    pub fn serialize_option<S>(date: &Option<NaiveDate>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(date) => serializer.serialize_some(&date.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }
}

/// NaiveTime 序列化为 "HH:mm:ss" 格式
pub mod time_format {
    use chrono::NaiveTime;
    use serde::{self, Serializer};

    const FORMAT: &str = "%H:%M:%S";

    pub fn serialize<S>(time: &NaiveTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&time.format(FORMAT).to_string())
    }

    pub fn serialize_option<S>(time: &Option<NaiveTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match time {
            Some(time) => serializer.serialize_some(&time.format(FORMAT).to_string()),
            None => serializer.serialize_none(),
        }
    }
}

/// 百分比字段序列化保留两位小数
pub mod percent_f64 {
    use serde::{self, Serializer};

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64((*value * 100.0).round() / 100.0)
    }
}

/// 百分比字段（f32）序列化保留两位小数
pub mod percent_f32 {
    use serde::{self, Serializer};

    pub fn serialize<S>(value: &f32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f32((*value * 100.0).round() / 100.0)
    }
}
