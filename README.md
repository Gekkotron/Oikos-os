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

The hub sends the house's push notifications to the Android app through Firebase Cloud Messaging (see [Push notifications](#push-notifications)). Without `credentials` it keeps the alert history and sends nothing, with one warning at start; a key file that is missing or unreadable is the same, and the warning names its path. The hub starts either way.

```toml
[push]
prefix = "oikos/push"            # <prefix>/notification/<source>, /tokens/<installId>, /responses, /history
legacy_prefix = "oikos-push"     # OikosPush's topics, still read during the switch-over; "" turns them off
credentials = "/path/to/firebase-service-account.json"  # git-ignored, never logged; leave out to send nothing
```

`retry_interval` (60 s), `max_retries` (10), `history_size` (100 alerts) and `history_days` (7) keep OikosPush's values. The old `history_topic` and `responses_topic` keys are refused at start: the error names the keys `[push]` takes.

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

Devices that Zigbee2MQTT does not cover reach the hub through a bridge publishing the same discovery: see [Adding a device](docs/adding-a-device.md).

The [WaterHeaterController](https://github.com/Gekkotron/WaterHeaterController) firmware publishes no discovery, so publish it once, retained, for its existing `water-heater/*` topics:

```sh
mosquitto_pub -h 192.168.1.91 -u <user> -P <password> -r -t oikos/device/water_heater/config -m '{"dev":{"ids":["water_heater_controller"],"name":"Water heater","mf":"Gekkotron","mdl":"WaterHeaterController"},"stat_t":"water-heater/data","cmps":{"heater":{"p":"water_heater","name":null,"uniq_id":"water_heater","unit_of_meas":"°C","modes":[],"curr_temp_t":"water-heater/data","curr_temp_tpl":"{{ (80 + (0.35 - value_json.adc_result_mv / 1000) * 86.67) | round(1) }}","mode_stat_t":"water-heater/data","mode_stat_tpl":"{{ \"heat\" if (value_json.triac_0_power + value_json.triac_1_power + value_json.triac_2_power) > 0 else \"off\" }}","json_attr_t":"water-heater/data","json_attr_tpl":"{{ {\"inlet_temperature\": value_json.temp_0 | round(1), \"power\": ((value_json.triac_0_power + value_json.triac_1_power + value_json.triac_2_power) / 3) | round | int} | tojson }}"},"tank_temp":{"p":"sensor","name":"Tank temperature","uniq_id":"water_heater_tank_temp","dev_cla":"temperature","stat_cla":"measurement","unit_of_meas":"°C","val_tpl":"{{ (80 + (0.35 - value_json.adc_result_mv / 1000) * 86.67) | round(1) }}"},"power_0":{"p":"number","name":"Power 1","uniq_id":"water_heater_power_0","cmd_t":"water-heater/control","cmd_tpl":"{\"power0\": {{ value | int }}}","val_tpl":"{{ value_json.triac_0_power }}","min":0,"max":100,"step":1,"unit_of_meas":"%"},"power_1":{"p":"number","name":"Power 2","uniq_id":"water_heater_power_1","cmd_t":"water-heater/control","cmd_tpl":"{\"power1\": {{ value | int }}}","val_tpl":"{{ value_json.triac_1_power }}","min":0,"max":100,"step":1,"unit_of_meas":"%"},"power_2":{"p":"number","name":"Power 3","uniq_id":"water_heater_power_2","cmd_t":"water-heater/control","cmd_tpl":"{\"power2\": {{ value | int }}}","val_tpl":"{{ value_json.triac_2_power }}","min":0,"max":100,"step":1,"unit_of_meas":"%"},"temp_0":{"p":"sensor","name":"Cold water inlet temperature","uniq_id":"water_heater_temp_0","dev_cla":"temperature","stat_cla":"measurement","unit_of_meas":"°C","val_tpl":"{{ value_json.temp_0 | round(1) }}"},"temp_1":{"p":"sensor","name":"Triac temperature","uniq_id":"water_heater_temp_1","dev_cla":"temperature","stat_cla":"measurement","unit_of_meas":"°C","val_tpl":"{{ value_json.temp_1 | round(1) }}","ent_cat":"diagnostic"},"frequency":{"p":"sensor","name":"Mains frequency","uniq_id":"water_heater_frequency","dev_cla":"frequency","stat_cla":"measurement","unit_of_meas":"Hz","ent_cat":"diagnostic","val_tpl":"{{ value_json.measuredFrequency | round(1) }}"},"frequency_error":{"p":"binary_sensor","name":"Mains frequency error","uniq_id":"water_heater_frequency_error","dev_cla":"problem","ent_cat":"diagnostic","pl_on":"1","pl_off":"0","val_tpl":"{{ value_json.frequencyError }}"}}}'
```

## Push notifications

The hub took over from OikosPush. Producers publish an alert (OikosPush's v1 or legacy payload) on `oikos/push/notification/<source>`; the hub normalises it, adds it to its history and sends it through FCM to every app install registered on `oikos/push/tokens/<installId>`. Sends that fail for a transient reason are retried from memory every `retry_interval` seconds, up to `max_retries` times; a dead token is cleared on the broker. Parts of the hub that raise alerts themselves hand them to the same path in process, without the broker.

| Topic | Direction | Retained | Content |
| --- | --- | --- | --- |
| `oikos/push/notification/<source>` | producer → hub | no | v1 or legacy payload |
| `oikos/push/tokens/<installId>` | app → hub | yes | `{token, device_name, lang, updated_at}`; an empty payload unregisters |
| `oikos/push/responses` | app / web → producer | no | `{id, action, install_id, ts}` |
| `oikos/push/history` | hub → app, web | yes | JSON array of the normalised alerts, newest first |

After each (re)connect the hub waits for its subscription to be acknowledged, then 2 s more for the retained tokens, before it sends; alerts arriving meanwhile wait in the queue. The history is the retained `oikos/push/history` itself: the hub adopts it at its first sync, and only when that topic holds no list yet does it seed it once from OikosPush's `oikos-push/history`.

While `legacy_prefix` is set the hub also reads `oikos-push/notification/+`, `oikos-push/tokens/+` and `oikos-push/history`, so producers outside this repo and an app build that is not updated yet keep working; a token registered on both topics is sent to once. The hub never publishes there, except an empty retained payload clearing a dead token that came from `oikos-push/tokens/<id>`.

### Switching over from OikosPush

0. **Broker ACL.** The hub connects as the `[mqtt] user` of `oikos.toml`. That user needs:
   - [ ] read and write on `oikos/push/#`;
   - [ ] read on `oikos-push/notification/+` and `oikos-push/history`;
   - [ ] read and write on `oikos-push/tokens/+` (the hub clears dead tokens there);
   - [ ] read on `<frigate prefix>/events` and `encryption-gateway/health/+`.

   Mosquitto denies silently, but the Python ACL watchdog keeps running throughout, so a missing grant shows up on the phone as a system alert. The ACL file lives on the broker host, outside this repo.
1. Ship the hub with push on but **no `[push] credentials`**, and the Frigate bridge and the gateway watchdog **off**: it builds its history from the producers' notifications and shows alerts, and sends nothing. OikosPush is still the sender. Check that the hub's history matches OikosPush's: for alerts received while both run, the same ids in the same order with the same `cancel` state. A mismatch is a bug to fix before step 2.
2. Stop the OikosPush core, the Frigate bridge container and the gateway watchdog container (`docker compose down` on the broker host). Leave the ACL watchdog container running.
3. Set `[push] credentials` (copy the service-account key from the OikosPush deploy directory, git-ignored, never printed), turn `[frigate] bridge` and `[watchdog.gateway]` on, and restart the hub.
4. Send a test notification over MCP and check that it arrives once. Trigger a camera event and check the snapshot, then the clip.

Until step 3 both the hub and OikosPush publish a retained history, on different topics (`oikos/push/history` and `oikos-push/history`); the current app reads the old one, so nothing changes for the phone.

## HTTP API

The hub listens on `[http] bind` (default `0.0.0.0:8081`: next to Jeedom on 80 and clear of Zigbee2MQTT on 8080 on the same server), CORS open to any origin, JSON in and out.

| Route | What it does |
| --- | --- |
| `GET /health` | `status`, the broker connection state and the entity count. |
| `GET /api/v1/snapshot` | The whole house: connection, bridge, areas, devices, entities with their state, ignored topics. |
| `POST /api/v1/entities/{id}/command` | Sends a command (`{"type": "turn_on", …}`) and reports whether the device confirmed it. |
| `PATCH /api/v1/entities/{id}/override` | Changes fields of the entity's `oikos/ui/<id>` override and returns the entity as it now appears. |
| `DELETE /api/v1/entities/{id}/override` | Removes the override (204). |
| `GET /api/v1/entities/{id}/picture` | The picture of a camera or image entity: the stored bytes, or the bytes of its URL fetched by the hub now (10 s timeout, 5 MiB cap, no redirects followed). See [Entity types](#entity-types). |
| `PUT /api/v1/areas` | Replaces the declared rooms (`oikos/ui/_areas`) and returns the resulting list. |
| `PATCH /api/v1/areas/{id}/groups/{kind}` | Sets or clears (`null`) the room group card name and the favourite flag of one kind (`cover` or `light`) in the area. `200` with the area list, `422` for a kind other than `cover`/`light`, `Unassigned` or an empty body, `404` for an unknown area, `503` while the broker link is down. |
| `PUT /api/v1/virtual/{id}` | Creates or replaces a virtual switch, number or button (`{"kind", "name", …}`) and returns the entity `virtual_<id>`; the initial state is published only when it is new. `422` for a bad id or spec, `503` while the broker link is down. See [Adding a device](docs/adding-a-device.md#virtual-entities). |
| `DELETE /api/v1/virtual/{id}` | Removes a virtual entity and clears its retained topics (204), or `404`. |
| `GET /api/v1/ignored` | The discovery topics that did not become an entity, each with the reason. |
| `GET /ws` | WebSocket: a snapshot first, then every hub event as a JSON text frame. |
| `GET /api/v1/alerts` | The push alerts the hub keeps in its retained history (`oikos/push/history`), newest first. |
| `POST /api/v1/alerts/{id}/actions/{action}` | Publishes the reply the app publishes when a button is tapped (`{"id","action","install_id":"oikos-web","ts"}` on `oikos/push/responses`); `202`, or `404` for an unknown alert or action, `503` while the broker link is down. |
| `GET /api/v1/push` | Push status: `{"state","installs","queued"}`, `state` being `off` (no usable `[push] credentials`), `syncing` (rebuilding the token map after a connect) or `ready`; `installs` counts the registered app installs, `queued` the alerts waiting in the retry queue. |
| `GET /api/v1/media?url=…` | A Frigate snapshot or clip fetched by the hub with the Frigate login (`[frigate]`); `Range` is passed through so clips seek. `400` without a valid URL, `403` off the Frigate origin, `502` when Frigate is unreachable. |
| `GET /api/v1/schedule?cron=…&count=…` | Previews a 5-field cron in the house local time: the next `count` ticks. `200` with the preview, `422` for a bad cron. |
| `GET /api/v1/flows` | Every flow with its engine state: definition, why it is invalid, the active run and the recent runs. |
| `GET /api/v1/flows/{id}` | One flow; `404` for an unknown id. |
| `PUT /api/v1/flows/{id}` | Creates or replaces a flow (`oikos/automation/<id>`). The path id wins: a body without `id` takes it, another one is `400 id mismatch`. `422` for a flow that does not validate, `503` while the broker link is down. |
| `DELETE /api/v1/flows/{id}` | Removes the flow (204), or `404`. |
| `POST /api/v1/flows/{id}/enabled` | `{"enabled": bool}`; returns the flow. |
| `POST /api/v1/flows/{id}/run` | Starts a run by hand; `202` with the run, `422` when the flow is disabled, invalid or already running. |
| `GET /api/v1/history/series?entity=&range=` | A chart series over `24h`, `7d`, `30d` or `1y`: `{window, points:[[t,mean,min,max],…]}`. See [History](#history). |
| `GET /api/v1/history/daily?entity=&days=&kind=` | kWh per calendar day for the last `days` days (7, 30 or 365), with `kind` (`counter` or `power`). |
| `GET /api/v1/history/today?entity=&kind=` | kWh since local midnight: `{kwh, since}`. |
| `GET /api/v1/history/last_change?entity=&entity=…` | Last change time of 1 to 50 entities, in request order. |

Errors are `{"error": "…"}` with 400 (bad patch, or a flow body whose `id` differs from the path: `id mismatch`), 404 (unknown entity, flow, virtual entity or route), 422 (command refused, a bad virtual id or spec, or a flow that is invalid, disabled or already running) or 503 (the broker link is down, so the write was refused and nothing was applied; the same publish refusal is an in-band tool error over MCP). Every other path serves the web UI (see [Web UI](#web-ui)).

## MCP

The MCP server at `http://<host>:8081/mcp` (streamable HTTP) offers twenty tools and one resource:

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
- `set_flow` — creates or replaces a flow (`trigger.state`, `trigger.schedule`, `flow.enable`, `if`, `delay` and `command` nodes; `mode: restart` for "N minutes after the last …"), validated against the house. A `trigger.schedule` fires only while the flow is enabled; `flow.enable` turns another flow (or its own) on or off.
- `delete_flow` — removes a flow.
- `set_flow_enabled` — turns a flow on or off.
- `run_flow` — starts a manual run of a flow.
- `set_virtual` — creates or replaces a virtual switch, number or button the hub owns.
- `delete_virtual` — removes a virtual entity.
- `get_history` — one slice of an entity's history: a chart `series` over a `range`, kWh per `daily` with `days` and `kind`, kWh `today` with `kind`, or `last_change`. Available only when `[history] url` is set.
- Resource `oikos://snapshot` — the whole house as one JSON document.

A failed tool call answers in-band (`isError: true` with the reason) so the assistant can retry with better arguments.

Claude Code:

```sh
claude mcp add --transport http oikos http://<host>:8081/mcp
```

Codex CLI, in `~/.codex/config.toml`:

```toml
[mcp_servers.oikos]
url = "http://<host>:8081/mcp"
```

Claude Desktop only speaks stdio, so it launches `oikos-os mcp-stdio`, a proxy that relays every request to the running hub. In its config JSON, with the path of a built binary (`cargo build --release -p oikos-os`):

```json
{
  "mcpServers": {
    "oikos": {"command": "<path>/oikos-os", "args": ["mcp-stdio", "--server", "http://<host>:8081"]}
  }
}
```

The proxy exits with `cannot reach Oikos-os MCP server at …` when the hub is down; its logs go to stderr, stdout carries the protocol.

## Web UI

The web UI is a Leptos app that runs in the browser as wasm, built with Trunk. The hub embeds its build output, `crates/oikos-web/dist/`, with rust-embed and serves it on the hub's port for every path that is not `/api`, `/ws` or `/mcp`. Unknown paths fall back to `index.html`, so a deep link such as `/entity/<id>` loads the app. `index.html` is never cached; the hashed assets are.

Screens:

- Dashboards (`/dashboards/<id>`): the top nav opens a `Dashboards ▾` picker. House is always the first entry — the comprehensive rooms overview, synthesized by the hub (id `"house"`, non-editable, non-deletable), rendered on the client with the existing rooms-overview screen. Below House are user-authored named dashboards built from widgets. A widget is one of: `light`, `cover`, `climate`, `lock`, `sensor`, `room-group`, `flow-button`, `energy-stat`, `history-chart`, `camera`, `water-heater`, each with a kind-specific config and a `span` from a kind-specific set (`1x1`, `2x1`, `2x2`, `1x2`). The grid adapts phone (2 columns) ↔ tablet (4) ↔ desktop (6). On a user-authored dashboard the header's pencil enters edit mode: click a widget to open its config sheet (title, size, delete), use move left/right to reorder, cycle the span with the resize button, Save or Discard. The gear opens the dashboard settings sheet (name, delete, move up/down in the picker), and `＋ New dashboard` at the bottom of the picker creates a new one at `max(order) + 1`. Dashboards persist as MQTT retained payloads under `<ui_prefix>/dashboards/<id>` (the Home section's placement rule still applies on House — an entity with an energy role or shown on Security leaves House unless its override pins it with `homePinned`; cover/light rooms still fold into group cards exactly as before).
- Energy (`/energy`): the live power flow as in the app. Solar on top, the house in the centre, the grid on the left, consumers on an arc to the right, the battery or the water meter bottom-left; lines thicken and run faster with power and follow import or export. Under the diagram, one card per node with the live value and the lifetime kWh of its paired energy counter. Roles come from the `energyRole` override (`solar`, `grid`, `grid_export`, `battery`, `house`, `consumer`, `water`), set from the entity sheet or over MCP.
- Security (`/security`): the alarm panel as a hero card (arm modes, Disarm, inline code when the panel asks for one), the opted-in actions (`securityAction`, asking first unless `securityAsk` is off), a summary line ("1 smoke · 2 open" or "All clear"), the safety and intrusion sensors in sections (Openings, Motion, Fire, Water, Other) as two-column tiles, and the recent alerts. `securityHidden` takes a sensor off the tab.
- Entity sheet (`/entity/<id>`): the controls, the device info, the raw state and the override editor.
- Devices: the administration view of the Zigbee devices.
- Settings: the hub status, an MCP panel with the endpoint and copy-ready connection snippets for Claude Code, Codex and Claude Desktop (built from the page's origin, so they always carry the hub's real address), and the theme switch (Auto, Night, Warm).
- Alerts (`/alerts`): the push alerts the hub keeps in `oikos/push/history`, with channel chips and day groups, and a "Recent alerts" section on Home; the detail (`/alerts/<id>`) shows the snapshot, plays the clip, offers the alert's buttons and "Open device". Same screens as the Android app; the body stays in the producer's language.
- Flows (`/flows`, `/flows/<id>`): the flows list with an enable switch per flow and a **New flow** button, and a full-bleed canvas per flow (see [Flows](#flows)).

Development runs in two terminals:

```sh
cargo run -p oikos-os -- run       # the hub on 8081
cd crates/oikos-web && trunk serve # the UI on 127.0.0.1:3000, rebuilt on save
```

Open `http://127.0.0.1:3000`. Trunk proxies `/api`, `/ws` and `/mcp` to the hub on 8081.

Release builds embed the UI, so build it before the hub:

```sh
cd crates/oikos-web && trunk build --release
cd ../.. && cargo build --release -p oikos-os
```

The hub reads `dist/` when it is compiled. Rebuild the hub after every `trunk build`. A binary built before `trunk build` serves a 503 page that says the UI has not been built.

`tools/restart.sh` does both builds, stops the running hub and starts the new one (`--debug` for the debug profile, `--no-web` to reuse `dist/`). It warns when `target/` passes 10 GB; `--sweep` first deletes build artifacts untouched for 14 days (`cargo install cargo-sweep`).

The smoke test mounts the app in headless Chrome and checks the shell:

```sh
cd crates/oikos-web && wasm-pack test --headless --chrome
```

Install the tools with `cargo install trunk --locked` and `cargo install wasm-pack --locked`. `wasm-pack` fetches a matching chromedriver on first use; Google Chrome must be installed.

## Entity types

Components beyond the hand-written kinds are described by a JSON type table, `crates/oikos-core/src/types/table.json`, compiled into the hub. Each entry says where the entity shows (`placement`), which field is its primary state, which controls it offers, what puts it in alert style and how its history is charted. Sixteen components are covered: `camera`, `device_automation`, `device_tracker`, `event`, `fan`, `humidifier`, `image`, `lawn_mower`, `lock`, `notify`, `scene`, `siren`, `tag`, `text`, `update` and `valve`. Only `device` (device-based discovery) is folded by routing; any other component the table does not describe is reported as `ignored component <name>` under `GET /api/v1/ignored`.

Some behaviour to know about:

- Secret text (`mode: password`) is masked as `••••` on every state path out of the hub, and it is left out of history.
- Several `device_automation` configs for one device fold into a single remote entity, whose `capabilities.generic.signals` lists their payloads. `trigger.signal` in a flow fires on those remotes, on events and on tags.
- A per-entity override `mainControl` names one announced control, or `"none"` to hide the main control.
- `update` entities feed the firmware marks (`installed`, `latest`, `available`) that the device view shows.

The hub publishes the table retained on `oikos/ui/_types` (`{ui_prefix}/_types`) on every connection, next to `oikos/ui/_areas` and the per-entity `oikos/ui/<id>` overrides, so the Android app can read it. Its own echo is ignored. The table's vocabulary is explained in [Adding a device](docs/adding-a-device.md#a-type-table-entry-json-no-rust).

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
| `trigger.state` | `entity` (id), optional `to`, optional `from` | one | Starts a run when the entity's scalar changes to a different value. `to`/`from` filter on the new/previous scalar. The replayed value after boot or reconnect arms the entity silently; it never fires on that replay, on an availability-only change, or on an equal-value report. |
| `trigger.schedule` | `cron` (5 fields, house local time) | one | Starts a run at each wall-clock tick while the flow is enabled; the first run is at the next tick. |
| `flow.enable` | optional `flow` (id), `enabled` (bool) | one | Enables or disables the target flow, its own by default. A flow that disables itself still finishes the current run. |
| `if` | `op` one of `eq ne gt lt gte lte`, `value` (bool, number or string), optional `entity`, optional `missing` (`fail`, `true` or `false`) | `true`, `false` | Compares the scalar of `entity` (default: the entity that triggered the run, read live) to `value`. `gt lt gte lte` need both sides numeric, otherwise the node fails. When the value is unknown, `missing` picks that port; the default `fail` fails the node. |
| `delay` | `for`: `"90s"`, `"5m"`, `"1h30m"` | one | Pauses the run. Cancelled with the run. |
| `command` | `entity` (id), `command` (the API's `Command` JSON) | one | Sends the command through the hub's command path, confirmed by the entity's state echo within `confirm_timeout_ms`, exactly like the API's `send_command`. No echo, a rejection or an offline entity fails the node. |

A run follows the graph from the trigger that fired, depth first. A failing node ends the run as `failed` at that step. A non-trigger node with no incoming edge never runs.

The scalar is the one value of a state that triggers and `if` compare: the on/off bool, the number, the text, a light's `on`, a cover's `state`.

`mode` says what a trigger does while a run of the same flow is still active. `single` (the default) ignores the new trigger and records a skipped run. `restart` cancels the active run and starts a new one, which is what makes "5 minutes after the last motion" work: motion coming back cancels the pending `delay`.

Two config keys: `[topics] automation_prefix` (default `oikos/automation`) and `[automation] runs_kept` (recent runs remembered per flow, default 20). Runs live in memory only: a hub restart forgets them, the flow definitions stay on the broker.

The assistant writes flows through the `set_flow` MCP tool (with `list_flows`, `run_flow` and friends), and the web UI shows them at `/flows`. See [HTTP API](#http-api) for the REST routes.

The flow page (`/flows/<id>`) is a full-bleed canvas: each node is a pill with a coloured icon and a sentence title ("Motion at Entrée", "Turn off LED", "Wait 5 min"), joined by routed wires that go around the cards, with green "yes" and red "no" labels on the two outputs of a check. The last run, or one clicked in the Runs drawer, is replayed on the graph: visited cards and wires lit, the rest dimmed, failed steps red; a run in progress pulses live. The **Edit** button switches to a React Flow-style editor: click a card to select it (amber ring) and edit its settings in the drawer, drag it to move it, drag from an output pin onto another card to wire it, click a wire for its ✕, press ⌫ to delete the selection and ⌘/Ctrl-Z to undo. **＋ Add** opens a searchable picker grouped WHEN / AND / THEN / WAIT / FLOW (type an entity name to list its triggers, checks and actions), **Tidy** re-lays the graph out, and nothing reaches the hub until **Save** (**Cancel** discards the draft). **New flow** on `/flows` opens an unsaved draft in edit mode; its first Save creates the flow, disabled, under a slug of its name. On a phone (narrow or touch-only screens) the canvas is view-only: Edit and New flow are hidden, while pan, pinch-zoom, the drawer, the switch and Run still work.

## History

The hub records every entity state change in TimescaleDB for years, so the web UI and MCP can answer "what did this sensor read" and "how much energy today". History is off by default; point the hub at a TimescaleDB instance by adding `[history]` to `oikos.toml`:

```toml
[history]
url = "postgres://oikos:<password>@<host>:<port>/oikos"
buffer_rows = 100000       # states queued when the database is down
```

Any TimescaleDB 2.17+ on Postgres 17 works, as long as the role can log in and `CREATE EXTENSION timescaledb` has been run inside the target database (the extension itself must be in `shared_preload_libraries` on the server). The hub applies its own schema migrations on first connect, writes every state change inline from `Hub::ingest` (never on the hot MQTT path — the writer runs in a bounded, try-send queue) and rolls each hour up into `history_hourly` every five minutes. A down database never blocks the house: up to `buffer_rows` states wait, past that the oldest are dropped with one warning per outage.

Routes under `/api/v1/history/*` (see the table above): `series` for charts, `daily` for kWh per day over 7, 30 or 365 days, `today` for kWh since local midnight, and `last_change` for 1 to 50 entities. Errors are `400 bad_request`, `404 not_recorded` (the entity has no rows) and `503 history_unavailable` (`[history] url` not set, or the database is down). MCP exposes the same through `get_history`.

Back up with `pg_dump` from any client that can reach the database:

```sh
pg_dump -h <host> -p <port> -U oikos -Fc oikos > oikos-history-$(date +%F).dump
```

## Security

There is none yet, and that is deliberate for v1: the hub binds on `0.0.0.0`, answers any origin (CORS `*`) and its MCP endpoint accepts any `Host` header. Any device on the LAN, and any web page open in a browser on the LAN (through DNS rebinding), can therefore read the whole house and send commands, alarm arming and disarming included. Keep the hub off the internet: no port forwarding, no exposure through a reverse proxy without authentication in front of it. `get_entity` and the `bindings` it returns hand out the raw discovery config; for a bridge other than Zigbee2MQTT that config could carry credentials in topics or templates. With a `[frigate]` section the hub also fetches alert media from Frigate with the configured login; it only ever requests an event's `snapshot.jpg`, `clip.mp4` or `thumbnail.jpg` on that origin and follows no redirect, but any LAN client can request those, so give it a Frigate *viewer* account, not the admin. Flows are retained on the broker in clear, so the hub refuses a flow whose `command` node carries an alarm code; arm and disarm from a flow only on a panel that needs none. Authentication and TLS arrive in a later phase.

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
