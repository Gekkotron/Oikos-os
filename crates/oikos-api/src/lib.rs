#![forbid(unsafe_code)]
//! Wire types shared by the hub and the WebAssembly UI: views over the registry, the
//! snapshot, WebSocket events and REST bodies. Pure data, no I/O, no clock.

mod alerts;
mod dashboards;
mod events;
mod flows;
mod groups;
mod history;
mod snapshot;
mod views;
mod virtual_entities;

pub use alerts::{AlertAction, AlertChannel, AlertSeverity, AlertView, parse_history};
pub use dashboards::{
    Dashboard, DashboardReorder, DashboardView, HOUSE_ID, HOUSE_NAME, Span, Widget, WidgetConfig,
    WidgetKind, allowed_spans, decode_dashboard,
};
pub use events::{ApiError, CommandResponse, ConnectionStatus, OverridePatch, WsEvent};
pub use flows::{
    FlowView, FlowWriteOutcome, FlowsConfigView, RunStatus, RunView, SchedulePreview, StepOutcome,
    StepView, TriggerInfo,
};
pub use groups::{GroupMemberView, GroupSpec, GroupView};
pub use history::{
    EnergyKind, HistoryDaily, HistoryRange, HistorySeries, HistoryToday, LastChange, LastChangeItem,
};
pub use snapshot::{Snapshot, area_views, device_views, entity_views, snapshot};
pub use views::{
    AreaView, BridgeView, DeviceView, EntityView, IgnoredView, UpdateView, mask_secret,
};
pub use virtual_entities::{VirtualKind, VirtualSpec};
