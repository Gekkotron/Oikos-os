//! The embedded web UI (`crates/oikos-web/dist`, built by `trunk build`), served for every
//! path the API does not claim. Unknown paths get `index.html` so the client-side router can
//! resolve them.

use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../oikos-web/dist/"]
struct Dist;

const INDEX: &str = "index.html";
const NOT_BUILT: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
    <title>Oikos-os</title></head><body><p>The Oikos-os web UI has not been built. \
    Run <code>trunk build --release</code> in <code>crates/oikos-web</code>.</p></body></html>";

pub(super) async fn index(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty()
        && path != INDEX
        && let Some(file) = Dist::get(path)
    {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache = if is_hashed(path) {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return file_response(mime.as_ref(), cache, file.data.into_owned());
    }
    match Dist::get(INDEX) {
        Some(file) => file_response(
            "text/html; charset=utf-8",
            "no-cache",
            file.data.into_owned(),
        ),
        None => (StatusCode::SERVICE_UNAVAILABLE, Html(NOT_BUILT)).into_response(),
    }
}

fn file_response(content_type: &str, cache: &'static str, body: Vec<u8>) -> Response {
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// Whether the file name carries a Trunk content hash: a `-` followed by at least eight hex
/// digits (`oikos-web-836a2c4c498c2ec4_bg.wasm`, `main-c920ca43256fdcb9.css`).
fn is_hashed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('-')
        .skip(1)
        .any(|rest| rest.bytes().take_while(u8::is_ascii_hexdigit).count() >= 8)
}

#[cfg(test)]
mod tests {
    use super::is_hashed;

    #[test]
    fn trunk_hashes_are_detected() {
        assert!(is_hashed("oikos-web-836a2c4c498c2ec4_bg.wasm"));
        assert!(is_hashed("main-c920ca43256fdcb9.css"));
        assert!(!is_hashed("index.html"));
        assert!(!is_hashed("oikos-web.wasm"));
        assert!(!is_hashed("my-favicon.ico"));
    }
}
