//! whatsapp-reply orchestrator — HTTP API for message capture + Claude reply loop

pub mod server;
pub mod models;
pub mod queue;
pub mod executor;
pub mod claude;
pub mod trace;

pub use models::{Action, ActionStatus, Snapshot};

/// Placeholder server struct (to be implemented in Week 1)
pub struct Server {
    db_path: String,
    port: u16,
}

impl Server {
    pub fn new(db_path: &str, port: u16) -> anyhow::Result<Self> {
        Ok(Server {
            db_path: db_path.to_string(),
            port,
        })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Server running on port {} with db at {}", self.port, self.db_path);
        // TODO: Implement HTTP server
        std::future::pending().await
    }
}
