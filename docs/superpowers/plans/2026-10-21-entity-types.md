# Generic Entity Types Implementation Plan (hub + web)

Author: Gekkotron

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the 16 Home Assistant discovery components Oikos ignores today into the hub and the web UI through one data-driven type table, a single `Kind::Generic`, one generic card and one generic sheet panel.

**Architecture:** A JSON type table embedded in `oikos-core` (wasm-safe, `include_str!`) describes each new component: fields, controls, payload words with their HA defaults, main control, alert values, placement, history mapping. Discovery turns any component in the table into an `Entity { kind: Generic, component }`; one generic state mapper, one generic command mapper, one generic card and one generic sheet panel read that table and never branch on a component name. The hub publishes the table retained at `{ui_prefix}/_types` for the Android app (separate plan, separate worktree).

**Tech Stack:** Rust workspace (`oikos-core`, `oikos-api`, `oikos-os`, `oikos-web` on Leptos → wasm), serde/serde_json, minijinja templates, axum, rumqttc, wasm-bindgen-test.

**Spec:** `docs/superpowers/specs/2026-10-21-entity-types-design.md` (approved 2026-10-21). Read it first; this plan argues from it.

## Global Constraints

- The 16 components: `lock`, `fan`, `siren`, `valve`, `humidifier`, `lawn_mower`, `text`, `scene`, `device_tracker`, `camera`, `image`, `device_automation`, `event`, `tag`, `notify`, `update`. `device` stays device-based discovery.
- No `match` / `if` on a new component's name outside `oikos-core/src/types/` (the table loader and its mappers). Cards, sheet, flows, history read the table.
- The 12 existing kinds keep their current code paths and cards; their behaviour must not change.
- `oikos-core` and `oikos-api` stay wasm-clean (no OS clock, no tokio, no reqwest).
- Flows compare canonical values only (HA state words such as `locked`, `home`, `open`, numbers, bools); translated labels are display-only.
- A retained or replayed message never starts a flow.
- `text` with `mode: password` is masked in the UI and absent from snapshot values and history.
- Override field `mainControl`: a control name the entity announces, or `"none"`; absent/`null` = table default.
- Table topic: `{ui_prefix}/_types` (default `oikos/ui/_types`), retained, JSON with an integer `version` (start at `1`).
- Picture route: `GET /api/v1/entities/{id}/picture`; latest picture in memory only; size cap 5 MiB (bigger payloads dropped with a `warn!`).
- History: no schema change; text values map to numbers only where the table says so.
- Gates after every task: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo check -p oikos-core --target wasm32-unknown-unknown`, `cargo check -p oikos-api --target wasm32-unknown-unknown`, `cargo check -p oikos-web --target wasm32-unknown-unknown`. Web tasks also: `cd crates/oikos-web && wasm-pack test --headless --chrome`.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Conventional prefixes as in `git log` (`feat(core):`, `feat(web):`, `fix(hub):`, `docs:`).
- Work on branch `feat/entity-types` in worktree `~/.superconductor/worktrees/Oikos-os/oikos-entity-types`. Hub listens on :8081.

## Review Focus

1. **A config that announces only part of a type** (a fan with no `percentage_command_topic`, a lock without `payload_open`): the missing controls are not offered anywhere (card, sheet, flow picker, `mainControl` choices) and the card still renders. Test: Task 3 `fan_without_percentage_topic_offers_power_only`, Task 13 `generic_card_without_main_control_shows_state_only`.
2. **A payload that matches no mapped word** (lock reports `"FOO"`): the field holds the raw payload as text, the card shows it, no alert, no panic; flows compare against the raw text. Test: Task 4 `unmapped_payload_is_kept_as_raw_text`.
3. **Retained signal at boot or after reconnect** (a retained Z2M `action` message): updates nothing that starts a flow. Test: Task 7 `retained_signal_never_fires_a_flow`.
4. **Removing one `device_automation` config** (empty retained payload): only that action disappears; removing the last one removes the entity. Test: Task 6 `removing_last_action_removes_the_remote`.
5. **A `mainControl` override naming a control the device no longer announces** (device re-paired with a smaller config): falls back to the table default, the PATCH that set it later stays readable. Test: Task 2 `main_control_falls_back_when_not_announced`.

---

## File Structure

`oikos-core/src/types/` (new module, the only place that knows component names):

| File | Responsibility |
|---|---|
| `types/table.json` | The type table data (Task 1, filled in Task 3) |
| `types/mod.rs` | Serde schema of the table, `table()` accessor, lookups, `main_control()`, `alerting()`, `history_num()` |
| `types/caps.rs` | `GenericCaps::from_config(def, cfg)` — which fields/controls a given config announces |
| `types/state.rs` | Generic state mapper (`apply`) |
| `types/command.rs` | Generic command mapper (`map`) |
| `types/signals.rs` | Signal extraction from a message (`signal_of`) |

Other touch points are named in each task.

---

### Task 1: Type table schema and loader

**Files:**
- Create: `crates/oikos-core/src/types/mod.rs`, `crates/oikos-core/src/types/table.json`
- Modify: `crates/oikos-core/src/lib.rs` (add `pub mod types;`)
- Test: inline `#[cfg(test)] mod tests` in `types/mod.rs`

**Interfaces:**
- Produces (all `pub`, serde `snake_case`, `Deserialize + Serialize + Clone + Debug + PartialEq`, `schemars` behind the feature like the rest of the crate):

```rust
pub struct TypeTable { pub version: u32, pub types: BTreeMap<String, TypeDef> }
pub struct TypeDef {
    pub placement: Placement,              // card | signal | target | update
    pub icon: String,                      // glyph name known to oikos-web icons.rs
    pub primary: Option<String>,           // field shown as the card value
    #[serde(default)] pub fields: BTreeMap<String, FieldDef>,
    #[serde(default)] pub controls: BTreeMap<String, ControlDef>,
    #[serde(default)] pub main: Vec<String>, // default main control, in preference order
    #[serde(default)] pub alerts: Vec<AlertDef>,
    pub history: Option<HistoryDef>,
    #[serde(default)] pub requires_state: bool,   // reject configs without the primary field's topic
}
pub enum Placement { Card, Signal, Target, Update }
pub enum Shape { OnOff, Number, Choice, Text, Picture, Version }
pub struct FieldDef {
    pub shape: Shape,
    pub topic: String,                     // config key of the topic, e.g. "state_topic"
    pub template: Option<String>,          // config key of the template, e.g. "value_template"
    #[serde(default)] pub values: Vec<WordDef>,   // payload word -> canonical value
    pub options: Option<String>,           // config key of a list (e.g. "preset_modes")
    pub range: Option<RangeDef>,
    pub on: Option<String>,                // OnOff: canonical value meaning "on" (e.g. "locked")
}
pub struct WordDef { pub value: String, pub key: Option<String>, pub default: String }
pub struct RangeDef { pub min_key: Option<String>, pub max_key: Option<String>, pub step_key: Option<String>, pub min: f64, pub max: f64, pub step: f64 }
pub struct ControlDef {
    pub shape: Shape,                      // OnOff | Number | Choice | Text, or Press via `press`
    pub topic: String,                     // config key, e.g. "command_topic"
    pub template: Option<String>,          // config key, e.g. "command_template"
    pub field: Option<String>,             // field this control writes (echo + current value)
    #[serde(default)] pub press: bool,     // a momentary action (no value)
    pub payload: Option<WordDef>,          // Press payload
    pub payload_on: Option<WordDef>,       // OnOff payloads
    pub payload_off: Option<WordDef>,
    pub json_key: Option<String>,          // send `{json_key: value}` instead of the bare value
    pub requires: Option<String>,          // extra config key that must be present to announce it
    pub labels: Option<ActionLabels>,      // card button words for OnOff ({on: "Lock", off: "Unlock"})
}
pub struct ActionLabels { pub on: String, pub off: String }
pub struct AlertDef { pub field: String, pub any_of: Vec<String> }
pub struct HistoryDef { pub field: String, #[serde(default)] pub map: BTreeMap<String, f64> } // empty map = number as-is

pub fn table() -> &'static TypeTable;                 // LazyLock, include_str!("table.json"), expect on parse error
pub fn type_def(component: &str) -> Option<&'static TypeDef>;
pub fn table_json() -> &'static str;                 // the raw embedded JSON (published by the hub, Task 11)
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn embedded_table_parses_and_is_version_one() {
    assert_eq!(table().version, 1);
}

#[test]
fn every_reference_in_the_table_resolves() {
    for (name, def) in &table().types {
        if let Some(p) = &def.primary { assert!(def.fields.contains_key(p), "{name}: primary {p}"); }
        for m in &def.main { assert!(def.controls.contains_key(m), "{name}: main {m}"); }
        for a in &def.alerts { assert!(def.fields.contains_key(&a.field), "{name}: alert {}", a.field); }
        if let Some(h) = &def.history { assert!(def.fields.contains_key(&h.field), "{name}: history"); }
        for (c, ctl) in &def.controls {
            if let Some(f) = &ctl.field { assert!(def.fields.contains_key(f), "{name}.{c}: field {f}"); }
            if ctl.press { assert!(ctl.payload.is_some(), "{name}.{c}: press needs payload"); }
            if ctl.shape == Shape::OnOff && !ctl.press { assert!(ctl.payload_on.is_some() && ctl.payload_off.is_some(), "{name}.{c}"); }
        }
    }
}

#[test]
fn unknown_component_has_no_type() {
    assert!(type_def("light").is_none());
    assert!(type_def("no_such_component").is_none());
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-core types::` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** the structs and accessors in `types/mod.rs`, following the `discovery/abbreviations.rs` pattern (`const TABLE_JSON: &str = include_str!("table.json"); static TABLE: LazyLock<TypeTable>`). `table.json` starts as `{"version": 1, "types": {}}`.
- [ ] **Step 4: Run** `cargo test -p oikos-core types::` — Expected: PASS. Run the wasm check for `oikos-core`.
- [ ] **Step 5: Commit** `feat(core): type table schema and embedded loader`.

---

### Task 2: Table helpers — announced controls, main control, alerts, history

**Files:**
- Create: `crates/oikos-core/src/types/caps.rs`
- Modify: `crates/oikos-core/src/types/mod.rs`, `crates/oikos-core/src/model/entity.rs` (`Capabilities` gains `generic: Option<GenericCaps>`)
- Test: inline tests in `types/caps.rs` and `types/mod.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:

```rust
// types/caps.rs
pub struct GenericCaps {                       // stored in Capabilities.generic, serde default
    pub controls: Vec<String>,                 // announced control names, table order
    pub options: BTreeMap<String, Vec<String>>,// per field or control: choice options from config
    pub ranges: BTreeMap<String, Range>,       // per field: resolved min/max/step
    pub signals: Vec<String>,                  // signal names (filled by Tasks 6–7)
    pub secret: bool,                          // text mode: password
}
pub struct Range { pub min: f64, pub max: f64, pub step: f64 }
impl GenericCaps { pub fn from_config(def: &TypeDef, cfg: &serde_json::Value) -> GenericCaps; }

// types/mod.rs
pub fn main_control<'a>(def: &'a TypeDef, caps: &'a GenericCaps, chosen: Option<&'a str>) -> Option<&'a str>;
// alerting() and history_num() need GenericState and land in Task 4.
```

Rules: a control is announced when `cfg[ctl.topic]` is a string and, if set, `cfg[ctl.requires]` is present. Choice options come from `cfg[field.options]` (string list) else from the field's `values`. `main_control`: `chosen == Some("none")` → `None`; `chosen` naming an announced control → it; otherwise the first `def.main` entry that is announced; otherwise `None`. `secret` = `cfg["mode"] == "password"`.

- [ ] **Step 1: Write the failing tests** (use small hand-written `TypeDef`s built from JSON in the test, not the real table, so they hold before Task 3):
  - `announced_controls_follow_config_topics` — def with controls `power` (`command_topic`) and `speed` (`percentage_command_topic`); cfg with only `command_topic` → `controls == ["power"]`.
  - `requires_key_gates_a_control` — control `open` with `requires: "payload_open"`; cfg without it → not announced; with it → announced.
  - `options_come_from_config_list_then_values` — field `preset` with `options: "preset_modes"`; cfg `preset_modes: ["eco","boost"]` → `options["preset"] == ["eco","boost"]`.
  - `main_control_prefers_override_then_table_order` — `main: ["power","speed"]`; chosen `Some("speed")` → `"speed"`; chosen `None` → `"power"`; chosen `Some("none")` → `None`.
  - `main_control_falls_back_when_not_announced` — chosen `Some("speed")`, `speed` not announced → `"power"`.
  - `password_mode_is_secret` — cfg `{"mode":"password"}` → `secret == true`.
- [ ] **Step 2: Run** `cargo test -p oikos-core types::` — Expected: FAIL.
- [ ] **Step 3: Implement** `GenericCaps::from_config`, `main_control`; add `pub generic: Option<GenericCaps>` to `Capabilities` (serde default, `skip_serializing_if = "Option::is_none"` so existing snapshots are unchanged).
- [ ] **Step 4: Run** tests — Expected: PASS; existing `oikos-core` tests still pass.
- [ ] **Step 5: Commit** `feat(core): announced controls and main control from the type table`.

---

### Task 3: `Kind::Generic`, `Entity.component`, discovery of the 16 components, table entries

**Files:**
- Modify: `crates/oikos-core/src/model/kind.rs`, `crates/oikos-core/src/model/entity.rs`, `crates/oikos-core/src/discovery/parse.rs`, `crates/oikos-core/src/discovery/capabilities.rs:51`, `crates/oikos-core/src/state/kinds/mod.rs:27`, `crates/oikos-core/src/command/kinds.rs:12`, `crates/oikos-core/src/registry/mod.rs:452`, `crates/oikos-api/src/views.rs:15,48`, `crates/oikos-core/src/types/table.json`, `crates/oikos-web/src/icons.rs:282`, `crates/oikos-web/src/screens/home.rs:108`, `crates/oikos-web/src/format.rs:11` (temporary arms only, real UI in Tasks 13–17)
- Create: one fixture per component in `crates/oikos-core/tests/fixtures/discovery/` named `<component>__<node>__<object>.json` (Zigbee2MQTT payloads where Z2M publishes the type: `lock`, `fan`, `siren`, `valve`, `update`, `device_automation`, `event`, `text`; HA-docs examples for the others)
- Test: `crates/oikos-core/tests/discovery.rs`

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces: `Kind::Generic` (serde `"generic"`, added to `Kind::ALL`; `component()` of `Generic` is never used for lookup); `Entity.component: String` (the HA component for every entity, including the 12 kinds); `EntityView.component: String`; `IGNORED_COMPONENTS` no longer lists the 16 (it keeps `device`).

Discovery rule in `parse.rs`: after the `device` branch, if `types::type_def(component)` is `Some(def)` → `Kind::Generic`, `capabilities.generic = Some(GenericCaps::from_config(def, &value))`, and if `def.requires_state` and the primary field's topic key is absent → `DiscoveryError` like `REQUIRES_STATE_TOPIC`. `Bindings::data_topics` (`entity.rs`) also treats a key named exactly `topic` as a state topic (used by `camera`, `tag`, `device_automation`).

Table entries to write in `table.json` (canonical value ← config key, HA default; payload keys and defaults are HA's MQTT integration defaults — verify each against the HA docs page of the component while writing):

| Component | Placement | Primary | Fields | Controls (topic) | Main | Alerts | History |
|---|---|---|---|---|---|---|---|
| lock | card | state | state: choice `state_topic`/`value_template`; values locked←state_locked "LOCKED", unlocked←state_unlocked "UNLOCKED", locking "LOCKING", unlocking "UNLOCKING", jammed "JAMMED", open "OPEN", opening "OPENING" | locked: on_off `command_topic`/`command_template`, field state, on "locked", payload_on←payload_lock "LOCK", payload_off←payload_unlock "UNLOCK", labels Lock/Unlock; open: press, payload←payload_open "OPEN", requires payload_open | locked | state ∈ unlocked, jammed | state: locked 1, unlocked 0 |
| fan | card | state | state: on_off `state_topic`/`state_value_template`, payload_on "ON"/payload_off "OFF"; percentage: number `percentage_state_topic`/`percentage_value_template`, range 0–100 step 1; preset: choice `preset_mode_state_topic`/`preset_mode_value_template`, options preset_modes; oscillation: on_off `oscillation_state_topic`, "oscillate_on"/"oscillate_off"; direction: choice `direction_state_topic`, forward/reverse | power (command_topic), percentage (percentage_command_topic), preset (preset_mode_command_topic), oscillation (oscillation_command_topic), direction (direction_command_topic) | power | — | state: on 1, off 0 |
| siren | card | state | state: on_off `state_topic`/`state_value_template`, state_on "ON"/state_off "OFF" | power: on_off command_topic/command_template, payload_on "ON"/payload_off "OFF"; tone: choice command_topic, json_key "tone", options available_tones, requires available_tones | power | state ∈ on | state: on 1, off 0 |
| valve | card | state | state: choice `state_topic`/`value_template`, values open←state_open "open", opening "opening", closed←state_closed "closed", closing "closing"; position: number (same topic, only when `reports_position`), range 0–100 | valve: on_off command_topic/command_template, on "open", payload_on←payload_open "OPEN", payload_off←payload_close "CLOSE", labels Open/Close; stop: press payload_stop (requires payload_stop) | valve | — | state: open 1, closed 0 |
| humidifier | card | state | state: on_off state_topic, "ON"/"OFF"; target: number target_humidity_state_topic, range min_humidity 0 / max_humidity 100; mode: choice mode_state_topic, options modes; current: number current_humidity_topic; action: choice action_topic | power (command_topic), target (target_humidity_command_topic), mode (mode_command_topic) | power | — | target as-is |
| lawn_mower | card | activity | activity: choice activity_state_topic/activity_value_template, values mowing, paused, docked, error, returning | start: press start_mowing_command_topic/start_mowing_command_template payload "start_mowing"; pause: press pause_command_topic payload "pause"; dock: press dock_command_topic payload "dock" | start | activity ∈ error | — |
| text | card | value | value: text state_topic/value_template | value: text command_topic/command_template, field value | value | — | — |
| scene | card | — | — | activate: press command_topic, payload←payload_on "ON" | activate | — | — |
| device_tracker | card | state | state: choice state_topic/value_template, values home←payload_home "home", not_home←payload_not_home "not_home" | — | — | — | state: home 1, not_home 0 |
| camera | card | picture | picture: picture `topic` | — | — | — | — |
| image | card | picture | picture: picture `image_topic` (bytes) or `url_topic`/`url_template` (URL) | — | — | — | — |
| device_automation | signal | last | last: choice `topic` | — | — | — | — |
| event | signal | last | last: choice state_topic/value_template (JSON `event_type`), options event_types | — | — | — | — |
| tag | signal | last | last: text `topic`/value_template | — | — | — | — |
| notify | target | — | — | message: text command_topic/command_template | — | — | — |
| update | update | latest | installed: version state_topic/value_template; latest: version latest_version_topic/latest_version_template | — | — | — | — |

`requires_state: true` for lock, siren, valve, device_tracker, event; picture fields have no template rule (bytes).

- [ ] **Step 1: Write the failing tests** in `tests/discovery.rs`:
  - `each_new_component_becomes_a_generic_entity` — for each fixture of the 16 (skip `device_automation`, owned by Task 6): `entity.kind == Kind::Generic`, `entity.component == <component>`.
  - `existing_kinds_carry_their_component` — the light fixture has `component == "light"`, `kind == Kind::Light`.
  - `fan_without_percentage_topic_offers_power_only` — `entity("fan", json!{"command_topic":"f/set","state_topic":"f"})` → `capabilities.generic.unwrap().controls == ["power"]`.
  - `lock_without_payload_open_has_no_open_control`.
  - `lock_without_state_topic_is_rejected`.
  - Update `parses_every_recorded_discovery_file` expectations if counts shift.
- [ ] **Step 2: Run** `cargo test -p oikos-core --test discovery` — Expected: FAIL.
- [ ] **Step 3: Implement** the Kind variant, the `component` field (every `Entity` literal gains it; `build` sets it from the topic), the parse branch, the `Bindings` `topic` rule, the table entries, and exhaustive-match arms: `capabilities::from_config` → `Capabilities::default()` for Generic (generic caps set in parse), `state/kinds` and `command/kinds` → delegate to `types::state::apply` / `types::command::map` (stub returning `NotForThisEntity` / empty until Tasks 4–5), web `kind_icon` → `table` icon lookup, `card_for` → a placeholder `SensorCard`-like readout, `display_value` → `"—"` (real work in Task 13). Keep `room_groups`, `placement`, `switch_type` ignoring Generic (they already use `_`/`==`).
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): generic kind and the 16 component entries`.

---

### Task 4: Generic state

**Files:**
- Create: `crates/oikos-core/src/types/state.rs`
- Modify: `crates/oikos-core/src/model/state.rs` (`StateValue::Generic`), `crates/oikos-core/src/state/kinds/mod.rs`, `crates/oikos-core/src/history/mod.rs:23` (`num_of`), `crates/oikos-core/src/flow/scalar.rs:23` (`Scalar::of_state`), `crates/oikos-api/src/views.rs:107`, `crates/oikos-web/src/format.rs:11`
- Test: `crates/oikos-core/tests/state.rs`, inline tests in `types/state.rs`, `history/mod.rs`, `flow/scalar.rs`

**Interfaces:**
- Consumes: Tasks 1–3; `state/util.rs` (`Message::read`, `on_off`, `parse_number`), `Templates`.
- Produces:

```rust
// model/state.rs
pub enum GenericValue { Bool(bool), Number(f64), Text(String) }   // serde untagged
pub struct GenericState { pub fields: BTreeMap<String, GenericValue> } // serde default
// StateValue::Generic(GenericState)  (tag "generic")

// types/state.rs
pub fn apply(entity: &Entity, def: &TypeDef, state: &mut EntityState, topic: &str, payload: &[u8], templates: &Templates) -> Result<Applied, StateError>;
pub fn primary(def: &TypeDef, state: &GenericState) -> Option<&GenericValue>;

// types/mod.rs
pub fn alerting(def: &TypeDef, state: &GenericState) -> bool;
pub fn history_num(def: &TypeDef, state: &GenericState) -> Option<f64>;
```

Rules: for every field whose `cfg[field.topic] == topic`, read through `field.template` (or raw payload), then by shape: on_off → `Bool` by matching the payload words (`payload_on`/`state_on` keys with defaults); choice → the canonical `value` of the `WordDef` whose configured word (or default) equals the payload, case-sensitive, else `Text(raw)`; number → `Number` (non-finite → field removed); text/version → `Text`; picture → `Number(at_ms)` (bytes handled by the hub, Task 10). Merge into the existing `GenericState` (other fields kept). Fields not yet seen are absent. `Scalar::of_state(Generic)` = `primary` converted (Bool→bool, Number→number, Text→text). `num_of(Generic)` returns `None` (no table access there); history uses `history_num` instead — see Task 8.

- [ ] **Step 1: Write the failing tests** (using the real table):
  - `lock_payload_maps_to_canonical_word` — `"LOCKED"` on state_topic → `fields["state"] == Text("locked")`.
  - `custom_state_word_is_honoured` — cfg `state_locked: "closed"`, payload `"closed"` → `"locked"`.
  - `unmapped_payload_is_kept_as_raw_text` — `"FOO"` → `Text("FOO")`, `alerting == false`.
  - `fan_fields_merge_across_topics` — state then percentage messages → both fields present.
  - `alerting_lock_when_unlocked_or_jammed`.
  - `history_num_maps_lock_and_tracker` — locked → 1.0, unlocked → 0.0, `home` → 1.0, a fan percentage `60` → `None` for fan (history field is `state`), humidifier target `45` → 45.0.
  - `scalar_of_generic_is_its_primary`.
- [ ] **Step 2: Run** `cargo test -p oikos-core` — Expected: FAIL.
- [ ] **Step 3: Implement**; wire `state/kinds/mod.rs` Generic arm to `types::state::apply` with `type_def(&entity.component)`; `format.rs::display_value` Generic arm returns the primary value as text (refined in Task 13); `views.rs` keeps its `_` arm.
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): generic state from the type table`.

---

### Task 5: Generic command and fire-and-forget confirmation

**Files:**
- Create: `crates/oikos-core/src/types/command.rs`
- Modify: `crates/oikos-core/src/model/command.rs` (`Command::Control`), `crates/oikos-core/src/command/mod.rs:73` (`command_name`), `crates/oikos-core/src/command/kinds.rs`, `crates/oikos-os/src/hub.rs` (`Hub::send_command` ~l.526), `crates/oikos-os/src/mcp/params.rs` (`SendCommandParams.command` doc), `crates/oikos-os/src/mcp/mod.rs` (`instructions()`)
- Test: `crates/oikos-core/tests/command.rs`, `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Consumes: Tasks 1–4; `command::Mapper` helpers (`topic`, `word`, `render`, `publish`, `in_range`, `listed`).
- Produces:

```rust
// Command::Control { control: String, #[serde(default, skip_serializing_if = "Option::is_none")] value: Option<serde_json::Value> }  (tag "control")
// command_name(Command::Control{..}) == "control"
pub fn map(m: &Mapper, def: &TypeDef) -> Result<Vec<Publish>, CommandError>;
pub fn expects_echo(def: &TypeDef, entity: &Entity, control: &str) -> bool; // control.field is Some and that field's topic is in the config
```

Rules: unknown or unannounced control → `CommandError::Unsupported`. Value by shape: press → no value (any given value rejected); on_off → `Bool`; number → `Number` within the resolved range; choice → string listed in `GenericCaps.options`; text → string (honour `min`/`max` length and `pattern` from config when present). Payload: press → `payload` word; on_off → `payload_on`/`payload_off` word; others → value as string; through `ctl.template` when present (`value` in the template context); `json_key` → `{"<json_key>": value}` JSON. In `Hub::send_command`, when the entity is Generic and `expects_echo` is false, return `CommandResponse { accepted: true, reason: None, state: None }` right after a successful publish (no wait).

- [ ] **Step 1: Write the failing tests**:
  - `lock_control_publishes_lock_and_unlock_words` — `Control{control:"locked", value: Some(true)}` → publish `"LOCK"` on command_topic; `false` → `"UNLOCK"`.
  - `fan_percentage_out_of_range_is_refused`.
  - `siren_tone_is_sent_as_json` — `{"tone":"fire"}`.
  - `unannounced_control_is_unsupported`.
  - `press_with_value_is_refused`.
  - `text_longer_than_max_is_refused`.
  - hub test `scene_activate_is_accepted_without_echo` — `send_command` returns `accepted: true` immediately.
  - hub test `lock_command_waits_for_echo` — no echo within `hub_with_timeout(50)` → `accepted: false`.
- [ ] **Step 2: Run** `cargo test --workspace` — Expected: FAIL.
- [ ] **Step 3: Implement**; update the `command` field doc in `SendCommandParams` and `instructions()` to mention `{"type":"control","control":"<name>","value":…}` and that control names come from the entity's `capabilities.generic.controls`.
- [ ] **Step 4: Run** all gates — Expected: PASS (MCP instruction tests updated).
- [ ] **Step 5: Commit** `feat(core): generic control command`.

---

### Task 6: One remote per device (`device_automation` grouping)

**Files:**
- Modify: `crates/oikos-core/src/registry/routing.rs` (~l.94–170), `crates/oikos-core/src/registry/mod.rs`, `crates/oikos-core/src/discovery/parse.rs`, `crates/oikos-core/src/model/entity.rs` (`Bindings::data_topics` reads `actions[].topic`)
- Test: `crates/oikos-core/tests/registry.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: a `device_automation` discovery config `{automation_type:"trigger", topic, type, subtype, payload, device}` at `<prefix>/device_automation/<node>/<object>/config` yields one entity with `unique_id = "device_automation.<node>"`, `component = "device_automation"`, `kind = Generic`, `bindings.config = {"device": …, "actions": [{"object": "<object>", "topic", "payload", "type", "subtype"}, …]}` and `capabilities.generic.signals` = the action payloads in arrival order (deduplicated). The registry remembers which discovery topic contributed which action.

- [ ] **Step 1: Write the failing tests**:
  - `actions_of_one_device_fold_into_one_remote` — three configs for node `0xd4fe…` → one entity, `signals == ["on","off","brightness_move_up"]`.
  - `removing_one_action_keeps_the_others` — empty payload on one config topic → two signals left.
  - `removing_last_action_removes_the_remote` — `EntityRemoved` emitted.
  - `remote_subscribes_to_its_action_topics` — `Bindings::topics()` contains the action topic.
- [ ] **Step 2: Run** `cargo test -p oikos-core --test registry` — Expected: FAIL.
- [ ] **Step 3: Implement** the fold in the registry's discovery path (parse returns a per-action fragment; the registry merges into or creates the remote and emits `EntityUpserted`/`EntityRemoved`).
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): fold device_automation configs into one remote`.

---

### Task 7: Signals and the `trigger.signal` flow node

**Files:**
- Create: `crates/oikos-core/src/types/signals.rs`
- Modify: `crates/oikos-api/src/events.rs` (`WsEvent::SignalReceived`), `crates/oikos-core/src/flow/model.rs` (`NodeKind::TriggerSignal`, `type_name`, `is_trigger`, `ports`), `crates/oikos-core/src/flow/validate.rs` (`check_ifs_after_valueless_triggers` treats it like `trigger.state`: the signal name is the trigger value), `crates/oikos-os/src/hub.rs` (`ingest_message` ~l.326, `subscription_topics` ~l.455), `crates/oikos-os/src/flows/hub.rs` (`check_house` ~l.278, watched topics like `watched_button_topics` l.50), `crates/oikos-os/src/flows/engine.rs` (`handle_event` l.416, `run_node` l.687), `crates/oikos-os/src/mcp/mod.rs` (`instructions()`)
- Test: `crates/oikos-core/tests/state.rs`, inline flow model tests, `crates/oikos-os/tests/flows.rs`, `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Consumes: Tasks 4, 6; the button mechanism (`ButtonPressed`, retain flag on `ingest_message`).
- Produces:

```rust
// types/signals.rs
pub fn signal_of(entity: &Entity, def: &TypeDef, topic: &str, payload: &[u8], templates: &Templates) -> Option<String>;
// device_automation: the action whose topic matches and whose payload equals the message → its payload;
// event: the rendered event_type; tag: the rendered tag id.

// WsEvent::SignalReceived { id: String, signal: String }
// NodeKind::TriggerSignal(TriggerSignalParams)  serde "trigger.signal"
pub struct TriggerSignalParams { pub entity: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub signal: Option<String>, #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")] pub extra: BTreeMap<String, Value> }
```

Rules: on a **non-retained** message whose topic belongs to a signal-placed entity, the hub updates the entity's `last` field and an `at` field (`Number`, ms) through the normal state path — so the sheet shows it and history records every press, even a repeated one — **and** broadcasts `SignalReceived`. A retained message updates the state only. `check_house` also rejects `trigger.state` on a signal-placed entity (its state changes on every press; `trigger.signal` is the way). The engine fires `trigger.signal` on `SignalReceived` when `entity` matches and `signal` is `None` or equal; the trigger value is the signal name (so an `if` with no entity compares it). `check_house` rejects an unknown entity, a non-signal entity, and a `signal` not in `capabilities.generic.signals` (event: `options["last"]`).

- [ ] **Step 1: Write the failing tests**:
  - `remote_press_is_a_signal` — `signal_of` on the action topic with payload `"on"` → `Some("on")`.
  - `trigger_signal_round_trips_json` — `{"type":"trigger.signal","params":{"entity":"device_automation.0xd4fe…","signal":"on"}}`.
  - engine `signal_fires_matching_trigger_only` — `on` fires, `off` does not.
  - engine/hub `retained_signal_never_fires_a_flow` — `ingest_message(topic, b"on", true)` → no `SignalReceived`, no run.
  - flows hub `signal_trigger_rejects_unannounced_signal`.
  - flows hub `state_trigger_on_a_remote_is_rejected`.
- [ ] **Step 2: Run** `cargo test --workspace` — Expected: FAIL.
- [ ] **Step 3: Implement**; mention `trigger.signal` in `instructions()` next to `trigger.state`.
- [ ] **Step 4: Run** all gates (web arms for the new `NodeKind`: add minimal arms in `flow_text.rs`, `flows.rs`, `flow_edit.rs` prefix `"signal"`, `icons.rs` node icon, `screens/flow_canvas/settings.rs` title + `kind_fields` placeholder; real editor in Task 17) — Expected: PASS.
- [ ] **Step 5: Commit** `feat(flows): trigger.signal for remotes, events and tags`.

---

### Task 8: History for generic entities and secret text

**Files:**
- Modify: `crates/oikos-core/src/history/mod.rs` (`LastWritten::diff` ~l.112), `crates/oikos-os/src/hub.rs` (`ingest` history feed ~l.280), `crates/oikos-api/src/views.rs` (`EntityView::from`), `crates/oikos-api/src/snapshot.rs`
- Test: inline `history/mod.rs` tests, `crates/oikos-api/tests/snapshot.rs`

**Interfaces:**
- Consumes: Tasks 2, 4.
- Produces: `Incoming` gains `num: Option<f64>` computed by the hub (`types::history_num` for Generic, `num_of` otherwise) — or an equivalent that keeps `oikos-core::history` free of table lookups; secret entities are not offered to history at all; other generic entities are recorded as today (non-numeric rows get `num: None`). One helper `oikos_api::mask_secret(entity, &mut EntityState)` blanks every `GenericState` field of a secret entity (`capabilities.generic.secret`) to `Text("••••")`; it is applied everywhere a state leaves the hub: `EntityView::from` (snapshot, MCP `get_entity`/`oikos://snapshot`), `WsEvent::StateChanged`, and `CommandResponse.state`.

- [ ] **Step 1: Write the failing tests**:
  - `lock_change_is_recorded_as_one_and_zero`.
  - `fan_preset_change_is_not_recorded` (history field is `state`; a preset-only change writes no row).
  - `secret_text_never_reaches_history`.
  - snapshot `secret_text_is_masked_in_the_snapshot`.
  - hub `secret_text_is_masked_in_state_changed_events`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(history): record generic entities through the type table`.

---

### Task 9: `mainControl` override and device update marks

**Files:**
- Modify: `crates/oikos-core/src/model/ui_override.rs` (field, `Default`, `FIELD_TYPES`, `FIELDS`, doc counts), `crates/oikos-os/src/hub.rs` (`set_override` path: reject a `mainControl` that is neither `"none"` nor an announced control, with `PatchError`), `crates/oikos-os/src/mcp/params.rs:76-83` (patch doc), `crates/oikos-os/src/mcp/mod.rs:148,169`, `crates/oikos-api/src/views.rs:77` (`DeviceView.update: Option<UpdateView>`), `crates/oikos-api/src/snapshot.rs` / devices builder, `crates/oikos-core/tests/ui_override.rs`
- Test: `crates/oikos-core/tests/ui_override.rs`, `crates/oikos-os/tests/http.rs`, `crates/oikos-api/tests/snapshot.rs`

**Interfaces:**
- Produces: `UiOverride.main_control: Option<String>` (wire `mainControl`); `pub struct UpdateView { pub installed: Option<String>, pub latest: Option<String>, pub available: bool }` on `DeviceView.update`, filled from the device's `update` entity (`available` = both present and different).

- [ ] **Step 1: Write the failing tests**:
  - `main_control_round_trips_and_null_clears`.
  - http `main_control_patch_rejects_unknown_control` — 400 for `{"mainControl":"warp"}` on a lock; 200 for `"none"` and `"locked"`.
  - `set_override_patch_doc_names_every_field_and_value` keeps passing (add `mainControl` to the doc).
  - snapshot `device_with_newer_firmware_is_marked` — installed `"1.0"`, latest `"1.1"` → `available: true`; equal → `false`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates (fix `DeviceView` literals in web tests) — Expected: PASS.
- [ ] **Step 5: Commit** `feat(api): mainControl override and firmware update marks`.

---

### Task 10: Pictures for camera and image

**Files:**
- Create: `crates/oikos-os/src/pictures.rs`
- Modify: `crates/oikos-os/src/hub.rs` (`ingest_message`: store bytes for picture fields before applying state), `crates/oikos-os/src/http/api.rs:33` (route), `crates/oikos-os/src/lib.rs` or `main.rs` module list
- Test: `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Produces:

```rust
pub struct Pictures { /* Mutex<HashMap<String, Picture>> */ }
pub enum Picture { Bytes { content_type: String, body: bytes::Bytes }, Url(String) }
impl Pictures { pub fn put(&self, id: &str, p: Picture); pub fn get(&self, id: &str) -> Option<Picture>; }
// GET /api/v1/entities/{id}/picture -> 200 image bytes | 404 unknown/no picture | 502 URL fetch failed
```

Rules: bytes payloads over 5 MiB are dropped with `warn!`; `image_encoding: "b64"` decodes base64; `content_type` from config (default `image/jpeg`). URL pictures are fetched on request with `reqwest` (timeout 10 s, `redirect(Policy::none())`, same 5 MiB cap); `url_template` renders the URL from the payload.

- [ ] **Step 1: Write the failing tests**: `camera_bytes_are_served_by_the_picture_route`, `unknown_entity_picture_is_404`, `oversized_picture_is_dropped`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test http` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(hub): picture route for camera and image entities`.

---

### Task 11: Publish the type table for the app

**Files:**
- Modify: `crates/oikos-os/src/hub.rs` (publish on `ConnectionChanged → Online`; `ingest` ignores `{ui_prefix}/_types` like it treats `_areas`), `crates/oikos-os/src/mqtt.rs` if needed
- Test: `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Produces: retained publish of `types::table_json()` to `{ui_prefix}/_types` each time the broker link comes Online; the hub never parses its own echo of that topic as an override.

- [ ] **Step 1: Write the failing tests**: `type_table_is_published_retained_on_connect` (receiver gets `Publish{topic:"oikos/ui/_types", retain:true, payload == table_json()}`), `type_table_echo_is_not_an_override`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(hub): publish the type table retained`.

---

### Task 12: Docs

**Files:**
- Modify: `README.md` (supported types list; `_types` topic; picture route), `docs/adding-a-device.md` (adding a type = a table entry; the field/control vocabulary)
- Test: none (docs)

- [ ] **Step 1:** Update both files; list the 16 components and the table vocabulary (placement, shapes, fields, controls, main, alerts, history).
- [ ] **Step 2: Commit** `docs: generic entity types and the type table`.

---

### Task 13: Generic card (web)

**Files:**
- Create: `crates/oikos-web/src/cards/generic.rs`
- Modify: `crates/oikos-web/src/cards/mod.rs` (`pub mod generic;`), `crates/oikos-web/src/screens/home.rs:108` (`card_for` Generic arm → `GenericCard`, signal/target/update placements return no card on Home), `crates/oikos-web/src/format.rs:11`, `crates/oikos-web/src/icons.rs:282` (glyphs for the table's `icon` names), `crates/oikos-web/styles/main.css`
- Test: `crates/oikos-web/tests/home_groups.rs`-style new file `crates/oikos-web/tests/generic_card.rs`; inline tests in `format.rs`

**Interfaces:**
- Consumes: `oikos_core::types::{type_def, main_control, alerting, primary}`, `CardFrame`, `Sender`, `PowerToggle`, `ModeSelect`, `Stepper`.
- Produces: `#[component] pub fn GenericCard(entity: EntityView) -> impl IntoView`; `format::generic_value(def, state, caps) -> String` (choice → canonical word humanised, e.g. `not_home` → "Away", `locked` → "Locked"; number with unit; bool → On/Off; secret → "••••"; missing → "—").

Main control by shape: on_off with `labels` → one button showing the label for the opposite of the current state (locked → "Unlock"); on_off without labels → `PowerToggle`; press → button with the control name humanised ("Activate", "Start"); number → `Stepper`; choice → `ModeSelect`; text → "Edit" button that opens the sheet. Picture primary → `<img src="/api/v1/entities/{id}/picture?v={at}">` with the alert-row fallback glyph on error. Alert → `CardFrame alert=true`.

- [ ] **Step 1: Write the failing tests** (wasm): `lock_card_shows_locked_and_unlock_button`, `unlocked_lock_card_has_alert_class`, `generic_card_without_main_control_shows_state_only` (device_tracker "Home"), `main_control_override_none_hides_the_control`, `remote_has_no_home_card`.
- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome --test generic_card` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates + wasm tests — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): generic card for the new entity types`.

---

### Task 14: Generic sheet panel (web)

**Files:**
- Create: `crates/oikos-web/src/screens/generic_panel.rs`
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs` (~l.196 `SheetBody`: insert `<GenericPanel>` after Controls, like `ColourPanel`), `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/styles/main.css`
- Test: `crates/oikos-web/tests/sheet.rs`

**Interfaces:**
- Produces: `#[component] pub fn GenericPanel(entity: Memo<Option<EntityView>>) -> impl IntoView` — one row per announced control by shape (toggle / slider with resolved range / chip row / button / text field with min/max/pattern, password input when secret), plus: signal placement → "Last: <signal>, <relative time>" line; update placement → "Firmware <installed> → <latest>" with an "Update available" chip when `available`; picture → full-width image.

- [ ] **Step 1: Write the failing tests**: `fan_sheet_lists_power_speed_and_preset`, `secret_text_uses_a_password_field`, `remote_sheet_shows_last_press`, `update_sheet_shows_available_version`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates + wasm tests — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): generic controls in the device sheet`.

---

### Task 15: "Main control" setting (web)

**Files:**
- Modify: `crates/oikos-web/src/settings_rules.rs` (`Shown.main_control` = entity is Generic with at least one announced control; `other_saved` array `[(..); 13]` → 14; `clear_field`), `crates/oikos-web/src/override_editor.rs:21` (`diff!` gains `main_control => "mainControl"`), `:652` (new `Section`/`ChipField` "Main control" next to "Show as", options = announced controls humanised + "None")
- Test: inline tests in `settings_rules.rs` and `override_editor.rs`

- [ ] **Step 1: Write the failing tests**: `main_control_is_shown_for_generic_with_controls`, `main_control_is_hidden_for_lights`, `main_control_patch_uses_camel_case`, `clearing_main_control_sends_null`.
- [ ] **Step 2: Run** `cargo test -p oikos-web` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): main control setting in the device sheet`.

---

### Task 16: Updates list in Settings (web)

**Files:**
- Modify: `crates/oikos-web/src/screens/settings.rs` (~l.160, new `<Panel title="Updates available">` between Counts and MCP, one row per device with `update.available`, linking to the device; empty state "All devices are up to date"), `crates/oikos-web/src/screens/devices.rs:41` (`DeviceRow` badge)
- Test: new `crates/oikos-web/tests/settings_updates.rs`

- [ ] **Step 1: Write the failing tests**: `settings_lists_devices_with_updates`, `settings_shows_up_to_date_when_none`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates + wasm tests — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): firmware updates list in Settings`.

---

### Task 17: Flow canvas support (web)

**Files:**
- Modify: `crates/oikos-web/src/flow_picker.rs` (~l.69 `commands`: Generic → one `Command::Control` entry per announced control with a sensible default value; signal entities → one `TriggerSignal` entry per signal plus "any"; `empty_query_has_generic_entries` unchanged unless a generic entry is added), `crates/oikos-web/src/flow_text.rs` (`command_sentence` arm for `Control`: "Lock Front door", "Set Bedroom fan percentage to 60"; `sentence` arm for `TriggerSignal`: "When Remote sends on"), `crates/oikos-web/src/screens/flow_canvas/settings.rs` (`kind_fields` TriggerSignal: entity field + signal chip row from `capabilities.generic.signals`), `crates/oikos-web/src/flows.rs` (`trigger_summary` arm)
- Test: inline tests in `flow_picker.rs`, `flow_text.rs`; `crates/oikos-web/tests/flow_canvas.rs`

- [ ] **Step 1: Write the failing tests**: `picker_offers_lock_controls`, `picker_offers_remote_signals`, `control_command_sentence_reads_naturally`, `signal_trigger_sentence_names_the_signal`, wasm `signal_trigger_editor_lists_signals`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all gates + wasm tests — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): signal triggers and generic controls on the flow canvas`.

---

### Task 18: Live-house check

**Files:** none (verification); fix commits if something breaks.

- [ ] **Step 1:** `tools/restart.sh` (trunk build + hub rebuild + restart on :8081).
- [ ] **Step 2:** MCP `list_ignored` → none of the 16 components remain (only `device`-based reasons or genuinely unknown ones).
- [ ] **Step 3:** MCP `search` "remote" → each of the 3 remotes is one `device_automation.<ieee>` entity with its signals; press a button physically or ask the user to → `SignalReceived` and the sheet's "Last:" line updates.
- [ ] **Step 4:** Web Settings → "Updates available" lists the devices whose `update` entity reports a newer version (10 `update` entities exist); device sheets show the version line.
- [ ] **Step 5:** The phone `device_tracker` (`oikos_phone`) shows Home / Away on its card; the `text` entity shows its value and edits round-trip.
- [ ] **Step 6:** Take headless-Chrome CDP screenshots (window ≥ 500 px wide) of Home and one generic sheet; report them.
- [ ] **Step 7:** Commit any fixes; report the commit range.
