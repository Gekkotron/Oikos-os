//! The embedded web UI: the index, the SPA fallback and the JSON 404 under `/api`.
//! Needs `trunk build` in `crates/oikos-web` for the embedded-page assertions.

use oikos_os::config::Config;
use oikos_os::http::router;
use oikos_os::hub::Hub;
use reqwest::StatusCode;
use reqwest::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderMap};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// Serves a broker-less hub on an ephemeral port and fetches `path`.
async fn get(path: &str) -> (StatusCode, Option<String>, String) {
    let (status, headers, body) = fetch(path).await;
    let content_type = headers
        .get(CONTENT_TYPE)
        .map(|v| v.to_str().expect("content type").to_owned());
    (status, content_type, body)
}

async fn fetch(path: &str) -> (StatusCode, HeaderMap, String) {
    let (tx, _publisher) = mpsc::channel(8);
    let hub = Hub::new(&Config::default(), tx);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(axum::serve(listener, router(hub)).into_future());
    let response = reqwest::get(format!("http://{addr}{path}"))
        .await
        .expect("GET");
    let status = response.status();
    let headers = response.headers().clone();
    (status, headers, response.text().await.expect("body"))
}

#[tokio::test]
async fn root_returns_the_embedded_index_or_a_missing_page() {
    let (status, _, body) = get("/").await;
    let built = status == StatusCode::OK && body.contains("<!doctype html>");
    let missing = status == StatusCode::SERVICE_UNAVAILABLE && body.contains("has not been built");
    assert!(built || missing, "unexpected response: {status} {body}");
}

#[tokio::test]
async fn unknown_path_falls_back_to_index() {
    let (status, content_type, body) = get("/entity/0xabc").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<!doctype html>"), "{body}");
    assert!(content_type.is_some_and(|t| t.starts_with("text/html")));
}

#[tokio::test]
async fn api_fallback_is_not_html() {
    let (status, content_type, _) = get("/api/v1/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(content_type.is_some_and(|t| t.contains("json")));
}

#[tokio::test]
async fn index_is_never_cached_and_hashed_assets_are_immutable() {
    let (status, headers, body) = fetch("/").await;
    if status != StatusCode::OK {
        return; // dist not built: nothing to cache
    }
    assert_eq!(headers[CACHE_CONTROL], "no-cache");
    let asset = body
        .split('"')
        .find(|s| s.ends_with(".wasm") || s.ends_with(".js"))
        .expect("a script or wasm reference in index.html");
    let (status, headers, _) = fetch(asset).await;
    assert_eq!(status, StatusCode::OK, "{asset}");
    assert_eq!(
        headers[CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
}
