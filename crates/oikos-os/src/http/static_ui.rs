//! The placeholder page served for every path the API does not claim, until the web UI lands.

use axum::response::Html;

const INDEX: &str = include_str!("../../assets/index.html");

pub(super) async fn index() -> Html<&'static str> {
    Html(INDEX)
}
