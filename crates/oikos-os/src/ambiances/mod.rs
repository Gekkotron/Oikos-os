//! The hub's copy of the retained ambiance definitions.

mod apply;
pub(crate) mod hub;
pub mod store;

pub use store::{AmbianceStore, StoreApplied};
