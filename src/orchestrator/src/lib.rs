//! whatsapp-reply orchestrator — HTTP API for message capture + Claude reply loop

pub mod server;
pub mod models;
pub mod queue;
pub mod executor;
pub mod claude;
pub mod trace;

pub use models::{Action, ActionStatus, Capture, EnqueueOutcome};
pub use queue::Queue;

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
        let state = server::AppState::new(Queue::open(&self.db_path)?);

        // Loopback only. This process holds private message content; it must
        // never be reachable off-host.
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(%addr, db = %self.db_path, "orchestrator listening");

        axum::serve(listener, server::router(state)).await?;
        Ok(())
    }
}
