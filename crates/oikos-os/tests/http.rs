//! The HTTP API, the WebSocket stream and the placeholder page, served on an ephemeral port
//! over a hub driven by fixtures (no broker).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use oikos_core::command::Publish;
use oikos_os::config::Config;
use oikos_os::http::router;
use oikos_os::hub::Hub;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A recorded ZBMINI switch: the `rooms/stair/light` device.
const SWITCH_FIXTURE: &str = "discovery/switch__0x847127fffe24e322__switch.json";
const SWITCH_ID: &str = "0x847127fffe24e322_switch_zigbee2mqtt";
const SWITCH_TOPIC: &str = "oikos/switch/0x847127fffe24e322/switch/config";

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

/// The switch's `state_topic`, from its discovery config.
fn switch_state_topic() -> String {
    let config: Value = serde_json::from_slice(&fixture(SWITCH_FIXTURE)).expect("switch config");
    config["state_topic"]
        .as_str()
        .expect("state_topic")
        .to_owned()
}

struct Server {
    hub: Arc<Hub>,
    publisher: mpsc::Receiver<Publish>,
    addr: SocketAddr,
    client: reqwest::Client,
}

impl Server {
    /// A hub with the switch discovered, served on `127.0.0.1:0`.
    async fn start() -> Server {
        let (tx, publisher) = mpsc::channel(64);
        let hub = Hub::new(&config(), tx);
        hub.ingest(SWITCH_TOPIC, &fixture(SWITCH_FIXTURE));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(axum::serve(listener, router(Arc::clone(&hub))).into_future());
        Server {
            hub,
            publisher,
            addr,
            client: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        self.client.get(self.url(path)).send().await.expect("GET")
    }

    async fn ws(&self) -> Ws {
        let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", self.addr))
            .await
            .expect("websocket");
        stream
    }
}

/// The next text frame of `ws`, parsed.
async fn next_json(ws: &mut Ws) -> Value {
    let frame = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("a frame within 2 s")
        .expect("open stream")
        .expect("a frame");
    match frame {
        Message::Text(text) => serde_json::from_str(text.as_str()).expect("json frame"),
        other => panic!("expected a text frame, got {other:?}"),
    }
}

#[tokio::test]
async fn health_and_snapshot() {
    let server = Server::start().await;

    let health = server.get("/health").await;
    assert_eq!(health.status(), StatusCode::OK);
    let health: Value = health.json().await.expect("json");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["connection"]["type"], "connecting");
    assert_eq!(health["entities"], 1);

    let snapshot = server.get("/api/v1/snapshot").await;
    assert_eq!(snapshot.status(), StatusCode::OK);
    assert_eq!(
        snapshot
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
    let snapshot: Value = snapshot.json().await.expect("json");
    assert_eq!(snapshot["connection"]["type"], "connecting");
    assert_eq!(snapshot["entities"].as_array().map(Vec::len), Some(1));
    assert_eq!(snapshot["entities"][0]["id"], SWITCH_ID);
    assert_eq!(snapshot["entities"][0]["kind"], "switch");

    let missing = server.get("/api/v1/nope").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let missing: Value = missing.json().await.expect("json error body");
    assert!(missing["error"].is_string());
}

#[tokio::test]
async fn command_unknown_entity_404() {
    let server = Server::start().await;

    let response = server
        .client
        .post(server.url("/api/v1/entities/nope/command"))
        .json(&json!({"type": "turn_off"}))
        .send()
        .await
        .expect("POST");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"], "unknown entity `nope`");
}

#[tokio::test]
async fn command_offline_entity_422_and_timeout_200() {
    let mut server = Server::start().await;
    server
        .hub
        .ingest("zigbee2mqtt/bridge/state", br#"{"state":"offline"}"#);

    let response = server
        .client
        .post(server.url(&format!("/api/v1/entities/{SWITCH_ID}/command")))
        .json(&json!({"type": "turn_off"}))
        .send()
        .await
        .expect("POST");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"], "rejected: entity is offline");
    assert!(server.publisher.try_recv().is_err());

    server
        .hub
        .ingest("zigbee2mqtt/bridge/state", br#"{"state":"online"}"#);
    let response = server
        .client
        .post(server.url(&format!("/api/v1/entities/{SWITCH_ID}/command")))
        .json(&json!({"type": "turn_on"}))
        .send()
        .await
        .expect("POST");
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["accepted"], false);
    assert_eq!(body["reason"], "no state change within 50 ms");
    let published = server.publisher.try_recv().expect("the command publish");
    assert_eq!(published.topic, "zigbee2mqtt/rooms/stair/light/set");
    assert_eq!(published.payload, "ON");
}

#[tokio::test]
async fn patch_override_then_snapshot_reflects_it() {
    let mut server = Server::start().await;

    let response = server
        .client
        .patch(server.url(&format!("/api/v1/entities/{SWITCH_ID}/override")))
        .json(&json!({"asLight": true, "favorite": true}))
        .send()
        .await
        .expect("PATCH");
    assert_eq!(response.status(), StatusCode::OK);
    let view: Value = response.json().await.expect("json");
    assert_eq!(view["id"], SWITCH_ID);
    assert_eq!(view["kind"], "light");
    assert_eq!(view["ui"]["favorite"], true);
    let published = server.publisher.try_recv().expect("the override publish");
    assert_eq!(published.topic, format!("oikos/ui/{SWITCH_ID}"));
    assert!(published.retain);

    let snapshot: Value = server
        .get("/api/v1/snapshot")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(snapshot["entities"][0]["kind"], "light");
    assert_eq!(snapshot["entities"][0]["ui"]["favorite"], true);

    let response = server
        .client
        .delete(server.url(&format!("/api/v1/entities/{SWITCH_ID}/override")))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let published = server.publisher.try_recv().expect("the clearing publish");
    assert_eq!(published.payload, "");
    assert!(published.retain);
    let snapshot: Value = server
        .get("/api/v1/snapshot")
        .await
        .json()
        .await
        .expect("json");
    assert_eq!(snapshot["entities"][0]["kind"], "switch");
    assert_eq!(snapshot["entities"][0]["ui"]["favorite"], Value::Null);

    let response = server
        .client
        .delete(server.url("/api/v1/entities/nope/override"))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_patch_400() {
    let server = Server::start().await;

    let response = server
        .client
        .patch(server.url(&format!("/api/v1/entities/{SWITCH_ID}/override")))
        .json(&json!({"nope": true}))
        .send()
        .await
        .expect("PATCH");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.expect("json");
    assert_eq!(body["error"], "unknown override field `nope`");

    let response = server
        .client
        .patch(server.url("/api/v1/entities/nope/override"))
        .json(&json!({"favorite": true}))
        .send()
        .await
        .expect("PATCH");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_areas_and_list_ignored() {
    let mut server = Server::start().await;

    let response = server
        .client
        .put(server.url("/api/v1/areas"))
        .json(&json!({"areas": [{"id": "stair", "name": "Stair", "order": 2}]}))
        .send()
        .await
        .expect("PUT");
    assert_eq!(response.status(), StatusCode::OK);
    let areas: Value = response.json().await.expect("json");
    let stair = areas
        .as_array()
        .expect("an array")
        .iter()
        .find(|a| a["id"] == "stair")
        .expect("the declared area");
    assert_eq!(stair["name"], "Stair");
    assert_eq!(stair["order"], 2);
    let published = server.publisher.try_recv().expect("the areas publish");
    assert_eq!(published.topic, "oikos/ui/_areas");
    assert!(published.retain);

    let ignored = server.get("/api/v1/ignored").await;
    assert_eq!(ignored.status(), StatusCode::OK);
    let ignored: Value = ignored.json().await.expect("json");
    assert!(ignored.is_array());
}

#[tokio::test]
async fn ws_sends_snapshot_then_events() {
    let server = Server::start().await;
    let mut ws = server.ws().await;

    let first = next_json(&mut ws).await;
    assert_eq!(first["type"], "snapshot");
    assert_eq!(first["entities"][0]["id"], SWITCH_ID);

    server
        .hub
        .ingest(&switch_state_topic(), br#"{"state":"ON"}"#);
    let event = next_json(&mut ws).await;
    assert_eq!(event["type"], "state_changed");
    assert_eq!(event["id"], SWITCH_ID);
    assert_eq!(
        event["state"]["value"],
        json!({"type": "on_off", "value": true})
    );

    ws.close(None).await.expect("close");
}

/// The test runtime is single-threaded, so the client cannot be served while the test floods
/// the hub: the broadcast receiver lags and the stream recovers with a fresh snapshot.
#[tokio::test]
async fn ws_lag_sends_fresh_snapshot() {
    let server = Server::start().await;
    let mut ws = server.ws().await;
    assert_eq!(next_json(&mut ws).await["type"], "snapshot");

    let topic = switch_state_topic();
    for i in 0..1100 {
        let payload: &[u8] = if i % 2 == 0 {
            br#"{"state":"ON"}"#
        } else {
            br#"{"state":"OFF"}"#
        };
        server.hub.ingest(&topic, payload);
    }

    let recovered = next_json(&mut ws).await;
    assert_eq!(recovered["type"], "snapshot");
    assert_eq!(
        recovered["entities"][0]["state"]["value"],
        json!({"type": "on_off", "value": false})
    );

    server.hub.ingest(&topic, br#"{"state":"ON"}"#);
    loop {
        let event = next_json(&mut ws).await;
        if event["type"] == "state_changed" && event["state"]["value"]["value"] == true {
            break;
        }
    }
}

#[tokio::test]
async fn root_serves_placeholder() {
    let server = Server::start().await;

    for path in ["/", "/rooms/stair", "/index.html"] {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/html; charset=utf-8"),
            "{path}"
        );
        let body = response.text().await.expect("body");
        assert!(body.contains("<title>Oikos-os</title>"), "{path}");
        assert!(
            body.contains("Oikos-os is running. The web UI arrives in the next phase."),
            "{path}"
        );
        assert!(body.contains("href=\"/api/v1/snapshot\""), "{path}");
        assert!(body.contains("href=\"/health\""), "{path}");
    }

    let cors = server
        .client
        .get(server.url("/api/v1/snapshot"))
        .header("origin", "http://localhost:8081")
        .send()
        .await
        .expect("GET");
    assert_eq!(
        cors.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*")
    );
}
