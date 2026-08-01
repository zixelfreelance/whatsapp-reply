//! Data models for orchestrator

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "executing")]
    Executing,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "failed")]
    Failed,
}

impl std::fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionStatus::Pending => write!(f, "pending"),
            ActionStatus::Executing => write!(f, "executing"),
            ActionStatus::Done => write!(f, "done"),
            ActionStatus::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub action: String,                    // "send_whatsapp", "send_linkedin", etc.
    pub params: serde_json::Value,         // { to, message, ... }
    pub status: ActionStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Action {
    pub fn new(action: String, params: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            action,
            params,
            status: ActionStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub action_id: String,
    pub dom_json: serde_json::Value,  // { refs: [{ref, role, label}] }
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Result {
    pub id: String,
    pub action_id: String,
    pub success: bool,
    pub error_msg: Option<String>,
    pub claude_response: Option<serde_json::Value>,
    pub commands: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
}
