use crate::{cdc::CdcRecord, error::Result};

pub trait RowTransform: Send + Sync + 'static {
    fn transform(&self, record: CdcRecord) -> Result<CdcRecord>;

    fn descriptor(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

#[derive(Debug, Clone, Default)]
pub struct RowTransformer;

impl RowTransformer {
    pub fn map(
        &self,
        record: &CdcRecord,
        mut mapper: impl FnMut(&mut serde_json::Value),
    ) -> CdcRecord {
        let mut payload = record.payload.clone();
        mapper(&mut payload);
        CdcRecord {
            payload,
            ..record.clone()
        }
    }
}

impl RowTransform for RowTransformer {
    fn transform(&self, record: CdcRecord) -> Result<CdcRecord> {
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use crate::cdc::{CdcOperation, CdcRecord, RowTransform, RowTransformer};

    fn record() -> CdcRecord {
        CdcRecord {
            table: "ai.log".to_string(),
            key: "1".to_string(),
            payload: serde_json::json!({"tenant":"T-001","v":1}),
            operation: CdcOperation::Insert,
            source_lsn: Some("0/1".to_string()),
        }
    }

    #[test]
    fn row_transformer_map_rewrites_payload_without_touching_other_fields() {
        let transformer = RowTransformer;
        let input = record();

        let output = transformer.map(&input, |payload| {
            payload["tenant"] = serde_json::json!("T-002")
        });

        assert_eq!(output.key, input.key);
        assert_eq!(output.source_lsn, input.source_lsn);
        assert_eq!(output.payload["tenant"], "T-002");
        assert_eq!(input.payload["tenant"], "T-001");
    }

    #[test]
    fn row_transformer_transform_is_passthrough() {
        let transformer = RowTransformer;
        let input = record();

        let output = transformer.transform(input.clone()).expect("transform");

        assert_eq!(output, input);
    }
}
