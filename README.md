# Oikos-os

Author: Gekkotron

Oikos-os is the Rust home-automation hub that replaces Jeedom. It runs on the home server next to Mosquitto, owns the device registry built from Zigbee2MQTT and Home Assistant MQTT discovery, and is the brain behind the Oikos Android app. It is configured through conversation: an MCP server lets an AI assistant inspect and change the house instead of a rules editor. The MCP server is served at `/mcp` and, like the REST API on the same port, is LAN-trusted with no auth in v1.

Status: v1 hub: registry, API, WebSocket and MCP; web UI next.

## Run

```sh
cp oikos.example.toml oikos.toml   # then edit the [mqtt] credentials
cargo run -p oikos-os -- run
```

`oikos.toml` is git-ignored and read from the working directory (`run --config <path>` reads another file). Every key can be overridden from the environment as `OIKOS_<SECTION>__<KEY>`, for example `OIKOS_MQTT__PASSWORD=… cargo run -p oikos-os -- run`; `RUST_LOG` wins over `[log] level`.

The hub keeps a persistent MQTT session under `[mqtt] client_id` (default `oikos-os`), so a write queued just before a connection blip is retransmitted on reconnect. Two hubs on the same broker must not share a `client_id`: the broker would bounce them off each other and hand one the other's session.

Until Zigbee2MQTT publishes under `oikos`, list the Home Assistant prefix next to it so the discovery topics it still retains under `homeassistant/` are picked up too:

```toml
[topics]
discovery_prefixes = ["oikos", "homeassistant"]
```

## Zigbee2MQTT

The hub discovers entities under the `oikos` prefix. In Zigbee2MQTT's `configuration.yaml`:

```yaml
homeassistant:
  enabled: true
  discovery_topic: oikos
```

Restart Zigbee2MQTT, then set the Oikos app's discovery prefix to `oikos` as well.

## HTTP API

The hub listens on `[http] bind` (default `0.0.0.0:8080`), CORS open to any origin, JSON in and out.

| Route | What it does |
| --- | --- |
| `GET /health` | `status`, the broker connection state and the entity count. |
| `GET /api/v1/snapshot` | The whole house: connection, bridge, areas, devices, entities with their state, ignored topics. |
| `POST /api/v1/entities/{id}/command` | Sends a command (`{"type": "turn_on", …}`) and reports whether the device confirmed it. |
| `PATCH /api/v1/entities/{id}/override` | Changes fields of the entity's `oikos/ui/<id>` override and returns the entity as it now appears. |
| `DELETE /api/v1/entities/{id}/override` | Removes the override (204). |
| `PUT /api/v1/areas` | Replaces the declared rooms (`oikos/ui/_areas`) and returns the resulting list. |
| `GET /api/v1/ignored` | The discovery topics that did not become an entity, each with the reason. |
| `GET /ws` | WebSocket: a snapshot first, then every hub event as a JSON text frame. |

Errors are `{"error": "…"}` with 400 (bad patch), 404 (unknown entity or route), 422 (command refused) or 503 (the broker link is down, so the write was refused and nothing was applied; the same publish refusal is an in-band tool error over MCP). Every other path serves a placeholder page until the web UI lands.

## MCP

The MCP server at `http://<host>:8080/mcp` (streamable HTTP) offers eleven tools and one resource:

- `bridge_info` — the Zigbee2MQTT bridge and the state of the link to the broker.
- `list_areas` — the rooms, each with its entity count.
- `set_areas` — replaces the declared rooms and returns the resulting list.
- `list_devices` — the Zigbee devices with availability, battery and link quality, optionally filtered by area or by name, model or vendor.
- `list_entities` — a page of entities with their state; by default only what the app shows (`include: "all"` for everything).
- `get_entity` — one entity with its state, its MQTT bindings and its raw override.
- `set_override` — changes fields of the app's `oikos/ui/<id>` override (`null` clears a field).
- `clear_override` — removes the override entirely.
- `send_command` — publishes a command and reports `accepted` once the device echoes the change.
- `search` — devices (name, model, vendor) and entities (name, id) containing the query.
- `list_ignored` — the discovery topics that did not become an entity, with the reason.
- Resource `oikos://snapshot` — the whole house as one JSON document.

A failed tool call answers in-band (`isError: true` with the reason) so the assistant can retry with better arguments.

Claude Code:

```sh
claude mcp add --transport http oikos http://<host>:8080/mcp
```

Codex CLI, in `~/.codex/config.toml`:

```toml
[mcp_servers.oikos]
url = "http://<host>:8080/mcp"
```

Claude Desktop only speaks stdio, so it launches `oikos-os mcp-stdio`, a proxy that relays every request to the running hub. In its config JSON, with the path of a built binary (`cargo build --release -p oikos-os`):

```json
{
  "mcpServers": {
    "oikos": {"command": "<path>/oikos-os", "args": ["mcp-stdio", "--server", "http://<host>:8080"]}
  }
}
```

The proxy exits with `cannot reach Oikos-os MCP server at …` when the hub is down; its logs go to stderr, stdout carries the protocol.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo check -p oikos-core --target wasm32-unknown-unknown
cargo check -p oikos-api --target wasm32-unknown-unknown
```

The `wasm32-unknown-unknown` target comes with `rust-toolchain.toml`. The tests run against recorded broker messages under `crates/oikos-core/tests/fixtures` and never need a broker; refresh the recording from the house with `MQTT_HOST=<broker> MQTT_USER=… MQTT_PASS=… tools/record-fixtures.sh` (credentials are read from the environment only).

## License

Apache-2.0, Copyright 2026 Gekkotron.
