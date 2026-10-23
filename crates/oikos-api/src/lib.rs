#![forbid(unsafe_code)]
//! Wire types shared by the hub and the WebAssembly UI: views over the registry, the
//! snapshot, WebSocket events and REST bodies. Pure data, no I/O, no clock.

mod alerts;
mod ambiances;
mod dashboards;
mod events;
mod flows;
mod groups;
mod history;
mod home;
mod snapshot;
mod views;
mod virtual_entities;
mod weather;

pub use alerts::{AlertAction, AlertChannel, AlertSeverity, AlertView, parse_history};
pub use ambiances::{
    AMBIANCE_ICONS, ActionReport, Ambiance, AmbianceAction, AmbianceView, ApplyReport, Category,
    CategoryAction, CategoryPlan, ClimateAction, DeviceAction, FlowAction, FlowDo, Outcome,
    Problem, Target, already_there, category_members, is_valid_ambiance_id, plan_category,
    validate_ambiance,
};
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
pub use home::HomeSettings;
pub use snapshot::{Snapshot, area_views, device_views, entity_views, snapshot};
pub use views::{
    AreaView, BridgeView, DeviceView, EntityView, IgnoredView, UpdateView, mask_secret,
};
pub use virtual_entities::{VirtualKind, VirtualSpec};
pub use weather::{
    WeatherCondition, WeatherCurrent, WeatherDay, WeatherPlace, WeatherToday, WeatherView,
};
