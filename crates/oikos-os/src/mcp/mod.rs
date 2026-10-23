//! The MCP server: thirty-four tools and the `oikos://snapshot` resource over the hub, served at
//! `/mcp` over streamable HTTP with stateful sessions, and the stdio proxy in front of it.
//!
//! A failed tool call is answered in-band (`isError: true` with the reason as text) rather
//! than as a JSON-RPC error, so the assistant reads why and retries with better arguments.

mod climate_tools;
mod flow_tools;
mod params;
mod stdio_proxy;
mod tools;

use std::sync::Arc;

use oikos_core::model::UiOverride;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::model::{
    CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation, ListResourcesResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult,
    Resource, ResourceContents, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{RoleServer, ServerHandler, tool_handler};

use crate::hub::{Hub, HubError};

pub use stdio_proxy::run_stdio_proxy;

/// The resource carrying the whole house as one JSON document.
pub const SNAPSHOT_URI: &str = "oikos://snapshot";

/// The MCP server over one hub; one instance serves one session.
pub struct OikosMcp {
    hub: Arc<Hub>,
    tool_router: ToolRouter<OikosMcp>,
}

impl OikosMcp {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self {
            hub,
            tool_router: Self::tool_router()
                + Self::flow_tool_router()
                + Self::climate_tool_router(),
        }
    }
}

/// The streamable HTTP service to nest at `/mcp`, with stateful sessions kept in memory.
///
/// Any `Host` is accepted: rmcp's DNS-rebinding guard (loopback hosts only by default) is
/// switched off. That is acceptable in v1 only because the REST API on the same router is
/// equally open, with permissive CORS and no auth, and the hub is reached by LAN name or
/// address. Revisit with auth: prefer `with_allowed_hosts([...])` derived from the configured
/// bind address or hostname.
pub fn mcp_service(hub: Arc<Hub>) -> StreamableHttpService<OikosMcp, LocalSessionManager> {
    let config = StreamableHttpServerConfig::default().disable_allowed_hosts();
    StreamableHttpService::new(
        move || Ok(OikosMcp::new(Arc::clone(&hub))),
        Arc::new(LocalSessionManager::default()),
        config,
    )
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OikosMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("oikos-os", env!("CARGO_PKG_VERSION")))
        .with_instructions(instructions())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let snapshot = Resource::new(SNAPSHOT_URI, "snapshot")
            .with_title("House snapshot")
            .with_description(
                "The whole house at this instant: connection, bridge, areas, devices, \
                 entities with their states, and the ignored discovery topics.",
            )
            .with_mime_type("application/json");
        Ok(ListResourcesResult::with_all_items(vec![snapshot]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != SNAPSHOT_URI {
            return Err(ErrorData::resource_not_found(
                format!("unknown resource `{}`", request.uri),
                None,
            ));
        }
        let text = serde_json::to_string(&self.hub.snapshot())
            .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;
        let contents =
            ResourceContents::text(text, SNAPSHOT_URI).with_mime_type("application/json");
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

/// One paragraph telling the assistant what the house is and how the tools behave.
fn instructions() -> String {
    format!(
        "Oikos-os is the hub of a Zigbee2MQTT home behind a Mosquitto broker: devices are the \
         Zigbee nodes, entities are their Home Assistant discovery entries (lights, switches, \
         covers, climates, sensors and so on) and areas are the rooms. `list_entities` hides \
         diagnostic, config, disabled-by-default and folded entities unless you pass \
         `include: \"all\"`; use `search` to find a device or an entity by name and `get_entity` \
         to see an entity's MQTT bindings (topics, templates) and its raw override. \
         `set_override` writes the Android app's `oikos/ui/<id>` override (fields: {fields}; \
         `null` clears a field) and `clear_override` removes it. `send_command` publishes to \
         the device and reports `accepted: true` once the next state echo confirms it, or \
         `accepted: false` with the reason; a generic entity (lock, fan, siren, scene, text and so on) \
         takes `{{\"type\":\"control\",\"control\":\"<name>\",\"value\":...}}` with a name from its \
         `capabilities.generic.controls`, and a control with no state to echo is accepted as soon \
         as it is published. The `oikos://snapshot` resource is the whole house \
         as one JSON document. Automations are flows: `list_flows` and `get_flow` read them, \
         `set_flow` creates or replaces one (validated against the house, saved retained on \
         `oikos/automation/<id>`, the prefix being configurable), `set_flow_enabled` \
         switches it and `delete_flow` removes it; `run_flow` starts a manual run. A flow's optional `area` (an area id; a room name is resolved) is the room the UI lists it under; it does not change what the flow does. `set_virtual` and `delete_virtual` manage virtual \
         entities (a `switch`, `number`, `button` or `select` the hub owns, as `virtual_<id>`) for flows \
         to read and write. `list_groups`, `set_group` and `delete_group` manage flexible \
         groups (hub-published entities as `group_<id>` whose definition — kind, members, \
         hideMembers — is retained at `oikos/group/<id>/definition`); the hub fans group \
         commands out to member entities and republishes the combined state as members \
         change. A flow is a \
         graph of nodes: `trigger.state` (an entity's state changes), `trigger.signal` (a remote press, an event or a tag scan: `entity` is a `device_automation.*`, `event.*` or `tag.*` id and `signal` optionally names one of its announced signals; the trigger value is the signal name, and `trigger.state` is refused on those entities), `trigger.schedule` (a \
         5-field cron in the house local time; it fires only while the flow is enabled and \
         the first run is at the next tick), `flow.enable` (sets another flow's `enabled`, or its own flow when `flow` is absent; a flow \
         that disables itself still finishes the current run), `if` (compares a value; edges \
         leaving it name the port `true` or `false`; `missing` is `fail` (the default), `true` \
         or `false` and picks that port when the value is unknown), `delay` (`for`: `5m`, `1h30m`; \
         with `up_to` it waits a random time between the two), \
         `command` (the same JSON as `send_command`) and `notify` (a phone push: `title`, optional `body`, `channel`, `severity`, `entity_id`, `image_url`, `silent`, `id`). Entity ids come from `search` or \
         `list_entities`. For \"N minutes after the last motion\" use `mode: restart`, so \
         every new trigger restarts the wait; the default `single` ignores triggers while a \
         run is active. Sun events use `trigger.sun` with `which` one of `sunrise`, `sunset`, \
         `dusk` (civil), `dawn` (civil), and an optional `offset_minutes` in [-720, 720]. `if` \
         with `op: is_dark` is true between civil dusk and civil dawn at the configured point. \
         Set `[flows] latitude` and `[flows] longitude` in `oikos.toml`; the hub falls back to \
         Paris otherwise and reports a warning on save. `get_history` returns one slice of an entity's history — a chart \
         series (`what: series` with `range`), kWh per day (`daily` with `days` and \
         `kind`), kWh since midnight (`today` with `kind`), or `last_change` — when \
         `[history] url` is set in the hub's config. `send_notification` sends a phone push (`title` \
         required; `channel` alarm, security, camera, energy, climate, system or misc; `severity` \
         info, warning or critical; the same `id` replaces an earlier alert) and `list_alerts` \
         returns the recent ones, newest first. Climate (when `[climate] enabled`): each area with a \
         climate device is a zone (`off`, `pending`, `active`). `climate_status` shows every \
         zone's reading, decision and why (`source`, `reason`), overrides and pending proposal. \
         The engine learns rules from history and proposes them; nothing applies until \
         `approve_climate_proposal` (or the push button). Safety (frost, overheat, stale \
         readings) always acts. A temperature set by hand holds until the zone's next scheduled \
         change, at most `manual_hold_max_min`.",
        fields = UiOverride::FIELDS.join(", ")
    )
}

/// Why a tool call failed, reported in-band with the reason as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError(pub String);

impl IntoCallToolResult for ToolError {
    fn into_call_tool_result(self) -> Result<CallToolResponse, ErrorData> {
        Ok(CallToolResult::error(vec![ContentBlock::text(self.0)]).into())
    }
}

impl From<HubError> for ToolError {
    /// A rejected patch names the valid fields (and, through `PatchError`'s own message, the
    /// valid values of the rejected one); the other errors keep the hub's wording.
    fn from(err: HubError) -> Self {
        match err {
            HubError::Patch(patch) => ToolError(format!(
                "{patch}; the override fields are {}",
                UiOverride::FIELDS.join(", ")
            )),
            other => ToolError(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::model::{ENERGY_ROLES, PatchError};

    #[test]
    fn patch_errors_list_the_fields_and_the_valid_values() {
        let unknown = ToolError::from(HubError::Patch(PatchError::UnknownField("nope".into())));
        assert!(
            unknown.0.starts_with("unknown override field `nope`"),
            "{}",
            unknown.0
        );
        for field in UiOverride::FIELDS {
            assert!(
                unknown.0.contains(field),
                "{field} missing from {}",
                unknown.0
            );
        }

        let invalid = ToolError::from(HubError::Patch(PatchError::InvalidValue {
            field: "energyRole".into(),
            expected: format!("one of {}", ENERGY_ROLES.join(", ")),
        }));
        assert!(invalid.0.contains("energyRole"), "{}", invalid.0);
        assert!(invalid.0.contains("grid_export"), "{}", invalid.0);
    }

    #[test]
    fn other_hub_errors_keep_their_wording() {
        assert_eq!(
            ToolError::from(HubError::UnknownEntity("x".into())).0,
            "unknown entity `x`"
        );
        assert_eq!(
            ToolError::from(HubError::Rejected("entity is offline".into())).0,
            "rejected: entity is offline"
        );
    }

    #[test]
    fn instructions_mention_the_tools_and_the_fields() {
        let text = instructions();
        for needle in [
            "list_entities",
            "set_override",
            "send_command",
            "search",
            "get_entity",
        ] {
            assert!(text.contains(needle), "{needle} missing");
        }
        assert!(text.contains("lightOnBrightness"));
        assert!(text.contains(SNAPSHOT_URI));
    }

    #[test]
    fn mcp_instructions_mention_trigger_sun_and_is_dark() {
        let instr = instructions();
        assert!(instr.contains("trigger.sun"));
        assert!(instr.contains("is_dark"));
        assert!(instr.contains("Paris"));
    }

    #[test]
    fn instructions_mention_flows() {
        let text = instructions();
        for needle in ["set_flow", "restart", "oikos/automation", "run_flow"] {
            assert!(text.contains(needle), "{needle} missing");
        }
    }
}
