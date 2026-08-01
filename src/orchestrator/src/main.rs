use orchestrator::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("whatsapp-reply orchestrator starting");

    let server = Server::new("sqlite:data/whatsapp-reply.db", 9999)?;
    server.run().await?;

    Ok(())
}
