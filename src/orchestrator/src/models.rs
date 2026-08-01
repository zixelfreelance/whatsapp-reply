//! Data models for orchestrator

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Where an action sits in the capture → draft → approve → send lifecycle.
///
/// Variants are added as tests demand them; see `docs/ARCHITECTURE.md` for the
/// full state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    #[serde(rename = "captured")]
    Captured,
}

impl ActionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionStatus::Captured => "captured",
        }
    }
}

impl std::fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An inbound message as the extension observed it.
///
/// `arrived_at` is the message's real arrival time and is threaded through the
/// pipeline unchanged — nothing re-stamps it at projection time. The rate
/// governor's `min_age_seconds` is meaningless otherwise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
    pub platform: String,
    pub chat_id: String,
    pub sender: String,
    pub text: String,
    pub arrived_at: DateTime<Utc>,
}

/// A queued action — one inbound message and everything that happens to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub chat_id: String,
    pub sender: String,
    pub inbound_text: String,
    pub status: ActionStatus,
    pub arrived_at: DateTime<Utc>,
}

/// Result of offering a capture to the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Inserted(String),
    Duplicate,
}
