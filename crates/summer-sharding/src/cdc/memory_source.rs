use std::collections::BTreeMap;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::{
    cdc::{CdcBatch, CdcRecord, CdcSource, CdcSubscribeRequest, CdcSubscription},
    error::{Result, ShardingError},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReplicationIdentity {
    slot: String,
    publication: String,
}

impl ReplicationIdentity {
    fn new(slot: &str, publication: &str) -> Self {
        Self {
            slot: slot.to_string(),
            publication: publication.to_string(),
        }
    }
}

#[derive(Debug, Default)]
pub struct InMemoryCdcSource {
    snapshots: RwLock<BTreeMap<String, Vec<CdcRecord>>>,
    changes: Mutex<Vec<CdcRecord>>,
    replication: RwLock<ReplicationIdentity>,
}

impl InMemoryCdcSource {
    pub fn new() -> Self {
        Self {
            snapshots: RwLock::new(BTreeMap::new()),
            changes: Mutex::new(Vec::new()),
            replication: RwLock::new(ReplicationIdentity::new(
                "summer_cdc_slot",
                "summer_cdc_pub",
            )),
        }
    }

    pub fn with_replication(self, slot: &str, publication: &str) -> Self {
        *self.replication.write() = ReplicationIdentity::new(slot, publication);
        self
    }

    pub fn with_snapshot(self, table: &str, records: Vec<CdcRecord>) -> Self {
        self.snapshots.write().insert(table.to_string(), records);
        self
    }

    pub fn with_change(self, record: CdcRecord) -> Self {
        self.changes.lock().push(record);
        self
    }
}

#[derive(Debug)]
struct InMemorySubscription {
    changes: Vec<CdcRecord>,
    offset: usize,
    position: Option<String>,
}

impl InMemorySubscription {
    fn new(changes: Vec<CdcRecord>, from_position: Option<String>) -> Self {
        let offset = from_position
            .as_ref()
            .and_then(|lsn| {
                changes.iter().position(|record| {
                    record
                        .source_lsn
                        .as_ref()
                        .map(|value| value > lsn)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(0);
        Self {
            changes,
            offset,
            position: from_position,
        }
    }
}

#[async_trait]
impl CdcSubscription for InMemorySubscription {
    async fn next_batch(&mut self, limit: usize) -> Result<CdcBatch> {
        let end = self
            .offset
            .saturating_add(limit.max(1))
            .min(self.changes.len());
        let records = if self.offset < end {
            self.changes[self.offset..end].to_vec()
        } else {
            Vec::new()
        };
        self.offset = end;
        if let Some(lsn) = records.last().and_then(|record| record.source_lsn.clone()) {
            self.position = Some(lsn);
        }
        Ok(CdcBatch {
            next_position: self.position.clone(),
            records,
        })
    }

    fn position(&self) -> Option<String> {
        self.position.clone()
    }
}

#[async_trait]
impl CdcSource for InMemoryCdcSource {
    async fn snapshot(&self, table: &str, cursor: Option<&str>, limit: i64) -> Result<CdcBatch> {
        let rows = self
            .snapshots
            .read()
            .get(table)
            .cloned()
            .unwrap_or_default();
        let start = cursor
            .map(|value| {
                rows.iter()
                    .position(|record| record.key == value)
                    .map(|index| index + 1)
                    .or_else(|| rows.iter().position(|record| record.key.as_str() > value))
                    .unwrap_or(rows.len())
            })
            .unwrap_or_default();
        let records = rows
            .into_iter()
            .skip(start)
            .take(limit.max(0) as usize)
            .collect::<Vec<_>>();
        let next_position = records.last().map(|record| record.key.clone());
        Ok(CdcBatch {
            records,
            next_position,
        })
    }

    async fn subscribe(&self, request: CdcSubscribeRequest) -> Result<Box<dyn CdcSubscription>> {
        let replication = self.replication.read().clone();
        if replication.slot != request.slot {
            return Err(ShardingError::Unsupported(format!(
                "replication slot mismatch, expected `{}`, got `{}`",
                replication.slot, request.slot
            )));
        }
        if replication.publication != request.publication {
            return Err(ShardingError::Unsupported(format!(
                "publication mismatch, expected `{}`, got `{}`",
                replication.publication, request.publication
            )));
        }

        let table_filter = request.source_tables;
        let changes = self
            .changes
            .lock()
            .iter()
            .filter(|record| table_filter.iter().any(|table| table == &record.table))
            .cloned()
            .collect::<Vec<_>>();
        Ok(Box::new(InMemorySubscription::new(
            changes,
            request.from_position,
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::cdc::{CdcOperation, CdcRecord, CdcSource, CdcSubscribeRequest, InMemoryCdcSource};

    #[tokio::test]
    async fn in_memory_source_subscribe_honors_slot_publication_and_position() {
        let source = InMemoryCdcSource::new()
            .with_replication("summer_cdc_slot", "summer_cdc_pub")
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "1".to_string(),
                payload: serde_json::json!({"v":1}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/1".to_string()),
            })
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "2".to_string(),
                payload: serde_json::json!({"v":2}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/2".to_string()),
            });

        let mut stream = source
            .subscribe(CdcSubscribeRequest {
                slot: "summer_cdc_slot".to_string(),
                publication: "summer_cdc_pub".to_string(),
                source_tables: vec!["ai.log".to_string()],
                from_position: Some("0/1".to_string()),
            })
            .await
            .expect("subscribe");
        let batch = stream.next_batch(10).await.expect("next batch");
        assert_eq!(batch.records.len(), 1);
        assert_eq!(batch.records[0].key, "2");
        assert_eq!(batch.next_position.as_deref(), Some("0/2"));
    }
}
