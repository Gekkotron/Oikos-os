//! The eleven tools of the house (the six flow tools are in `flow_tools.rs`). Read tools work on one snapshot (one read lock, released before any
//! await); write tools log their arguments and go through the hub's writers.

use std::collections::BTreeSet;

use oikos_api::{CommandResponse, DeviceView, EntityView};
use oikos_core::model::AreasDocument;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde::Serialize;
use serde_json::Value;

use super::params::{
    AreaList, BridgeStatus, ClearOverrideParams, DEFAULT_LIMIT, DEFAULT_SEARCH_LIMIT, DeviceList,
    EntityDetailView, EntityPage, GetEntityParams, IgnoredList, Include, ListDevicesParams,
    ListEntitiesParams, MAX_LIMIT, SearchParams, SearchResult, SendCommandParams, SetAreasParams,
    SetOverrideParams,
};
use super::{OikosMcp, ToolError};

pub(super) type ToolResult<T> = Result<Json<T>, ToolError>;

#[tool_router(vis = "pub(super)")]
impl OikosMcp {
    /// The Zigbee2MQTT bridge (version, coordinator, permit join, online or not) and the
    /// state of the link to the MQTT broker.
    #[tool(name = "bridge_info", annotations(read_only_hint = true))]
    async fn bridge_info(&self) -> Json<BridgeStatus> {
        let snapshot = self.hub.snapshot();
        Json(BridgeStatus {
            bridge: snapshot.bridge,
            connection: snapshot.connection,
        })
    }

    /// The rooms, each with the number of entities in it: the declared areas first, then the
    /// ones only carried by entities, then `Unassigned` when any entity has no room.
    #[tool(name = "list_areas", annotations(read_only_hint = true))]
    async fn list_areas(&self) -> Json<AreaList> {
        Json(AreaList {
            areas: self.hub.snapshot().areas,
        })
    }

    /// Replaces the declared rooms (retained on `oikos/ui/_areas`) and returns the resulting
    /// list.
    #[tool(name = "set_areas", annotations(idempotent_hint = true))]
    async fn set_areas(
        &self,
        Parameters(params): Parameters<SetAreasParams>,
    ) -> ToolResult<AreaList> {
        log_write("set_areas", &params);
        let doc = AreasDocument {
            areas: params.areas,
        };
        let areas = self.hub.set_areas(doc).await?;
        Ok(Json(AreaList { areas }))
    }

    /// The Zigbee devices with what their diagnostic entities report (availability, battery,
    /// link quality), optionally only those in an area or matching a name, model or vendor.
    #[tool(name = "list_devices", annotations(read_only_hint = true))]
    async fn list_devices(
        &self,
        Parameters(params): Parameters<ListDevicesParams>,
    ) -> Json<DeviceList> {
        let snapshot = self.hub.snapshot();
        let in_area: Option<BTreeSet<&str>> = params.area.as_deref().map(|area| {
            snapshot
                .entities
                .iter()
                .filter(|entity| entity.area == area)
                .filter_map(|entity| entity.device_id.as_deref())
                .collect()
        });
        let query = params.query.as_deref().map(str::to_lowercase);
        let devices = snapshot
            .devices
            .iter()
            .filter(|device| {
                in_area
                    .as_ref()
                    .is_none_or(|ids| ids.contains(device.id.as_str()))
            })
            .filter(|device| query.as_deref().is_none_or(|q| device_matches(device, q)))
            .cloned()
            .collect();
        Json(DeviceList { devices })
    }

    /// One page of entities with their current state. By default only what the app shows
    /// (`include: "primary"`): no diagnostic or config entity, none disabled by default, none
    /// folded into another entity, none hidden by its override. Filters combine.
    #[tool(name = "list_entities", annotations(read_only_hint = true))]
    async fn list_entities(
        &self,
        Parameters(params): Parameters<ListEntitiesParams>,
    ) -> Json<EntityPage> {
        let include = params.include.unwrap_or_default();
        let query = params.query.as_deref().map(str::to_lowercase);
        let matches: Vec<EntityView> = self
            .hub
            .snapshot()
            .entities
            .into_iter()
            .filter(|entity| include == Include::All || entity.is_primary())
            .filter(|entity| params.area.as_deref().is_none_or(|a| entity.area == a))
            .filter(|entity| params.kind.is_none_or(|k| entity.kind == k))
            .filter(|entity| {
                params
                    .device_id
                    .as_deref()
                    .is_none_or(|d| entity.device_id.as_deref() == Some(d))
            })
            .filter(|entity| query.as_deref().is_none_or(|q| entity_matches(entity, q)))
            .filter(|entity| {
                params
                    .changed_since
                    .is_none_or(|since| entity.state.last_update.is_some_and(|at| at >= since))
            })
            .collect();
        let total = matches.len();
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let items = matches
            .into_iter()
            .skip(params.offset.unwrap_or(0))
            .take(limit)
            .collect();
        Json(EntityPage { total, items })
    }

    /// One entity with its state, its MQTT bindings (the expanded discovery config: topics,
    /// templates, payload constants) and the override retained for it, verbatim.
    #[tool(name = "get_entity", annotations(read_only_hint = true))]
    async fn get_entity(
        &self,
        Parameters(params): Parameters<GetEntityParams>,
    ) -> ToolResult<EntityDetailView> {
        let detail = self
            .hub
            .entity_detail(&params.id)
            .ok_or_else(|| ToolError(format!("unknown entity `{}`", params.id)))?;
        Ok(Json(EntityDetailView {
            entity: detail.view,
            bindings: detail.bindings,
            raw_override: detail.raw_override,
        }))
    }

    /// Changes fields of the entity's override (the Android app's `oikos/ui/<id>` document,
    /// retained) and returns the entity as it now appears. Unknown fields and invalid values
    /// are refused and nothing is written.
    #[tool(name = "set_override", annotations(idempotent_hint = true))]
    async fn set_override(
        &self,
        Parameters(params): Parameters<SetOverrideParams>,
    ) -> ToolResult<EntityView> {
        log_write("set_override", &params);
        let view = self.hub.set_override(&params.id, &params.patch).await?;
        Ok(Json(view))
    }

    /// Removes the entity's override entirely and returns the entity as discovered.
    #[tool(name = "clear_override", annotations(idempotent_hint = true))]
    async fn clear_override(
        &self,
        Parameters(params): Parameters<ClearOverrideParams>,
    ) -> ToolResult<EntityView> {
        log_write("clear_override", &params);
        let view = self.hub.clear_override(&params.id).await?;
        Ok(Json(view))
    }

    /// Sends a command to an entity: it is published to the device and `accepted` is true once
    /// the next state echo confirms it (the state is returned), false with the reason when
    /// nothing came back in time. An unknown or offline entity, or a command the entity cannot
    /// take, is refused before anything is published.
    #[tool(name = "send_command")]
    async fn send_command(
        &self,
        Parameters(params): Parameters<SendCommandParams>,
    ) -> ToolResult<CommandResponse> {
        log_write("send_command", &params);
        let response = self.hub.send_command(&params.id, params.command).await?;
        Ok(Json(response))
    }

    /// Finds devices (by friendly name, model or vendor) and entities (by name or id, or
    /// belonging to a matching device) whose text contains the query, case-insensitively.
    #[tool(name = "search", annotations(read_only_hint = true))]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> Json<SearchResult> {
        let query = params.query.to_lowercase();
        let limit = params.limit.unwrap_or(DEFAULT_SEARCH_LIMIT).min(MAX_LIMIT);
        let snapshot = self.hub.snapshot();
        let matched_devices: BTreeSet<String> = snapshot
            .devices
            .iter()
            .filter(|device| device_matches(device, &query))
            .map(|device| device.id.clone())
            .collect();
        let entities = snapshot
            .entities
            .into_iter()
            .filter(|entity| {
                entity_matches(entity, &query)
                    || entity
                        .device_id
                        .as_deref()
                        .is_some_and(|id| matched_devices.contains(id))
            })
            .take(limit)
            .collect();
        let devices = snapshot
            .devices
            .into_iter()
            .filter(|device| matched_devices.contains(&device.id))
            .take(limit)
            .collect();
        Json(SearchResult { devices, entities })
    }

    /// The discovery topics that did not become an entity, each with the reason.
    #[tool(name = "list_ignored", annotations(read_only_hint = true))]
    async fn list_ignored(&self) -> Json<IgnoredList> {
        Json(IgnoredList {
            ignored: self.hub.ignored(),
        })
    }
}

/// Every write tool leaves a trace of what was asked, minus any alarm code.
pub(super) fn log_write<P: Serialize>(tool: &'static str, params: &P) {
    tracing::info!(tool, args = %logged_args(params), "mcp write");
}

/// The arguments as JSON, with `command.code` (an alarm panel code, a secret) replaced by
/// `***` when present, at the top level or in a flow's `command` nodes, so the rest of the call stays readable in the log.
fn logged_args<P: Serialize>(params: &P) -> String {
    match serde_json::to_value(params) {
        Ok(mut value) => {
            if let Some(code) = value.get_mut("command").and_then(|c| c.get_mut("code")) {
                *code = Value::from("***");
            }
            let nodes = value
                .get_mut("flow")
                .and_then(|flow| flow.get_mut("nodes"))
                .and_then(Value::as_array_mut);
            for node in nodes.into_iter().flatten() {
                let code = node
                    .get_mut("params")
                    .and_then(|params| params.get_mut("command"))
                    .and_then(|command| command.get_mut("code"));
                if let Some(code) = code {
                    *code = Value::from("***");
                }
            }
            value.to_string()
        }
        Err(err) => format!("<unserializable: {err}>"),
    }
}

/// `query` is already lowercase.
fn device_matches(device: &DeviceView, query: &str) -> bool {
    contains_ci(&device.friendly_name, query)
        || device
            .model
            .as_deref()
            .is_some_and(|m| contains_ci(m, query))
        || device
            .vendor
            .as_deref()
            .is_some_and(|v| contains_ci(v, query))
}

/// `query` is already lowercase.
fn entity_matches(entity: &EntityView, query: &str) -> bool {
    contains_ci(&entity.name, query) || contains_ci(&entity.id, query)
}

fn contains_ci(haystack: &str, lowercase_needle: &str) -> bool {
    haystack.to_lowercase().contains(lowercase_needle)
}

#[cfg(test)]
mod tests {
    use oikos_core::model::{AlarmMode, Command};

    use super::super::params::{SendCommandParams, SetFlowParams};
    use super::logged_args;

    #[test]
    fn send_command_log_hides_the_alarm_code() {
        for command in [
            Command::Arm {
                mode: AlarmMode::Away,
                code: Some("1234".to_owned()),
            },
            Command::Disarm {
                code: Some("1234".to_owned()),
            },
        ] {
            let logged = logged_args(&SendCommandParams {
                id: "panel_1".to_owned(),
                command,
            });
            assert!(!logged.contains("1234"), "{logged}");
            assert!(logged.contains("\"code\":\"***\""), "{logged}");
            assert!(logged.contains("\"id\":\"panel_1\""), "{logged}");
            assert!(logged.contains("\"type\":\""), "{logged}");
        }
    }

    #[test]
    fn set_flow_log_hides_the_alarm_code_of_command_nodes() {
        let flow = oikos_core::flow::Flow::decode(
            br#"{"id":"night","name":"Night","nodes":[{"id":"arm","type":"command",
                "params":{"entity":"panel","command":{"type":"arm","mode":"away","code":"1234"}}}]}"#,
        )
        .expect("flow");
        let logged = logged_args(&SetFlowParams { flow });
        assert!(!logged.contains("1234"), "{logged}");
        assert!(logged.contains("\"code\":\"***\""), "{logged}");
        assert!(logged.contains("\"id\":\"night\""), "{logged}");
    }

    #[test]
    fn send_command_log_keeps_a_command_without_code() {
        let logged = logged_args(&SendCommandParams {
            id: "lamp".to_owned(),
            command: Command::TurnOn {
                brightness: Some(42),
                color: None,
                effect: None,
            },
        });
        assert_eq!(
            logged,
            r#"{"command":{"brightness":42,"type":"turn_on"},"id":"lamp"}"#
        );
    }
}
