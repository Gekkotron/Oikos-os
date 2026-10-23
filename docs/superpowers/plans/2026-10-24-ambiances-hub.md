# Ambiances, hub side — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The hub stores ambiances (retained MQTT), validates them against the house, and applies them on request. Applying reports each action's outcome. It is exposed over HTTP, the WebSocket and MCP.

**Architecture:** Wire types, shape validation and category resolution go in a new `oikos-api/src/ambiances.rs`, so Plan 2 (web) can reuse them. The hub side is a new `oikos-os/src/ambiances/` module (store, CRUD, apply), wired into `Hub` the way dashboards are: the topic is `<ui_prefix>/ambiances/<id>`, intercepted in `Hub::ingest`. Apply goes through the existing `Hub::send_command`, `set_flow_enabled`, `run_flow` and climate override calls. Nothing new talks to MQTT directly except the retained definition.

**Tech Stack:** Rust 2024 workspace, tokio, axum 0.8, rmcp, serde/schemars, futures-util.

**Spec:** `docs/superpowers/specs/2026-10-24-ambiances-design.md`

## Global Constraints

- `oikos-api` must still build for `wasm32-unknown-unknown`: no I/O, no clock, no native-only dependency in it.
- Generic project: no house-specific names, entity ids, rooms or hosts in tracked files, tests or docs. Tests use the existing fixtures in `crates/oikos-core/tests/fixtures/`.
- Wire field names are snake_case (like `Dashboard`), except values the spec fixes (`"do"` in a flow action).
- Errors follow the existing `HubError` → HTTP mapping: `Rejected` → 422, `Publish` → 503, `Unknown*` → 404.
- No changes under `crates/oikos-web` except the smallest compile fix for the new `WsEvent` variants (Task 1).
- Every commit runs `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` clean. Commits use `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com` and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Build hygiene: the disk is tight. Before building, check `du -sh ~/dev-scratch/cargo-target`, and run `cargo clean` if it is over 5 GB. Build in this worktree with a private target and no debug info: `CARGO_TARGET_DIR=$HOME/dev-scratch/cargo-target-ambiances CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo …`.
- Docs: update the `docs/` pages named in Task 7. Never update the README.

## Decisions made while planning (refining the spec)

The spec was updated to match, in the same commit as Task 7.
- **Topic:** `<ui_prefix>/ambiances/<id>`, like dashboards, so there is no new `[ambiances]` config section.
- **Types in `oikos-api`:** `EntityView` lives there, and the web needs the same category rules.
- **Report shape (refines spec A.3):** one line per action, with `outcome`, `total` (entities or zones targeted), `failed` and `skipped` lists of `{id, reason}`, and an action-level `reason`. `outcome` is:
  - `failed` if anything failed;
  - `skipped` if nothing was attempted;
  - `ok` otherwise, even when some offline entities were skipped. They are still listed in `skipped`.
- **Climate setpoint on the wire:** the action carries `setpoint` as raw JSON (a number or `"off"`) and `mode` as a string. The hub turns `{setpoint, mode, minutes: hours×60}` into `oikos_climate::view::OverrideRequest` with serde, so the climate crate's own parsing and range checks apply. `oikos-api` does not depend on `oikos-climate`.
- **Climate on save:** a climate action is refused on save while `[climate]` is off ("the climate engine is off"). At apply time it is reported `skipped` with the same reason, in case the engine was turned off after the save.
- **"Every zone"** (`zone: null`) means every zone whose state is `Active` at apply time. The engine refuses overrides on any other zone.

## Review Focus

1. **A light that is already off, inside "every light off".** It must not be commanded and must count as ok. Otherwise Bonne nuit waits for timeouts and reports failures on a quiet house. Pinned by `already_off_light_is_not_commanded` (Task 4).
2. **A door, siren or chime switch in a house-wide "every plug off".** It must never be targeted. Pinned by `plugs_never_include_door_siren_chime` (Task 2).
3. **A group entity and its members.** Both match "every light", so a member would be commanded twice. `group_*` ids must be excluded. Pinned by `groups_are_not_category_members` (Task 2).
4. **Two taps on the same ambiance (two tabs, or web + MCP).** The second must be refused while the first runs, and the lock must be released when a run ends, including when it fails. Pinned by `second_apply_while_running_is_refused` and `lock_released_after_failed_run` (Task 4).
5. **An ambiance whose entity was deleted after saving.** It must stay valid and listed, and apply must report that action `skipped: no longer exists` instead of erroring the whole apply. Pinned by `vanished_entity_is_skipped_not_fatal` (Task 4).

---

### Task 1: Wire types and shape validation

**Files:**
- Create: `crates/oikos-api/src/ambiances.rs`
- Modify: `crates/oikos-api/src/lib.rs` (module + re-exports)
- Modify: `crates/oikos-api/src/events.rs` (two `WsEvent` variants)
- Modify: `crates/oikos-web/src/house.rs:188` region and `crates/oikos-web/src/house_state.rs:79` (ignore the new variants, compile fix only)

**Interfaces:**
- Produces (all `pub`, derive `Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema`):
  - `struct Ambiance { id: String, name: String, subtitle: Option<String>, icon: Option<String>, pinned: bool, order: u32, actions: Vec<AmbianceAction>, #[serde(flatten)] extra: serde_json::Map<String, Value> }`. Every field after `name` is `#[serde(default)]`; `subtitle` and `icon` are `skip_serializing_if = "Option::is_none"`.
  - `#[serde(tag = "type", rename_all = "snake_case")] enum AmbianceAction { Category(CategoryAction), Device(DeviceAction), Climate(ClimateAction), Flow(FlowAction) }`
  - `struct CategoryAction { what: Category, to: Target, brightness: Option<u8>, area: Option<String>, except: Vec<String> }`. `brightness` is in %, skipped when `None`; `except` is skipped when empty.
  - `#[serde(rename_all = "snake_case")] enum Category { Lights, Plugs, Covers }`
  - `#[serde(rename_all = "snake_case")] enum Target { On, Off, Open, Close }`
  - `struct DeviceAction { entity: String, command: oikos_core::model::Command }`
  - `struct ClimateAction { zone: Option<String>, setpoint: Option<Value>, mode: Option<String>, hours: Option<u32>, clear: bool }`. All fields default; `clear` is skipped when false.
  - `struct FlowAction { flow: String, #[serde(rename = "do")] action: FlowDo }`
  - `#[serde(rename_all = "snake_case")] enum FlowDo { Enable, Disable, Run }`
  - `struct AmbianceView { id: String, ambiance: Option<Ambiance>, invalid: Option<String> }`
  - `struct ApplyReport { ambiance: String, started_at: u64, actions: Vec<ActionReport> }`
  - `struct ActionReport { index: usize, outcome: Outcome, total: usize, failed: Vec<Problem>, skipped: Vec<Problem>, reason: Option<String> }`
  - `#[serde(rename_all = "snake_case")] enum Outcome { Ok, Failed, Skipped }`
  - `struct Problem { id: String, reason: String }`
  - `const AMBIANCE_ICONS: [&str; 12] = ["sofa", "moon", "door_out", "book", "sun", "sparkle", "film", "plate", "bath", "leaf", "flame", "house"]`
  - `fn is_valid_ambiance_id(id: &str) -> bool`: 1 to 48 characters from `[a-z0-9_]`.
  - `fn validate_ambiance(a: &Ambiance) -> Result<(), String>`: shape only, no house.
  - `WsEvent::AmbiancesChanged { ambiances: Vec<AmbianceView> }` and `WsEvent::AmbianceApplied { report: ApplyReport }`.

- [ ] **Step 1: Write the failing tests** in `ambiances.rs` `mod tests`:
  - `spec_example_round_trips`: the JSON of spec Appendix A.2, with the `// brightness` comment removed, deserializes, and serializing it gives back the same `Value`.
  - `unknown_top_level_fields_survive`: `{"id":"a","name":"A","color":"#fff"}` round-trips with `extra["color"] == "#fff"`.
  - `minimal_ambiance_defaults`: `{"id":"a","name":"A"}` gives `pinned == false`, `order == 0` and empty `actions`.
  - `validate_rejects`: each of these gives `Err` from `validate_ambiance`:
    - id `"Bonne Nuit"`, or 49 characters;
    - name `"  "`, or longer than 60 characters;
    - subtitle longer than 80 characters;
    - icon `"rocket"`;
    - `lights` with `to: open`;
    - `covers` with `to: off`;
    - `plugs` with `brightness: 50`;
    - `lights`, `to: off`, `brightness: 50`;
    - `lights`, `to: on`, `brightness: 0`, or `brightness: 101`;
    - a climate action with `clear: true` and `setpoint: 16`;
    - a climate action with neither `clear` nor a setpoint or mode;
    - a climate action with a setpoint and no `hours`, or with `hours: 0`, or `hours: 721`;
    - a climate action with `mode: "dry"`.
  - `validate_accepts_spec_example`: the A.2 example is `Ok`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-api ambiances`
Expected: FAIL to compile (the module does not exist yet).

- [ ] **Step 3: Implement the types, `is_valid_ambiance_id` and `validate_ambiance`** with the rules the tests pin:
  - `mode` must be `heat` or `cool`;
  - `setpoint`, when present, must be a JSON number or the string `"off"`;
  - an action's error names its index, e.g. `"action 2: covers take open or close"`.

  Re-export everything above from `lib.rs`. Add the two `WsEvent` variants, then the ignore arms in the web (`=> {}`).

- [ ] **Step 4: Run the tests and check the wasm build**

Run: `cargo test -p oikos-api ambiances && cargo build -p oikos-api --target wasm32-unknown-unknown && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS, and both builds succeed.

- [ ] **Step 5: Commit** with `feat(api): ambiance wire types and shape validation`.

### Task 2: Category resolution and "already there"

**Files:**
- Modify: `crates/oikos-api/src/ambiances.rs`

**Interfaces:**
- Consumes: `CategoryAction`, `Category`, `Target` (Task 1); `EntityView` (`kind` is the effective kind, `protocol_kind`, `ui`, `area`, `category`, `enabled_by_default`, `part_of`, `capabilities.brightness`, `state`); `oikos_core::model::{SwitchType, Kind, Command, StateValue, Availability}`.
- Produces:
  - `fn category_members<'a>(action: &CategoryAction, entities: &'a [EntityView]) -> Vec<&'a EntityView>`. Offline entities are included: the web uses this for its "12 lights" count.
  - `struct CategoryPlan { send: Vec<(String, Command)>, already: Vec<String>, offline: Vec<String> }`
  - `fn plan_category(action: &CategoryAction, entities: &[EntityView]) -> CategoryPlan`
  - `fn already_there(view: &EntityView, cmd: &Command) -> bool`

**Rules** (spec §4.1):
- **Membership:**
  - `category == EntityCategory::Primary`, `enabled_by_default`, `part_of.is_none()`;
  - the id does not start with `group_`, and is not in `except`;
  - when `area` is `Some`, `view.area == area`;
  - `ui.hidden` does **not** exclude.
- **Lights:** `kind == Kind::Light`.
- **Plugs:** `protocol_kind == Kind::Switch` and `SwitchType::of(protocol_kind, &ui)` is `Some(Switch | Plug)`.
- **Covers:** `kind == Kind::Cover`.
- **Command per member:**
  - lights `off`, and plugs `off`: `TurnOff`;
  - lights `on`: `TurnOn { brightness: Some(round(pct × 255 / 100)) }` when the action has a brightness and `capabilities.brightness` is true, else `TurnOn` with all fields `None`;
  - plugs `on`: `TurnOn` with all fields `None`;
  - covers: `Open` or `Close`.
- **`already_there`:**
  - `TurnOff`: `OnOff(false)` or `Light { on: false }`;
  - `TurnOn` without brightness: on;
  - `TurnOn { brightness: b }`: on, and brightness within ±5 of `b` (2 % of 255);
  - `Close`: `position == Some(0)` or `state == "closed"`;
  - `Open`: `position == Some(100)` or `state == "open"`;
  - `SetPosition { p }`: `position == Some(p)`;
  - `SelectOption { o }`: `Text(o)`;
  - `SetTemperature { t }`: climate `target` within 0.05 of `t`;
  - `SetHvacMode { m }`: climate `mode == Some(m)`;
  - anything else, or `Unknown` state: false.
- **`plan_category`:** offline members go to `offline`, members already there go to `already`, the rest to `send`.

- [ ] **Step 1: Write the failing tests** (build `EntityView`s with a small helper like `screens/home.rs`'s `entity()` test helper):
  - `plugs_never_include_door_siren_chime`: switches with `showAs` `door`, `siren` and `chime`, plus one with no `showAs` and one with `showAs: plug`. Members are exactly the last two.
  - `switch_shown_as_light_is_a_light_not_a_plug`.
  - `groups_are_not_category_members`: `group_x` (a light) is excluded.
  - `non_primary_and_folded_are_excluded_hidden_is_kept`.
  - `area_and_except_filter`.
  - `brightness_pct_maps_to_255_scale_only_when_dimmable`: 30 % → `Some(77)` on a dimmable light, and `None` on a non-dimmable one.
  - `plan_splits_offline_already_and_send`: an off light, an on light and an offline light, with `to: off`. One `send`, one `already`, one `offline`.
  - `already_there_cases`: one assertion per bullet of the rule above, including brightness 250 vs `Some(255)` → true, and 240 → false.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-api ambiances`
Expected: FAIL (the functions are not defined).

- [ ] **Step 3: Implement the three functions** in `ambiances.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-api ambiances && cargo build -p oikos-api --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(api): resolve ambiance categories against the house`.

### Task 3: Hub store, ingest and CRUD

**Files:**
- Create: `crates/oikos-os/src/ambiances/mod.rs`, `store.rs`, `hub.rs`
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod ambiances;`)
- Modify: `crates/oikos-os/src/hub.rs`:
  - fields `ambiance_store: Mutex<AmbianceStore>` and `ambiances_running: Mutex<BTreeSet<String>>`, initialised in `Hub::new`;
  - an `ingest` branch next to the `dashboard_id_of` branch (line ~371);
  - `HubError::UnknownAmbiance(String)`, with `#[error("unknown ambiance `{0}`")]`;
  - accessors next to `dashboard_store()` (line ~1800).
- Modify: `crates/oikos-os/src/http/api.rs:188-193` (`UnknownAmbiance` → 404)
- Test: `crates/oikos-os/tests/ambiances.rs` (new), with the same `hub()`, `fixture()` and `config()` helpers as `tests/hub.rs`.

**Interfaces:**
- Consumes: `Ambiance`, `AmbianceView`, `AmbianceAction`, `validate_ambiance`, `is_valid_ambiance_id` (Task 1).
- Produces:
  - In `store.rs`: `enum StoreApplied { Changed, Unchanged, Removed, NotFound }`; `AmbianceStore::new()`; `apply(&mut self, id: &str, payload: &[u8]) -> StoreApplied`; `get(&self, id: &str) -> Option<&Ambiance>`; `knows(&self, id: &str) -> bool` (valid or invalid); `list(&self) -> Vec<AmbianceView>`, sorted by `(order, id)`, with invalid entries last by id.
  - On `Hub`:
    - `pub fn ambiance_topic(&self, id: &str) -> String`: `"{ui_prefix}/ambiances/{id}"`;
    - `fn ambiance_id_of<'a>(&self, topic: &'a str) -> Option<&'a str>`;
    - `pub(crate) fn ingest_ambiance(&self, id: &str, payload: &[u8])`. It broadcasts `WsEvent::AmbiancesChanged { ambiances: self.ambiances() }` on `Changed` or `Removed`;
    - `pub fn ambiances(&self) -> Vec<AmbianceView>`;
    - `pub fn ambiance(&self, id: &str) -> Option<AmbianceView>`;
    - `pub async fn set_ambiance(&self, a: Ambiance) -> Result<AmbianceView, HubError>`;
    - `pub async fn delete_ambiance(&self, id: &str) -> Result<(), HubError>`.

**Rules:**
- **Decode** (follow `dashboards/store.rs`):
  - an empty payload removes the entry;
  - a payload byte-identical to the last one is `Unchanged`;
  - a missing or null `id` takes the topic id; an `id` different from the topic is invalid (`"id `x` does not match the topic"`);
  - a payload that `validate_ambiance` refuses is kept as invalid, with that reason.
- **`set_ambiance`:** `validate_ambiance` (→ `Rejected`), then `check_house` (→ `Rejected` with the action index), then publish retained and `ingest_ambiance` locally, like `write_dashboard`. `check_house` checks:
  - **device:** the entity exists, `map_command` succeeds, and alarm codes are refused, as in `flows/hub.rs:325-412`;
  - **category:** `area` resolves to a known, non-unassigned area, as in `flows/hub.rs` `check_area`. It is stored as the resolved id. Every `except` id exists;
  - **flow:** `flow_store().exists(flow)`;
  - **climate:** `[climate]` is on (else `"the climate engine is off"`); a `Some(zone)` is a known zone; and `serde_json::from_value::<OverrideRequest>(json!({"setpoint", "mode", "minutes": hours*60}))` parses, unless `clear`.
- **`delete_ambiance`:** `UnknownAmbiance` unless `knows`. Publishes an empty retained payload, then ingests it locally.

- [ ] **Step 1: Write the failing tests** in `tests/ambiances.rs`:
  - `retained_payload_is_listed_and_empty_payload_deletes`;
  - `invalid_payload_is_kept_with_reason`: `{"id":"a","name":""}` on `oikos/ui/ambiances/a`, listed with `invalid.is_some()`;
  - `topic_id_mismatch_is_invalid`;
  - `set_ambiance_publishes_retained_and_lists_it`: the publish topic is `oikos/ui/ambiances/a`, `retain == true`;
  - `set_ambiance_rejects_unknown_entity_area_flow`: three cases;
  - `set_ambiance_resolves_area_name_to_id`: use an area from `set_areas`;
  - `set_ambiance_rejects_climate_while_engine_off`: the default config has `[climate]` off;
  - `delete_unknown_is_404_kind`: `HubError::UnknownAmbiance`;
  - `ingest_broadcasts_ambiances_changed`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-os --test ambiances`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the store, the `Hub` wiring and the CRUD** per the rules above.

- [ ] **Step 4: Run the tests and the whole crate**

Run: `cargo test -p oikos-os --test ambiances && cargo test -p oikos-os`
Expected: PASS, with no regression in `tests/hub.rs` (the ingest order matters: ambiance topics must not reach the override handler).

- [ ] **Step 5: Commit** with `feat(hub): store ambiances retained under <ui_prefix>/ambiances`.

### Task 4: Apply

**Files:**
- Create: `crates/oikos-os/src/ambiances/apply.rs`
- Test: `crates/oikos-os/tests/ambiances.rs`

**Interfaces:**
- Consumes:
  - `plan_category` and `already_there` (Task 2);
  - `Hub::send_command(&self, id, Command) -> Result<CommandResponse, HubError>`;
  - `Hub::set_flow_enabled(&self, id, bool)`;
  - `Hub::run_flow(&self, id)`;
  - `Hub::set_climate_override(self: &Arc<Self>, zone, Override)`;
  - `Hub::clear_climate_override(self: &Arc<Self>, zone)`;
  - `Hub::climate_enabled()`;
  - `climate_store().zones` and `zone_settings(zone).state` for "every active zone";
  - `oikos_api::entity_views(&self.read())`, recomputed before each action.
- Produces: `pub async fn apply_ambiance(self: &Arc<Self>, id: &str) -> Result<ApplyReport, HubError>`. It also broadcasts `WsEvent::AmbianceApplied { report }`.

**Rules** (spec §5.2):
- **Refusals before anything runs:**
  - broker not `Online` → `HubError::Publish("broker offline")`;
  - unknown id → `UnknownAmbiance`;
  - invalid id → `Rejected(reason)`;
  - already in `ambiances_running` → `Rejected("already running")`.
- **The lock** is a guard type that removes the id on `Drop`, so a panic or early return still releases it.
- **Order:** actions run in order. Within one action, the commands go out together with `futures_util::future::join_all`. A failure never stops the run.
- **Category:**
  - `plan.offline` → `skipped` with reason `"offline"`;
  - `plan.already` → counted in `total`, not commanded;
  - each `send` that returns `Err(e)` or `accepted: false` → `failed` with `e.to_string()` or the response reason.
- **Device:**
  - the entity is gone → the whole action is `skipped`, `reason: "no longer exists"`;
  - offline → `skipped`;
  - `already_there` → ok, not sent;
  - otherwise `send_command`.
- **Climate:**
  - the engine is off → `skipped`, `reason: "the climate engine is off"`;
  - zones are `[zone]` (unknown → `skipped: "no longer exists"`), or every `Active` zone;
  - per zone, `clear_climate_override`, or `set_climate_override(zone, request.into_override(clock::now())?)`. An error goes in `failed`, with the zone id as `id`.
- **Flow:**
  - unknown → `skipped: "no longer exists"`;
  - `enable` or `disable` → `set_flow_enabled`;
  - `run` → `run_flow`, not awaited beyond its return;
  - an error → `failed`, with the flow id.
- **Outcome per action:**
  - `Failed` if `failed` is non-empty;
  - else `Skipped` if nothing was attempted (an action-level `reason`, or every target skipped);
  - else `Ok`.
- `started_at = clock::now()`.

- [ ] **Step 1: Write the failing tests.** Use the LED light fixture and the switch fixture from `tests/hub.rs`, a 50 ms confirm timeout, and an echo task that ingests the state topic after each publish, as `send_command_confirmed_by_state` does:
  - `already_off_light_is_not_commanded`: the LED's state is off, with "every light off". Nothing is published, and the outcome is `Ok` with `total == 1`.
  - `on_light_is_turned_off_and_reported_ok`.
  - `silent_device_is_failed_and_next_action_still_runs`: no echo for the switch, then a second action on the LED that echoes. The report is `[Failed, Ok]`.
  - `offline_member_is_skipped_rest_ok`.
  - `vanished_entity_is_skipped_not_fatal`: save a device action, remove the entity (an empty discovery config), then apply. That action is `Skipped` with reason `no longer exists`.
  - `flow_enable_action_enables_flow`: use a flow from `tests/flows.rs` fixtures or a minimal `set_flow`.
  - `climate_action_skipped_when_engine_off`: write the payload retained directly; validation would refuse it.
  - `second_apply_while_running_is_refused`: a 500 ms timeout and no echo. Spawn the first apply, then the second gets `Rejected("already running")`.
  - `lock_released_after_failed_run`: once a failing apply returns, a second apply runs.
  - `apply_broadcasts_report`.
  - `apply_refused_while_broker_offline`: no publish.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-os --test ambiances`
Expected: FAIL (`apply_ambiance` is not defined).

- [ ] **Step 3: Implement `apply_ambiance`** in `apply.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-os --test ambiances`
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(hub): apply an ambiance and report each action`.

### Task 5: HTTP routes

**Files:**
- Modify: `crates/oikos-os/src/http/api.rs` (routes next to `/groups`, handlers next to `put_group`)
- Test: `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Consumes: `Hub::{ambiances, set_ambiance, delete_ambiance, apply_ambiance}`.
- Produces:
  - `GET /api/v1/ambiances` → `200 Vec<AmbianceView>`;
  - `PUT /api/v1/ambiances/{id}` with body `Ambiance` → `200 AmbianceView`. An empty body id is set from the path; a different one → 422;
  - `DELETE /api/v1/ambiances/{id}` → 204, or 404;
  - `POST /api/v1/ambiances/{id}/apply` → `200 ApplyReport`, 404, 422 (`already running`, invalid) or 503 (broker offline).

- [ ] **Step 1: Write the failing tests** in `tests/http.rs`, following the existing group route tests:
  - `ambiances_crud_round_trip`;
  - `put_ambiance_path_body_mismatch_is_422`;
  - `apply_unknown_ambiance_is_404`;
  - `apply_returns_report_json`: check the keys `ambiance`, `started_at` and `actions[0].outcome`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-os --test http ambiance`
Expected: FAIL (404 from the fallback).

- [ ] **Step 3: Add the routes and handlers.** Apply needs `Arc<Hub>`, and `State(hub)` already is one.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-os --test http`
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(http): ambiance routes`.

### Task 6: MCP tools

**Files:**
- Create: `crates/oikos-os/src/mcp/ambiance_tools.rs` (`#[tool_router(router = ambiance_tool_router, vis = "pub(super)")]`, like `flow_tools.rs`)
- Modify: `crates/oikos-os/src/mcp/mod.rs` (register `+ Self::ambiance_tool_router()`, add the instructions paragraph)
- Modify: `crates/oikos-os/src/mcp/params.rs` (`SetAmbianceParams { ambiance: Ambiance }` and `AmbianceIdParams { id: String }`)
- Test: `crates/oikos-os/tests/mcp.rs` (add the four names to the tool list near line 40, plus new tests)

**Interfaces:**
- Produces the tools:
  - `list_ambiances` (read-only);
  - `set_ambiance` (idempotent);
  - `delete_ambiance` (idempotent);
  - `apply_ambiance`, which returns the `ApplyReport`.

  Wrap list results in an object if rmcp's structured output requires it; follow what `list_groups` does.
- Instructions paragraph, appended after the groups sentence in `mod.rs` (exact copy):

  > Ambiances are one-tap house states: `list_ambiances`, `set_ambiance` (create or replace, validated against the house, retained on `<ui_prefix>/ambiances/<id>`), `delete_ambiance` and `apply_ambiance` (runs the actions once, in order, and returns one report line per action: `ok`, `failed` or `skipped` with the entities and reasons). An action is `category` (`what`: `lights`, `plugs` or `covers`; `to`: `on`/`off` or `open`/`close`; optional `brightness` in %, `area`, `except`. The set is resolved at apply time, so new devices are included; door, siren and chime switches are never plugs), `device` (`entity` plus the same `command` JSON as `send_command`), `climate` (`zone` or null for every active zone, `setpoint` and/or `mode` for `hours`, or `clear: true`) or `flow` (`flow` id, `do`: `enable`, `disable` or `run`).

- [ ] **Step 1: Write the failing tests:**
  - add the four names to the expected tool list;
  - `mcp_set_then_list_ambiance`;
  - `mcp_apply_unknown_ambiance_errors`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-os --test mcp`
Expected: FAIL (the tool-list mismatch).

- [ ] **Step 3: Implement the tools and the params,** and add the instructions paragraph.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-os --test mcp`
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(mcp): ambiance tools`.

### Task 7: Docs and spec sync

**Files:**
- Create: `docs/users/ambiances.md` (what an ambiance is, the four action types, the category rules including "new devices are included" and "door/siren/chime are never plugs", what the report means)
- Modify:
  - `docs/reference/http-api.md` (the four routes);
  - `docs/reference/mcp-tools.md` (the four tools);
  - `docs/reference/mqtt-topics.md` (`<ui_prefix>/ambiances/<id>`, retained, empty = deleted);
  - `docs/users/_category_.json` (only if it lists pages explicitly);
  - `docs/superpowers/specs/2026-10-24-ambiances-design.md` (replace A.3 with the report shape from "Decisions made while planning", and note that the climate setpoint is raw JSON).

- [ ] **Step 1: Write the pages** using only generic examples (the A.2 example's placeholder ids).
- [ ] **Step 2: Check that the docs site builds**

Run: `cd website && npm run build`, if the toolchain is installed; otherwise check the front matter by eye against `docs/users/groups-and-virtual-entities.md`.
Expected: the build succeeds, with no broken links.

- [ ] **Step 3: Run the full workspace checks**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS.

- [ ] **Step 4: Commit** with `docs: ambiances user and reference pages`.
