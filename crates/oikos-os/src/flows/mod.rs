//! The hub's flow engine: the store of decoded definitions and their run history.

pub mod engine;
mod hub;
mod store;

pub use store::{FlowStore, StoreApplied};

/// What the hub asks of the flow engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineRequest {
    /// The definitions changed: re-read them and re-arm.
    Reload,
    /// Execute a run the hub already created (a manual run).
    Run { flow_id: String, run_id: String },
}
