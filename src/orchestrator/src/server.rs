//! HTTP API. Bound to loopback only — never `0.0.0.0`.

use crate::models::Capture;
use crate::queue::{Queue, PLATFORM};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    queue: Arc<Mutex<Queue>>,
}

impl AppState {
    pub fn new(queue: Queue) -> Self {
        Self {
            queue: Arc::new(Mutex::new(queue)),
        }
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        Ok(Self::new(Queue::open_in_memory()?))
    }
}

/// One message as the extension observed it. Field names are the wire contract
/// with the extension — see README § API.
#[derive(Debug, Deserialize)]
pub struct CaptureMessage {
    pub chat_id: String,
    pub from: String,
    pub text: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CaptureBatch {
    pub messages: Vec<CaptureMessage>,
}

#[derive(Debug, Default, Serialize)]
pub struct CaptureReport {
    /// Messages in the request body.
    pub accepted: usize,
    /// Messages that became new queue entries.
    pub queued: usize,
    /// Messages already known — WhatsApp re-renders, so this is expected traffic.
    pub duplicates: usize,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/capture", post(capture))
        .with_state(state)
}

async fn capture(
    State(state): State<AppState>,
    Json(batch): Json<CaptureBatch>,
) -> Result<Json<CaptureReport>, StatusCode> {
    let queue = state.queue.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut report = CaptureReport {
        accepted: batch.messages.len(),
        ..Default::default()
    };

    for message in batch.messages {
        let capture = Capture {
            platform: PLATFORM.to_string(),
            chat_id: message.chat_id,
            sender: message.from,
            text: message.text,
            arrived_at: message.timestamp,
        };

        match queue.enqueue(&capture) {
            Ok(crate::models::EnqueueOutcome::Inserted(_)) => report.queued += 1,
            Ok(crate::models::EnqueueOutcome::Duplicate) => report.duplicates += 1,
            Err(e) => {
                tracing::error!(error = %e, "failed to enqueue capture");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    Ok(Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn batch_body(text: &str) -> Body {
        Body::from(format!(
            r#"{{"messages":[{{
                 "chat_id":"98912xxxxxxx@c.us",
                 "from":"+98912xxxxxxx",
                 "text":"{text}",
                 "timestamp":"2026-08-01T03:00:00Z"
               }}]}}"#
        ))
    }

    async fn post_capture(app: axum::Router, text: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/capture")
                    .header("content-type", "application/json")
                    .body(batch_body(text))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn posting_a_batch_queues_each_message() {
        let state = AppState::in_memory().unwrap();

        let (status, body) = post_capture(router(state), "can you do Friday?").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["accepted"], 1);
        assert_eq!(body["queued"], 1);
        assert_eq!(body["duplicates"], 0);
    }

    #[tokio::test]
    async fn re_posting_the_same_message_is_reported_as_a_duplicate() {
        let state = AppState::in_memory().unwrap();

        post_capture(router(state.clone()), "can you do Friday?").await;
        let (_, body) = post_capture(router(state), "can you do Friday?").await;

        assert_eq!(body["accepted"], 1);
        assert_eq!(body["queued"], 0);
        assert_eq!(body["duplicates"], 1);
    }
}
