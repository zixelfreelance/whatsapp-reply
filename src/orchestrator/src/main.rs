use orchestrator::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("whatsapp-reply orchestrator starting");

    // Port 8812 is allocated, not chosen — the estate coexists on one host.
    // Taken: captur 8787/8799, linkedin 8788, insta 8789, fiver 8790,
    // FPIOS 8810, freelance-index 8811.
    let server = Server::new("data/whatsapp-reply.db", 8812)?;
    server.run().await?;

    Ok(())
}
