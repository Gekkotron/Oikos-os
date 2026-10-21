#![forbid(unsafe_code)]
//! Wire types shared by the hub and the WebAssembly UI: views over the registry, the
//! snapshot, WebSocket events and REST bodies. Pure data, no I/O, no clock.

mod alerts;
mod events;
mod flows;
mod history;
mod snapshot;
mod views;
mod virtual_entities;

pub use alerts::{AlertAction, AlertChannel, AlertSeverity, AlertView, parse_history};
pub use events::{ApiError, CommandResponse, ConnectionStatus, OverridePatch, WsEvent};
pub use flows::{
    FlowView, FlowWriteOutcome, FlowsConfigView, RunStatus, RunView, SchedulePreview, StepOutcome,
    StepView, TriggerInfo,
};
pub use history::{
    EnergyKind, HistoryDaily, HistoryRange, HistorySeries, HistoryToday, LastChange, LastChangeItem,
};
pub use snapshot::{Snapshot, area_views, device_views, entity_views, snapshot};
pub use views::{AreaView, BridgeView, DeviceView, EntityView, IgnoredView};
pub use virtual_entities::{VirtualKind, VirtualSpec};
