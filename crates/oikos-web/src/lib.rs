#![forbid(unsafe_code)]

pub mod app;
pub mod house;
pub mod house_state;
#[cfg(target_arch = "wasm32")]
pub mod ws;
