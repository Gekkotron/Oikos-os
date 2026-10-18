# Oikos-os — v1 design

Date: 2026-10-18 · Status: approved 2026-10-18 · Author: Gekkotron

## 1. Intent

Oikos-os is the Rust home-automation hub that replaces Jeedom in the Gekkotron house. It runs on the home server next to Mosquitto, owns the house's device registry, and is the brain behind the existing Android app **Oikos** (`SourcesPerso/Oikos`). Over time it absorbs the Python services around the app (OikosPush, OikosHistory) and the Jeedom automations. It is configured through conversation: an MCP server lets an AI assistant (Claude) inspect and change the house, and later author automations, instead of a rules editor.

**Why now.** Jeedom is the last piece of the old stack. The app already runs without it (HA MQTT discovery straight from Zigbee2MQTT), so the hub can be rebuilt from an empty repo without breaking the house. An earlier attempt at this repo (the "Oikos/Moli" codebase) is abandoned; nothing of it is reused.

**Success for v1.** From a Mac on the home Wi-Fi, `cargo run` connects to the home broker, every Zigbee2MQTT device shows up in a beautiful web dashboard with live state and working controls, and Claude can list devices and change how a device is presented in the app (for example, show a switch as a light, give a power sensor an energy role, move a device to a room) through MCP, with the app picking the change up immediately.

## 2. Goals, non-goals, anti-goals

### Goals (v1)
1. **Device registry from MQTT.** Discover every entity published in HA MQTT discovery format under the Oikos discovery prefix `oikos/#` (Zigbee2MQTT is pointed at it with `homeassistant.discovery_topic: oikos`), every Zigbee device from `zigbee2mqtt/bridge/devices`, and every override under `oikos/ui/#`. Keep them live.
2. **Live state and commands.** Subscribe to every entity's state, attribute and availability topics, evaluate the discovery templates, and send commands on the command topics, exactly as the app does.
3. **MCP server** over plain HTTP on the LAN, no auth, with tools to list and inspect devices, entities, areas and state, to send commands, and to read and write `oikos/ui/…` overrides.
4. **Web UI**, Rust end to end (Leptos → WebAssembly), served by the same binary: a dashboard of all devices by room with live state and controls, a device detail sheet, and an override editor. It is a sibling of the Android app: same palettes, type and card language.
5. **Shared contract with the app.** `oikos/ui/<unique_id>` and `oikos/ui/_areas` are the single source of truth for presentation overrides, with the app's JSON field names and semantics unchanged.
6. **Runs on the Mac first.** No Docker, no systemd. `cargo run` with a `oikos.toml`.

### Non-goals (v1), planned for later phases
- Automations and scenes (phase 2, through MCP).
- Push notifications, FCM, replacing OikosPush (phase 3).
- Frigate events and media (phase 3).
- History, charts, replacing OikosHistory and Athena-Core; InfluxDB stays external for now (phase 4, behind a storage trait).
- Authentication, TLS, remote access through the Athena gateway.
- Docker image, Linux deployment, update script.
- Other MQTT sources as first-class devices (Jeedom `jeedom/data`, Philips, Roborock, water heater…). Whatever they publish in discovery format under a configured prefix is still discovered, since the registry is source-agnostic.
- Voice shortcuts (`oikos/voice/#`), energy screen, security screen.
- Zigbee network administration (pairing, renaming, OTA, bindings) beyond read-only device information.

### Anti-goals (what would make v1 a failure)
- **Breaking the app.** Oikos-os must never write an override the app cannot read, drop a field the app wrote, or publish discovery payloads that change what the app shows.
- **A second config language.** No YAML rule files, no hand-edited device database. The broker (retained topics) is the state; MCP and the web UI are the editors.
- **Node in the build.** No npm, no JavaScript toolchain. Rust, Cargo and Trunk only.
- **Polling.** Everything is subscription driven.

## 3. Context: what exists (checked 2026-10-18)

| Piece | Fact |
|---|---|
| Broker | Mosquitto on the LAN, `192.168.1.91:1883`, username/password, plain TCP. |
| Zigbee2MQTT | 2.14.1, ZStack coordinator, `base_topic: zigbee2mqtt`, `output: json`, HA discovery **enabled** with prefix `homeassistant`, status topic `homeassistant/status`, legacy options all off. 67 devices. Friendly names are hierarchical: `rooms/<room>/<device>`, `outdoor/<device>`, `alarm/<device>`, `powers/<device>`, `greenhouse/<device>`, `miscellaneous/<device>`. |
| Discovery on the broker | Today under `homeassistant/`; the user moves Zigbee2MQTT to `oikos` (`homeassistant.discovery_topic: oikos` in Z2M's configuration, then a Z2M restart) and sets the app's discovery prefix to `oikos` in its settings. Per-entity topics `<prefix>/<component>/<node_id>/<object_id>/config`. Components seen: sensor 287, select 49, switch 48, binary_sensor 42, device_automation 13, number 11, button 10, update 10, cover 9, light 7, climate 4, device_tracker 2, alarm_control_panel 1, text 1, vacuum 1. Some come from non-Z2M bridges (climate, vacuum, device_tracker). |
| Unique ids | Z2M shape, e.g. `0xbc33acfffe36db49_switch_alarm_zigbee2mqtt`. Device identifiers `zigbee2mqtt_<ieee>`. |
| Overrides | Retained `oikos/ui/<unique_id>` JSON and `oikos/ui/_areas`. Already in use (areas: living, bedroom1, bedroom2, bedroom3, office, …; per-entity `area`, `securityAction`). |
| Device state | All 65 `zigbee2mqtt/<friendly_name>` state messages on the broker are **retained** (checked with `mosquitto_sub -F '%r %t'`), so a fresh client sees every device's last state immediately. Discovery configs are retained too. |
| App model | `UiOverride` (`core/model/.../UiOverride.kt`), `Area`/`AreasDocument` (`Area.kt`), `Kind` (`LIGHT, SWITCH, COVER, CLIMATE, SENSOR, BINARY_SENSOR, ALARM_PANEL, NUMBER, SELECT, BUTTON, VACUUM, WATER_HEATER`), `Command` (`Command.kt`), `Capabilities` (`Entity.kt`). Discovery rules in the app spec `docs/superpowers/specs/2026-09-24-oikos-design.md` §3.D and §4.3. |
| App JSON policy | Lenient: unknown keys ignored, nulls omitted, unknown enum values fall back to default. Defaults are not encoded. |
| Templates the app evaluates | A Jinja subset: `value_json.<key>`, `value`, filters `default`, `float`, `int`, `round`, `lower`, `upper`, `string`, `none`, comparisons and `if … else`. |
| Design system | Night Glass (dark, default) and Warm Minimal (light). Tokens in §7. |
| Python services | OikosPush (`oikos-push/#`, FCM, Frigate bridge) and OikosHistory (`:8090`, InfluxDB) keep running unchanged in v1. |

## 4. Architecture

One Cargo workspace, one binary, four crates:

```
oikos-os/
├── Cargo.toml                  workspace
├── oikos.example.toml
├── crates/
│   ├── oikos-core/             pure domain: model, discovery parsing, templates, registry, command mapping. No I/O, no tokio.
│   ├── oikos-api/              wire types shared by server and web (serde). Compiles to native and wasm32.
│   ├── oikos-web/              Leptos CSR app → WebAssembly, built by Trunk into dist/.
│   └── oikos-os/               the binary: config, MQTT client, Axum server (API, WebSocket, MCP, static UI).
└── docs/superpowers/specs/
```

Data flow:

```
 Mosquitto ──rumqttc──▶ MQTT task ──events──▶ Registry (oikos-core, in memory, behind a watch)
     ▲                                             │
     │ publish (commands, overrides)               ├──▶ /api/*  JSON (Axum)
     │                                             ├──▶ /ws     snapshot + deltas (Axum WebSocket)
     └──────── Command / Override service ◀────────┤
                                                   └──▶ /mcp    rmcp streamable HTTP
 Leptos UI (wasm) ◀── /  static dist (rust-embed) ──┘
```

**Source of truth.** The broker is the database. v1 keeps the registry in memory and rebuilds it from retained messages on every connect: discovery configs, overrides and device states are all retained on this broker (§3), so the dashboard is complete within a second of connecting. Nothing is written to disk except logs. SQLite arrives with history (phase 4).

**Startup fallback.** A config flag `mqtt.send_ha_birth` (default `false`) makes Oikos-os publish `online` to `homeassistant/status` after subscribing. Zigbee2MQTT treats that as a Home Assistant birth and republishes every discovery config and cached state. It is off by default because the replay also reaches the app, OikosPush and Jeedom; it exists for brokers where states are not retained.

### 4.1 `oikos-core` (no I/O)

- **Model.** `Device` (from Z2M `bridge/devices`: ieee, friendly name, type, model, vendor, description, supported, interview state, power source, `definition.exposes` kept as JSON), `Entity` (unique id, device id, name, kind, protocol kind, device class, unit, icon, capabilities, bindings, effective area, override, `part_of`, device name/label split), `Area`, `AreasDocument`, `UiOverride`, `Command`, `EntityState` (value, attributes, availability, last seen), `Kind`.
- **`UiOverride`** mirrors the app field for field (camelCase names, same types, same defaults): `name, area, hidden, order, cardStyle, favorite, energyRole, energyInvert, securityHidden, energyIcon, energyMeter, homePinned, securityAction, securityAsk, ungrouped, asLight, lightOnPreset, lightOnBrightness, climatePresets`. It carries an `extra: Map<String, Value>` (`#[serde(flatten)]`) so a field written by a newer app survives a read-modify-write by Oikos-os. Encoding omits defaults and nulls (`skip_serializing_if`), as the app does.
- **Discovery parser.** Accepts `<prefix>/<component>/[<node_id>/]<object_id>/config` and device-based `<prefix>/device/<object_id>/config` with `cmps`, for every configured prefix. Because the default prefix `oikos` also holds `oikos/ui/…` (and later `oikos/automation/…`), the parser only treats topics whose second segment is a known HA component as discovery; `ui`, `automation` and other reserved subtrees are routed to their own handlers and never logged as ignored discovery. Expands HA abbreviations (table embedded from the HA docs, same source as the app's `abbreviations.json`) and `~`. An empty payload removes the entity. Unsupported components (`device_automation`, `update`, `device_tracker`, `text`, `event`, …) are recorded as "ignored" with their topic so the Devices view can show them, and never become entities. Invalid payloads are logged and skipped.
- **Kind mapping.** `light, switch, cover, climate, sensor, binary_sensor, alarm_control_panel, number, select, button, vacuum, water_heater` → `Kind`. `asLight` on a switch changes `kind` to `LIGHT` while `protocol_kind` stays `SWITCH`.
- **Entity category and folding.** Unlike the app, Oikos-os keeps *every* supported entity in the registry, including `entity_category: diagnostic|config` and `enabled_by_default: false` ones (Z2M battery, linkquality, settings), flagged with `category` and `enabled_by_default`. It also computes `part_of` the way the app does (a device's numbers and selects folded into its single light, Z2M "Moving" folded into its cover). The web Home and the MCP `list_entities` default filter hide diagnostic, config, disabled, `part_of` and `hidden` entities; the Devices view, `get_entity`, `list_devices` (battery and LQI come from these entities) and an explicit `include: all` filter see everything. `set_override` may target any entity.
- **Effective area.** `override.area` → `device.suggested_area` → path area (second-to-last segment of a device name with ≥ 3 `/` segments) → `"Unassigned"`. Declared areas match by id, then by name ignoring case.
- **Templates.** `minijinja` with HA-compatible behaviour: `value`, `value_json`, filters `default, float, int, round, lower, upper, string, is_defined, is_number, replace, abs`, tests `defined, none, number, string`, `iif`. Unknown filters make the template fail closed: the state becomes "unknown", never a crash. A failing `value_template` is logged once per entity.
- **State engine.** `apply(topic, payload)` → updated `EntityState`s. Per kind, a mapper turns raw payload into a typed value (on/off, brightness 0–255, HS colour, position, temperature, option, number, alarm state, vacuum state, availability). The optimistic command tracker (expect a state echo within `T` ms, else report failure) lives here too.
- **Command mapping.** `Command` × `Entity` → `(topic, payload, retain=false)`, following HA MQTT semantics: JSON schema and default schema lights, cover `payload_open/close/stop` and `set_position_topic`, climate `temperature_command_topic` and `mode_command_topic`, number/select/button/alarm/vacuum command topics with `command_template`.
- **Registry.** Pure state machine: `fn apply(&mut self, event) -> Vec<Change>`. Changes are what the WebSocket and MCP stream. Also computes the *subscription set* (state, attribute, availability topics of all entities), debounced by the caller.

Everything in this crate is unit tested with recorded broker payloads (`crates/oikos-core/tests/fixtures/`), taken from this house.

### 4.2 `oikos-os` binary

- **Config** (`oikos.toml`, env overrides with prefix `OIKOS_`, e.g. `OIKOS_MQTT__PASSWORD`; figment): broker host/port/user/password/client id, discovery prefixes (a list, default `["oikos"]`; `homeassistant` can be added for bridges not yet moved), HA status topic for the birth flag (default `homeassistant/status`), Z2M base topic (`zigbee2mqtt`), UI prefix (`oikos/ui`), HTTP bind (`0.0.0.0:8080`), log level, command confirm timeout. `oikos.example.toml` is committed; `oikos.toml` is git-ignored.
- **MQTT task** (rumqttc, MQTT 3.1.1, QoS 1, clean session, auto reconnect with backoff). Subscribes on connect to each `<prefix>/#`, `<z2m>/bridge/#`, `<ui>/#`, then to the computed entity topic set (recomputed on registry changes, debounced 500 ms, diffed so only new topics are subscribed and dropped ones unsubscribed). Feeds the registry through a channel; publishes commands and overrides from a second channel.
- **Override writer.** `set_override(unique_id, patch)` reads the current override, merges the patch (only the fields present in the patch change; `null` clears a field), encodes with defaults omitted, and publishes retained QoS 1 on `oikos/ui/<unique_id>`. An override equal to the default publishes an empty retained payload, which deletes the topic. `set_areas(document)` publishes the whole `oikos/ui/_areas` document.
- **Axum server** on one port: `/` static UI (rust-embed of `oikos-web/dist`, with SPA fallback), `/api/v1/*`, `/ws`, `/mcp`, `/health`.
- **Logging** with `tracing`, pretty in the terminal, JSON available by flag.
- **Shutdown** on Ctrl-C: disconnects cleanly so Mosquitto drops the subscriptions.

### 4.3 HTTP API (`/api/v1`, JSON, for the web UI)

| Method and path | Purpose |
|---|---|
| `GET /snapshot` | Everything the UI needs at load: areas, devices, entities (with effective fields and override), states, bridge info, connection status. |
| `POST /entities/{id}/command` | Body is a `Command`; returns accepted or rejected with reason. |
| `PATCH /entities/{id}/override` | Partial `UiOverride`; returns the new effective entity. |
| `DELETE /entities/{id}/override` | Drops the override. |
| `PUT /areas` | Replaces the areas document. |
| `GET /ignored` | Discovery topics seen and not supported. |

`/ws` sends the snapshot on connect, then deltas: `entity_upserted`, `entity_removed`, `state_changed`, `areas_changed`, `device_upserted`, `connection_changed`, `command_failed`. The wire types live in `oikos-api` and are shared with the Leptos app, so the UI and the server cannot drift.

### 4.4 MCP server (client-agnostic, no auth)

The MCP server is generic: it speaks the standard protocol and nothing vendor-specific, so Claude Code, Codex CLI, Claude Desktop, IDE agents and any other MCP client can use it. Two transports, both served by rmcp from the same tool set:

- **Streamable HTTP** at `http://<host>:8080/mcp` (current MCP spec transport), for clients that take a URL: Claude Code (`claude mcp add --transport http oikos http://…/mcp`), Codex CLI (`[mcp_servers.oikos] url = "http://…/mcp"` in `~/.codex/config.toml`), and others.
- **stdio** through `oikos-os mcp-stdio --server http://<host>:8080`: a thin proxy process that a client launches locally and that forwards every request to the running server over the HTTP transport. For clients that only launch commands (Claude Desktop, some IDEs). It never runs a second registry.

The README documents the setup for Claude Code, Codex CLI and Claude Desktop. Tool names, descriptions and schemas are the same on both transports.

Tools (names are stable; inputs and outputs are JSON schemas generated from the Rust types):

| Tool | What it does |
|---|---|
| `bridge_info` | Z2M version, coordinator, permit-join state, broker connection status. |
| `list_areas` | Areas with id, name, icon, order, and the entities count per area. |
| `set_areas` | Replace the areas document (add, rename, reorder, set icon). `Area` mirrors the app's `Area` including `groupNames` and `groupFavorites`, plus a flattened `extra` map, so a document written by a newer app round-trips. |
| `list_devices` | Zigbee devices: ieee, friendly name, model, vendor, availability, battery/LQI if known, and their entity ids. Filter by area or text. |
| `list_entities` | Entities with kind, protocol kind, effective name and area, unit, current value, availability, and the override if any. Filters: area, kind, device, text, `changed_since`. Paginated. |
| `get_entity` | One entity in full: discovery payload (expanded), capabilities, bindings (topics and templates), state and attributes, override. |
| `set_override` | Patch an entity's `oikos/ui` override. Accepts any `UiOverride` field; rejects unknown fields and out-of-range values with a message that names the valid options. |
| `clear_override` | Remove the override. |
| `send_command` | Send a `Command` to an entity; waits up to the confirm timeout and reports the resulting state or the failure. |
| `search` | Free-text search over device names, entity names, ids and models. |
| `list_ignored` | Discovery topics seen but not supported, with the reason. |

Resources: `oikos://snapshot` (the same document as `GET /snapshot`) so a client can read the whole house in one call. No prompts in v1.

Every tool's description says what the tool is for and what a good next call is, since the assistant is the main user. Writes are logged at `info` with the tool name and arguments.

### 4.5 `oikos-web` (Leptos 0.8, client-side rendering)

Built with Trunk to `crates/oikos-web/dist/`; the binary embeds it at compile time. In development, `trunk serve` proxies `/api`, `/ws` and `/mcp` to the running binary, so UI work has hot reload.

**Screens (v1).**
- **Home.** Room chips across the top (order from `_areas`, "Unassigned" last, a "Favorites" chip when any entity is favorite). Home shows every primary entity (§4.1 category rule) that is not `hidden`. It deliberately differs from the app's Home in one way: entities the app moves to its Energy and Security tabs (alarm panel, door, motion and smoke sensors, power sensors with an energy role) stay on the web Home, because v1 has no other screen for them. When those screens arrive, the web Home adopts the app's placement rule. A responsive card grid (phone, tablet, desktop widths) with one card per visible entity, grouped by area. Card per kind: light (toggle, brightness glow), switch, cover (open/close/stop, position), climate (current and target), sensor (value and unit), binary sensor (state and icon), number, select, button, alarm panel (state), vacuum (state), water heater. Tapping a control sends the command optimistically with a pending dot; a failed command reverts and shows a toast.
- **Entity sheet.** Opens from a card: full controls (brightness slider, HS colour wheel, effects, position slider, HVAC mode, fan mode, options, temperature steps), device information (model, vendor, ieee, last seen, LQI, battery), raw state, and the **override editor**: name, area, hidden, favorite, order, show as light (switches only), card style, energy role, security options, light-on preset, climate presets. Saves write through `PATCH /override`.
- **Devices.** The Z2M device list: every device, its entities, availability, interview state, and the ignored discovery topics. Read-only administration view.
- **Settings / status.** Broker connection, Z2M bridge state, counts, theme switch.

**Design language.** Night Glass by default, Warm Minimal following `prefers-color-scheme: light` or the theme switch. Tokens as CSS variables on `:root` (§7). Fonts Inter and Fraunces bundled as static assets (no Google Fonts). Cards radius 20 px, nav radius 24 px, 4 px grid, 48 px minimum touch targets, 200–350 ms emphasized easing. Icons are inline SVG in Rust, one per kind and device class, mirroring the app's icon set where it makes sense.

### 4.6 Connection and error handling

- Broker unreachable: the UI shows a status pill ("Reconnecting…"), the registry keeps its last state, and MCP tools return `connection: offline` in their result rather than failing.
- Discovery payload invalid: logged with topic, entity skipped, visible in Devices → ignored.
- Template failure: state "unknown", logged once per entity, shown as "—" on the card.
- Command not confirmed in time: UI reverts, `command_failed` event, MCP `send_command` reports the timeout.
- Override publish fails: the API returns 503 and the UI keeps the previous value.
- A `homeassistant/status` `online` message from Z2M republishes discovery; the registry treats re-published identical payloads as no-ops.

## 5. Constraints and conventions

- Rust 2024 edition, stable toolchain (1.99 at the time of writing). `wasm32-unknown-unknown` target and Trunk required for the UI.
- Crates (versions current on crates.io on 2026-10-18): tokio 1.53, rumqttc 0.25, axum 0.8, rmcp 3.5, leptos 0.8 (stable; 0.9 is rc), minijinja (latest), serde/serde_json, figment, tracing, rust-embed, schemars for MCP schemas.
- Attribution: author `Gekkotron` everywhere (Cargo `authors`, LICENSE, docs). Licence Apache-2.0, matching the Oikos app (builder's call, confirmed below).
- Package and binary name `oikos-os` (lowercase). Crate names `oikos-core`, `oikos-api`, `oikos-web`.
- Secrets never in git: `oikos.toml` ignored, `oikos.example.toml` committed with placeholders.
- No `unsafe`. `cargo clippy -D warnings` and `cargo fmt --check` clean. `cargo test` runs the core fixtures; the web crate has a `wasm-bindgen-test` smoke test at most.
- Logs never print the broker password.

## 6. Decisions already made (by the user unless marked)

| Decision | Who |
|---|---|
| Fresh repo; nothing from Oikos/Moli reused. | User |
| Rust end to end, including the web UI (Leptos + WebAssembly). | User |
| MCP server inside Oikos-os, plain HTTP on the LAN, no auth for now. | User |
| MCP is generic and client-agnostic (Codex, Claude, any standard client): streamable HTTP plus a stdio proxy mode. | User |
| Web UI is a beautiful dashboard to control the house, plus device detail and override editing. | User ("beautiful web UI"); the admin Devices view is the builder's addition |
| `oikos/ui/…` stays the shared contract between the OS and the app. | User |
| InfluxDB stays external for now; history later behind a trait. | User |
| Run on the Mac with `cargo run`; no Docker for v1. | User |
| Automations later, through MCP. Frigate and FCM later. | User |
| Leptos CSR with Trunk, hand-written CSS, no Node. | Claude |
| Registry in memory, broker as the database, no SQLite in v1. | Claude |
| Workspace of four crates. | Claude |
| Apache-2.0 licence. | Claude |
| `minijinja` for templates. | Claude |
| Source-agnostic discovery under configurable prefixes, Z2M-only Devices view. | Claude |
| Discovery prefix is `oikos` (Z2M `discovery_topic: oikos`); `homeassistant` is optional for bridges not yet moved. | User |

## 7. Design tokens (from the app, for the CSS)

Night Glass (dark, default): background radial gradient `#1D2B55` → `#0B1020`; ink `#E8ECFF`; card white 6 % with 9 % border; on-state card amber gradient; accents mint `#6FF0BF` (connected), amber `#FFC457` (on / solar), cyan `#58E1FF` (water), violet `#8F7BFF` (grid), rose `#FF7A9A` (heat), offline `#FF9AB2`, connected container `#50E6AA` at 15 %.

Warm Minimal (light): paper `#F5F1EA`; card `#FFFDF9` with border `#EFE8DC`; espresso ink `#1F1B16`; on-state card inverted (espresso background, paper text); chip border `#D9D0C2`; connected `#3F6B3A` on `#E6EFE3`; warning `#8A5A00` on `#F6E7C8`; offline `#A23B4F` on `#F6DDE2`; solar `#D98B3A`; water `#2B8FB3`; grid `#6A5ACD`; heat `#C4526B`; shadow `#3C280A` at 10 %.

Type: display 26 px Inter bold, letter-spacing −0.02 em (dark) or 28 px Fraunces medium (light); card name 12/16 Inter; card value 17/22 Inter semibold; label 12/16 Inter medium; body 14/20 Inter.

Dimensions: card radius 20, nav radius 24, card padding 12, card min height 88, grid gap 8, min touch 48. Motion: 200 ms short, 350 ms medium, easing `cubic-bezier(0.2, 0, 0, 1)`.

## 8. Left to the builder

- Exact Leptos component structure, routing and state management (signals vs. a store).
- The CSS file organisation and how the theme switch is persisted (localStorage is fine).
- The precise icon set; reuse the app's SVG geometry where it exists.
- WebSocket reconnection strategy in the UI.
- Pagination sizes and filter syntax for MCP list tools.
- The abbreviation table format (copy the app's resource or regenerate from the HA docs).
- Test fixture selection: enough recorded payloads to cover every supported kind present in the house.
- The order of implementation, as long as the core crate lands with tests before the server and the UI.

## 9. Roadmap after v1 (for orientation only)

1. **Automations** through MCP: triggers on state changes, time and sun; conditions; actions as commands; stored as retained JSON under `oikos/automation/<id>`; a web page listing them.
2. **Push and Frigate**: absorb OikosPush (`oikos-push/#` contract kept) and the Frigate bridge.
3. **History**: storage trait, InfluxDB reader first, then SQLite writer; absorb OikosHistory's `/v1/*` API so the app needs no change.
4. **Deployment**: Docker image, Linux service, auth and gateway access.

## 10. Risks and open points

- **Leptos 0.8 vs 0.9.** 0.9 is at release candidate; the builder starts on 0.8 and upgrades only when 0.9 is stable.
- **Template fidelity.** Z2M templates are simple, but non-Z2M bridges (climate, vacuum) may use filters outside the list; the ignored list and per-entity logging make these visible rather than silent.
- **Override round-trip.** The flatten-extra map (on `UiOverride` and `Area`) must be tested against every override currently on the broker, to prove a read-modify-write leaves other fields byte-equivalent in meaning.
- **One port for everything.** If the MCP client needs a dedicated path or port later, the Axum router can split without touching the tools.
