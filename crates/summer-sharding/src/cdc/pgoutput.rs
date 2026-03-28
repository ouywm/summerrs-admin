use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use bytes::Bytes;
use pgwire_replication::Lsn;
use serde_json::{Map, Number, Value as JsonValue};

use crate::{
    cdc::{CdcOperation, CdcRecord},
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
struct RelationColumn {
    name: String,
    type_oid: u32,
    is_key: bool,
}

#[derive(Debug, Clone)]
struct RelationMetadata {
    full_table_name: String,
    columns: Vec<RelationColumn>,
}

#[derive(Debug, Clone)]
enum TupleValue {
    Null,
    UnchangedToast,
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, Default)]
pub struct PgOutputDecoder {
    relations: BTreeMap<u32, RelationMetadata>,
    source_tables: BTreeSet<String>,
    primary_keys: BTreeMap<String, Vec<String>>,
}

impl PgOutputDecoder {
    pub fn with_primary_keys(
        source_tables: &[String],
        primary_keys: BTreeMap<String, Vec<String>>,
    ) -> Self {
        Self {
            relations: BTreeMap::new(),
            source_tables: source_tables.iter().cloned().collect(),
            primary_keys,
        }
    }

    pub fn decode_chunk(&mut self, data: &Bytes, lsn: Lsn) -> Result<Vec<CdcRecord>> {
        let mut input = Input::new(data.as_ref());
        let mut records = Vec::new();

        while !input.is_empty() {
            let tag = input.read_u8()?;
            match tag {
                b'R' => self.parse_relation(&mut input)?,
                b'Y' => self.parse_type(&mut input)?,
                b'I' => {
                    if let Some(record) = self.parse_insert(&mut input, lsn)? {
                        records.push(record);
                    }
                }
                b'U' => {
                    if let Some(record) = self.parse_update(&mut input, lsn)? {
                        records.push(record);
                    }
                }
                b'D' => {
                    if let Some(record) = self.parse_delete(&mut input, lsn)? {
                        records.push(record);
                    }
                }
                b'T' => self.parse_truncate(&mut input)?,
                b'O' => self.parse_origin(&mut input)?,
                other => {
                    return Err(ShardingError::Parse(format!(
                        "unsupported pgoutput message tag `{}`",
                        other as char
                    )));
                }
            }
        }

        Ok(records)
    }

    fn parse_relation(&mut self, input: &mut Input<'_>) -> Result<()> {
        let relation_id = input.read_u32()?;
        let namespace = input.read_cstring()?;
        let relation_name = input.read_cstring()?;
        let _replica_identity = input.read_u8()?;
        let column_count = input.read_u16()?;
        let mut columns = Vec::with_capacity(column_count as usize);

        for _ in 0..column_count {
            let flags = input.read_u8()?;
            let name = input.read_cstring()?;
            let type_oid = input.read_u32()?;
            let _type_modifier = input.read_i32()?;
            columns.push(RelationColumn {
                name,
                type_oid,
                is_key: flags & 1 == 1,
            });
        }

        let schema = if namespace.is_empty() {
            "pg_catalog".to_string()
        } else {
            namespace
        };
        self.relations.insert(
            relation_id,
            RelationMetadata {
                full_table_name: format!("{schema}.{relation_name}"),
                columns,
            },
        );
        Ok(())
    }

    fn parse_type(&mut self, input: &mut Input<'_>) -> Result<()> {
        let _type_oid = input.read_u32()?;
        let _namespace = input.read_cstring()?;
        let _name = input.read_cstring()?;
        Ok(())
    }

    fn parse_insert(&self, input: &mut Input<'_>, lsn: Lsn) -> Result<Option<CdcRecord>> {
        let relation_id = input.read_u32()?;
        let tuple_tag = input.read_u8()?;
        if tuple_tag != b'N' {
            return Err(ShardingError::Parse(format!(
                "insert message expected `N`, got `{}`",
                tuple_tag as char
            )));
        }
        let tuple = input.read_tuple()?;
        self.build_record(relation_id, CdcOperation::Insert, Some(&tuple), None, lsn)
    }

    fn parse_update(&self, input: &mut Input<'_>, lsn: Lsn) -> Result<Option<CdcRecord>> {
        let relation_id = input.read_u32()?;
        let mut old_tuple = None;
        let first_tag = input.read_u8()?;
        let next_tag = match first_tag {
            b'K' | b'O' => {
                old_tuple = Some(input.read_tuple()?);
                input.read_u8()?
            }
            b'N' => b'N',
            other => {
                return Err(ShardingError::Parse(format!(
                    "update message unexpected tuple tag `{}`",
                    other as char
                )));
            }
        };
        if next_tag != b'N' {
            return Err(ShardingError::Parse(format!(
                "update message expected `N`, got `{}`",
                next_tag as char
            )));
        }
        let new_tuple = input.read_tuple()?;
        self.build_record(
            relation_id,
            CdcOperation::Update,
            Some(&new_tuple),
            old_tuple.as_deref(),
            lsn,
        )
    }

    fn parse_delete(&self, input: &mut Input<'_>, lsn: Lsn) -> Result<Option<CdcRecord>> {
        let relation_id = input.read_u32()?;
        let tuple_tag = input.read_u8()?;
        if tuple_tag != b'K' && tuple_tag != b'O' {
            return Err(ShardingError::Parse(format!(
                "delete message expected `K` or `O`, got `{}`",
                tuple_tag as char
            )));
        }
        let tuple = input.read_tuple()?;
        self.build_record(relation_id, CdcOperation::Delete, Some(&tuple), None, lsn)
    }

    fn parse_truncate(&self, input: &mut Input<'_>) -> Result<()> {
        let relation_count = input.read_u32()?;
        let _options = input.read_u8()?;
        for _ in 0..relation_count {
            let _relation_id = input.read_u32()?;
        }
        Ok(())
    }

    fn parse_origin(&self, input: &mut Input<'_>) -> Result<()> {
        let _commit_lsn = input.read_i64()?;
        let _origin_name = input.read_cstring()?;
        Ok(())
    }

    fn build_record(
        &self,
        relation_id: u32,
        operation: CdcOperation,
        tuple: Option<&[TupleValue]>,
        fallback: Option<&[TupleValue]>,
        lsn: Lsn,
    ) -> Result<Option<CdcRecord>> {
        let Some(relation) = self.relations.get(&relation_id) else {
            return Err(ShardingError::Parse(format!(
                "missing relation metadata for relation id {relation_id}"
            )));
        };
        if !self
            .source_tables
            .contains(relation.full_table_name.as_str())
        {
            return Ok(None);
        }

        let row_payload = tuple_to_json(&relation.columns, tuple, fallback)?;
        let key = record_key(
            &relation.columns,
            &row_payload,
            self.primary_keys
                .get(relation.full_table_name.as_str())
                .map(Vec::as_slice),
        )?;
        Ok(Some(CdcRecord {
            table: relation.full_table_name.clone(),
            key,
            payload: JsonValue::Object(row_payload),
            operation,
            source_lsn: Some(lsn.to_string()),
        }))
    }
}

fn tuple_to_json(
    columns: &[RelationColumn],
    tuple: Option<&[TupleValue]>,
    fallback: Option<&[TupleValue]>,
) -> Result<Map<String, JsonValue>> {
    let tuple = tuple.ok_or_else(|| ShardingError::Parse("missing tuple data".to_string()))?;
    let mut map = Map::new();

    for (index, column) in columns.iter().enumerate() {
        let value = tuple.get(index).cloned().unwrap_or(TupleValue::Null);
        let fallback_value = fallback.and_then(|values| values.get(index));
        let json_value = match value {
            TupleValue::Null => JsonValue::Null,
            TupleValue::UnchangedToast => {
                let Some(fallback_value) = fallback_value else {
                    return Err(ShardingError::Unsupported(format!(
                        "pgoutput column `{}` is unchanged TOAST without previous row image; enable REPLICA IDENTITY FULL for safe updates",
                        column.name
                    )));
                };
                decode_value(column.type_oid, fallback_value.clone())?
            }
            other => decode_value(column.type_oid, other)?,
        };
        map.insert(column.name.clone(), json_value);
    }

    Ok(map)
}

fn record_key(
    columns: &[RelationColumn],
    payload: &Map<String, JsonValue>,
    primary_keys: Option<&[String]>,
) -> Result<String> {
    let key_columns = primary_keys
        .filter(|columns| !columns.is_empty())
        .map(|columns| columns.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            columns
                .iter()
                .filter(|column| column.is_key)
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>()
        });
    let key_columns = if key_columns.is_empty() {
        if payload.contains_key("id") {
            vec!["id"]
        } else {
            return Err(ShardingError::Parse(
                "pgoutput record has no key columns and no `id` fallback".to_string(),
            ));
        }
    } else {
        key_columns
    };

    Ok(key_columns
        .into_iter()
        .map(|column| json_key_part(payload.get(column).unwrap_or(&JsonValue::Null)))
        .collect::<Vec<_>>()
        .join(":"))
}

fn json_key_part(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => String::new(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => value.clone(),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}

fn decode_value(type_oid: u32, value: TupleValue) -> Result<JsonValue> {
    match value {
        TupleValue::Null => Ok(JsonValue::Null),
        TupleValue::Binary(bytes) => Ok(JsonValue::String(
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )),
        TupleValue::UnchangedToast => Err(ShardingError::Unsupported(
            "pgoutput unchanged TOAST value requires previous row image".to_string(),
        )),
        TupleValue::Text(text) => decode_text_value(type_oid, text),
    }
}

fn decode_text_value(type_oid: u32, text: String) -> Result<JsonValue> {
    match type_oid {
        16 => Ok(JsonValue::Bool(matches!(
            text.as_str(),
            "t" | "true" | "TRUE"
        ))),
        20 | 21 | 23 => text
            .parse::<i64>()
            .ok()
            .map(Number::from)
            .map(JsonValue::Number)
            .ok_or_else(|| ShardingError::Parse(format!("invalid integer value `{text}`"))),
        700 | 701 => Number::from_f64(
            text.parse::<f64>()
                .map_err(|_| ShardingError::Parse(format!("invalid float value `{text}`")))?,
        )
        .map(JsonValue::Number)
        .ok_or_else(|| ShardingError::Parse(format!("invalid float value `{text}`"))),
        114 | 3802 => {
            serde_json::from_str(text.as_str()).map_err(|err| ShardingError::Parse(err.to_string()))
        }
        _ => Ok(JsonValue::String(text)),
    }
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    fn read_u8(&mut self) -> Result<u8> {
        self.read_exact(1).map(|slice| slice[0])
    }

    fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i32(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_i64(&mut self) -> Result<i64> {
        let bytes = self.read_exact(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_cstring(&mut self) -> Result<String> {
        let remaining = &self.bytes[self.offset..];
        let end = remaining
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| {
                ShardingError::Parse("unterminated cstring in pgoutput message".to_string())
            })?;
        let value = std::str::from_utf8(&remaining[..end])
            .map_err(|err| ShardingError::Parse(err.to_string()))?
            .to_string();
        self.offset += end + 1;
        Ok(value)
    }

    fn read_tuple(&mut self) -> Result<Vec<TupleValue>> {
        let column_count = self.read_u16()?;
        let mut values = Vec::with_capacity(column_count as usize);
        for _ in 0..column_count {
            let tag = self.read_u8()?;
            let value = match tag {
                b'n' => TupleValue::Null,
                b'u' => TupleValue::UnchangedToast,
                b't' => {
                    let len = self.read_i32()?;
                    let bytes = self.read_exact(len as usize)?;
                    let text = std::str::from_utf8(bytes)
                        .map_err(|err| ShardingError::Parse(err.to_string()))?
                        .to_string();
                    TupleValue::Text(text)
                }
                b'b' => {
                    let len = self.read_i32()?;
                    TupleValue::Binary(self.read_exact(len as usize)?.to_vec())
                }
                other => {
                    return Err(ShardingError::Parse(format!(
                        "unsupported tuple value tag `{}`",
                        other as char
                    )));
                }
            };
            values.push(value);
        }
        Ok(values)
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.offset + len > self.bytes.len() {
            return Err(ShardingError::Parse(
                "truncated pgoutput message".to_string(),
            ));
        }
        let slice = &self.bytes[self.offset..self.offset + len];
        self.offset += len;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{RelationColumn, TupleValue, record_key, tuple_to_json};

    #[test]
    fn tuple_to_json_rejects_unchanged_toast_without_previous_row_image() {
        let columns = vec![RelationColumn {
            name: "payload".to_string(),
            type_oid: 3802,
            is_key: false,
        }];
        let error = tuple_to_json(&columns, Some(&[TupleValue::UnchangedToast]), None)
            .expect_err("missing previous row image should fail");

        assert!(error.to_string().contains("enable REPLICA IDENTITY FULL"));
    }

    #[test]
    fn tuple_to_json_reuses_previous_row_image_for_unchanged_toast() {
        let columns = vec![RelationColumn {
            name: "payload".to_string(),
            type_oid: 3802,
            is_key: false,
        }];
        let row = tuple_to_json(
            &columns,
            Some(&[TupleValue::UnchangedToast]),
            Some(&[TupleValue::Text(r#"{"name":"alpha"}"#.to_string())]),
        )
        .expect("row");

        assert_eq!(row.get("payload"), Some(&json!({"name":"alpha"})));
    }

    #[test]
    fn record_key_prefers_configured_primary_key_over_replica_identity_columns() {
        let columns = vec![
            RelationColumn {
                name: "id".to_string(),
                type_oid: 20,
                is_key: true,
            },
            RelationColumn {
                name: "tenant_id".to_string(),
                type_oid: 1043,
                is_key: true,
            },
            RelationColumn {
                name: "payload".to_string(),
                type_oid: 3802,
                is_key: true,
            },
        ];
        let payload = json!({
            "id": 42,
            "tenant_id": "T-001",
            "payload": { "name": "alpha" }
        })
        .as_object()
        .expect("object")
        .clone();

        let key = record_key(&columns, &payload, Some(&["id".to_string()])).expect("key");

        assert_eq!(key, "42");
    }
}
