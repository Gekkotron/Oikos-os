# Oikos-os

Author: Gekkotron

Oikos-os is the Rust home-automation hub that replaces Jeedom. It runs on the home server next to Mosquitto, owns the device registry built from Zigbee2MQTT and Home Assistant MQTT discovery, and is the brain behind the Oikos Android app. It is configured through conversation: an MCP server lets an AI assistant inspect and change the house instead of a rules editor. The MCP server is served at `/mcp` and, like the REST API on the same port, is LAN-trusted with no auth in v1.

Status: v1 hub with its web UI: registry, API, WebSocket, MCP and the browser app.

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

The web UI's Alerts screens read OikosPush's retained history, and alert buttons reply where the app replies. Both topics are configuration, with OikosPush's current names as defaults; they become `oikos/history` and `oikos/push/responses` once the hub takes over push:

```toml
[push]
history_topic = "oikos-push/history"
responses_topic = "oikos-push/responses"
```

Alert snapshots and clips are URLs on Frigate. When they point at Frigate's login port (8971) the browser cannot load them, so give the hub the Frigate login and it serves them itself: media on that origin is rewritten to `/api/v1/media?url=…`, and the hub fetches it with Frigate's session cookie (logging in on demand, once more on a 401). Any other origin is refused. Leave `user` and `password` out for the unauthenticated port (5000).

```toml
[frigate]
url = "http://192.168.1.20:8971"
user = "oikos"
password = "…"
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
| `GET /api/v1/alerts` | The push alerts OikosPush keeps in its retained history, newest first. |
| `POST /api/v1/alerts/{id}/actions/{action}` | Publishes the reply the app publishes when a button is tapped (`{"id","action","install_id":"oikos-web","ts"}` on the responses topic); `202`, or `404` for an unknown alert or action, `503` while the broker link is down. |
| `GET /api/v1/media?url=…` | A Frigate snapshot or clip fetched by the hub with the Frigate login (`[frigate]`); `Range` is passed through so clips seek. `400` without a valid URL, `403` off the Frigate origin, `502` when Frigate is unreachable. |
| `GET /api/v1/flows` | Every flow with its engine state: definition, why it is invalid, the active run and the recent runs. |
| `GET /api/v1/flows/{id}` | One flow; `404` for an unknown id. |
| `PUT /api/v1/flows/{id}` | Creates or replaces a flow (`oikos/automation/<id>`). The path id wins: a body without `id` takes it, another one is `400 id mismatch`. `422` for a flow that does not validate, `503` while the broker link is down. |
| `DELETE /api/v1/flows/{id}` | Removes the flow (204), or `404`. |
| `POST /api/v1/flows/{id}/enabled` | `{"enabled": bool}`; returns the flow. |
| `POST /api/v1/flows/{id}/run` | Starts a run by hand; `202` with the run, `422` when the flow is disabled, invalid or already running. |

Errors are `{"error": "…"}` with 400 (bad patch, or a flow body whose `id` differs from the path: `id mismatch`), 404 (unknown entity, flow or route), 422 (command refused, or a flow that is invalid, disabled or already running) or 503 (the broker link is down, so the write was refused and nothing was applied; the same publish refusal is an in-band tool error over MCP). Every other path serves the web UI (see [Web UI](#web-ui)).

## MCP

The MCP server at `http://<host>:8080/mcp` (streamable HTTP) offers seventeen tools and one resource:

- `bridge_info` — the Zigbee2MQTT bridge and the state of the link to the broker.
- `list_areas` — the rooms, each with its entity count and the count of entities shown by default (`primary_count`).
- `set_areas` — replaces the declared rooms and returns the resulting list.
- `list_devices` — the Zigbee devices with availability, battery and link quality, optionally filtered by area or by name, model or vendor.
- `list_entities` — a page of entities with their state; by default only what the app shows (`include: "all"` for everything).
- `get_entity` — one entity with its state, its MQTT bindings and its raw override.
- `set_override` — changes fields of the app's `oikos/ui/<id>` override (`null` clears a field).
- `clear_override` — removes the override entirely.
- `send_command` — publishes a command and reports `accepted` once the device echoes the change.
- `search` — devices (name, model, vendor) and entities (name, id, or belonging to a matching device) containing the query.
- `list_ignored` — the discovery topics that did not become an entity, with the reason.
- `list_flows` — the automations (flows) with name, enabled state, node count and latest run.
- `get_flow` — one flow with its definition and recent runs.
- `set_flow` — creates or replaces a flow (`trigger.state`, `if`, `delay` and `command` nodes; `mode: restart` for "N minutes after the last …"), validated against the house.
- `delete_flow` — removes a flow.
- `set_flow_enabled` — turns a flow on or off.
- `run_flow` — starts a manual run of a flow.
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

## Web UI

The web UI is a Leptos app that runs in the browser as wasm, built with Trunk. The hub embeds its build output, `crates/oikos-web/dist/`, with rust-embed and serves it on the hub's port for every path that is not `/api`, `/ws` or `/mcp`. Unknown paths fall back to `index.html`, so a deep link such as `/entity/<id>` loads the app. `index.html` is never cached; the hashed assets are.

Screens:

- Home: room chips to filter by area, and one card per entity kind.
- Entity sheet (`/entity/<id>`): the controls, the device info, the raw state and the override editor.
- Devices: the administration view of the Zigbee devices.
- Settings: the hub status and the theme switch (Auto, Night, Warm).
- Alerts (`/alerts`): the push alerts OikosPush keeps, with channel chips and day groups, and a "Recent alerts" section on Home; the detail (`/alerts/<id>`) shows the snapshot, plays the clip, offers the alert's buttons and "Open device". Same screens as the Android app; the body stays in the producer's language.
- Flows (`/flows`, `/flows/<id>`): the flows list with an enable switch per flow, and a read-only canvas per flow with the last run lit on the graph and the run history below it.

Development runs in two terminals:

```sh
cargo run -p oikos-os -- run       # the hub on 8080
cd crates/oikos-web && trunk serve # the UI on 127.0.0.1:3000, rebuilt on save
```

Open `http://127.0.0.1:3000`. Trunk proxies `/api`, `/ws` and `/mcp` to the hub on 8080.

Release builds embed the UI, so build it before the hub:

```sh
cd crates/oikos-web && trunk build --release
cd ../.. && cargo build --release -p oikos-os
```

The hub reads `dist/` when it is compiled. Rebuild the hub after every `trunk build`. A binary built before `trunk build` serves a 503 page that says the UI has not been built.

The smoke test mounts the app in headless Chrome and checks the shell:

```sh
cd crates/oikos-web && wasm-pack test --headless --chrome
```

Install the tools with `cargo install trunk --locked` and `cargo install wasm-pack --locked`. `wasm-pack` fetches a matching chromedriver on first use; Google Chrome must be installed.

## Flows

A flow is an n8n-style automation: a small graph of nodes stored as one retained JSON document under `oikos/automation/<id>` on the broker (the id is `[a-z0-9_-]{1,64}`). The hub subscribes to that prefix, validates every document it reads, and runs the valid, enabled ones. An invalid document is kept and shown with the reason, but never runs.

```json
{
  "id": "living_led_motion", "name": "Living LED on motion", "enabled": true, "mode": "restart",
  "nodes": [
    { "id": "motion", "type": "trigger.state", "params": { "entity": "0x00158d00044c4d4c_occupancy_zigbee2mqtt" } },
    { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
    { "id": "on", "type": "command", "params": { "entity": "<light id>", "command": { "type": "turn_on" } } },
    { "id": "wait", "type": "delay", "params": { "for": "5m" } },
    { "id": "off", "type": "command", "params": { "entity": "<light id>", "command": { "type": "turn_off" } } }
  ],
  "edges": [
    { "from": "motion", "to": "moving" },
    { "from": "moving", "port": "true", "to": "on" },
    { "from": "moving", "port": "false", "to": "wait" },
    { "from": "wait", "to": "off" }
  ]
}
```

| Type | Params | Outputs | Behaviour |
| --- | --- | --- | --- |
| `trigger.state` | `entity` (id), optional `to`, optional `from` | one | Starts a run when the entity's scalar changes to a different value. `to`/`from` filter on the new/previous scalar. Never fires on the first value after boot or reconnect, on an availability-only change, or on an equal-value report. |
| `if` | `op` one of `eq ne gt lt gte lte`, `value` (bool, number or string), optional `entity` | `true`, `false` | Compares the scalar of `entity` (default: the entity that triggered the run, read live) to `value`. `gt lt gte lte` need both sides numeric, otherwise the node fails. |
| `delay` | `for`: `"90s"`, `"5m"`, `"1h30m"` | one | Pauses the run. Cancelled with the run. |
| `command` | `entity` (id), `command` (the API's `Command` JSON) | one | Sends the command through the hub's command path, confirmed by the entity's state echo within `confirm_timeout_ms`, exactly like the API's `send_command`. No echo, a rejection or an offline entity fails the node. |

A run follows the graph from the trigger that fired, depth first. A failing node ends the run as `failed` at that step. A non-trigger node with no incoming edge never runs.

The scalar is the one value of a state that triggers and `if` compare: the on/off bool, the number, the text, a light's `on`, a cover's `state`.

`mode` says what a trigger does while a run of the same flow is still active. `single` (the default) ignores the new trigger and records a skipped run. `restart` cancels the active run and starts a new one, which is what makes "5 minutes after the last motion" work: motion coming back cancels the pending `delay`.

Two config keys: `[topics] automation_prefix` (default `oikos/automation`) and `[automation] runs_kept` (recent runs remembered per flow, default 20). Runs live in memory only: a hub restart forgets them, the flow definitions stay on the broker.

The assistant writes flows through the `set_flow` MCP tool (with `list_flows`, `run_flow` and friends), and the web UI shows them at `/flows` with the last run lit on the canvas. See [HTTP API](#http-api) for the REST routes.

## Security

There is none yet, and that is deliberate for v1: the hub binds on `0.0.0.0`, answers any origin (CORS `*`) and its MCP endpoint accepts any `Host` header. Any device on the LAN, and any web page open in a browser on the LAN (through DNS rebinding), can therefore read the whole house and send commands, alarm arming and disarming included. Keep the hub off the internet: no port forwarding, no exposure through a reverse proxy without authentication in front of it. `get_entity` and the `bindings` it returns hand out the raw discovery config; for a bridge other than Zigbee2MQTT that config could carry credentials in topics or templates. With a `[frigate]` section the hub also fetches alert media from Frigate with the configured login; it only ever requests an event's `snapshot.jpg`, `clip.mp4` or `thumbnail.jpg` on that origin and follows no redirect, but any LAN client can request those, so give it a Frigate *viewer* account, not the admin. Authentication and TLS arrive in a later phase.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo check -p oikos-core --target wasm32-unknown-unknown
cargo check -p oikos-api --target wasm32-unknown-unknown
cargo check -p oikos-web --target wasm32-unknown-unknown
```

The `wasm32-unknown-unknown` target comes with `rust-toolchain.toml`. The tests run against recorded broker messages under `crates/oikos-core/tests/fixtures` and never need a broker; refresh the recording from the house with `MQTT_HOST=<broker> MQTT_USER=… MQTT_PASS=… tools/record-fixtures.sh` (credentials are read from the environment only).

## License

Apache-2.0, Copyright 2026 Gekkotron.
