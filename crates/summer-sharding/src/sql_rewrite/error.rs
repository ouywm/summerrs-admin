use sea_orm::{DbErr, RuntimeErr};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SqlRewriteError>;

const DBERR_PREFIX: &str = "summer_sql_rewrite::";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SqlRewriteError {
    #[error("sql rewrite parse error: {0}")]
    Parse(String),
    #[error("sql rewrite error: {0}")]
    Rewrite(String),
    #[error("sql rewrite plugin `{plugin}` failed: {message}")]
    Plugin { plugin: String, message: String },
}

impl From<sqlparser::parser::ParserError> for SqlRewriteError {
    fn from(value: sqlparser::parser::ParserError) -> Self {
        Self::Parse(value.to_string())
    }
}

impl SqlRewriteError {
    pub fn from_db_err(err: &DbErr) -> Option<Self> {
        match err {
            DbErr::Custom(message) => Self::from_db_err_message(message),
            DbErr::Conn(RuntimeErr::Internal(message))
            | DbErr::Exec(RuntimeErr::Internal(message))
            | DbErr::Query(RuntimeErr::Internal(message)) => Self::from_db_err_message(message),
            _ => None,
        }
    }

    pub fn from_db_err_message(message: &str) -> Option<Self> {
        let payload = message.strip_prefix(DBERR_PREFIX)?;
        let (kind, encoded) = payload.split_once(':')?;
        match kind {
            "parse" => Some(Self::Parse(decode_component(encoded))),
            "rewrite" => Some(Self::Rewrite(decode_component(encoded))),
            "plugin" => {
                let (plugin, message) = encoded.split_once(':')?;
                Some(Self::Plugin {
                    plugin: decode_component(plugin),
                    message: decode_component(message),
                })
            }
            _ => None,
        }
    }

    fn to_db_err_payload(&self) -> String {
        match self {
            Self::Parse(message) => format!("{DBERR_PREFIX}parse:{}", encode_component(message)),
            Self::Rewrite(message) => {
                format!("{DBERR_PREFIX}rewrite:{}", encode_component(message))
            }
            Self::Plugin { plugin, message } => format!(
                "{DBERR_PREFIX}plugin:{}:{}",
                encode_component(plugin),
                encode_component(message)
            ),
        }
    }
}

impl From<SqlRewriteError> for DbErr {
    fn from(value: SqlRewriteError) -> Self {
        DbErr::Custom(value.to_db_err_payload())
    }
}

impl TryFrom<&DbErr> for SqlRewriteError {
    type Error = &'static str;

    fn try_from(value: &DbErr) -> std::result::Result<Self, Self::Error> {
        Self::from_db_err(value).ok_or("db error does not contain a sql rewrite payload")
    }
}

fn encode_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'%' => "%25".bytes().collect::<Vec<_>>(),
            b':' => "%3A".bytes().collect(),
            b'\n' => "%0A".bytes().collect(),
            b'\r' => "%0D".bytes().collect(),
            _ => vec![byte],
        })
        .map(char::from)
        .collect()
}

fn decode_component(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            let hex = &value[idx + 1..idx + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                decoded.push(char::from(byte));
                idx += 3;
                continue;
            }
        }
        decoded.push(char::from(bytes[idx]));
        idx += 1;
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::SqlRewriteError;

    #[test]
    fn sql_rewrite_error_roundtrips_through_db_err_custom_payload() {
        let error = SqlRewriteError::Plugin {
            plugin: "tenant:scope".to_string(),
            message: "boom:line1\nline2".to_string(),
        };

        let db_err = sea_orm::DbErr::from(error.clone());
        let decoded = SqlRewriteError::from_db_err(&db_err).expect("decode sql rewrite error");

        assert_eq!(decoded, error);
    }

    #[test]
    fn sql_rewrite_error_does_not_claim_foreign_db_err_payloads() {
        let db_err = sea_orm::DbErr::Custom("plain custom error".to_string());
        assert!(SqlRewriteError::from_db_err(&db_err).is_none());
    }
}
