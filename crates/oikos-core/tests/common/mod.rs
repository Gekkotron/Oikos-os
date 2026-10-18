#![allow(dead_code)]
//! Shared test helpers: load recorded broker fixtures.

use std::path::PathBuf;

/// Directory holding the recorded fixtures.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/"))
}

/// Read a fixture file (path relative to the fixtures directory) as UTF-8.
pub fn fixture(rel: &str) -> String {
    let path = fixture_dir().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()))
}
