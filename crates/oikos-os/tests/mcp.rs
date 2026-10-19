//! The MCP server at `/mcp`, driven by rmcp's streamable HTTP client over a hub fed with
//! fixtures (no broker).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use oikos_api::ConnectionStatus;
use oikos_core::command::Publish;
use oikos_os::config::Config;
use oikos_os::http::router;
use oikos_os::hub::Hub;
use rmcp::model::{CallToolRequestParams, CallToolResult, ReadResourceRequestParams};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

type Client = RunningService<RoleClient, ()>;

/// A recorded ZBMINI switch: the `rooms/stair/light` device, with its diagnostic sensor.
const SWITCH_FIXTURE: &str = "discovery/switch__0x847127fffe24e322__switch.json";
const SWITCH_ID: &str = "0x847127fffe24e322_switch_zigbee2mqtt";
const SWITCH_TOPIC: &str = "oikos/switch/0x847127fffe24e322/switch/config";
const LINKQUALITY_FIXTURE: &str = "discovery/sensor__0x847127fffe24e322__linkquality.json";
const LINKQUALITY_ID: &str = "0x847127fffe24e322_linkquality_zigbee2mqtt";
const LINKQUALITY_TOPIC: &str = "oikos/sensor/0x847127fffe24e322/linkquality/config";

const EXPECTED_TOOLS: &[&str] = &[
    "bridge_info",
    "list_areas",
    "set_areas",
    "list_devices",
    "list_entities",
    "get_entity",
    "set_override",
    "clear_override",
    "send_command",
    "search",
    "list_ignored",
];

fn fixtures() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oikos-core/tests/fixtures/"
    ))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).expect("fixture")
}

fn config() -> Config {
    let mut cfg = Config::default();
    cfg.commands.confirm_timeout_ms = 50;
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    cfg
}

struct Server {
    hub: Arc<Hub>,
    publisher: mpsc::Receiver<Publish>,
    addr: SocketAddr,
}

impl Server {
    /// A hub with the switch and its linkquality sensor discovered, served on `127.0.0.1:0`.
    async fn start() -> Server {
        let (tx, publisher) = mpsc::channel(64);
        let hub = Hub::new(&config(), tx);
        hub.ingest(SWITCH_TOPIC, &fixture(SWITCH_FIXTURE));
        hub.ingest(LINKQUALITY_TOPIC, &fixture(LINKQUALITY_FIXTURE));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(axum::serve(listener, router(Arc::clone(&hub))).into_future());
        Server {
            hub,
            publisher,
            addr,
        }
    }

    /// Marks the broker link up, so the hub's writers publish.
    fn online(&self) {
        self.hub.set_connection(ConnectionStatus::Online);
    }

    /// An initialized MCP client session against `/mcp`.
    async fn client(&self) -> Client {
        let transport =
            StreamableHttpClientTransport::from_uri(format!("http://{}/mcp", self.addr));
        ().serve(transport).await.expect("mcp initialize")
    }
}

async fn call(client: &Client, name: &'static str, args: Value) -> CallToolResult {
    let arguments = match args {
        Value::Object(map) => map,
        other => panic!("tool arguments must be an object, got {other}"),
    };
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
        .await
        .unwrap_or_else(|err| panic!("calling {name}: {err}"))
}

/// The JSON a successful tool call returned.
fn json_of(name: &str, result: &CallToolResult) -> Value {
    assert_ne!(
        result.is_error,
        Some(true),
        "{name} failed: {}",
        text_of(result)
    );
    if let Some(structured) = &result.structured_content {
        return structured.clone();
    }
    serde_json::from_str(&text_of(result)).unwrap_or_else(|err| panic!("{name}: not json: {err}"))
}

/// The text content of a tool result, concatenated.
fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn lists_the_expected_tools() {
    let server = Server::start().await;
    let client = server.client().await;

    let info = client.peer_info().expect("server info");
    let implementation = info.server_info.as_ref().expect("implementation");
    assert_eq!(implementation.name, "oikos-os");
    assert_eq!(implementation.version, env!("CARGO_PKG_VERSION"));
    let instructions = info.instructions.as_deref().expect("instructions");
    assert!(instructions.contains("list_entities"), "{instructions}");
    assert!(instructions.contains("set_override"), "{instructions}");
    assert!(instructions.contains("send_command"), "{instructions}");

    let tools = client.list_all_tools().await.expect("list tools");
    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    let mut expected = EXPECTED_TOOLS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
    for tool in &tools {
        assert!(
            tool.description.as_deref().is_some_and(|d| !d.is_empty()),
            "{} has no description",
            tool.name
        );
    }
    let list_entities = tools
        .iter()
        .find(|tool| tool.name == "list_entities")
        .expect("list_entities");
    let properties = list_entities.input_schema["properties"]
        .as_object()
        .expect("properties");
    for field in [
        "area",
        "kind",
        "device_id",
        "query",
        "changed_since",
        "include",
        "limit",
        "offset",
    ] {
        let property = &properties[field];
        assert!(
            property.get("description").is_some(),
            "list_entities.{field} is undocumented: {property}"
        );
    }

    client.cancel().await.expect("close");
}

#[tokio::test]
async fn list_entities_primary_hides_diagnostics_and_all_shows_them() {
    let server = Server::start().await;
    let client = server.client().await;

    let primary = json_of(
        "list_entities",
        &call(&client, "list_entities", json!({})).await,
    );
    assert_eq!(primary["total"], 1);
    assert_eq!(primary["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(primary["items"][0]["id"], SWITCH_ID);

    let all = json_of(
        "list_entities",
        &call(&client, "list_entities", json!({"include": "all"})).await,
    );
    assert_eq!(all["total"], 2);
    let ids: Vec<&str> = all["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert!(ids.contains(&SWITCH_ID), "{ids:?}");
    assert!(ids.contains(&LINKQUALITY_ID), "{ids:?}");

    let paged = json_of(
        "list_entities",
        &call(
            &client,
            "list_entities",
            json!({"include": "all", "limit": 1, "offset": 1}),
        )
        .await,
    );
    assert_eq!(paged["total"], 2);
    assert_eq!(paged["items"].as_array().map(Vec::len), Some(1));

    let by_kind = json_of(
        "list_entities",
        &call(
            &client,
            "list_entities",
            json!({"include": "all", "kind": "sensor"}),
        )
        .await,
    );
    assert_eq!(by_kind["total"], 1);
    assert_eq!(by_kind["items"][0]["id"], LINKQUALITY_ID);

    client.cancel().await.expect("close");
}

#[tokio::test]
async fn set_override_round_trip_through_tool() {
    let mut server = Server::start().await;
    let client = server.client().await;

    // The broker link is still connecting: the write is refused in-band, nothing is queued.
    let refused = call(
        &client,
        "set_override",
        json!({"id": SWITCH_ID, "patch": {"asLight": true}}),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(text_of(&refused), "publish: broker offline");
    assert!(server.publisher.try_recv().is_err());
    server.online();

    let view = json_of(
        "set_override",
        &call(
            &client,
            "set_override",
            json!({"id": SWITCH_ID, "patch": {"asLight": true}}),
        )
        .await,
    );
    assert_eq!(view["id"], SWITCH_ID);
    assert_eq!(view["kind"], "light");
    let published = server.publisher.try_recv().expect("the override publish");
    assert_eq!(published.topic, format!("oikos/ui/{SWITCH_ID}"));
    assert!(published.retain);

    let detail = json_of(
        "get_entity",
        &call(&client, "get_entity", json!({"id": SWITCH_ID})).await,
    );
    assert_eq!(detail["id"], SWITCH_ID);
    assert_eq!(detail["kind"], "light");
    assert_eq!(detail["raw_override"]["asLight"], true);
    assert_eq!(
        detail["bindings"]["state_topic"],
        "zigbee2mqtt/rooms/stair/light"
    );

    let view = json_of(
        "clear_override",
        &call(&client, "clear_override", json!({"id": SWITCH_ID})).await,
    );
    assert_eq!(view["kind"], "switch");
    let published = server.publisher.try_recv().expect("the clearing publish");
    assert_eq!(published.payload, "");

    let detail = json_of(
        "get_entity",
        &call(&client, "get_entity", json!({"id": SWITCH_ID})).await,
    );
    assert_eq!(detail["kind"], "switch");
    assert_eq!(detail["raw_override"], Value::Null);

    let missing = call(&client, "get_entity", json!({"id": "nope"})).await;
    assert_eq!(missing.is_error, Some(true));
    assert!(text_of(&missing).contains("unknown entity `nope`"));

    client.cancel().await.expect("close");
}

#[tokio::test]
async fn set_override_unknown_field_is_a_tool_error_listing_fields() {
    let mut server = Server::start().await;
    let client = server.client().await;

    let result = call(
        &client,
        "set_override",
        json!({"id": SWITCH_ID, "patch": {"nope": true}}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    let message = text_of(&result);
    assert!(
        message.contains("unknown override field `nope`"),
        "{message}"
    );
    assert!(message.contains("asLight"), "{message}");
    assert!(message.contains("lightOnBrightness"), "{message}");
    assert!(server.publisher.try_recv().is_err());

    let result = call(
        &client,
        "set_override",
        json!({"id": SWITCH_ID, "patch": {"energyRole": "nope"}}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    let message = text_of(&result);
    assert!(message.contains("energyRole"), "{message}");
    assert!(message.contains("solar"), "{message}");
    assert!(message.contains("grid_export"), "{message}");
    assert!(server.publisher.try_recv().is_err());

    let result = call(
        &client,
        "set_override",
        json!({"id": "nope", "patch": {"favorite": true}}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    assert!(text_of(&result).contains("unknown entity `nope`"));

    client.cancel().await.expect("close");
}

#[tokio::test]
async fn snapshot_resource_reads() {
    let server = Server::start().await;
    let client = server.client().await;

    let resources = client.list_all_resources().await.expect("list resources");
    let snapshot = resources
        .iter()
        .find(|resource| resource.uri == "oikos://snapshot")
        .expect("the snapshot resource");
    assert_eq!(snapshot.mime_type.as_deref(), Some("application/json"));

    let read = client
        .read_resource(ReadResourceRequestParams::new("oikos://snapshot"))
        .await
        .expect("read resource");
    let (uri, mime_type, text) = match read.contents.first().expect("one content") {
        rmcp::model::ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => (uri.clone(), mime_type.clone(), text.clone()),
        other => panic!("expected text contents, got {other:?}"),
    };
    assert_eq!(uri, "oikos://snapshot");
    assert_eq!(mime_type.as_deref(), Some("application/json"));
    let snapshot: Value = serde_json::from_str(&text).expect("json snapshot");
    assert_eq!(snapshot["connection"]["type"], "connecting");
    assert_eq!(snapshot["entities"].as_array().map(Vec::len), Some(2));

    let missing = client
        .read_resource(ReadResourceRequestParams::new("oikos://nope"))
        .await;
    assert!(missing.is_err(), "unknown resource must be an error");

    client.cancel().await.expect("close");
}

#[tokio::test]
async fn send_command_search_areas_and_the_small_tools() {
    let mut server = Server::start().await;
    let client = server.client().await;

    server
        .hub
        .ingest("zigbee2mqtt/bridge/state", br#"{"state":"offline"}"#);
    let rejected = call(
        &client,
        "send_command",
        json!({"id": SWITCH_ID, "command": {"type": "turn_off"}}),
    )
    .await;
    assert_eq!(rejected.is_error, Some(true));
    assert_eq!(text_of(&rejected), "rejected: entity is offline");
    assert!(server.publisher.try_recv().is_err());

    server
        .hub
        .ingest("zigbee2mqtt/bridge/state", br#"{"state":"online"}"#);
    server.online();
    let response = json_of(
        "send_command",
        &call(
            &client,
            "send_command",
            json!({"id": SWITCH_ID, "command": {"type": "turn_on"}}),
        )
        .await,
    );
    assert_eq!(response["accepted"], false);
    assert_eq!(response["reason"], "no state change within 50 ms");
    let published = server.publisher.try_recv().expect("the command publish");
    assert_eq!(published.topic, "zigbee2mqtt/rooms/stair/light/set");
    assert_eq!(published.payload, "ON");

    let unknown = call(
        &client,
        "send_command",
        json!({"id": "nope", "command": {"type": "turn_on"}}),
    )
    .await;
    assert_eq!(unknown.is_error, Some(true));
    assert_eq!(text_of(&unknown), "unknown entity `nope`");

    let areas = json_of(
        "set_areas",
        &call(
            &client,
            "set_areas",
            json!({"areas": [{"id": "stair", "name": "Stair", "order": 2}]}),
        )
        .await,
    );
    let stair = areas["areas"]
        .as_array()
        .expect("areas")
        .iter()
        .find(|a| a["id"] == "stair")
        .expect("the declared area");
    assert_eq!(stair["name"], "Stair");
    let published = server.publisher.try_recv().expect("the areas publish");
    assert_eq!(published.topic, "oikos/ui/_areas");
    let listed = json_of("list_areas", &call(&client, "list_areas", json!({})).await);
    assert_eq!(listed, areas);

    // The device `rooms/stair/light` names both entities "light …"; the search is
    // case-insensitive and no `bridge/devices` was ingested, so no device matches.
    let found = json_of(
        "search",
        &call(&client, "search", json!({"query": "LIGHT"})).await,
    );
    let ids: Vec<&str> = found["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert!(ids.contains(&SWITCH_ID), "{ids:?}");
    assert!(ids.contains(&LINKQUALITY_ID), "{ids:?}");
    assert_eq!(found["devices"].as_array().map(Vec::len), Some(0));
    let limited = json_of(
        "search",
        &call(&client, "search", json!({"query": "light", "limit": 1})).await,
    );
    assert_eq!(limited["entities"].as_array().map(Vec::len), Some(1));
    let none = json_of(
        "search",
        &call(&client, "search", json!({"query": "kitchen"})).await,
    );
    assert_eq!(none["entities"].as_array().map(Vec::len), Some(0));

    // Once the devices are known, a query matching a device (`rooms/stair/light`) also
    // returns that device's entities, although their names do not contain it.
    server.hub.ingest(
        "zigbee2mqtt/bridge/devices",
        &fixture("bridge_devices.json"),
    );
    let by_device = json_of(
        "search",
        &call(&client, "search", json!({"query": "stair"})).await,
    );
    assert!(
        by_device["devices"]
            .as_array()
            .expect("devices")
            .iter()
            .any(|d| d["friendly_name"] == "rooms/stair/light"),
        "{by_device}"
    );
    let ids: Vec<&str> = by_device["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert!(ids.contains(&SWITCH_ID), "{ids:?}");
    assert!(ids.contains(&LINKQUALITY_ID), "{ids:?}");

    let devices = json_of(
        "list_devices",
        &call(&client, "list_devices", json!({"query": "zbmini"})).await,
    );
    assert!(devices["devices"].is_array());

    let bridge = json_of(
        "bridge_info",
        &call(&client, "bridge_info", json!({})).await,
    );
    assert_eq!(bridge["connection"]["type"], "online");
    assert_eq!(bridge["bridge"]["state"], "online");

    let ignored = json_of(
        "list_ignored",
        &call(&client, "list_ignored", json!({})).await,
    );
    assert!(ignored["ignored"].is_array());

    client.cancel().await.expect("close");
}
