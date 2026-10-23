//! Measurements of the page chrome that floating panels sit under.

use leptos::prelude::*;

/// Measures where a floating panel starts: just under the sticky top bar.
pub(crate) fn top_offset() -> f64 {
    document()
        .query_selector(".topbar")
        .ok()
        .flatten()
        .map_or(0.0, |bar| bar.get_bounding_client_rect().bottom() + 8.0)
}
