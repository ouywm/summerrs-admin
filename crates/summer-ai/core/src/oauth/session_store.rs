use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

#[derive(Debug)]
struct SessionEntry<S> {
    value: S,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct SessionStore<S> {
    ttl: Duration,
    sessions: RwLock<HashMap<String, SessionEntry<S>>>,
}

impl<S> SessionStore<S>
where
    S: Clone,
{
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn set(&self, session_id: String, value: S) {
        let expires_at = Utc::now()
            + chrono::Duration::from_std(self.ttl)
                .expect("session store ttl should fit within chrono::Duration");
        let entry = SessionEntry { value, expires_at };

        self.sessions.write().await.insert(session_id, entry);
    }

    pub async fn get(&self, session_id: &str) -> Option<S> {
        let now = Utc::now();

        {
            let sessions = self.sessions.read().await;
            if let Some(entry) = sessions.get(session_id) {
                if entry.expires_at > now {
                    return Some(entry.value.clone());
                }
            } else {
                return None;
            }
        }

        self.sessions.write().await.remove(session_id);
        None
    }

    pub async fn remove(&self, session_id: &str) -> Option<S> {
        self.sessions
            .write()
            .await
            .remove(session_id)
            .map(|entry| entry.value)
    }
}
