//! The parameter and result types of the MCP tools: documented, JSON-schema'd wire shapes.
//! Results are objects (the MCP `structuredContent` must be one), so lists are wrapped.

use oikos_api::{
    AreaView, BridgeView, ConnectionStatus, DeviceView, EntityView, IgnoredView, OverridePatch,
    RunStatus,
};
use oikos_core::flow::Flow;
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFlowParams {
    /// The flow id, as `list_flows` returns it.
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetFlowParams {
    /// The whole flow; it replaces any flow with the same `id` (retained on
    /// `oikos/automation/<id>`) and is checked before it is saved, so a refusal names what to
    /// fix. Fields: `id` (1 to 64 characters of `a-z`, `0-9`, `_` and `-`), `name` (free text),
    /// `description` (optional), `enabled` (default `true`; a disabled flow is kept but never
    /// runs), `mode`, `nodes` and `edges`.
    ///
    /// `mode` is `single` (the default: a trigger that fires while a run is active is ignored)
    /// or `restart` (it cancels the active run and starts over). Use `restart` for "N minutes
    /// after the last ..." (motion, door, button): each new trigger restarts the wait, so the
    /// final action only happens once the events stop.
    ///
    /// Each node is `{"id": "<unique in the flow>", "type": "<type>", "params": {...}}` with an
    /// optional `name` label. The four types:
    ///
    /// - `trigger.state` starts a run when an entity's state changes. params: `entity` (an id
    ///   from `search` or `list_entities`), optional `to` (only when the new value equals it)
    ///   and `from` (only when the previous value equalled it), as JSON values such as `true`,
    ///   `"ON"` or `21.5`. A flow needs at least one trigger, and the run carries that entity's
    ///   new value.
    /// - `if` branches on a comparison. params: `op` (`eq`, `ne`, `gt`, `lt`, `gte`, `lte`),
    ///   `value` (the constant to compare with) and optional `entity`. Without `entity` it
    ///   compares the triggering entity's live value, the one that fired the run; with it, that
    ///   entity's current value. It has two output ports, `true` and `false`.
    /// - `delay` waits. params: `for`, a duration such as `90s`, `5m` or `1h30m` (units `s`, `m`,
    ///   `h`, combined without spaces).
    /// - `command` sends a command to an entity. params: `entity` (an id) and `command`, the same
    ///   JSON as `send_command`'s, e.g. `{"type": "turn_on", "brightness": 200}` or
    ///   `{"type": "turn_off"}`. The entity must exist and support the command.
    ///
    /// `edges` is a list of `{"from": "<node id>", "to": "<node id>"}` followed from a trigger.
    /// An edge leaving an `if` must also carry `"port": "true"` or `"port": "false"` (the branch
    /// it belongs to; omit a branch to do nothing there); edges leaving any other node have no
    /// `port`.
    ///
    /// Example, LED on at motion and off five minutes after the last motion: `mode: "restart"`,
    /// nodes `motion` (`trigger.state`, entity the sensor), `moving` (`if`, `op: "eq"`,
    /// `value: true`), `on` (`command`, `turn_on`), `wait` (`delay`, `for: "5m"`) and `off`
    /// (`command`, `turn_off`); edges `motion` to `moving`, `moving` port `true` to `on`,
    /// `moving` port `false` to `wait`, `wait` to `off`.
    pub flow: Flow,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeleteFlowParams {
    /// The flow id, as `list_flows` returns it.
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetFlowEnabledParams {
    /// The flow id, as `list_flows` returns it.
    pub id: String,
    /// `true` lets the flow run; `false` keeps it saved but stops it from running.
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunFlowParams {
    /// The flow id, as `list_flows` returns it.
    pub id: String,
}

/// The flows, one summary each; `get_flow` has the definition and the run history.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FlowList {
    pub flows: Vec<FlowSummary>,
}

/// One flow in a list.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FlowSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Why the flow cannot run, when it cannot (a stored flow that no longer validates).
    pub invalid: Option<String>,
    /// How many nodes the flow has.
    pub nodes: usize,
    /// The most recent run; `None` when the flow never ran.
    pub last_run: Option<LastRun>,
}

/// How the latest run of a flow went.
#[derive(Debug, Serialize, JsonSchema)]
pub struct LastRun {
    pub status: RunStatus,
    /// Unix seconds.
    pub started_at: u64,
}

/// The id of a flow that `delete_flow` removed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Deleted {
    pub id: String,
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

    /// The `flow` documentation is written by hand: it must name every node type, both `if`
    /// ports and both modes, since it is all an assistant reads before writing a flow.
    #[test]
    fn set_flow_doc_names_every_node_type_and_mode() {
        let schema = schemars::schema_for!(SetFlowParams);
        let description = schema.as_value()["properties"]["flow"]["description"]
            .as_str()
            .expect("flow is documented")
            .to_owned();
        for needle in [
            "trigger.state",
            "if",
            "delay",
            "command",
            "restart",
            "single",
            "true",
            "false",
        ] {
            assert!(description.contains(needle), "`{needle}` is missing");
        }
    }
}
