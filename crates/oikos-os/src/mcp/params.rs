//! The parameter and result types of the MCP tools: documented, JSON-schema'd wire shapes.
//! Results are objects (the MCP `structuredContent` must be one), so lists are wrapped.

use oikos_api::{
    AreaView, BridgeView, ConnectionStatus, DeviceView, EntityView, IgnoredView, OverridePatch,
};
use oikos_core::model::{Area, Command, Kind, UiOverride};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which entities `list_entities` returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Include {
    /// What the app shows: no diagnostic or config entity, none disabled by default, none
    /// folded into another entity, none hidden by its override.
    #[default]
    Primary,
    /// Every entity.
    All,
}

/// Default page size of `list_entities`.
pub const DEFAULT_LIMIT: usize = 100;
/// Largest page `list_entities` returns.
pub const MAX_LIMIT: usize = 500;
/// Default number of devices and of entities `search` returns.
pub const DEFAULT_SEARCH_LIMIT: usize = 20;

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListDevicesParams {
    /// Only devices with at least one entity in this area (an area id from `list_areas`).
    pub area: Option<String>,
    /// Case-insensitive substring matched against the friendly name, the model and the vendor.
    pub query: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListEntitiesParams {
    /// Only entities in this area (an area id from `list_areas`).
    pub area: Option<String>,
    /// Only entities shown as this kind, in snake_case (`light`, `switch`, `cover`, `climate`,
    /// `sensor`, `binary_sensor`, `alarm_panel`, `number`, `select`, `button`, `vacuum`,
    /// `water_heater`); a switch overridden as a light counts as a light.
    pub kind: Option<Kind>,
    /// Only the entities of this device (a device id from `list_devices`).
    pub device_id: Option<String>,
    /// Case-insensitive substring matched against the entity name and id only; device names,
    /// models and vendors are searched by `list_devices` and `search`.
    pub query: Option<String>,
    /// Only entities whose last state message is at or after this unix time, in seconds.
    pub changed_since: Option<u64>,
    /// `primary` (the default) hides diagnostic, config, disabled-by-default, folded and hidden
    /// entities; `all` lists every entity.
    pub include: Option<Include>,
    /// Page size: 100 by default, 500 at most.
    pub limit: Option<usize>,
    /// Entities to skip before the page, 0 by default. `total` counts the matches before paging.
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetEntityParams {
    /// The entity id, as `list_entities` returns it.
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetOverrideParams {
    /// The entity id, as `list_entities` returns it.
    pub id: String,
    /// The fields to change; the others keep their value and `null` clears one. Fields: `name`,
    /// `area`, `hidden`, `order`, `cardStyle`, `favorite`, `energyRole` (solar, grid,
    /// grid_export, battery, house, consumer, water), `energyInvert`, `securityHidden`,
    /// `energyIcon`, `energyMeter`, `homePinned`, `securityAction`, `securityAsk`, `ungrouped`,
    /// `asLight`, `lightOnPreset` (white, warm, red, green, blue), `lightOnBrightness` (1 to
    /// 255), `climatePresets`.
    pub patch: OverridePatch,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClearOverrideParams {
    /// The entity id, as `list_entities` returns it.
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SendCommandParams {
    /// The entity id, as `list_entities` returns it.
    pub id: String,
    /// The command, tagged by `type`: `turn_on` (optional `brightness` 0-255, `color` {h, s},
    /// `effect`), `turn_off`, `toggle`, `set_position` {position 0-100}, `open`, `close`,
    /// `stop`, `set_temperature` {temperature}, `set_hvac_mode` {mode}, `set_fan_mode` {mode},
    /// `set_number` {value}, `select_option` {option}, `press`, `arm` {mode, code?}, `disarm`
    /// {code?} and the vacuum and water-heater commands the entity's capabilities list.
    pub command: Command,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetAreasParams {
    /// The whole list of rooms; it replaces the declared areas. Each has an `id`, a `name` and
    /// optionally an `icon`, an `order` and the app's `groupNames` and `groupFavorites`.
    pub areas: Vec<Area>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Case-insensitive substring matched against device friendly names, models and vendors
    /// and against entity names and ids.
    pub query: String,
    /// At most this many devices and this many entities: 20 by default, 500 at most.
    pub limit: Option<usize>,
}

/// The bridge and the broker link.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BridgeStatus {
    pub bridge: BridgeView,
    pub connection: ConnectionStatus,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AreaList {
    pub areas: Vec<AreaView>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeviceList {
    pub devices: Vec<DeviceView>,
}

/// One page of entities and the number of matches before paging.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EntityPage {
    pub total: usize,
    pub items: Vec<EntityView>,
}

/// An entity view with what the view leaves out (the wire shape of the hub's `EntityDetail`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct EntityDetailView {
    #[serde(flatten)]
    pub entity: EntityView,
    /// The expanded discovery config: topics, templates and payload constants.
    pub bindings: Value,
    /// The override retained for the entity, verbatim; `null` when there is none.
    pub raw_override: Option<UiOverride>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    pub devices: Vec<DeviceView>,
    pub entities: Vec<EntityView>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct IgnoredList {
    pub ignored: Vec<IgnoredView>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::model::{ENERGY_ROLES, LIGHT_ON_PRESETS};

    /// The `patch` documentation is written by hand: it must name every app field and value.
    #[test]
    fn set_override_patch_doc_names_every_field_and_value() {
        let schema = schemars::schema_for!(SetOverrideParams);
        let description = schema.as_value()["properties"]["patch"]["description"]
            .as_str()
            .expect("patch is documented")
            .to_owned();
        for name in UiOverride::FIELDS
            .iter()
            .chain(ENERGY_ROLES)
            .chain(LIGHT_ON_PRESETS)
        {
            assert!(description.contains(name), "`{name}` is missing");
        }
    }

    #[test]
    fn include_defaults_to_primary_and_reads_lowercase() {
        assert_eq!(Include::default(), Include::Primary);
        assert_eq!(
            serde_json::from_str::<Include>("\"all\"").expect("all"),
            Include::All
        );
        assert!(serde_json::from_str::<Include>("\"All\"").is_err());
    }
}
