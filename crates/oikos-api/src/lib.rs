#![forbid(unsafe_code)]
//! Wire types shared by the hub and the WebAssembly UI: views over the registry, the
//! snapshot, WebSocket events and REST bodies. Pure data, no I/O, no clock.

mod events;
mod snapshot;
mod views;

pub use events::{ApiError, CommandResponse, ConnectionStatus, OverridePatch, WsEvent};
pub use snapshot::{Snapshot, area_views, device_views, entity_views, snapshot};
pub use views::{AreaView, BridgeView, DeviceView, EntityView, IgnoredView};
