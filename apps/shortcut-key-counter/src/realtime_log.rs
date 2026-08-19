use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{unbounded, Receiver, Sender};

use chrono::NaiveDateTime;
use serde::Serialize;

const MAX_ENTRIES: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RealtimeLogEntry {
    pub sequence: u64,
    pub occurred_at: String,
    pub shortcut: String,
    pub application: String,
    pub outcome: String,
}

#[derive(Clone, Default)]
pub struct RealtimeLog {
    entries: Arc<Mutex<VecDeque<RealtimeLogEntry>>>,
    next_sequence: Arc<AtomicU64>,
    subscribers: Arc<Mutex<Vec<Sender<u64>>>>,
}

impl RealtimeLog {
    pub fn push(
        &self,
        occurred_at: NaiveDateTime,
        shortcut: String,
        application: String,
        outcome: impl Into<String>,
    ) {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        if let Ok(mut entries) = self.entries.lock() {
            entries.push_back(RealtimeLogEntry {
                sequence,
                occurred_at: occurred_at.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
                shortcut,
                application,
                outcome: outcome.into(),
            });
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.send(sequence).is_ok());
        }
    }

    pub fn subscribe(&self) -> Receiver<u64> {
        let (sender, receiver) = unbounded();
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(sender);
        }
        receiver
    }

    pub fn entries_after(&self, after_sequence: u64, limit: usize) -> Vec<RealtimeLogEntry> {
        self.entries
            .lock()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry.sequence > after_sequence)
                    .take(limit.min(MAX_ENTRIES))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn subscriber_is_notified_for_every_log_entry() {
        let log = RealtimeLog::default();
        let receiver = log.subscribe();
        let at = NaiveDate::from_ymd_opt(2026, 8, 16)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        log.push(at, "Ctrl+C".into(), "Code.exe".into(), "recorded");
        log.push(at, "Ctrl+V".into(), "Code.exe".into(), "recorded");
        assert_eq!(receiver.recv().unwrap(), 1);
        assert_eq!(receiver.recv().unwrap(), 2);
    }

    #[test]
    fn ring_buffer_returns_only_new_entries() {
        let log = RealtimeLog::default();
        let at = NaiveDate::from_ymd_opt(2026, 8, 16)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        log.push(at, "Ctrl+C".into(), "Code.exe".into(), "recorded");
        log.push(at, "Ctrl+V".into(), "Code.exe".into(), "recorded");
        let entries = log.entries_after(1, 100);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].shortcut, "Ctrl+V");
    }
}
