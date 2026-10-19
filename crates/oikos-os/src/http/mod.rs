//! The HTTP surface over the hub: `/health`, the JSON API under `/api/v1`, the WebSocket
//! stream at `/ws`, the MCP server at `/mcp` and the placeholder page for every other path.

mod api;
mod static_ui;
mod ws;

use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::routing::{any, get};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tower_http::cors::CorsLayer;

use crate::config::HttpConfig;
use crate::hub::Hub;
use crate::mcp;

/// The API, the WebSocket, the MCP server and the placeholder page, with CORS open to any
/// origin (no auth in v1). Unknown paths under `/api` answer a JSON 404; every other unknown
/// path gets the page. The MCP service is nested, so it takes precedence over the fallback.
pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/health", get(api::health))
        .nest("/api/v1", api::routes())
        .route("/api/{*rest}", any(api::not_found))
        .route("/ws", get(ws::upgrade))
        .nest_service("/mcp", mcp::mcp_service(Arc::clone(&hub)))
        .fallback(static_ui::index)
        .layer(CorsLayer::permissive())
        .with_state(hub)
}

/// Binds `cfg.bind` and serves `router` on a spawned task; the task only ends when aborted
/// or when the listener fails.
pub async fn serve(cfg: &HttpConfig, router: Router) -> anyhow::Result<JoinHandle<()>> {
    let listener = TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    let addr = listener.local_addr().context("local address")?;
    tracing::info!(%addr, "http listening");
    Ok(tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(error = %err, "http server stopped");
        }
    }))
}
