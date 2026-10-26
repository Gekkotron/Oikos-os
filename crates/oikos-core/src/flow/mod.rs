//! Flows: n8n-style automations as a small graph of typed nodes.
//! Pure data and helpers, no I/O, so the hub, the API and the web canvas share one definition.

mod duration;
mod graph;
mod model;
mod scalar;
mod schedule;
pub mod sun;
pub mod template;
mod validate;

pub use duration::{DurationError, format_duration, parse_duration, pick_duration};
pub use model::{
    CommandParams, DelayParams, Edge, Flow, FlowEnableParams, FlowMode, IfParams, MAX_AVERAGE_SECS,
    Missing, Node, NodeKind, NotifyParams, Op, Position, SunWhich, TriggerScheduleParams,
    TriggerSignalParams, TriggerStateParams, TriggerSunParams, uses_sun_or_is_dark,
};
pub use scalar::{CompareError, Scalar, compare};
pub use schedule::{Schedule, ScheduleError};
pub use sun::{
    APPARENT_ALTITUDE_DEG, CIVIL_ALTITUDE_DEG, SunEvents, is_dark_at, sun_altitude, sun_events,
};
pub use template::{Lang, Reading, Template, TemplateError};
pub use validate::{
    FlowError, NOTIFY_CHANNELS, NOTIFY_SEVERITIES, is_valid_flow_id, is_valid_node_id,
    notify_channel_error, notify_severity_error, validate,
};

/// Shared fixture for the flow tests: the reference flow of the design.
#[cfg(test)]
pub(crate) fn reference_flow() -> Flow {
    Flow::decode(
        br#"{
      "id": "living_led_motion", "name": "Living LED on motion", "enabled": true, "mode": "restart",
      "nodes": [
        { "id": "motion", "type": "trigger.state", "params": { "entity": "m" } },
        { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
        { "id": "on", "type": "command", "params": { "entity": "l", "command": { "type": "turn_on" } } },
        { "id": "wait", "type": "delay", "params": { "for": "5m" } },
        { "id": "off", "type": "command", "params": { "entity": "l", "command": { "type": "turn_off" } } }
      ],
      "edges": [
        { "from": "motion", "to": "moving" },
        { "from": "moving", "port": "true", "to": "on" },
        { "from": "moving", "port": "false", "to": "wait" },
        { "from": "wait", "to": "off" }
      ]
    }"#,
    )
    .unwrap()
}
