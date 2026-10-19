//! Flows: n8n-style automations as a small graph of typed nodes.
//! Pure data and helpers, no I/O, so the hub, the API and the web canvas share one definition.

mod duration;
mod model;
mod scalar;

pub use duration::{DurationError, parse_duration};
pub use model::{
    CommandParams, DelayParams, Edge, Flow, FlowMode, IfParams, Node, NodeKind, Op, Position,
    TriggerStateParams,
};
pub use scalar::{CompareError, Scalar, compare};
