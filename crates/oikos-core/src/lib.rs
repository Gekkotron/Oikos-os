#![forbid(unsafe_code)]
//! Pure domain of Oikos-os: the model shared by the hub, the API and the web UI.
//! No I/O, no async; compiles to native and `wasm32-unknown-unknown`.

pub mod model;

pub mod discovery;
pub mod state;
pub mod template;
