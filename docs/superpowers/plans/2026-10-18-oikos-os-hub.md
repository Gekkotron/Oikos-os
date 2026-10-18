# Oikos-os Hub (core, MQTT, API, MCP) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust binary that connects to the home broker, builds a live registry of every discovered entity, Zigbee device, area and `oikos/ui` override, and exposes it through a JSON API, a WebSocket and a client-agnostic MCP server.

**Architecture:** `oikos-core` is a pure, I/O-free state machine (discovery parsing, templates, state mapping, command mapping, registry) tested against payloads recorded from the house. `oikos-api` holds the serde wire types shared with the future web UI. The `oikos-os` binary wraps the registry in a `Hub` (RwLock + broadcast), drives it from a rumqttc task, and serves Axum routes for `/api/v1`, `/ws`, `/mcp` and a placeholder `/`. The web UI (Leptos) is a separate plan.

**Tech Stack:** Rust 2024, tokio 1.53, rumqttc 0.25, axum 0.8, rmcp 3.5 (server + client, streamable HTTP + stdio), minijinja 3, serde/serde_json, schemars 1, figment, clap 4, tracing, thiserror.

**Spec:** `docs/superpowers/specs/2026-10-18-oikos-os-design.md`

## Global Constraints

- Rust 2024 edition, stable toolchain (1.99). No `unsafe`. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` clean before every commit.
- Workspace `authors = ["Gekkotron <60887050+Gekkotron@users.noreply.github.com>"]`, `license = "Apache-2.0"`, LICENSE copyright line `Copyright 2026 Gekkotron`. The user's real name appears nowhere.
- Every commit uses `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …` and ends with the trailer `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Below, `$GITC` stands for that `git -c … -c …` prefix.
- Package and binary name `oikos-os`; crate names `oikos-core`, `oikos-api`, `oikos-web` (created, empty, in this plan), `oikos-os`.
- Topic defaults: discovery prefixes `["oikos"]`, UI prefix `oikos/ui`, Z2M base `zigbee2mqtt`, HA status topic `homeassistant/status`, HTTP bind `0.0.0.0:8080`, command confirm timeout 5000 ms, subscription debounce 500 ms. `send_ha_birth` defaults to `false`.
- `oikos.toml` is git-ignored; `oikos.example.toml` is committed with placeholders. Logs never print the broker password.
- `oikos-core` and `oikos-api` must compile for `wasm32-unknown-unknown` (no tokio, no std::time::SystemTime in core; timestamps are passed in as `u64` seconds).
- `oikos/ui/<unique_id>` JSON uses the app's camelCase field names and omits defaults and nulls. Unknown fields round-trip untouched.
- No polling: everything is driven by MQTT subscriptions.

## Review Focus

1. A discovery payload that mixes abbreviations, `~` inside abbreviated keys (`"stat_t": "~/state"`) and the list form of availability must parse to the same entity as its expanded form. Pinned in Task 4.
2. An override patch must round-trip unknown fields and encode `securityAsk: false` while omitting `securityAsk: true`; an override equal to the default is published as an empty retained payload. Pinned in Task 3 and Task 11.
3. A state message on a topic shared by several entities (`zigbee2mqtt/<device>`) that lacks one entity's key must leave that entity's value unchanged, not reset it to unknown. Pinned in Task 6.
4. Invalid UTF-8, invalid JSON or an empty payload on any topic must never panic and must leave the registry consistent. Pinned in Task 8.
5. A command that the entity's kind does not support, or sent to an unavailable entity, is rejected with a reason and publishes nothing. Pinned in Task 7 and Task 11.

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml` (workspace, replaces the `cargo init` file), `rust-toolchain.toml`, `.gitignore`, `LICENSE`, `README.md`, `oikos.example.toml`, `crates/oikos-core/Cargo.toml`, `crates/oikos-core/src/lib.rs`, `crates/oikos-api/Cargo.toml`, `crates/oikos-api/src/lib.rs`, `crates/oikos-web/Cargo.toml`, `crates/oikos-web/src/lib.rs`, `crates/oikos-os/Cargo.toml`, `crates/oikos-os/src/main.rs`
- Delete: `src/main.rs`

**Interfaces:**
- Produces: the workspace every later task builds in. Workspace dependencies declared once in `[workspace.dependencies]`: tokio (features `rt-multi-thread, macros, sync, time, signal`), rumqttc, axum (features `ws, json`), rmcp (features `server, client, transport-streamable-http-server, transport-streamable-http-client, transport-io`), minijinja (features `json, builtins`), serde (derive), serde_json, schemars, figment (`toml, env`), clap (`derive`), tracing, tracing-subscriber (`env-filter, json`), thiserror, anyhow, reqwest (`json`), tower-http (`fs, cors`), uuid.

- [ ] **Step 1: Write the workspace `Cargo.toml`** with `members = ["crates/*"]`, `resolver = "3"`, `[workspace.package]` (edition 2024, version 0.1.0, authors, license, repository `https://github.com/Gekkotron/Oikos-os`), and the dependency table above using the versions current on crates.io.
- [ ] **Step 2: Create the four crates.** `oikos-core` and `oikos-api` are libraries with an empty `lib.rs` and `#![forbid(unsafe_code)]`. `oikos-web` is a library with `crate-type = ["cdylib", "rlib"]` and no dependencies yet. `oikos-os` is a binary whose `main.rs` prints `oikos-os 0.1.0` and exits.
- [ ] **Step 3: Write `.gitignore`** (`/target`, `oikos.toml`, `crates/oikos-web/dist`, `.DS_Store`), `rust-toolchain.toml` (`channel = "stable"`, `targets = ["wasm32-unknown-unknown"]`), `LICENSE` (Apache-2.0 text, `Copyright 2026 Gekkotron`), a README with the one-paragraph intent from spec §1 and a "Status: scaffold" line, and `oikos.example.toml` with every key from spec §4.2 and placeholder values (`host = "192.168.1.91"`, `user = "oikos"`, `password = "change-me"`).
- [ ] **Step 4: Verify** `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` succeed and `cargo run -p oikos-os` prints the version.
- [ ] **Step 5: Commit**

```bash
git rm -q --cached -r src 2>/dev/null; rm -rf src
git add -A
$GITC commit -m "chore: cargo workspace scaffold for oikos-os" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 2: Recorded broker fixtures

**Files:**
- Create: `tools/record-fixtures.sh`, `crates/oikos-core/tests/fixtures/README.md`, and the recorded files under `crates/oikos-core/tests/fixtures/`: `bridge_info.json`, `bridge_devices.json`, `bridge_state.json`, `discovery/<component>__<object_id>.json` (one file per discovery config, topic in the file name with `/` replaced by `__`), `states/<friendly_name with __>.json`, `ui/<unique_id>.json`, `ui/_areas.json`, and `topics.txt` (every recorded topic, one per line, with its retain flag).

**Interfaces:**
- Produces: fixtures every core test loads through `oikos_core::test_support::fixture(path) -> String` (a `#[cfg(test)]`-free helper in `crates/oikos-core/tests/common/mod.rs`).

- [ ] **Step 1: Write `tools/record-fixtures.sh`.** It reads `MQTT_HOST`, `MQTT_USER`, `MQTT_PASS` from the environment (never from a file in the repo), runs `mosquitto_sub -F '%r %t %p'` for 6 s on `homeassistant/#`, `oikos/#`, `zigbee2mqtt/bridge/info`, `zigbee2mqtt/bridge/devices`, `zigbee2mqtt/bridge/state` and `zigbee2mqtt/#`, and writes the files listed above. Discovery is recorded from `homeassistant/` today (the Z2M prefix move happens later); the file names carry only component and object id, so tests build topics under any prefix.
- [ ] **Step 2: Run it** with the broker credentials from the user's local setup file (`local.user`, `local.pass` in `~/Documents/SourcesPerso/Oikos/oikos-setup.json`) exported as environment variables for that one command. Expected: about 490 discovery files, 65 state files, `bridge_devices.json` with 67 entries, and at least the 8 `oikos/ui` overrides seen on 2026-10-18.
- [ ] **Step 3: Check the fixtures contain no credentials** (`grep -ril "pass" crates/oikos-core/tests/fixtures | wc -l` is 0 apart from `password`-named Z2M config keys inside `bridge_info.json`; if `bridge_info.json` contains `mqtt.password` or network keys, blank them with a one-line `jq` in the script).
- [ ] **Step 4: Write `fixtures/README.md`** stating the house, the date, the command, and that the files are test data only.
- [ ] **Step 5: Commit**

```bash
git add tools/record-fixtures.sh crates/oikos-core/tests/fixtures
$GITC commit -m "test: record broker fixtures from the house (2026-10-18)" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 3: Core model, `UiOverride` and `Area`

**Files:**
- Create: `crates/oikos-core/src/model/mod.rs`, `model/kind.rs`, `model/ui_override.rs`, `model/area.rs`, `model/command.rs`, `model/state.rs`, `model/entity.rs`, `model/device.rs`
- Test: `crates/oikos-core/tests/ui_override.rs`, `crates/oikos-core/tests/areas.rs`

**Interfaces:**
- Produces (all `Clone + Debug + PartialEq + Serialize + Deserialize`, re-exported from `oikos_core::model`):
  - `enum Kind { Light, Switch, Cover, Climate, Sensor, BinarySensor, AlarmPanel, Number, Select, Button, Vacuum, WaterHeater }` (`rename_all = "snake_case"`), `Kind::from_component(&str) -> Option<Kind>` (`"alarm_control_panel"` → `AlarmPanel`), `Kind::component(self) -> &'static str`.
  - `const IGNORED_COMPONENTS: &[&str]` = `device_automation, update, device_tracker, text, event, scene, siren, lock, fan, humidifier, camera, image, lawn_mower, notify, tag, valve, device`.
  - `struct UiOverride` with the 19 app fields (camelCase, `#[serde(default)]`, defaults omitted on encode; `security_ask` defaults to `true` and is omitted when true; `energy_role: Option<String>` kept as a string so unknown roles round-trip), plus `#[serde(flatten)] pub extra: BTreeMap<String, serde_json::Value>`. Methods: `UiOverride::decode(&[u8]) -> Option<UiOverride>` (lenient: unknown enum-like values keep as strings, invalid JSON → `None`), `encode(&self) -> String`, `is_default(&self) -> bool`, `UiOverride::FIELDS: &[&str]` (the 19 camelCase names), `apply_patch(&self, patch: &serde_json::Map<String, serde_json::Value>) -> Result<UiOverride, PatchError>` where a `null` value clears the field, an unknown key or a wrong type yields `PatchError::UnknownField(name)` / `PatchError::InvalidValue { field, expected }`, `lightOnBrightness` must be 1..=255, `energyRole` must be one of `solar, grid, grid_export, battery, house, consumer, water`, `lightOnPreset` one of `white, warm, red, green, blue`.
  - `struct Area { id, name, icon: Option<String>, order: Option<i32>, group_names: BTreeMap<String,String> (camelCase `groupNames`, omitted when empty), group_favorites: BTreeSet<String>, extra: flatten }`, `struct AreasDocument { areas: Vec<Area> }` with `decode`/`encode`. `pub const UNASSIGNED_AREA: &str = "Unassigned"`.
  - `enum Command` (`#[serde(tag = "type", rename_all = "snake_case")]`): `TurnOn { brightness: Option<u8>, color: Option<Hs>, effect: Option<String> }, TurnOff, Toggle, SetPosition { position: u8 }, Open, Close, Stop, SetTemperature { temperature: f64 }, SetHvacMode { mode: String }, SetFanMode { mode: String }, SetNumber { value: f64 }, SelectOption { option: String }, Press, Arm { mode: AlarmMode, code: Option<String> }, Disarm { code: Option<String> }, VacuumAction { action: String }, SetFanSpeed { speed: String }`; `struct Hs { h: f64, s: f64 }`; `enum AlarmMode { Home, Away, Night, Vacation, CustomBypass }`.
  - `enum Availability { Online, Offline, Unknown }`, `enum StateValue { OnOff(bool), Number(f64), Text(String), Light(LightState), Cover(CoverState), Climate(ClimateState), Alarm(String), Vacuum(VacuumState), WaterHeater(WaterHeaterState), Unknown }` with `LightState { on: bool, brightness: Option<u8>, hs: Option<Hs>, color_temp: Option<u32>, effect: Option<String> }`, `CoverState { state: Option<String>, position: Option<u8> }`, `ClimateState { current: Option<f64>, target: Option<f64>, mode: Option<String>, fan_mode: Option<String>, action: Option<String> }`, `VacuumState { state: Option<String>, battery: Option<u8>, fan_speed: Option<String> }`, `WaterHeaterState { mode: Option<String>, current: Option<f64>, target: Option<f64> }`; `struct EntityState { value: StateValue, attributes: serde_json::Map<String, serde_json::Value>, availability: Availability, last_update: Option<u64> }` with `Default` = `Unknown / empty / Unknown / None`.
  - `enum EntityCategory { Primary, Diagnostic, Config }`, `struct Capabilities` (fields exactly as the app's `Capabilities`, snake_case, all defaulted), `struct Bindings { pub config: serde_json::Value }` with `Bindings::topics(&self) -> BTreeSet<String>` (Task 4 defines the rule), `struct Entity { unique_id, device_id: Option<String>, name, device_name: Option<String>, label: Option<String>, kind, protocol_kind, device_class: Option<String>, unit: Option<String>, icon: Option<String>, category: EntityCategory, enabled_by_default: bool, capabilities, bindings, suggested_area: Option<String>, path_area: Option<String>, area: String, ui: UiOverride, part_of: Option<String>, discovery_topic: String }`.
  - `struct Device { id: String (= "zigbee2mqtt_<ieee>"), ieee: String, friendly_name: String, device_type: String, model: Option<String>, vendor: Option<String>, description: Option<String>, supported: bool, disabled: bool, interview_state: String, power_source: Option<String>, exposes: serde_json::Value }`, `Device::parse_list(&[u8]) -> Result<Vec<Device>, serde_json::Error>` (skips the `Coordinator` entry), `struct BridgeInfo { version: Option<String>, coordinator: Option<String>, permit_join: bool, state: Availability }` with `BridgeInfo::apply_info(&mut self, &[u8])` and `apply_state(&mut self, &[u8])`.

- [ ] **Step 1: Write the failing tests** in `tests/ui_override.rs`:

```rust
#[test] fn decodes_every_recorded_override() // each fixtures/ui/*.json except _areas decodes to Some
#[test] fn encode_omits_defaults() { assert_eq!(UiOverride::default().encode(), "{}"); let o = UiOverride { security_ask: false, ..Default::default() }; assert_eq!(o.encode(), r#"{"securityAsk":false}"#); }
#[test] fn unknown_field_round_trips() { let o = UiOverride::decode(br#"{"area":"living","futureField":{"a":1}}"#).unwrap(); assert_eq!(o.encode(), r#"{"area":"living","futureField":{"a":1}}"#); }
#[test] fn unknown_energy_role_is_kept() // {"energyRole":"fusion"} decodes with energy_role == Some("fusion") and re-encodes identically
#[test] fn patch_sets_clears_and_rejects() // patch {"asLight":true} sets; {"area":null} clears; {"nope":1} -> UnknownField; {"lightOnBrightness":300} -> InvalidValue; {"energyRole":"fusion"} -> InvalidValue
#[test] fn is_default_after_clearing_everything()
#[test] fn command_json_shape() { assert_eq!(serde_json::to_string(&Command::SetPosition{position: 40}).unwrap(), r#"{"type":"set_position","position":40}"#); }
```
and in `tests/areas.rs`: `recorded_areas_round_trip` (decode `ui/_areas.json`, encode, decode again, equal) and `group_names_and_extra_survive` (`{"areas":[{"id":"a","name":"A","groupNames":{"cover":"Volets"},"newKey":true}]}` encodes back with both).

- [ ] **Step 2: Run** `cargo test -p oikos-core --test ui_override --test areas`. Expected: compile failure, types missing.
- [ ] **Step 3: Implement the model files** with the signatures above. `apply_patch` works on `serde_json::Value` objects: encode self, merge keys, remove nulls, validate the listed fields, decode. `encode` uses `serde_json::to_string` with `skip_serializing_if` on every field (`Option::is_none`, `is_false`, `is_true` for `security_ask`, `BTreeMap::is_empty`).
- [ ] **Step 4: Run the tests.** Expected: all PASS.
- [ ] **Step 5: Commit** `feat(core): domain model, UiOverride and Area with lenient round-trip`.

---

### Task 4: Discovery parsing

**Files:**
- Create: `crates/oikos-core/src/discovery/mod.rs`, `discovery/abbreviations.rs` (+ `discovery/abbreviations.json` embedded with `include_str!`, copied from the app at `core/source/src/main/resources/com/gekkotron/oikos/source/abbreviations.json`), `discovery/topic.rs`, `discovery/expand.rs`, `discovery/parse.rs`, `discovery/capabilities.rs`, `discovery/naming.rs`
- Test: `crates/oikos-core/tests/discovery.rs`

**Interfaces:**
- Consumes: Task 3 model.
- Produces (`oikos_core::discovery`):
  - `struct DiscoveryTopic { prefix: String, component: String, node_id: Option<String>, object_id: String }`
  - `enum TopicClass { Discovery(DiscoveryTopic), Reserved(String /* sub-tree, e.g. "ui" */), Other }`, `fn classify(topic: &str, prefixes: &[String], reserved: &[String]) -> TopicClass`. Rules: topic starts with `<prefix>/`; the next segment is a reserved sub-tree → `Reserved`; it is a supported or ignored component and the topic ends with `/config` with 1 or 2 segments between → `Discovery`; else `Other`.
  - `fn expand(raw: serde_json::Value) -> serde_json::Value`: replaces abbreviated keys at every nesting level (including inside `availability[]`, `device`/`dev`, `origin`/`o`, `cmps`), and replaces a leading `~` in any string value with the `~` key's value (the `~` key itself is dropped).
  - `enum Parsed { Entity(Box<Entity>), Removed, Ignored { reason: String } }`, `fn parse(topic: &DiscoveryTopic, payload: &[u8], topic_str: &str) -> Result<Parsed, DiscoveryError>` where empty payload → `Removed`, ignored component → `Ignored`, invalid JSON / missing `state_topic` for a kind that needs one → `Err(DiscoveryError::Invalid(msg))`, device-based component `device` with `cmps` → `Ignored { reason: "device-based discovery: expanded by caller" }` and a companion `fn expand_device_based(topic, payload) -> Result<Vec<(DiscoveryTopic, serde_json::Value)>, DiscoveryError>` that yields one per-entity config per `cmps` entry (merging the shared `device`, `origin`, `availability`, `~`).
  - `Bindings::topics()` rule (implemented here, declared in Task 3): every string value whose key ends with `_topic` or equals `state_topic`/`json_attributes_topic`, minus keys ending in `command_topic` and the keys `set_position_topic`, `set_fan_speed_topic`, `send_command_topic`, plus every `availability[].topic`.
  - `naming::split(device_name: Option<&str>, entity_name: Option<&str>) -> (display_name: String, device_name: Option<String>, label: Option<String>, path_area: Option<String>)`: a device name with ≥ 3 `/` segments gives `path_area` = second-to-last segment and short device name = last segment; shorter names are unchanged; display name = `"<short device name> <label>"` when both exist, `label` alone when there is no device, short device name when the entity has no name.
  - `capabilities::from_config(kind: Kind, cfg: &serde_json::Value) -> Capabilities` reading HA fields: `brightness`, `supported_color_modes`, `effect_list`, `position_open/closed` ⇒ `position = set_position_topic present`, `modes`, `fan_modes`, `supported_features` (vacuum) ⇒ `vacuum_actions`, `fan_speed_list`, `min_temp/max_temp/temp_step`, `min/max/step`, `options`, `supported_features` (alarm) ⇒ `alarm_modes` (`arm_home, arm_away, arm_night, arm_vacation, arm_custom_bypass`), `code_arm_required`, `code_disarm_required`, `code` ⇒ `code_format` (`REMOTE_CODE_TEXT` → Text, else Number).
  - `category`: `entity_category` `diagnostic`/`config` → the variant, else `Primary`; `enabled_by_default` defaults to `true`.

- [ ] **Step 1: Write the failing tests** in `tests/discovery.rs`:

```rust
#[test] fn classifies_topics() // ("oikos/light/x/y/config") -> Discovery; ("oikos/ui/0xabc") -> Reserved("ui"); ("oikos/ui/_areas") -> Reserved; ("homeassistant/sensor/x/config", prefixes ["oikos","homeassistant"]) -> Discovery with node_id None; ("oikos/light/x/y/state") -> Other; ("zigbee2mqtt/rooms/a/b") -> Other
#[test] fn expands_abbreviations_and_tilde() // {"~":"zigbee2mqtt/x","stat_t":"~","cmd_t":"~/set","avty":[{"t":"zigbee2mqtt/bridge/state","val_tpl":"{{ value_json.state }}"}],"dev":{"ids":["zigbee2mqtt_0x1"],"sa":"Living"}} expands to state_topic "zigbee2mqtt/x", command_topic "zigbee2mqtt/x/set", availability[0].topic, device.identifiers, device.suggested_area, and no "~" key
#[test] fn parses_every_recorded_discovery_file() // for each fixtures/discovery/*.json: build topic "oikos/<component>/<object_id>/config", parse; count Entity / Ignored / Removed; assert zero Err, Entity count >= 400, and every Entity has a non-empty unique_id and at least one topic in bindings.topics()
#[test] fn light_capabilities_from_recorded_json_schema_light() // the light "outdoor/light_sensors_door_4": kind Light, brightness true, effects contains "blink", color_modes == {"brightness"}
#[test] fn cover_number_select_alarm_capabilities() // one recorded cover: position true; one number: min/max/step set; one select: options non-empty; the alarm panel: alarm_modes non-empty and code_arm_required present
#[test] fn naming_and_path_area() // ("rooms/bedroom1/thermometer", Some("Temperature")) -> ("thermometer Temperature", Some("thermometer"), Some("Temperature"), Some("bedroom1")); ("outdoor/pool", Some("Water")) -> path_area None, device_name Some("outdoor/pool")
#[test] fn diagnostic_and_disabled_are_kept_and_flagged() // a recorded "linkquality" sensor: category Diagnostic; a recorded "enabled_by_default": false entity (Z2M publishes some): enabled_by_default false
#[test] fn empty_payload_is_removed_and_ignored_component_is_ignored()
#[test] fn device_based_discovery_expands_cmps() // handwritten payload with two cmps entries sharing dev and "~" yields two per-entity configs with merged device and expanded topics
#[test] fn bindings_topics_rule() // config with state_topic, brightness_state_topic, command_topic, set_position_topic, json_attributes_topic, availability[]: topics() == {state, brightness_state, json_attributes, availability}
```
- [ ] **Step 2: Run** `cargo test -p oikos-core --test discovery`. Expected: compile failure.
- [ ] **Step 3: Implement** the discovery modules per the interfaces. Copy the app's `abbreviations.json` verbatim (it is the HA table). The unique id falls back to `<component>.<node_id>.<object_id>` when `unique_id` is absent. `device_id` = first `device.identifiers` entry (string, or first string of a list).
- [ ] **Step 4: Run the tests.** Expected: PASS. Print the Ignored reasons once (`cargo test … -- --nocapture`) and confirm they are only the ignored components list.
- [ ] **Step 5: Commit** `feat(core): HA MQTT discovery parsing with abbreviations, naming and capabilities`.

---

### Task 5: Template engine

**Files:**
- Create: `crates/oikos-core/src/template.rs`
- Test: `crates/oikos-core/tests/template.rs`

**Interfaces:**
- Produces: `struct Templates` (`Templates::new() -> Self`, `Send + Sync`), `fn render(&self, template: &str, payload: &str) -> Result<String, TemplateError>` with context `value` (the raw payload string) and `value_json` (the payload parsed as JSON when it is a JSON object or array, otherwise undefined), and `fn render_with(&self, template, payload, extra: &[(&str, serde_json::Value)])`. `enum TemplateError { Syntax(String), Undefined(String), Other(String) }`: an undefined variable or attribute access, or `is_defined` on undefined, yields `Undefined`; a parse failure yields `Syntax`. Filters beyond minijinja's built-ins: `float(default=None)`, `int(default=None)` (HA semantics: non-numeric input returns the default or raises `Undefined`-class error when no default), `is_defined`, `is_number`, `iif(if_true, if_false, if_none=None)`, and test `is_number`. Rendering uses `UndefinedBehavior::Strict`.

- [ ] **Step 1: Write the failing tests**:

```rust
#[test] fn value_json_key() { assert_eq!(t.render("{{ value_json.state }}", r#"{"state":"ON"}"#).unwrap(), "ON"); }
#[test] fn raw_value() { assert_eq!(t.render("{{ value | upper }}", "on").unwrap(), "ON"); }
#[test] fn float_round_and_default() { assert_eq!(t.render("{{ value_json.t | float | round(1) }}", r#"{"t":21.456}"#).unwrap(), "21.5"); assert_eq!(t.render("{{ value_json.x | default(3) }}", "{}").unwrap(), "3"); }
#[test] fn is_defined_and_undefined_errors() { assert!(matches!(t.render("{{ value_json.nope }}", "{}"), Err(TemplateError::Undefined(_)))); assert!(matches!(t.render("{{ value_json.nope | is_defined }}", "{}"), Err(TemplateError::Undefined(_)))); }
#[test] fn if_else_and_iif() { assert_eq!(t.render("{{ 'closed' if value_json.contact else 'open' }}", r#"{"contact":true}"#).unwrap(), "closed"); assert_eq!(t.render("{{ (value_json.p > 0) | iif('on','off') }}", r#"{"p":5}"#).unwrap(), "on"); }
#[test] fn syntax_error() { assert!(matches!(t.render("{{ value_json.", "{}"), Err(TemplateError::Syntax(_)))); }
#[test] fn every_recorded_value_template_compiles() // for each fixtures/discovery file: expand, collect every key ending in "_template", assert Templates::check(&tpl).is_ok() — add `fn check(&self, template: &str) -> Result<(), TemplateError>` to the interface
```
- [ ] **Step 2: Run** `cargo test -p oikos-core --test template`. Expected: compile failure.
- [ ] **Step 3: Implement `template.rs`** with minijinja: one `Environment<'static>` built in `new()`, custom filters registered there, `render` via `env.render_str`. Map `minijinja::ErrorKind::UndefinedError` and the custom `is_defined` failure to `Undefined`, `SyntaxError` to `Syntax`.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): minijinja template engine with HA filters`.

---

### Task 6: State mapping

**Files:**
- Create: `crates/oikos-core/src/state/mod.rs`, `state/availability.rs`, `state/kinds/{light,switch,cover,climate,sensor,binary_sensor,alarm,number_select,button,vacuum,water_heater}.rs`, `state/color.rs`
- Test: `crates/oikos-core/tests/state.rs`

**Interfaces:**
- Consumes: Task 3 `Entity`, `EntityState`; Task 5 `Templates`.
- Produces: `enum Applied { Changed, Unchanged, NotForThisEntity }`, `fn apply_message(entity: &Entity, state: &mut EntityState, topic: &str, payload: &str, at: u64, templates: &Templates) -> Result<Applied, StateError>`. Behaviour: availability topics set `availability` using `payload_available/payload_not_available` (defaults `online/offline`) after the availability `value_template`; `availability_mode` `all|any|latest` (default `latest`) when several; `json_attributes_topic` merges attributes (after `json_attributes_template`); kind modules read their state topics with the HA field names (light `schema: json` and default schema with `brightness_state_topic`, `brightness_scale`, `hs_state_topic`, `rgb_state_topic`, `color_temp_state_topic`, `effect_state_topic`, `state_value_template`; cover `state_topic` + `state_open/closed/opening/closing/stopped` + `position_topic` + `position_template`; climate `current_temperature_topic/template`, `temperature_state_topic/template`, `mode_state_topic/template`, `fan_mode_state_topic/template`, `action_topic/template`; sensor `value_template` → `Number` when it parses as f64 else `Text`; binary sensor `payload_on/off` (defaults `ON/OFF`); switch `state_on/state_off` falling back to `payload_on/off`; number/select `value_template` → `Number`/`Text`; alarm `state_topic` (+ `value_template`) → `Alarm`; vacuum JSON `{state, battery_level, fan_speed}`; water heater `mode_state_topic`, `current_temperature_topic`, `temperature_state_topic`). `color.rs`: `rgb_to_hs(r,g,b) -> Hs`, `xy_to_hs(x,y) -> Hs` (CIE 1931 to sRGB then to HS). A `TemplateError::Undefined` on a topic shared with other entities returns `Ok(Unchanged)`; a `TemplateError::Syntax` returns `Err(StateError::Template(..))` and the caller marks the entity unknown once.

- [ ] **Step 1: Write the failing tests** using recorded entities (parse the fixture discovery file, then feed the matching `fixtures/states/*.json` payload on its `state_topic`):

```rust
#[test] fn recorded_states_apply_to_every_entity() // for every recorded device state and every entity bound to that topic: apply; assert no Err; count Changed >= 300
#[test] fn thermometer_temperature_is_number_with_unit() // rooms/bedroom1/thermometer temperature sensor: Number(x), entity.unit == Some("°C")
#[test] fn contact_sensor_maps_to_on_off() // a recorded door sensor: contact false => StateValue::OnOff(true)  (HA: contact=false means open => binary_sensor "ON"), per its value_template
#[test] fn json_light_state() // handwritten: {"state":"ON","brightness":127,"color":{"h":30,"s":40},"effect":"blink"} on a json-schema light => Light{on:true, brightness:Some(127), hs:Some(Hs{30,40}), effect:Some("blink")}
#[test] fn default_schema_light_rgb_to_hs() // brightness_state_topic "100" with brightness_scale 100 => brightness Some(255); rgb_state_topic "255,0,0" => hs h=0 s=100
#[test] fn cover_state_and_position() // state "open" + position 40 => CoverState{Some("open"), Some(40)}
#[test] fn climate_fields() // current 20.5, target 21, mode "heat"
#[test] fn availability_from_bridge_state() { /* payload {"state":"offline"} on zigbee2mqtt/bridge/state with the recorded availability value_template => Offline */ }
#[test] fn missing_key_on_shared_topic_leaves_value_unchanged() // Review Focus 3: set value to Number(1.0), apply {"other":1} => Ok(Unchanged), value still Number(1.0)
#[test] fn attributes_merge()
#[test] fn message_for_unrelated_topic() // => Ok(NotForThisEntity)
```
- [ ] **Step 2: Run** `cargo test -p oikos-core --test state`. Expected: compile failure.
- [ ] **Step 3: Implement** the state modules. Each kind module exposes `fn apply(entity, state, topic, payload, templates) -> Result<Applied, StateError>`; `mod.rs` handles availability and attributes first and dispatches on `entity.protocol_kind`.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): per-kind state mapping with availability and attributes`.

---

### Task 7: Command mapping

**Files:**
- Create: `crates/oikos-core/src/command.rs`
- Test: `crates/oikos-core/tests/command.rs`

**Interfaces:**
- Consumes: Task 3 `Command`, `Entity`, `EntityState`; Task 5 `Templates`.
- Produces: `struct Publish { topic: String, payload: String, retain: bool }`, `fn map_command(entity: &Entity, state: &EntityState, cmd: &Command, templates: &Templates) -> Result<Vec<Publish>, CommandError>`, `enum CommandError { Unsupported { kind: Kind, command: String }, MissingTopic(String), OutOfRange(String), Template(String) }`. HA semantics per kind: light JSON schema → one publish on `command_topic` with `{"state":"ON"/"OFF", "brightness"?, "color":{"h","s"}?, "effect"?}` (brightness scaled to `brightness_scale`, default 255); default schema → `payload_on/off` on `command_topic` plus `brightness_command_topic`, `hs_command_topic` (`"h,s"`), `effect_command_topic`; `Toggle` reads `state.value` (unknown → `TurnOn`); switch `payload_on/off` (defaults `ON/OFF`); cover `payload_open/close/stop` (defaults `OPEN/CLOSE/STOP`) on `command_topic`, `SetPosition` on `set_position_topic` with `set_position_template` rendered with `position` (and `position_open/closed` scaling), else the bare number; climate `temperature_command_topic` (+ `temperature_command_template`), `mode_command_topic`, `fan_mode_command_topic`; number/select on `command_topic` with `command_template` (context `value`); button `payload_press` (default `PRESS`); alarm `payload_arm_home/away/night/vacation/custom_bypass`, `payload_disarm`, with `command_template` (context `action`, `code`) when present, and `code` required when `code_arm_required`; vacuum `command_topic` with `payload_start/pause/stop/return_to_base/locate` and `set_fan_speed_topic`; water heater `mode_command_topic`, `temperature_command_topic`. Anything else → `Unsupported`.

- [ ] **Step 1: Write the failing tests**: `json_light_turn_on_with_brightness_and_color`, `default_schema_light_publishes_three_topics`, `toggle_uses_current_state`, `cover_open_close_stop_and_position_template` (`set_position_template: "{{ position }}"`), `climate_set_temperature_with_template`, `select_option_with_command_template`, `alarm_arm_requires_code_when_configured` (→ `OutOfRange`-style error named `CommandError::MissingCode`, add it to the enum), `unsupported_command_for_sensor` (Review Focus 5: `SetPosition` on a sensor → `Unsupported`, empty vec never returned), `recorded_switch_turn_on` (a recorded ZBMINI switch: payload `ON` on its `command_topic`, which for Z2M is `zigbee2mqtt/<name>/set` with `{"state":"ON"}` when `command_template`/JSON is configured — assert whatever the recorded config implies, computed from the fixture).
- [ ] **Step 2: Run** `cargo test -p oikos-core --test command`. Expected: compile failure.
- [ ] **Step 3: Implement `command.rs`.**
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): command to MQTT publish mapping`.

---

### Task 8: Registry

**Files:**
- Create: `crates/oikos-core/src/registry/mod.rs`, `registry/areas.rs`, `registry/folding.rs`, `registry/ignored.rs`
- Test: `crates/oikos-core/tests/registry.rs`

**Interfaces:**
- Consumes: Tasks 3–7.
- Produces:
  - `struct RegistryConfig { discovery_prefixes: Vec<String>, ui_prefix: String, z2m_base: String, reserved: Vec<String> }` (`reserved` defaults to `["ui", "automation"]` relative to each discovery prefix).
  - `enum Inbound { Discovery(DiscoveryTopic, Vec<u8>), Override { unique_id: String, payload: Vec<u8> }, Areas(Vec<u8>), Z2mDevices(Vec<u8>), Z2mBridgeInfo(Vec<u8>), Z2mBridgeState(Vec<u8>), State { topic: String, payload: Vec<u8>, at: u64 }, Irrelevant }`, `Registry::route(&self, topic: &str, payload: Vec<u8>, at: u64) -> Inbound` (`<ui_prefix>/_areas` → Areas, `<ui_prefix>/<id>` → Override, `<z2m>/bridge/devices|info|state`, discovery via `classify`, a topic in `subscription_topics()` → State, else Irrelevant).
  - `enum Change { EntityUpserted(String), EntityRemoved(String), StateChanged(String), AreasChanged, DevicesChanged, BridgeChanged, IgnoredChanged, SubscriptionsChanged }`, `Registry::apply(&mut self, inbound: Inbound) -> Vec<Change>` (deduplicated, `SubscriptionsChanged` only when `subscription_topics()` differs).
  - Read accessors: `entity(&str) -> Option<&Entity>`, `entities() -> impl Iterator<Item = &Entity>`, `state(&str) -> Option<&EntityState>`, `devices() -> impl Iterator<Item = &Device>`, `device(&str)`, `areas_document() -> &AreasDocument`, `effective_areas() -> Vec<Area>` (declared areas by `order` then name, then implicit ones found on entities by id with `name = id`, then `Unassigned` when any entity has it), `bridge() -> &BridgeInfo`, `ignored() -> &[Ignored { topic: String, reason: String }]`, `subscription_topics() -> &BTreeSet<String>`, `raw_override(&str) -> Option<&UiOverride>`, `entities_on_topic(&str) -> &[String]`.
  - Effective entity rules applied on every upsert, override or areas change: `area = ui.area (resolved by id, or by name ignoring case against declared areas) → suggested_area → path_area → "Unassigned"`, `kind = Light when protocol_kind == Switch && ui.as_light`, `part_of` per `folding.rs`: for each device with exactly one `Light` entity, every `Number`/`Select` entity of that device gets `part_of = that light`; for each device with exactly one `Cover`, a `BinarySensor` with `device_class == "moving"` gets `part_of = that cover`.
  - Template failures: a `StateError::Template` marks `state.value = Unknown` and records the entity id in a `template_failures: BTreeSet<String>` exposed as `template_failures()`; it is logged by the caller once per id.

- [ ] **Step 1: Write the failing tests** in `tests/registry.rs`, with a helper `fn load_house() -> Registry` that applies `bridge_info`, `bridge_devices`, `bridge_state`, every discovery file under prefix `oikos`, every `ui/*` override and `_areas`, then every recorded state on its topic:

```rust
#[test] fn house_loads() // entities() >= 400, devices() == 66 (67 minus Coordinator), effective_areas() contains "living","bedroom1","bedroom2","bedroom3","office" in that order first, bridge().version == Some("2.14.1")
#[test] fn overrides_apply() // entity "0xbc33acfffe36db49_switch_alarm_zigbee2mqtt": area "living", ui.security_action true
#[test] fn path_area_and_as_light() // a rooms/stair/light ZBMINI switch entity: area "stair"; after Override {"asLight":true}: kind Light, protocol_kind Switch; after Override with empty payload: kind Switch
#[test] fn removal_and_subscriptions() // apply Discovery with empty payload for one entity: EntityRemoved, SubscriptionsChanged only if its topic is no longer used by any entity
#[test] fn state_routes_to_all_entities_on_shared_topic() // a thermometer payload yields StateChanged for temperature, humidity, battery, linkquality (voltage) entities
#[test] fn folding_moving_into_cover() // a recorded TS130F shutter: its "moving" binary sensor has part_of == the cover's unique_id
#[test] fn diagnostic_flag_present() // the linkquality entity category Diagnostic
#[test] fn garbage_never_panics() // Review Focus 4: apply Discovery(invalid utf8), Override(b"{"), Areas(b""), Z2mDevices(b"null"), State(b"\xff\xfe") on a known topic: no panic, entities() unchanged, each returns a Vec (possibly empty), ignored() grew for the invalid discovery
#[test] fn identical_payload_is_noop() // re-applying the same discovery config yields no Change
#[test] fn area_resolution_by_name_case_insensitive() // override area "Living" resolves to id "living"
```
- [ ] **Step 2: Run** `cargo test -p oikos-core --test registry`. Expected: compile failure.
- [ ] **Step 3: Implement the registry.** Keep an index `topic → Vec<unique_id>` rebuilt incrementally on upsert/remove, and compare the previous `subscription_topics` set after each apply to decide `SubscriptionsChanged`.
- [ ] **Step 4: Run the whole core suite** `cargo test -p oikos-core`. Expected: PASS.
- [ ] **Step 5: Verify wasm compatibility:** `cargo check -p oikos-core --target wasm32-unknown-unknown` (install the target with `rustup target add wasm32-unknown-unknown` if missing). Expected: success.
- [ ] **Step 6: Commit** `feat(core): registry with routing, effective entities, folding and subscription set`.

---

### Task 9: Wire types (`oikos-api`)

**Files:**
- Create: `crates/oikos-api/src/lib.rs`, `src/views.rs`, `src/events.rs`, `src/snapshot.rs`
- Test: `crates/oikos-api/tests/snapshot.rs`

**Interfaces:**
- Consumes: Task 8 `Registry`.
- Produces (all `Serialize + Deserialize + Clone + Debug + PartialEq`, and `schemars::JsonSchema`):
  - `enum ConnectionStatus { Connecting, Online, Offline { reason: String } }`
  - `struct EntityView { id, device_id, name, device_name, label, kind: Kind, protocol_kind: Kind, device_class, unit, icon, category: EntityCategory, enabled_by_default: bool, area: String, part_of: Option<String>, capabilities: Capabilities, ui: UiOverride, state: EntityState }`, `EntityView::from(&Entity, &EntityState)`.
  - `struct DeviceView { id, ieee, friendly_name, model, vendor, description, supported, interview_state, power_source, entity_ids: Vec<String>, availability: Availability, battery: Option<u8>, linkquality: Option<u32> }` (battery and linkquality read from that device's diagnostic entities' states).
  - `struct AreaView { id, name, icon, order, entity_count: usize }`, `struct BridgeView { version, coordinator, permit_join, state: Availability }`, `struct IgnoredView { topic, reason }`.
  - `struct Snapshot { connection: ConnectionStatus, bridge: BridgeView, areas: Vec<AreaView>, devices: Vec<DeviceView>, entities: Vec<EntityView>, ignored: Vec<IgnoredView>, generated_at: u64 }`, `fn snapshot(reg: &Registry, connection: ConnectionStatus, at: u64) -> Snapshot`.
  - `enum WsEvent` (`#[serde(tag = "type", rename_all = "snake_case")]`): `Snapshot(Snapshot), EntityUpserted(EntityView), EntityRemoved { id }, StateChanged { id, state: EntityState }, AreasChanged(Vec<AreaView>), DevicesChanged(Vec<DeviceView>), BridgeChanged(BridgeView), ConnectionChanged(ConnectionStatus), CommandFailed { id, reason }`.
  - `type OverridePatch = serde_json::Map<String, serde_json::Value>`, `struct CommandResponse { accepted: bool, reason: Option<String>, state: Option<EntityState> }`, `struct ApiError { error: String }`.

- [ ] **Step 1: Write the failing test** `snapshot_of_recorded_house_serializes` (build the registry as in Task 8's helper, call `snapshot`, serialize to JSON, deserialize back, equal; `entities.len() >= 400`; a `DeviceView` for a thermometer has `battery.is_some()`), and `ws_event_tag_shape` (`WsEvent::EntityRemoved{id:"x"}` → `{"type":"entity_removed","id":"x"}`).
- [ ] **Step 2: Run** `cargo test -p oikos-api`. Expected: compile failure.
- [ ] **Step 3: Implement** the crate (depends on `oikos-core`, `serde`, `serde_json`, `schemars`; add `schemars` derives to the core types it embeds, behind a `schemars` feature of `oikos-core` enabled by `oikos-api`).
- [ ] **Step 4: Run** `cargo test -p oikos-api` and `cargo check -p oikos-api --target wasm32-unknown-unknown`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(api): wire types, snapshot and WebSocket events`.

---

### Task 10: Binary config, CLI and logging

**Files:**
- Create: `crates/oikos-os/src/main.rs`, `src/cli.rs`, `src/config.rs`, `src/logging.rs`
- Test: `crates/oikos-os/tests/config.rs`

**Interfaces:**
- Produces: `struct Config { mqtt: MqttConfig { host, port: u16 (1883), user: Option<String>, password: Option<String>, client_id (default "oikos-os"), send_ha_birth: bool, ha_status_topic }, topics: TopicsConfig { discovery_prefixes: Vec<String>, ui_prefix, z2m_base }, http: HttpConfig { bind: String }, commands: CommandsConfig { confirm_timeout_ms: u64, subscription_debounce_ms: u64 }, log: LogConfig { level: String, json: bool } }`, `Config::load(path: Option<&Path>) -> Result<Config, figment::Error>` merging defaults, the TOML file (`oikos.toml` in the working directory when `path` is `None`, missing file allowed) and env `OIKOS_` with `__` as the nesting separator. `impl Debug for MqttConfig` redacts the password. `enum Cli { Run { config: Option<PathBuf> }, McpStdio { server: String } }` via clap (`oikos-os run`, `oikos-os mcp-stdio --server URL`, `run` is the default subcommand).

- [ ] **Step 1: Write the failing tests**: `defaults_match_spec` (no file, no env: prefixes `["oikos"]`, ui `oikos/ui`, z2m `zigbee2mqtt`, bind `0.0.0.0:8080`, timeout 5000, debounce 500, birth false, status topic `homeassistant/status`), `env_overrides_file` (write a temp TOML with `[mqtt] host = "a"`, set `OIKOS_MQTT__HOST=b` through figment's `Jail`, loaded host is `b`), `debug_redacts_password` (`format!("{:?}", cfg.mqtt)` does not contain the password).
- [ ] **Step 2: Run** `cargo test -p oikos-os --test config`. Expected: compile failure.
- [ ] **Step 3: Implement** `config.rs`, `cli.rs`, `logging.rs` (`tracing_subscriber` with `EnvFilter` from `log.level`, pretty or JSON by `log.json`) and a `main.rs` that parses the CLI, loads the config, initialises logging, and for `Run` logs `starting oikos-os` with host and port, then returns (the server arrives in Tasks 11–12).
- [ ] **Step 4: Run the tests.** Expected: PASS. `cargo run -p oikos-os -- run` prints the start line and exits.
- [ ] **Step 5: Commit** `feat(os): config loading, CLI and logging`.

---

### Task 11: Hub and MQTT task

**Files:**
- Create: `crates/oikos-os/src/hub.rs`, `src/mqtt.rs`, `src/clock.rs`
- Modify: `crates/oikos-os/src/main.rs`
- Test: `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Consumes: Task 8 `Registry`, Task 9 views/events, Task 10 `Config`.
- Produces:
  - `struct Hub` (`Arc`-shared), `Hub::new(cfg: &Config, publisher: mpsc::Sender<Publish>) -> Arc<Hub>`.
  - `fn ingest(&self, topic: &str, payload: &[u8]) -> Vec<Change>`: routes and applies under a `std::sync::RwLock<Registry>`, converts each `Change` into a `WsEvent` and broadcasts it on `tokio::sync::broadcast::Sender<WsEvent>` (capacity 1024), logs each new template failure once.
  - `fn snapshot(&self) -> Snapshot`, `fn subscribe(&self) -> broadcast::Receiver<WsEvent>`, `fn connection(&self) -> watch::Receiver<ConnectionStatus>`, `fn set_connection(&self, ConnectionStatus)`, `fn subscription_topics(&self) -> BTreeSet<String>`, `fn entity_view(&self, id) -> Option<EntityView>`.
  - `async fn send_command(&self, id: &str, cmd: Command) -> Result<CommandResponse, HubError>`: rejects when the entity is unknown or `state.availability == Offline` (`HubError::Rejected(reason)`, nothing published), maps the command, publishes each `Publish`, then waits up to `confirm_timeout_ms` for a `StateChanged` for that id and returns `accepted: true` with the new state, or `accepted: false, reason: "no state change within 5000 ms"` while also broadcasting `CommandFailed`.
  - `async fn set_override(&self, id: &str, patch: &OverridePatch) -> Result<EntityView, HubError>`: current override (or default) → `apply_patch` → encode (empty string when `is_default`) → publish retained QoS 1 on `<ui_prefix>/<id>` → apply the same bytes to the registry immediately (so the API response reflects it before the broker echo) → return the view. `async fn clear_override(&self, id)`, `async fn set_areas(&self, doc: AreasDocument) -> Result<Vec<AreaView>, HubError>`.
  - `enum HubError { UnknownEntity(String), Rejected(String), Patch(PatchError), Command(CommandError), Publish(String) }`.
  - `mqtt::run(cfg: MqttConfig, topics: TopicsConfig, hub: Arc<Hub>, publish_rx: mpsc::Receiver<Publish>) -> !`-style `async fn` that owns the rumqttc `EventLoop`: on `ConnAck` sets `Online`, subscribes (QoS 1) to each `<prefix>/#`, `<z2m>/bridge/#`, `<ui_prefix>/#` and the current subscription set, publishes `online` to `ha_status_topic` when `send_ha_birth`; on `Publish` calls `hub.ingest`; after any `SubscriptionsChanged`, waits the debounce and diffs the set to subscribe/unsubscribe only the difference; on `publish_rx` messages calls `client.publish`; on error sets `Offline { reason }` and lets rumqttc reconnect with its backoff (log at warn, once per failure).
  - `clock::now() -> u64` (unix seconds).

- [ ] **Step 1: Write the failing tests** in `tests/hub.rs` (no broker: the hub is driven by `ingest` and the publisher channel is inspected):

```rust
#[tokio::test] async fn ingest_broadcasts_and_snapshots() // feed bridge_devices + one discovery config + its state: subscribe() receives EntityUpserted then StateChanged; snapshot().entities.len() == 1
#[tokio::test] async fn set_override_publishes_retained_and_applies() // patch {"asLight":true} on a recorded switch: publisher receives Publish{topic:"oikos/ui/<id>", payload:r#"{"asLight":true}"#, retain:true}; returned view.kind == Light. Then patch {"asLight":null}: payload "" (Review Focus 2)
#[tokio::test] async fn send_command_publishes_then_times_out() // confirm_timeout_ms = 50; TurnOn on a recorded switch: publisher receives its command publish; result accepted == false with the timeout reason; a CommandFailed event was broadcast
#[tokio::test] async fn send_command_confirmed_by_state() // spawn a task that, on seeing the publish, calls ingest with an ON state on the entity's state_topic: accepted == true, state.value == OnOff(true)
#[tokio::test] async fn command_to_offline_entity_is_rejected_without_publish() // Review Focus 5
#[tokio::test] async fn unknown_entity_is_unknown_error()
```
- [ ] **Step 2: Run** `cargo test -p oikos-os --test hub`. Expected: compile failure.
- [ ] **Step 3: Implement `hub.rs`, `clock.rs`, `mqtt.rs`** and wire `main.rs`: build the publisher channel (capacity 64), the hub, spawn `mqtt::run`, and wait for Ctrl-C (`tokio::signal::ctrl_c`), then call `client.disconnect()` through a `mqtt::Handle` returned by `run`'s companion `mqtt::start(...) -> (Handle, JoinHandle)` (choose the shape; `main` must be able to disconnect cleanly).
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Manual check against the house:** create `oikos.toml` from the example with the real broker credentials (never committed), run `cargo run -p oikos-os -- run`, and confirm the log shows `connected`, the subscription count, `entities=<n>` ≥ 400 within a few seconds, and that Ctrl-C exits cleanly. Record the observed counts in the task's commit message body.
- [ ] **Step 6: Commit** `feat(os): hub with broadcast events, override writer, command confirmation and rumqttc task`.

---

### Task 12: HTTP API and WebSocket

**Files:**
- Create: `crates/oikos-os/src/http/mod.rs`, `http/api.rs`, `http/ws.rs`, `http/static_ui.rs`, `crates/oikos-os/assets/index.html`
- Modify: `crates/oikos-os/src/main.rs`
- Test: `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Consumes: Task 11 `Hub`.
- Produces: `fn router(hub: Arc<Hub>) -> axum::Router` mounting: `GET /health` → `{"status":"ok","connection":<ConnectionStatus>,"entities":n}`; `GET /api/v1/snapshot` → `Snapshot`; `POST /api/v1/entities/{id}/command` (body `Command`) → `CommandResponse` (200), 404 `ApiError` for an unknown entity, 422 for a rejected command; `PATCH /api/v1/entities/{id}/override` (body `OverridePatch`) → `EntityView`, 400 with the `PatchError` message, 503 on publish failure; `DELETE /api/v1/entities/{id}/override` → 204; `PUT /api/v1/areas` (body `AreasDocument`) → `Vec<AreaView>`; `GET /api/v1/ignored` → `Vec<IgnoredView>`; `GET /ws` → WebSocket that sends `WsEvent::Snapshot` first then every broadcast event as JSON text frames, drops the client on lag (`RecvError::Lagged` → send a fresh `Snapshot` instead of closing); `GET /` and any unknown non-API path → the embedded placeholder `assets/index.html` ("Oikos-os is running. The web UI arrives in the next phase." with a link to `/api/v1/snapshot` and `/health`). CORS allows any origin on `/api` and `/ws` (Trunk dev server will proxy, but a direct origin must work too). `serve(cfg: &HttpConfig, router) -> JoinHandle` binds `cfg.bind`.

- [ ] **Step 1: Write the failing tests** with `axum::serve` on `127.0.0.1:0` and `reqwest` + `tokio-tungstenite` as dev-dependencies: `health_and_snapshot`, `command_unknown_entity_404`, `patch_override_then_snapshot_reflects_it`, `invalid_patch_400`, `ws_sends_snapshot_then_events` (connect, read first frame `type == "snapshot"`, then `hub.ingest` a state message, read `state_changed`), `root_serves_placeholder`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test http`. Expected: compile failure.
- [ ] **Step 3: Implement** the `http` module and wire it in `main.rs` after the hub.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Manual check:** with the real broker, `curl -s localhost:8080/api/v1/snapshot | jq '.entities | length'` ≥ 400; `websocat ws://localhost:8080/ws | head -c 300` shows the snapshot; a `PATCH` with `{"favorite":true}` on one entity shows up in the Android app (the app reads `oikos/ui/<id>`) and the reverse `{"favorite":null}` clears it.
- [ ] **Step 6: Commit** `feat(os): JSON API, WebSocket stream and placeholder page`.

---

### Task 13: MCP server (streamable HTTP)

**Files:**
- Create: `crates/oikos-os/src/mcp/mod.rs`, `mcp/tools.rs`, `mcp/params.rs`
- Modify: `crates/oikos-os/src/http/mod.rs` (nest the MCP service at `/mcp`)
- Test: `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: Task 11 `Hub`, Task 9 views.
- Produces: `struct OikosMcp { hub: Arc<Hub> }` implementing rmcp `ServerHandler` with `#[tool_router]` tools, server info name `oikos-os`, instructions string (one paragraph: what the house is, that `list_entities` hides diagnostic and folded entities unless `include: "all"`, that `set_override` writes the app's `oikos/ui` override, and that `send_command` confirms by state). Tools and their parameter structs (`schemars::JsonSchema`, every field documented with `#[schemars(description = …)]`):
  - `bridge_info()` → `BridgeView` + `ConnectionStatus`.
  - `list_areas()` → `Vec<AreaView>`.
  - `set_areas(areas: Vec<Area>)` → `Vec<AreaView>`.
  - `list_devices(area: Option<String>, query: Option<String>)` → `Vec<DeviceView>`.
  - `list_entities(area: Option<String>, kind: Option<Kind>, device_id: Option<String>, query: Option<String>, changed_since: Option<u64>, include: Option<Include /* "primary" (default) | "all" */>, limit: Option<usize> (default 100, max 500), offset: Option<usize>)` → `{ total: usize, items: Vec<EntityView> }` where `primary` excludes `Diagnostic`, `Config`, `enabled_by_default == false`, `part_of.is_some()` and `ui.hidden`.
  - `get_entity(id: String)` → `EntityView` plus `bindings: serde_json::Value` (the expanded discovery config) and `raw_override: Option<UiOverride>`.
  - `set_override(id: String, patch: OverridePatch)` → `EntityView`; a `PatchError` becomes a tool error whose message lists `UiOverride::FIELDS` and the valid values of the rejected field.
  - `clear_override(id: String)` → `EntityView`.
  - `send_command(id: String, command: Command)` → `CommandResponse`; `HubError::Rejected` becomes a tool error with the reason.
  - `search(query: String, limit: Option<usize>)` → `{ devices: Vec<DeviceView>, entities: Vec<EntityView> }` matching case-insensitively on friendly name, entity name, id, model, vendor.
  - `list_ignored()` → `Vec<IgnoredView>`.
  - Resource `oikos://snapshot` (`application/json`, the `Snapshot`).
  - Every write tool logs at `info` with the tool name and the arguments.
  - `fn mcp_service(hub: Arc<Hub>) -> StreamableHttpService<…>` nested in the Axum router at `/mcp` (stateful sessions, `LocalSessionManager`).

- [ ] **Step 1: Write the failing tests** in `tests/mcp.rs` using rmcp's client with `StreamableHttpClientTransport` against the test server: `lists_the_expected_tools` (names equal the eleven above), `list_entities_primary_hides_diagnostics_and_all_shows_them`, `set_override_round_trip_through_tool` (call `set_override` `{"asLight": true}`; `get_entity` shows kind `light`; `clear_override`; kind `switch`), `set_override_unknown_field_is_a_tool_error_listing_fields`, `snapshot_resource_reads`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test mcp`. Expected: compile failure.
- [ ] **Step 3: Implement** the `mcp` module with rmcp 3.5 (check the crate docs for the exact `tool_router`/`tool_handler` macro names and the `StreamableHttpService` constructor; the plan fixes behaviour, not the macro spelling).
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Manual check with Claude Code:** `claude mcp add --transport http oikos http://localhost:8080/mcp`, then in a Claude Code session ask for the list of devices in the living room and to mark one entity as favorite; confirm the Android app shows the star. Remove with `claude mcp remove oikos` afterwards or keep it, user's choice.
- [ ] **Step 6: Commit** `feat(os): MCP server with device, entity, override and command tools`.

---

### Task 14: `mcp-stdio` proxy and README

**Files:**
- Create: `crates/oikos-os/src/mcp/stdio_proxy.rs`
- Modify: `crates/oikos-os/src/main.rs`, `README.md`
- Test: `crates/oikos-os/tests/mcp_stdio.rs`

**Interfaces:**
- Consumes: Task 13 server.
- Produces: `async fn run_stdio_proxy(server_url: String) -> anyhow::Result<()>`: connects an rmcp client over streamable HTTP to `<server_url>/mcp`, then serves an rmcp `ServerHandler` over stdio (`rmcp::transport::io::stdio()`) whose `list_tools`, `call_tool`, `list_resources`, `read_resource` and `get_info` forward to that client and return its results unchanged. It exits with a clear error when the server is unreachable.

- [ ] **Step 1: Write the failing test** `stdio_proxy_forwards_list_tools`: start the test HTTP server, spawn the proxy as a child process (`cargo_bin("oikos-os")` via `assert_cmd`, args `mcp-stdio --server http://127.0.0.1:<port>`), connect an rmcp client to its stdio (`TokioChildProcess`), `list_tools` returns the eleven names; a `call_tool("bridge_info")` returns a JSON content item.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test mcp_stdio`. Expected: compile failure.
- [ ] **Step 3: Implement** the proxy and wire `Cli::McpStdio` in `main.rs`.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Write the README**: intent (spec §1), status, `cargo run -p oikos-os -- run` with `oikos.toml`, the Z2M change (`homeassistant.discovery_topic: oikos` plus the app's discovery prefix setting), the API endpoints, and MCP setup for Claude Code (`claude mcp add --transport http oikos http://<host>:8080/mcp`), Codex CLI (`[mcp_servers.oikos]\nurl = "http://<host>:8080/mcp"` in `~/.codex/config.toml`) and Claude Desktop (`"oikos": {"command": "<path>/oikos-os", "args": ["mcp-stdio", "--server", "http://<host>:8080"]}`). Author line `Gekkotron`.
- [ ] **Step 6: Commit** `feat(os): stdio MCP proxy and setup documentation`.

---

### Task 15: End-to-end verification against the house

**Files:**
- Create: `docs/superpowers/plans/2026-10-18-oikos-os-hub-verification.md` (the record of this task)

- [ ] **Step 1:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Expected: all clean; paste the test summary line into the record.
- [ ] **Step 2:** Run the binary against the real broker. In the record note: time to first full snapshot, entity and device counts, ignored count and the distinct ignored reasons, any `template failure` log lines with their entity ids.
- [ ] **Step 3:** Through Claude Code's MCP connection: `list_areas`, `list_entities` for area `living`, `send_command` `turn_on` then `turn_off` on a light the user names, `set_override` `{"area":"office"}` on one entity and confirm the app moved it, then restore the original override with `set_override` using the values recorded from `get_entity` before the change.
- [ ] **Step 4:** Stop the binary with Ctrl-C and confirm Mosquitto logs (or a second `mosquitto_sub` on `$SYS/#`) show a clean disconnect.
- [ ] **Step 5: Commit** the record: `docs: hub verification run against the house`.

---

## Self-review notes

- Spec coverage: §4.1 → Tasks 3–8; §4.2 → Tasks 10–11; §4.3 → Task 12; §4.4 → Tasks 13–14; §4.6 error handling → Tasks 6, 8, 11, 12 tests; §5 constraints → Global Constraints; §4.5 (web UI) is deliberately the next plan; §4.2 "subscribe to each prefix/#" and the birth flag → Task 11; `Area.extra` and `groupNames` round-trip → Task 3.
- Names used across tasks: `Registry::route/apply`, `Change`, `Inbound`, `Hub::ingest/send_command/set_override/clear_override/set_areas/snapshot/subscribe`, `WsEvent`, `EntityView`, `OverridePatch`, `CommandResponse`, `Publish`, `Templates::render/check`, `apply_message`, `map_command` are spelled identically in every task.
- Review Focus 1–5 each name the task whose tests pin them.
