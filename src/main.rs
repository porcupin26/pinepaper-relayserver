use std::net::SocketAddr;
#[tokio::main(worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = std::env::var("PINEPAPER_EXCHANGE_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!(
        "PinePaper Exchange listening on {} (no document storage)",
        listener.local_addr()?
    );
    let relay = pinepaper_exchange::Relay::default();
    axum::serve(
        listener,
        pinepaper_exchange::router(relay.clone())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Install shutdown signal handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
        relay.shutdown();
    })
    .await?;
    Ok(())
}
