# Schedule trigger, flow switch node and virtual entities Implementation Plan

Author: Gekkotron

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let flows run on a cron schedule, enable or disable flows, and drive virtual switch/number/button entities, so "force heating until the tank reaches X" is assembled in the house with no water-heater code.

**Architecture:** Two new node kinds (`trigger.schedule`, `flow.enable`) and an `if` option (`missing`) in `oikos-core::flow`, validated purely there; the `oikos-os` engine arms schedule timers in the house's time zone and executes the new nodes; commands honour the discovery `retain` option; virtual entities are retained discovery configs the hub publishes under a reserved `virtual` sub-tree; the web canvas edits schedules through a picker that maps to and from cron.

**Tech Stack:** Rust workspace (`oikos-core` pure + wasm32, `oikos-os` tokio/axum/rmcp/rumqttc, `oikos-api` shared views, `oikos-web` Leptos 0.8 CSR). New crates: `croner` (cron parsing and next occurrence), `chrono` (no default features + `alloc`, `std` in `oikos-os`), `chrono-tz`, `iana-time-zone` (both `oikos-os` only). Add with `cargo add` at their current versions.

**Spec:** `docs/superpowers/specs/2026-10-20-flow-schedule-virtual-design.md`

## Global Constraints

- Attribution Gekkotron only; every commit `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- No per-device code in the hub or the web UI; no water-heater ids outside Task 9.
- `oikos-core` stays pure and builds for `wasm32-unknown-unknown`; OS time, time zones and timers live in `oikos-os`.
- Flow JSON stays lenient: unknown keys survive in `extra` (every new params struct has `#[serde(flatten)] extra: BTreeMap<String, Value>`).
- Validation keeps rejecting cycles.
- Engine safety: triggers fire only on known-to-known state changes; retained replays never start runs; schedules never fire on boot, reconnect or enable, only at the next wall-clock tick; no catch-up of missed ticks.
- Schedules are 5-field cron in the house's local time, DST-correct (Europe/Paris).
- Command publishes stay `retain: false` unless the entity's discovery config has `"retain": true`.
- Every task ends green on the README "Development" commands:
  `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo check -p oikos-core --target wasm32-unknown-unknown`, `cargo check -p oikos-api --target wasm32-unknown-unknown`, `cargo check -p oikos-web --target wasm32-unknown-unknown`.

## Review Focus

1. Hub down, laptop asleep or broker offline across several ticks: on return exactly zero catch-up runs fire; the next run is the next future tick. (Task 4: `no_catch_up_after_a_long_gap`.)
2. A flow that disables itself mid-run: the rest of that run (power 0 %, switch off) still executes; the run ends `done`, not `cancelled`. (Task 5: `self_disable_keeps_the_running_run`.)
3. DST change in Europe/Paris: `0 2 * * *` on the spring-forward day does not fire twice or panic; `*/10 * * * *` keeps 10-minute spacing across the autumn fallback. (Task 4: `next_ticks_across_dst`.)
4. Tank sensor unavailable during a boost: the `if` with `missing: false` takes `false`, so power goes to 0 %. (Task 5: `if_missing_false_takes_false`.)
5. A virtual switch recreated with `set_virtual` while ON keeps ON (no reset to OFF). (Task 6: `recreating_keeps_the_state`.)

---

### Task 1: `trigger.schedule` node and cron validation (oikos-core)

**Files:**
- Create: `crates/oikos-core/src/flow/schedule.rs`
- Modify: `crates/oikos-core/src/flow/model.rs` (new variant, params, `type_name`, `triggers()`), `crates/oikos-core/src/flow/validate.rs` (cron check, `if` reachability rule), `crates/oikos-core/src/flow/mod.rs` (exports), `crates/oikos-core/Cargo.toml`, workspace `Cargo.toml`
- Test: unit tests in `schedule.rs`, `model.rs`, `validate.rs`

**Interfaces:**
- Produces:
  - `NodeKind::TriggerSchedule(TriggerScheduleParams)`, wire tag `"trigger.schedule"`, `type_name() == "trigger.schedule"`, no named ports.
  - `pub struct TriggerScheduleParams { pub cron: String, #[serde(flatten)] pub extra: BTreeMap<String, Value> }`
  - `pub struct Schedule` (wraps `croner::Cron`), `pub fn Schedule::parse(expr: &str) -> Result<Schedule, ScheduleError>` accepting exactly 5 fields (seconds-field and year-field forms are errors), `pub fn next_after<Tz: chrono::TimeZone>(&self, after: &chrono::DateTime<Tz>) -> Option<chrono::DateTime<Tz>>` (strictly after `after`).
  - `pub struct ScheduleError(pub String)` with `Display` (the message the editor shows).
  - `Flow::triggers()` returns both trigger kinds; new `NodeKind::is_trigger(&self) -> bool`.
  - `FlowError::BadCron { node: String, reason: String }` ("node `{node}`: {reason}") and `FlowError::IfNeedsEntity { node: String }` ("node `{node}`: an if after a schedule must name an entity").
  - Exports from `oikos_core::flow`: `TriggerScheduleParams, Schedule, ScheduleError`.

- [ ] **Step 1: Write the failing tests**

```rust
// schedule.rs
#[test] fn every_ten_minutes_next_after() {
    let s = Schedule::parse("*/10 * * * *").unwrap();
    let t = chrono::Utc.with_ymd_and_hms(2026, 10, 20, 18, 3, 0).unwrap();
    assert_eq!(s.next_after(&t).unwrap(), chrono::Utc.with_ymd_and_hms(2026, 10, 20, 18, 10, 0).unwrap());
    let on_tick = chrono::Utc.with_ymd_and_hms(2026, 10, 20, 18, 10, 0).unwrap();
    assert_eq!(s.next_after(&on_tick).unwrap(), chrono::Utc.with_ymd_and_hms(2026, 10, 20, 18, 20, 0).unwrap());
}
#[test] fn rejects_non_five_field_and_garbage() {
    for bad in ["", "* * * *", "0 * * * * *", "0 0 * * * * 2026", "61 * * * *", "every ten"] {
        assert!(Schedule::parse(bad).is_err(), "{bad}");
    }
}
// model.rs
#[test] fn schedule_node_round_trips() {
    let node: Node = serde_json::from_str(r#"{"id":"tick","type":"trigger.schedule","params":{"cron":"*/10 * * * *","x":1}}"#).unwrap();
    assert!(node.kind.is_trigger());
    assert_eq!(node.kind.type_name(), "trigger.schedule");
    assert_eq!(serde_json::to_value(&node).unwrap()["params"]["x"], 1);
}
// validate.rs
#[test] fn bad_cron_is_rejected() { /* flow with schedule "61 * * * *" → Err(FlowError::BadCron { node: "tick", .. }) */ }
#[test] fn schedule_alone_is_a_trigger() { /* schedule → command: Ok(()) */ }
#[test] fn if_without_entity_after_a_schedule_is_rejected() { /* schedule → if{op eq, value true} → Err(IfNeedsEntity{node:"check"}) */ }
#[test] fn if_without_entity_reached_only_from_a_state_trigger_is_fine() { /* schedule → command; state → if(no entity): Ok */ }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p oikos-core flow::`
Expected: compile errors (`TriggerScheduleParams`, `Schedule` not found).

- [ ] **Step 3: Implement**

`Schedule::parse` counts whitespace-separated fields (must be 5) before handing the expression to croner; croner errors become `ScheduleError(err.to_string())`. `validate` calls `Schedule::parse` for each schedule node and, for `IfNeedsEntity`, walks the nodes reachable from each schedule trigger (reuse the successors walk in `graph.rs`). Add `chrono` to `oikos-core` with `default-features = false, features = ["alloc"]` (plus whatever croner needs); confirm `cargo check -p oikos-core --target wasm32-unknown-unknown`.

- [ ] **Step 4: Run tests and the wasm check**

Run: `cargo test -p oikos-core flow:: && cargo check -p oikos-core --target wasm32-unknown-unknown`
Expected: all PASS, check clean.

- [ ] **Step 5: Commit** — `feat(core): trigger.schedule node with cron validation`

---

### Task 2: `flow.enable` node and `if.missing` (oikos-core)

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs`, `crates/oikos-core/src/flow/validate.rs`, `crates/oikos-core/src/flow/mod.rs`
- Test: unit tests in the same files

**Interfaces:**
- Produces:
  - `NodeKind::FlowEnable(FlowEnableParams)`, wire tag `"flow.enable"`, single unnamed output.
  - `pub struct FlowEnableParams { #[serde(default, skip_serializing_if = "Option::is_none")] pub flow: Option<String>, pub enabled: bool, extra }` — `flow: None` means the node's own flow.
  - `pub enum Missing { Fail, True, False }`, serde `snake_case`; `IfParams.missing: Option<Missing>` (`None` behaves as `Fail`, omitted when `None`).
  - `FlowError::BadFlowTarget { node: String, flow: String }` ("node `{node}`: `{flow}` is not a valid flow id") when `flow` is `Some` and fails `is_valid_flow_id`.
  - Export `FlowEnableParams, Missing`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn flow_enable_defaults_to_self() {
    let node: Node = serde_json::from_str(r#"{"id":"stop","type":"flow.enable","params":{"enabled":false}}"#).unwrap();
    let NodeKind::FlowEnable(p) = node.kind else { panic!() };
    assert_eq!((p.flow, p.enabled), (None, false));
}
#[test] fn if_missing_round_trips_and_defaults() {
    let p: IfParams = serde_json::from_str(r#"{"op":"lt","value":65,"entity":"t","missing":"false"}"#).unwrap();
    assert_eq!(p.missing, Some(Missing::False));
    let p: IfParams = serde_json::from_str(r#"{"op":"lt","value":65}"#).unwrap();
    assert_eq!(p.missing, None);
    assert!(serde_json::to_value(&p).unwrap().get("missing").is_none());
}
#[test] fn bad_flow_target_is_rejected() { /* flow.enable {flow:"Bad Id"} → Err(BadFlowTarget{node:"stop", flow:"Bad Id"}) */ }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p oikos-core flow::` → compile errors.
- [ ] **Step 3: Implement** the types and the validation branch; `ports()` unchanged for the new kind.
- [ ] **Step 4: Run** `cargo test -p oikos-core flow:: && cargo check -p oikos-core --target wasm32-unknown-unknown` → PASS.
- [ ] **Step 5: Commit** — `feat(core): flow.enable node and if missing-value option`

---

### Task 3: Commands honour the discovery `retain` option

**Files:**
- Modify: `crates/oikos-core/src/command/mod.rs` (`Mapper::publish`, module doc "never retained")
- Test: `crates/oikos-core/tests/command.rs`

**Interfaces:**
- Produces: every `Publish` from `map_command` has `retain == entity config "retain"` when it is a JSON boolean, else `false`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn retain_option_retains_the_command() {
    let e = common::entity("switch", json!({"unique_id":"v","command_topic":"oikos/virtual/v","state_topic":"oikos/virtual/v","retain":true}));
    assert!(ok(&e, &Command::TurnOn).iter().all(|p| p.retain));
}
#[test] fn commands_are_not_retained_by_default() {
    let e = common::entity("switch", json!({"unique_id":"s","command_topic":"a/set","state_topic":"a"}));
    assert!(ok(&e, &Command::TurnOn).iter().all(|p| !p.retain));
}
```
(Use the `Command` variant the switch mapper accepts; check `command/kinds.rs`.)

- [ ] **Step 2: Run** `cargo test -p oikos-core --test command retain` → FAIL (`retain` is false).
- [ ] **Step 3: Implement** in `Mapper::publish`: `retain: self.entity.bindings.config.get("retain").and_then(Value::as_bool).unwrap_or(false)`; update the module doc.
- [ ] **Step 4: Audit**: with the hub running, `curl -s http://127.0.0.1:8081/api/v1/snapshot | jq '[.entities[] | select(.bindings.config.retain == true) | .id]'` (or the MCP `get_entity` bindings if the snapshot omits them) and record the list (expected `[]`) in the commit message body.
- [ ] **Step 5: Run** `cargo test -p oikos-core` → PASS.
- [ ] **Step 6: Commit** — `feat(core): commands honour the discovery retain option`

---

### Task 4: Engine arms schedules; time zone; schedule preview

**Files:**
- Create: `crates/oikos-os/src/flows/schedule.rs` (timer bookkeeping and `house_zone()`)
- Modify: `crates/oikos-os/src/flows/engine.rs` (arm on reload, new `select!` branch, fire), `crates/oikos-os/src/flows/mod.rs`, `crates/oikos-os/src/config.rs` (new `[flows] timezone`), `oikos.example.toml`, `crates/oikos-os/src/http/api.rs` (preview route), `crates/oikos-api/src/flows.rs` (preview view), `crates/oikos-os/src/mcp/mod.rs` (instructions text), `crates/oikos-os/Cargo.toml`
- Test: `crates/oikos-os/tests/flows.rs`, `crates/oikos-os/tests/http.rs`, `crates/oikos-os/tests/config.rs`, unit tests in `flows/schedule.rs`

**Interfaces:**
- Consumes: `Schedule::parse`, `Schedule::next_after` (Task 1).
- Produces:
  - Config: `FlowsConfig { timezone: Option<String> }` under `[flows]` (env `OIKOS_FLOWS__TIMEZONE`). Absent → `iana_time_zone::get_timezone()`; unparseable → UTC with a `tracing::warn!`. `pub fn house_zone(cfg: &FlowsConfig) -> chrono_tz::Tz`.
  - Engine state: `schedules: BTreeMap<String /*flow id*/, Armed { node: String, at: chrono::DateTime<Tz>, schedule: Schedule }>` (one entry per schedule node; key `"{flow}/{node}"` if a flow has several).
  - Arming rule: on `reload`, each schedule node of an enabled flow that is not already armed with an identical definition gets `at = next_after(now)`; disabled/removed/edited flows are disarmed. After a tick fires, re-arm with `pub(crate) fn rearm(schedule: &Schedule, previous: DateTime<Tz>, now: DateTime<Tz>) -> Option<DateTime<Tz>>` in `flows/schedule.rs`, which returns `next_after(max(previous, now))` (this is what forbids catch-up).
  - Tick: link `Online` → `fire(flow, node, trigger: None)` through the same path as state triggers (so `mode` applies); link not online → `hub.add_skipped_run(flow, node, None)` and re-arm.
  - Run detail of the trigger node for a schedule run: `"schedule {cron}"` (replace the `"manual"` fallback in `run_node` for `TriggerSchedule`).
  - `GET /api/v1/schedule?cron=<expr>&count=<n>` (n default 3, max 10) → `200 SchedulePreview { timezone: String, next: Vec<String> }` (RFC 3339 with offset) or `422 {"error": <ScheduleError>}`. `pub struct SchedulePreview` in `oikos-api::flows`, `JsonSchema` derived.
  - MCP server instructions mention `trigger.schedule` (5-field cron, local time, fires only while the flow is enabled, first run at the next tick).

- [ ] **Step 1: Write the failing tests** (`#[tokio::test(start_paused = true)]` like `motion_false_turns_off_after_the_delay`)

```rust
async fn schedule_fires_each_tick_while_enabled()   // set "*/10 * * * *" schedule → command flow; advance(10m+1s): 1 publish; advance(10m): 2
async fn schedule_never_fires_on_enable()            // right after set_flow: 0 publishes; advance(1s): 0
async fn disabled_schedule_never_fires()             // set_flow_enabled(false); advance(30m): 0
async fn tick_while_offline_is_skipped()             // link offline at the tick: a skipped run recorded, 0 publishes; back online: next tick publishes
fn no_catch_up_after_a_long_gap()                    // rearm("*/10 * * * *", previous = 18:00, now = 21:03) == 21:10 (unit test in flows/schedule.rs)
fn next_ticks_across_dst()                           // Europe/Paris, via rearm(): "0 2 * * *" walked from 2027-03-27T12:00 local never returns the same instant twice and never panics on 2027-03-28 (02:00 does not exist that day); "*/10 * * * *" walked from 2026-10-25T00:00Z to 02:00Z gives ticks exactly 600 s apart in UTC (no duplicate at the 03:00→02:00 fallback)
async fn schedule_preview_lists_three_ticks()        // http: GET /api/v1/schedule?cron=*/10%20*%20*%20*%20* → 200, next.len()==3
async fn schedule_preview_rejects_bad_cron()         // GET …cron=61%20*%20*%20*%20* → 422
fn flows_timezone_is_read()                          // config: [flows] timezone = "Europe/Paris" → Some("Europe/Paris")
```
For `next_ticks_across_dst`, first print croner's sequence once, check it against the rule "wall-clock times that exist, each at most once", then pin those exact values.

- [ ] **Step 2: Run** `cargo test -p oikos-os --test flows schedule && cargo test -p oikos-os --test http schedule` → FAIL.
- [ ] **Step 3: Implement**: the timer branch is one `tokio::time::sleep_until` on the earliest `Armed::at` (converted from wall-clock to a `tokio::time::Instant` delay at arm time, so paused-time tests work), added to the engine's `select!` after finished runs and before events. Wall-clock "now" comes from one helper in `flows/schedule.rs` (`chrono::Utc::now().with_timezone(&zone)`), injectable for the unit tests.
- [ ] **Step 4: Run** the README Development commands → all green.
- [ ] **Step 5: Commit** — `feat(flows): schedule trigger fires on cron ticks in the house time zone`

---

### Task 5: Engine executes `flow.enable` and `if.missing`; house checks

**Files:**
- Modify: `crates/oikos-os/src/flows/engine.rs` (`run_node`, `reload` stale filter), `crates/oikos-os/src/flows/hub.rs` (`check_house`, new `set_flow_enabled_by_run`), `crates/oikos-os/src/flows/mod.rs` (`EngineRequest::KeepRun`), `crates/oikos-os/src/mcp/mod.rs` (instructions)
- Test: `crates/oikos-os/tests/flows.rs`

**Interfaces:**
- Consumes: `FlowEnableParams`, `Missing` (Task 2).
- Produces:
  - `EngineRequest::KeepRun { flow_id: String, run_id: String }`.
  - `pub(crate) async fn Hub::set_flow_enabled_by_run(&self, id: &str, enabled: bool, run_id: &str) -> Result<FlowView, HubError>`: when `id` is the run's own flow and `enabled == false`, sends `KeepRun` before the write's `Reload`; otherwise behaves like `set_flow_enabled`.
  - `reload`: an active run listed in `KeepRun` survives a reload whose only difference for its flow is `enabled = false` (compare the definitions with `enabled` normalised); it is forgotten when the run ends. Every other disable still cancels (existing `disabling_cancels_the_run` stays green).
  - `run_node` `FlowEnable`: target = `params.flow` or the run's flow id; detail `"enabled {target}"` / `"disabled {target}"`; an unknown target fails the step (`"unknown flow `{id}`"`).
  - `run_node` `If`: when the left value is missing (`None`) and `missing` is `True`/`False`, take that port with detail `"missing → true"` / `"missing → false"`; `Fail`/`None` keeps today's failure.
  - `check_house`: `FlowEnable` with `Some(flow)` must name an existing flow (`flow_store().exists`) or equal the flow being saved; error `"unknown flow `{flow}` in node `{node}`"`.
  - MCP instructions mention `flow.enable` (defaults to its own flow) and `if.missing`.

- [ ] **Step 1: Write the failing tests**

```rust
async fn flow_enable_enables_another_flow()          // flow A run: flow.enable{flow:"b",enabled:true} → hub.flow("b").enabled == true
async fn self_disable_keeps_the_running_run()        // flow: trigger → flow.enable{enabled:false} → command; run ends Done, the command published, flow disabled
async fn if_missing_false_takes_false()              // entity with no value; if{missing:false} → false branch command published
async fn if_missing_default_still_fails()            // same without missing → run Failed, no publish
async fn set_flow_rejects_unknown_target_flow()      // set_flow with flow.enable{flow:"nope"} → HubError::InvalidFlow containing "unknown flow `nope`"
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test flows` → the five FAIL, the rest PASS.
- [ ] **Step 3: Implement** as specified in Interfaces.
- [ ] **Step 4: Run** the README Development commands → green.
- [ ] **Step 5: Commit** — `feat(flows): flow.enable node keeps its own run; if takes a branch on a missing value`

---

### Task 6: Virtual entities (hub, REST, MCP, docs)

**Files:**
- Create: `crates/oikos-os/src/virtual_entities.rs` (config building, pure, unit-tested), `crates/oikos-api/src/virtual_entities.rs` (`VirtualSpec`, `VirtualKind`)
- Modify: `crates/oikos-core/src/registry/mod.rs` (`default_reserved` adds `"virtual"`), `crates/oikos-os/src/hub.rs` (`set_virtual`, `delete_virtual`), `crates/oikos-os/src/http/api.rs`, `crates/oikos-os/src/mcp/tools.rs` + `params.rs` + `mod.rs` (instructions), `docs/adding-a-device.md` (new "Virtual entities" section), `README.md` (routes table)
- Test: unit tests in `virtual_entities.rs`, `crates/oikos-os/tests/hub.rs`, `crates/oikos-os/tests/http.rs`, `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: retained commands (Task 3).
- Produces:
  - `pub enum VirtualKind { Switch, Number, Button }` (serde snake_case); `pub struct VirtualSpec { pub kind: VirtualKind, pub name: String, pub min: Option<f64>, pub max: Option<f64>, pub step: Option<f64>, pub unit: Option<String>, pub initial: Option<serde_json::Value> }`.
  - Ids: `[a-z0-9_]{1,48}`; unique id `virtual_<id>`; device `{"identifiers":["oikos_virtual"],"name":"Virtual"}`.
  - Topics (first discovery prefix `P`, normally `oikos`): config `P/<component>/virtual/<id>/config`; state = command = `P/virtual/<id>`.
  - Switch config: `command_topic` = `state_topic` = `P/virtual/<id>`, `payload_on "ON"`, `payload_off "OFF"`, `retain: true`. Number: same topics, `min`/`max`/`step` (defaults 0/100/1), `unit_of_measurement`, `retain: true`. Button: `command_topic` only, `payload_press "PRESS"`, no retain.
  - `pub async fn Hub::set_virtual(&self, id: &str, spec: VirtualSpec) -> Result<EntityView, HubError>`: publishes the config retained; publishes the initial state retained (`"OFF"`, or `initial`, or the number's `min`) **only when the entity did not exist before**; validates `min <= initial <= max`.
  - `pub async fn Hub::delete_virtual(&self, id: &str) -> Result<(), HubError>`: empty retained payload on the config topic and the state topic; `UnknownEntity` when absent.
  - REST: `PUT /api/v1/virtual/{id}` (body `VirtualSpec`) → `200 EntityView`, `422` bad id/spec, `503` offline; `DELETE /api/v1/virtual/{id}` → `204`/`404`.
  - MCP tools `set_virtual { id, spec }` and `delete_virtual { id }`.

- [ ] **Step 1: Write the failing tests**

```rust
fn switch_config_echoes_on_its_own_topic()            // config JSON: command_topic == state_topic == "oikos/virtual/force_heating", retain true, unique_id "virtual_force_heating"
fn bad_ids_are_refused()                              // "Force", "", 49 chars, "a/b" → Err
async fn creating_publishes_config_and_initial_off()  // hub: publishes retained config + retained "OFF" on oikos/virtual/force_heating
async fn recreating_keeps_the_state()                 // entity exists with ON → set_virtual again: config republished, no state publish
async fn virtual_switch_command_is_accepted_and_retained() // send_command turn_on → publish retain true; echo → accepted: true, state on
async fn virtual_topics_are_not_ignored_discovery()   // oikos/virtual/x is not in GET /api/v1/ignored
async fn delete_virtual_clears_both_topics()
async fn http_put_virtual_round_trips()               // tests/http.rs
async fn mcp_set_virtual_lists_the_entity()           // tests/mcp.rs
```

- [ ] **Step 2: Run** `cargo test -p oikos-os virtual` → FAIL.
- [ ] **Step 3: Implement**; docs section explains the recipe, the topics, the initial state rule and that a virtual entity is moved to a room with the usual override.
- [ ] **Step 4: Run** the README Development commands → green.
- [ ] **Step 5: Commit** — `feat(hub): virtual switch, number and button entities`

---

### Task 7: Web — schedule picker, flow.enable and if.missing editors

**Files:**
- Create: `crates/oikos-web/src/schedule.rs` (pure picker ↔ cron ↔ text), `crates/oikos-web/src/screens/flow_schedule.rs` (`ScheduleField` component)
- Modify: `crates/oikos-web/src/flows.rs` (`node_title`, `node_detail`, `summary_label`, `orphans`), `crates/oikos-web/src/screens/flow_canvas.rs` (`type_class`, `is_trigger` checks, `node_editor_body`, `kind_fields`), `crates/oikos-web/src/api.rs` (`schedule_preview`), `crates/oikos-web/src/icons.rs` (clock and flow glyphs), `crates/oikos-web/styles/main.css` (`.flow-node--schedule`, `.flow-node--flow`, picker styles), `crates/oikos-web/src/lib.rs`
- Test: unit tests in `schedule.rs`, `flows.rs`

**Interfaces:**
- Consumes: `TriggerScheduleParams`, `FlowEnableParams`, `Missing` (Tasks 1–2), `GET /api/v1/schedule` (Task 4).
- Produces:
  - `pub enum SchedulePick { EveryMinutes(u8), Hourly { minute: u8 }, Daily { hour: u8, minute: u8 }, Weekdays { days: Vec<u8>, hour: u8, minute: u8 }, Advanced(String) }` (`days`: 0 = Sunday … 6 = Saturday, sorted).
  - `pub fn to_cron(pick: &SchedulePick) -> String`, `pub fn from_cron(cron: &str) -> SchedulePick`, `pub fn describe(cron: &str) -> String`.
  - `pub async fn api::schedule_preview(cron: &str) -> Result<SchedulePreview, ApiError>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn picks_map_to_cron_and_back() {
    for (pick, cron) in [
        (SchedulePick::EveryMinutes(10), "*/10 * * * *"),
        (SchedulePick::EveryMinutes(1), "* * * * *"),
        (SchedulePick::Hourly { minute: 15 }, "15 * * * *"),
        (SchedulePick::Daily { hour: 7, minute: 30 }, "30 7 * * *"),
        (SchedulePick::Weekdays { days: vec![1, 2, 3, 4, 5], hour: 7, minute: 30 }, "30 7 * * 1-5"),
        (SchedulePick::Weekdays { days: vec![0, 6], hour: 9, minute: 0 }, "0 9 * * 0,6"),
    ] {
        assert_eq!(to_cron(&pick), cron);
        assert_eq!(from_cron(cron), pick);
    }
    assert_eq!(from_cron("30 7 * * 1,2,3,4,5"), SchedulePick::Weekdays { days: vec![1, 2, 3, 4, 5], hour: 7, minute: 30 });
    assert_eq!(from_cron("0 */2 1 * *"), SchedulePick::Advanced("0 */2 1 * *".into()));
}
#[test] fn descriptions_read_as_english() {
    assert_eq!(describe("*/10 * * * *"), "Every 10 minutes");
    assert_eq!(describe("* * * * *"), "Every minute");
    assert_eq!(describe("15 * * * *"), "Every hour at :15");
    assert_eq!(describe("30 7 * * *"), "Every day at 07:30");
    assert_eq!(describe("30 7 * * 1-5"), "Weekdays at 07:30");
    assert_eq!(describe("0 9 * * 0,6"), "Weekends at 09:00");
    assert_eq!(describe("0 9 * * 1,3"), "Mon, Wed at 09:00");
    assert_eq!(describe("0 */2 1 * *"), "Custom: 0 */2 1 * *");
}
#[test] fn new_nodes_have_titles_and_details() {
    // flows.rs: node_title → "Schedule" / "Flow switch"; node_detail(schedule) == "Every 10 minutes";
    // node_detail(flow.enable{flow:None,enabled:false}) == "Disable this flow";
    // node_detail(flow.enable{flow:Some("force-heating-loop"),enabled:true}) == "Enable force-heating-loop"
}
#[test] fn schedule_triggers_are_not_orphans() { /* flows.rs orphans() ignores schedule nodes */ }
```

- [ ] **Step 2: Run** `cargo test -p oikos-web schedule flows` → FAIL.
- [ ] **Step 3: Implement**: `ScheduleField` shows the five choices as chips; each choice shows its inputs (minutes 1–59, `:MM`, `HH:MM`, weekday chips Mon…Sun, raw text under Advanced); the summary (`describe`) and the next three ticks from `schedule_preview` sit under the inputs; an Advanced value the hub rejects shows its message inline and blocks Save. `flow.enable` editor: target select ("This flow" + the other flows by name) and an Enable / Disable choice. `if` editor gains "When the value is unknown": Fail / Take Yes / Take No (`missing` `None`/`True`/`False`). Match the existing node and sheet styles in both themes; check at 390 px.
- [ ] **Step 4: Run** `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown && (cd crates/oikos-web && trunk build --release)` → green; take headless screenshots of a schedule node sheet (dark, light, 390 px) using the CDP approach in memory `reference_headless_chrome_cdp_screenshots`.
- [ ] **Step 5: Commit** — `feat(web): schedule picker and flow switch editor on the flow canvas`

---

### Task 8: Virtual button presses trigger flows

**Files:**
- Modify: `crates/oikos-core/src/registry/*` or `crates/oikos-os/src/mqtt.rs` (deliver messages on a watched button's `command_topic`, with the retain flag), `crates/oikos-os/src/hub.rs` (emit `WsEvent::ButtonPressed { id }`), `crates/oikos-api` (`WsEvent` variant), `crates/oikos-os/src/flows/engine.rs` (fire), `crates/oikos-core/src/flow/validate.rs` or `check_house` (reject `to`/`from` on a button trigger)
- Test: `crates/oikos-os/tests/flows.rs`

**Interfaces:**
- Produces: `WsEvent::ButtonPressed { id: String }`; the hub subscribes to the `command_topic` of every button entity a `trigger.state` watches; each **non-retained** message there emits `ButtonPressed`; the engine fires that button's `trigger.state` nodes on it (mode applies), bypassing the known map; `check_house` refuses `to`/`from` on a button trigger (`"node `{node}`: a button trigger takes no to/from"`).

- [ ] **Step 1: Write the failing tests**

```rust
async fn button_press_fires_its_flow()               // virtual button; flow trigger.state(button) → command; press via send_command → command published
async fn retained_button_message_never_fires()       // retained message on the button topic → no run
async fn button_trigger_refuses_to_filter()          // set_flow with to:"PRESS" on a button → InvalidFlow
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test flows button` → FAIL.
- [ ] **Step 3: Implement**; the web Home button card needs no change (it already sends `press`).
- [ ] **Step 4: Run** the README Development commands → green.
- [ ] **Step 5: Commit** — `feat(flows): button presses trigger flows`

---

### Task 9: House setup — force heating (live)

Done by the coordinating session after Tasks 1–8 are merged, the web built (`cd crates/oikos-web && trunk build --release`), the hub rebuilt (`cargo build --release -p oikos-os`) and restarted on 8081. No code; MCP only.

- [ ] **Step 1:** Ask the user for X (°C) and the room for the switch.
- [ ] **Step 2:** `set_virtual force_heating { kind: switch, name: "Force heating" }`; `set_override virtual_force_heating { area: <room>, favorite: true }`.
- [ ] **Step 3:** `set_flow force-heating-loop` (enabled `false`): `trigger.schedule "*/10 * * * *"` → `if { entity: water_heater_tank_temp, op: lt, value: X, missing: false }` → `true`: `command set_number 100` ×3 (`water_heater_power_0/1/2`); `false`: `set_number 0` ×3 → `command turn_off virtual_force_heating` → `flow.enable { enabled: false }`.
- [ ] **Step 4:** `set_flow force-heating-switch` (enabled, `mode: single`): `trigger.state virtual_force_heating to true` → `if tank lt X (missing false)` → `true`: `flow.enable { flow: force-heating-loop, enabled: true }` → `set_number 100` ×3; `false`: `turn_off virtual_force_heating`. Second trigger `to false` → `flow.enable { flow: force-heating-loop, enabled: false }` → `set_number 0` ×3. (Use the scalar the switch reports, `true`/`false`, for `to`; check with `get_entity`.)
- [ ] **Step 5: Verify live:** turn the switch on from the web Home; within seconds the three powers read 100 (`list_entities device_id=water_heater_controller`), the loop flow is enabled, the water heater tile shows "Heating · 100 %"; after the next tick the loop's run shows `true`; temporarily set X below the tank temperature with `set_flow` and confirm the next tick sends 0 %, turns the switch off and disables the loop; restore X.
- [ ] **Step 6:** Report the run ids and screenshots to the user.
