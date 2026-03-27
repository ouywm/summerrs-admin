use async_trait::async_trait;
use parking_lot::Mutex;

use crate::{
    cdc::{CdcOperation, CdcRecord, CdcSink},
    error::Result,
};

#[derive(Debug, Default)]
pub struct TableSink {
    rows: Mutex<Vec<CdcRecord>>,
}

impl TableSink {
    pub fn rows(&self) -> Vec<CdcRecord> {
        self.rows.lock().clone()
    }
}

#[async_trait]
impl CdcSink for TableSink {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize> {
        self.rows.lock().extend_from_slice(records);
        Ok(records.len())
    }

    async fn apply_change(&self, record: &CdcRecord) -> Result<()> {
        let mut rows = self.rows.lock();
        match record.operation {
            CdcOperation::Delete => rows.retain(|candidate| candidate.key != record.key),
            CdcOperation::Insert | CdcOperation::Update | CdcOperation::Snapshot => {
                rows.retain(|candidate| candidate.key != record.key);
                rows.push(record.clone());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::cdc::{CdcOperation, CdcRecord, CdcSink, TableSink};

    fn record(key: &str, payload: serde_json::Value, operation: CdcOperation) -> CdcRecord {
        CdcRecord {
            table: "ai.log".to_string(),
            key: key.to_string(),
            payload,
            operation,
            source_lsn: None,
        }
    }

    #[tokio::test]
    async fn table_sink_write_batch_and_apply_change_keep_latest_state() {
        let sink = TableSink::default();

        sink.write_batch(&[record("1", serde_json::json!({"v":1}), CdcOperation::Snapshot)])
            .await
            .expect("write batch");
        sink.apply_change(&record(
            "1",
            serde_json::json!({"v":2}),
            CdcOperation::Update,
        ))
        .await
        .expect("update");
        sink.apply_change(&record("2", serde_json::json!({"v":3}), CdcOperation::Insert))
            .await
            .expect("insert");
        sink.apply_change(&record("1", serde_json::json!({}), CdcOperation::Delete))
            .await
            .expect("delete");

        let rows = sink.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "2");
        assert_eq!(rows[0].payload["v"], 3);
    }
}
