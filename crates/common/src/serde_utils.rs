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
