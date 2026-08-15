mod api;
mod capability;
mod config;
mod notebook;
mod source;
mod storage;
mod types;

use std::sync::Arc;

use config::Config;
use storage::Store;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .init();

    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!("config: {e}");
            std::process::exit(1);
        }
    };

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let state = Arc::new(api::AppState {
        store: Store::new(&cfg.s3),
    });

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("bind {addr}: {e}");
            std::process::exit(1);
        }
    };

    tracing::info!("analyzer listening on {addr}");

    if let Err(e) = axum::serve(listener, api::routes(state))
        .with_graceful_shutdown(shutdown())
        .await
    {
        tracing::error!("serve: {e}");
        std::process::exit(1);
    }
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
