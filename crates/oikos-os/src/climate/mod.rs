//! The climate engine's hub side: the retained documents and the read view.

pub(crate) mod hub;
pub mod store;

use std::sync::Mutex;

pub use store::{Applied, ClimateStore, ZoneDocs};

/// What the hub keeps while `[climate] enabled`: the topic prefix and the retained documents.
pub struct ClimateHub {
    pub(crate) prefix: String,
    pub(crate) store: Mutex<ClimateStore>,
}

impl ClimateHub {
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.trim_end_matches('/').to_owned(),
            store: Mutex::new(ClimateStore::default()),
        }
    }
}
