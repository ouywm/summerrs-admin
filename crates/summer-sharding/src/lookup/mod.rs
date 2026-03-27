use std::{collections::BTreeMap, sync::Arc};

use chrono::{FixedOffset, TimeZone};
use parking_lot::RwLock;
use sea_orm::{QueryResult, Value};
use sqlparser::ast::{Expr, Statement as AstStatement, Value as SqlValue};

use crate::{
    algorithm::{ShardingValue, parse_datetime_string},
    config::LookupIndexConfig,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LookupCacheKey {
    table_key: (String, String),
    lookup_value: String,
}

impl LookupCacheKey {
    fn new(definition: &LookupDefinition, lookup_value: &ShardingValue) -> Self {
        Self {
            table_key: (
                definition.logic_table_key.clone(),
                definition.lookup_column_key.clone(),
            ),
            lookup_value: value_key(lookup_value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LookupDefinition {
    pub logic_table: String,
    pub lookup_column: String,
    pub lookup_table: String,
    pub sharding_column: String,
    logic_table_key: String,
    lookup_column_key: String,
}

impl LookupDefinition {
    pub fn from_config(config: &LookupIndexConfig) -> Self {
        let logic_table_key = normalize(&config.logic_table);
        let lookup_column_key = normalize(&config.lookup_column);
        Self {
            logic_table: config.logic_table.clone(),
            lookup_column: config.lookup_column.clone(),
            lookup_table: config.lookup_table.clone(),
            sharding_column: config.sharding_column.clone(),
            logic_table_key,
            lookup_column_key,
        }
    }

    fn key(&self) -> (String, String) {
        (self.logic_table_key.clone(), self.lookup_column_key.clone())
    }

    pub fn lookup_select_sql(&self) -> String {
        format!(
            "SELECT {} FROM {} WHERE {} = $1 LIMIT 1",
            self.sharding_column, self.lookup_table, self.lookup_column
        )
    }

    pub fn lookup_upsert_sql(&self) -> String {
        format!(
            "INSERT INTO {} ({}, {}) VALUES ($1, $2) \
             ON CONFLICT ({}) DO UPDATE SET {} = EXCLUDED.{}",
            self.lookup_table,
            self.lookup_column,
            self.sharding_column,
            self.lookup_column,
            self.sharding_column,
            self.sharding_column
        )
    }

    pub fn lookup_delete_sql(&self) -> String {
        format!(
            "DELETE FROM {} WHERE {} = $1",
            self.lookup_table, self.lookup_column
        )
    }
}

pub trait LookupStore: Send + Sync + 'static {
    fn resolve(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue>;

    fn insert(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
        sharding_value: &ShardingValue,
    );

    fn remove(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue>;
}

#[derive(Debug, Default)]
pub struct InMemoryLookupStore {
    entries: RwLock<BTreeMap<LookupCacheKey, ShardingValue>>,
}

impl LookupStore for InMemoryLookupStore {
    fn resolve(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue> {
        self.entries
            .read()
            .get(&LookupCacheKey::new(definition, lookup_value))
            .cloned()
    }

    fn insert(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
        sharding_value: &ShardingValue,
    ) {
        self.entries.write().insert(
            LookupCacheKey::new(definition, lookup_value),
            sharding_value.clone(),
        );
    }

    fn remove(
        &self,
        definition: &LookupDefinition,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue> {
        self.entries
            .write()
            .remove(&LookupCacheKey::new(definition, lookup_value))
    }
}

pub struct LookupIndex {
    cache: RwLock<BTreeMap<LookupCacheKey, ShardingValue>>,
    definitions: RwLock<BTreeMap<(String, String), Arc<LookupDefinition>>>,
    store: Arc<dyn LookupStore>,
}

impl LookupIndex {
    pub fn with_store(store: Arc<dyn LookupStore>) -> Self {
        Self {
            cache: RwLock::new(BTreeMap::new()),
            definitions: RwLock::new(BTreeMap::new()),
            store,
        }
    }

    pub fn register(&self, definition: LookupDefinition) {
        let definition = Arc::new(definition);
        let key = definition.key();
        self.definitions
            .write()
            .entry(key)
            .or_insert_with(|| definition.clone());
    }

    fn definition_for(
        &self,
        logic_table: &str,
        lookup_column: &str,
    ) -> Option<Arc<LookupDefinition>> {
        let key = (normalize(logic_table), normalize(lookup_column));
        self.definitions.read().get(&key).cloned()
    }

    pub fn resolve(
        &self,
        logic_table: &str,
        lookup_column: &str,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue> {
        let definition = self.definition_for(logic_table, lookup_column)?;
        let cache_key = LookupCacheKey::new(&definition, lookup_value);
        if let Some(value) = self.cache.read().get(&cache_key) {
            return Some(value.clone());
        }

        self.store.resolve(&definition, lookup_value).map(|value| {
            self.cache.write().insert(cache_key, value.clone());
            value
        })
    }

    pub fn insert(
        &self,
        logic_table: &str,
        lookup_column: &str,
        lookup_value: &ShardingValue,
        sharding_value: ShardingValue,
    ) {
        let definition = match self.definition_for(logic_table, lookup_column) {
            Some(value) => value,
            None => return,
        };
        let cache_key = LookupCacheKey::new(&definition, lookup_value);
        self.store
            .insert(&definition, lookup_value, &sharding_value);
        self.cache.write().insert(cache_key, sharding_value);
    }

    pub fn remove(
        &self,
        logic_table: &str,
        lookup_column: &str,
        lookup_value: &ShardingValue,
    ) -> Option<ShardingValue> {
        let definition = self.definition_for(logic_table, lookup_column)?;
        let cache_key = LookupCacheKey::new(&definition, lookup_value);
        self.cache.write().remove(&cache_key);
        self.store.remove(&definition, lookup_value)
    }
}

impl Default for LookupIndex {
    fn default() -> Self {
        Self::with_store(Arc::new(InMemoryLookupStore::default()))
    }
}

impl std::fmt::Debug for LookupIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookupIndex")
            .field(
                "definitions",
                &self.definitions.read().keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

fn normalize(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn value_key(value: &ShardingValue) -> String {
    match value {
        ShardingValue::Int(number) => format!("int:{number}"),
        ShardingValue::Str(text) => format!("str:{text}"),
        ShardingValue::DateTime(datetime) => format!("dt:{}", datetime.to_rfc3339()),
        ShardingValue::Null => "null".to_string(),
    }
}

pub fn split_qualified_name(value: &str) -> (Option<String>, String) {
    match value.split_once('.') {
        Some((schema, table)) => (Some(schema.to_string()), table.to_string()),
        None => (None, value.to_string()),
    }
}

pub fn normalize_column(value: &str) -> String {
    value.to_ascii_lowercase()
}

pub fn sharding_value_to_sea_value(value: &ShardingValue) -> Value {
    match value {
        ShardingValue::Int(number) => Value::BigInt(Some(*number)),
        ShardingValue::Str(text) => Value::String(Some(text.clone())),
        ShardingValue::DateTime(datetime) => Value::String(Some(datetime.to_rfc3339())),
        ShardingValue::Null => Value::String(None),
    }
}

pub fn query_result_to_sharding_value(row: &QueryResult, column: &str) -> Option<ShardingValue> {
    if let Ok(Some(value)) = row.try_get::<Option<i64>>("", column) {
        return Some(ShardingValue::Int(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<i32>>("", column) {
        return Some(ShardingValue::Int(i64::from(value)));
    }
    if let Ok(Some(value)) = row.try_get::<Option<chrono::DateTime<FixedOffset>>>("", column) {
        return Some(ShardingValue::DateTime(value));
    }
    if let Ok(Some(value)) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>>("", column) {
        return Some(ShardingValue::DateTime(value.fixed_offset()));
    }
    if let Ok(Some(value)) = row.try_get::<Option<chrono::NaiveDateTime>>("", column) {
        if let Some(datetime) =
            FixedOffset::east_opt(0).and_then(|offset| offset.from_local_datetime(&value).single())
        {
            return Some(ShardingValue::DateTime(datetime));
        }
    }
    if let Ok(Some(value)) = row.try_get::<Option<String>>("", column) {
        return parse_datetime_string(value.as_str())
            .map(ShardingValue::DateTime)
            .or_else(|| Some(ShardingValue::Str(value)));
    }
    if let Ok(None) = row.try_get::<Option<String>>("", column) {
        return Some(ShardingValue::Null);
    }
    None
}

pub fn update_assignment_value(
    ast: &AstStatement,
    values: Option<&sea_orm::Values>,
    column: &str,
) -> Option<ShardingValue> {
    let AstStatement::Update { assignments, .. } = ast else {
        return None;
    };
    assignments.iter().find_map(|assignment| {
        let target_column = match &assignment.target {
            sqlparser::ast::AssignmentTarget::ColumnName(object_name) => {
                object_name.0.last().map(|ident| ident.value.as_str())?
            }
            sqlparser::ast::AssignmentTarget::Tuple(_) => return None,
        };
        if !target_column.eq_ignore_ascii_case(column) {
            return None;
        }
        expr_to_sharding_value(&assignment.value, values)
    })
}

pub fn update_assigns_column(ast: &AstStatement, column: &str) -> bool {
    let AstStatement::Update { assignments, .. } = ast else {
        return false;
    };
    assignments.iter().any(|assignment| {
        let target_column = match &assignment.target {
            sqlparser::ast::AssignmentTarget::ColumnName(object_name) => {
                object_name.0.last().map(|ident| ident.value.as_str())
            }
            sqlparser::ast::AssignmentTarget::Tuple(_) => None,
        };
        target_column.is_some_and(|target| target.eq_ignore_ascii_case(column))
    })
}

fn expr_to_sharding_value(expr: &Expr, values: Option<&sea_orm::Values>) -> Option<ShardingValue> {
    match expr {
        Expr::Value(value) => sql_value_to_sharding_value(value, values),
        Expr::Cast { expr, .. } => expr_to_sharding_value(expr, values),
        Expr::Nested(expr) => expr_to_sharding_value(expr, values),
        Expr::UnaryOp { op, expr } if *op == sqlparser::ast::UnaryOperator::Minus => {
            expr_to_sharding_value(expr, values)?
                .as_i64()
                .map(|value| ShardingValue::Int(-value))
        }
        _ => None,
    }
}

fn sql_value_to_sharding_value(
    value: &SqlValue,
    values: Option<&sea_orm::Values>,
) -> Option<ShardingValue> {
    match value {
        SqlValue::Number(number, _) => number.parse::<i64>().ok().map(ShardingValue::Int),
        SqlValue::SingleQuotedString(text)
        | SqlValue::DoubleQuotedString(text)
        | SqlValue::EscapedStringLiteral(text)
        | SqlValue::UnicodeStringLiteral(text)
        | SqlValue::NationalStringLiteral(text) => parse_datetime_string(text)
            .map(ShardingValue::DateTime)
            .or_else(|| Some(ShardingValue::Str(text.clone()))),
        SqlValue::Boolean(value) => Some(ShardingValue::Int(i64::from(*value))),
        SqlValue::Null => Some(ShardingValue::Null),
        SqlValue::Placeholder(name) => placeholder_value(name.as_str(), values),
        _ => None,
    }
}

fn placeholder_value(name: &str, values: Option<&sea_orm::Values>) -> Option<ShardingValue> {
    let values = values?;
    if let Some(index) = name
        .strip_prefix('$')
        .and_then(|value| value.parse::<usize>().ok())
    {
        return values
            .0
            .get(index.saturating_sub(1))
            .and_then(sea_value_to_sharding_value);
    }
    if name == "?" {
        return values.0.first().and_then(sea_value_to_sharding_value);
    }
    None
}

fn sea_value_to_sharding_value(value: &Value) -> Option<ShardingValue> {
    match value {
        Value::BigInt(value) => value.map(ShardingValue::Int),
        Value::Int(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::SmallInt(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::TinyInt(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::BigUnsigned(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::Unsigned(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::SmallUnsigned(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::TinyUnsigned(value) => value.map(|value| ShardingValue::Int(i64::from(value))),
        Value::String(value) => value.as_ref().and_then(|value| {
            parse_datetime_string(value)
                .map(ShardingValue::DateTime)
                .or_else(|| Some(ShardingValue::Str(value.to_string())))
        }),
        Value::ChronoDateTimeWithTimeZone(value) => value.map(ShardingValue::DateTime),
        Value::ChronoDateTimeUtc(value) => {
            value.map(|value| ShardingValue::DateTime(value.fixed_offset()))
        }
        Value::ChronoDateTimeLocal(value) => {
            value.map(|value| ShardingValue::DateTime(value.fixed_offset()))
        }
        Value::ChronoDateTime(value) => value.as_ref().and_then(|value| {
            FixedOffset::east_opt(0).and_then(|offset| {
                offset
                    .from_local_datetime(value)
                    .single()
                    .map(ShardingValue::DateTime)
            })
        }),
        Value::ChronoDate(value) => value.and_then(|value| {
            value.and_hms_opt(0, 0, 0).and_then(|datetime| {
                FixedOffset::east_opt(0).and_then(|offset| {
                    offset
                        .from_local_datetime(&datetime)
                        .single()
                        .map(ShardingValue::DateTime)
                })
            })
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{algorithm::ShardingValue, config::LookupIndexConfig};

    use super::{InMemoryLookupStore, LookupDefinition, LookupIndex};

    fn fixture_config() -> LookupIndexConfig {
        LookupIndexConfig {
            logic_table: "ai.log".to_string(),
            lookup_column: "trace_id".to_string(),
            lookup_table: "ai.log_lookup_trace_id".to_string(),
            sharding_column: "create_time".to_string(),
        }
    }

    #[test]
    fn cache_resolves_after_registering_definition() {
        let index = LookupIndex::default();
        let config = fixture_config();
        index.register(LookupDefinition::from_config(&config));

        let lookup_value = ShardingValue::Str("req-42".to_string());
        let shard_key = ShardingValue::Int(42);

        index.insert(
            &config.logic_table,
            &config.lookup_column,
            &lookup_value,
            shard_key.clone(),
        );

        assert_eq!(
            index.resolve(&config.logic_table, &config.lookup_column, &lookup_value),
            Some(shard_key.clone())
        );
    }

    #[test]
    fn store_used_when_cache_is_cleared() {
        let store = Arc::new(InMemoryLookupStore::default());
        let index = LookupIndex::with_store(store.clone());
        let config = fixture_config();
        index.register(LookupDefinition::from_config(&config));

        let lookup_value = ShardingValue::Str("req-99".to_string());
        let shard_key = ShardingValue::Str("persisted".to_string());

        index.insert(
            &config.logic_table,
            &config.lookup_column,
            &lookup_value,
            shard_key.clone(),
        );

        index.cache.write().clear();

        assert_eq!(
            index.resolve(&config.logic_table, &config.lookup_column, &lookup_value),
            Some(shard_key.clone())
        );
    }

    #[test]
    fn remove_clears_cache_and_store() {
        let index = LookupIndex::default();
        let config = fixture_config();
        index.register(LookupDefinition::from_config(&config));

        let lookup_value = ShardingValue::Str("req-55".to_string());
        let shard_key = ShardingValue::Int(55);

        index.insert(
            &config.logic_table,
            &config.lookup_column,
            &lookup_value,
            shard_key.clone(),
        );

        assert_eq!(
            index.remove(&config.logic_table, &config.lookup_column, &lookup_value),
            Some(shard_key.clone())
        );

        assert_eq!(
            index.resolve(&config.logic_table, &config.lookup_column, &lookup_value),
            None
        );
    }
}
