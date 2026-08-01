//! SQLite-backed action queue.

use crate::models::{Action, ActionStatus, Capture, EnqueueOutcome};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS actions (
    id            TEXT PRIMARY KEY,
    platform      TEXT NOT NULL CHECK (platform = 'whatsapp'),
    content_hash  TEXT NOT NULL UNIQUE,
    chat_id       TEXT NOT NULL,
    sender        TEXT NOT NULL,
    inbound_text  TEXT NOT NULL,
    status        TEXT NOT NULL,
    arrived_at    TEXT NOT NULL
);
"#;

/// Stable identity of an inbound message.
///
/// Derived only from what the message *is* — never from wall-clock at insert
/// time — so re-observing the same message always collides.
fn content_hash(capture: &Capture) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for field in [
        capture.platform.as_str(),
        capture.chat_id.as_str(),
        capture.sender.as_str(),
        capture.text.as_str(),
        &capture.arrived_at.to_rfc3339(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update(b"\x1f"); // unit separator — keeps fields unambiguous
    }
    format!("{:x}", hasher.finalize())
}

/// The only platform this queue will accept. See `docs/ESTATE_LESSONS.md` §4.
pub const PLATFORM: &str = "whatsapp";

pub struct Queue {
    conn: Connection,
}

impl Queue {
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn enqueue(&self, capture: &Capture) -> Result<EnqueueOutcome> {
        // Checked here as well as by the CHECK constraint, because the insert
        // below uses OR IGNORE — which would swallow a constraint violation as
        // a no-op and report it as a duplicate. A foreign capture must be loud.
        if capture.platform != PLATFORM {
            anyhow::bail!(
                "refusing capture from platform {:?}: this queue accepts {:?} only",
                capture.platform,
                PLATFORM
            );
        }

        let id = Uuid::new_v4().to_string();
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO actions
                 (id, platform, content_hash, chat_id, sender, inbound_text, status, arrived_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id,
                capture.platform,
                content_hash(capture),
                capture.chat_id,
                capture.sender,
                capture.text,
                ActionStatus::Captured.as_str(),
                capture.arrived_at.to_rfc3339(),
            ],
        )?;

        if inserted == 0 {
            Ok(EnqueueOutcome::Duplicate)
        } else {
            Ok(EnqueueOutcome::Inserted(id))
        }
    }

    pub fn count(&self) -> Result<i64> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM actions", [], |row| row.get(0))?;
        Ok(n)
    }

    pub fn get(&self, id: &str) -> Result<Option<Action>> {
        // Columns are read as raw values and converted afterwards so a bad
        // timestamp surfaces as an error. Defaulting to `Utc::now()` here
        // would silently re-stamp arrival time — the exact bug that breaks
        // both replay determinism and the governor's min_age check.
        let row: Option<(String, String, String, String, String)> = self
            .conn
            .query_row(
                "SELECT id, chat_id, sender, inbound_text, arrived_at
                 FROM actions WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((id, chat_id, sender, inbound_text, arrived_at)) = row else {
            return Ok(None);
        };

        Ok(Some(Action {
            id,
            chat_id,
            sender,
            inbound_text,
            status: ActionStatus::Captured,
            arrived_at: DateTime::parse_from_rfc3339(&arrived_at)?.with_timezone(&Utc),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample_capture() -> Capture {
        Capture {
            platform: "whatsapp".into(),
            chat_id: "98912xxxxxxx@c.us".into(),
            sender: "+98912xxxxxxx".into(),
            text: "can you do Friday?".into(),
            arrived_at: Utc.with_ymd_and_hms(2026, 8, 1, 3, 0, 0).unwrap(),
        }
    }

    #[test]
    fn enqueuing_a_capture_stores_it_as_a_captured_action() {
        let q = Queue::open_in_memory().unwrap();

        let outcome = q.enqueue(&sample_capture()).unwrap();

        let id = match outcome {
            EnqueueOutcome::Inserted(id) => id,
            EnqueueOutcome::Duplicate => panic!("first enqueue must insert, not dedupe"),
        };

        let action = q.get(&id).unwrap().expect("action should be readable back");
        assert_eq!(action.chat_id, "98912xxxxxxx@c.us");
        assert_eq!(action.inbound_text, "can you do Friday?");
        assert_eq!(action.status, ActionStatus::Captured);
    }

    #[test]
    fn re_enqueuing_an_identical_message_is_a_no_op() {
        // WhatsApp Web re-renders its message list constantly (read receipts,
        // presence, reactions, virtualised scroll), so a MutationObserver
        // re-emits the same message many times. Without dedup a duplicate
        // capture becomes a duplicate draft, a duplicate Telegram card, and
        // the same reply sent twice.
        let q = Queue::open_in_memory().unwrap();

        q.enqueue(&sample_capture()).unwrap();
        let second = q.enqueue(&sample_capture()).unwrap();

        assert_eq!(second, EnqueueOutcome::Duplicate);
        assert_eq!(q.count().unwrap(), 1);
    }

    #[test]
    fn a_different_message_in_the_same_chat_is_not_a_duplicate() {
        let q = Queue::open_in_memory().unwrap();
        q.enqueue(&sample_capture()).unwrap();

        let mut other = sample_capture();
        other.text = "actually, make it Saturday".into();
        let outcome = q.enqueue(&other).unwrap();

        assert!(matches!(outcome, EnqueueOutcome::Inserted(_)));
        assert_eq!(q.count().unwrap(), 2);
    }

    #[test]
    fn a_capture_from_another_platform_is_rejected_not_coerced() {
        // On 2026-07-12 a LinkedIn sensor wrote private DMs into another
        // repo's lake. Every sensor in this estate forks the same spine,
        // which is how it happened. A non-whatsapp capture is an error,
        // never silently accepted.
        let q = Queue::open_in_memory().unwrap();

        let mut foreign = sample_capture();
        foreign.platform = "linkedin".into();

        assert!(q.enqueue(&foreign).is_err());
        assert_eq!(q.count().unwrap(), 0);
    }

    #[test]
    fn arrival_time_survives_the_round_trip_unchanged() {
        // min_age_seconds in the rate governor is meaningless if arrival
        // time is re-stamped anywhere in the pipeline.
        let q = Queue::open_in_memory().unwrap();
        let capture = sample_capture();

        let id = match q.enqueue(&capture).unwrap() {
            EnqueueOutcome::Inserted(id) => id,
            EnqueueOutcome::Duplicate => panic!("expected an insert"),
        };

        let action = q.get(&id).unwrap().unwrap();
        assert_eq!(action.arrived_at, capture.arrived_at);
    }

    #[test]
    fn a_file_backed_queue_still_dedupes_after_a_restart() {
        // The orchestrator is long-lived but restarts; dedup must survive it,
        // or a restart replays every message still on screen.
        let dir = std::env::temp_dir().join(format!("wr-queue-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");

        let first = Queue::open(&path).unwrap();
        first.enqueue(&sample_capture()).unwrap();
        drop(first);

        let reopened = Queue::open(&path).unwrap();
        let outcome = reopened.enqueue(&sample_capture()).unwrap();

        assert_eq!(outcome, EnqueueOutcome::Duplicate);
        assert_eq!(reopened.count().unwrap(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}
