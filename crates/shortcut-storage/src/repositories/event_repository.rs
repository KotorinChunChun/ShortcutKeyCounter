use rusqlite::{params, Transaction};
use shortcut_core::model::RecordableEvent;

use crate::{format_timestamp, StorageError};

/// 詳細イベントをtransactionへ追加する。
pub fn insert_event(
    transaction: &Transaction<'_>,
    event: &RecordableEvent,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO shortcut_events (occurred_at, shortcut, application)
         VALUES (?1, ?2, ?3)",
        params![
            format_timestamp(event.occurred_at),
            &event.shortcut,
            &event.application
        ],
    )?;
    Ok(())
}
