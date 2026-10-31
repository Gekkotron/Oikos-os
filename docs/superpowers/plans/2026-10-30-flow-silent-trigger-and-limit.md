# Silent trigger, Limit card and first-value triggers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flows get `trigger.silent` (no report from an entity for a duration) and a `limit` node (at most N passes per rolling window), and `trigger.state` with an explicit `to` fires when a value appears after a confirmed unknown.

**Architecture:** Two new `NodeKind` variants in `oikos-core` with structural validation. The hub's flow engine keeps one silent clock per (flow, node), fed by the `StateSeen` stream it already receives, and joins them into its single `next_deadline`. Limit counts live in the `Hub` (runs execute in spawned tasks that only hold `Arc<Hub>`), keyed by flow and node and pruned on reload. The first-value fix is a per-key "confirmed unknown" mark in the engine, set by `StateSeen`, read by `state_changed`.

**Tech Stack:** Rust workspace: `oikos-core` (model, validation), `oikos-os` (tokio flow engine, hub, MCP), `oikos-web` (Leptos/wasm editor).

**Spec:** `docs/superpowers/specs/2026-10-30-flow-silent-trigger-and-limit-design.md`

## Global Constraints

- Generic: no vendor, model, room or house name in code, strings, tests or docs. Fixtures use neutral ids (`binary_sensor.smoke`, `sensor.battery`).
- Wire names: node types `trigger.silent` and `limit`; params `entity` + `for` (silent), `max` + `per` (limit).
- Bounds: `for` and `per` parse with `oikos_core::flow::parse_duration`, `1m` ≤ d ≤ `168h`; `max` 1..=100.
- Saved flows without the new nodes load, validate, save and run exactly as today.
- Never fire `trigger.silent` at boot, on a flow save, on enable, or while the broker link is down; clocks restart when the link comes back and when the `StateSeen` receiver lags.
- `trigger.state` stays silent on first sighting, restore (no `StateSeen`), seed at save, value vanishing, and on any trigger without `to` or with `from`.
- en + fr strings for every new UI text.
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`), message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Every task ends with `cargo check --workspace --all-targets` green. Before building, check `du -sh ~/dev-scratch/cargo-target` (> 5 GB → `cargo clean`) and `df -h ~` (< 5 GB free → per-crate tests only).

## Review Focus

1. **Hub restart with six silent detectors**: nothing fires before `for` has elapsed since boot, even though the entities' last reports are hours old (Task 3: `silent_counts_from_arming_not_last_update`).
2. **Broker outage longer than `for`**: no fire during the outage, no burst at reconnect; each clock restarts at link-up (Task 3: `silent_frozen_while_link_down_and_restarts_on_link_up`).
3. **Editing a flow's name or another node in the web**: the silent clock and the Limit count carry over (Task 3: `silent_clock_survives_unrelated_edit`; Task 4: `limit_count_survives_flow_save`).
4. **Boot restore of the `to` value**: a recorded `true` restored at boot onto an entity watched with `to: true` must not fire (Task 2: `restore_of_the_to_value_does_not_fire`).
5. **Enabling a flow whose entity has been silent for hours**: it waits `for` from the enable, it does not fire at once (Task 3: `silent_starts_at_enable`).

---

### Task 1: Model — `trigger.silent` and `limit`

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (variants, params, `type_name`, `is_trigger`, `ports` unchanged = one unnamed output), `crates/oikos-core/src/flow/validate.rs`, `crates/oikos-core/src/flow/mod.rs` (re-exports)
- Modify (exhaustive matches only, behaviour comes later): `crates/oikos-os/src/flows/engine.rs` (`run_node`: silent → `(Ok, format!("{entity} silent for {for}"), [])`; limit → `(Ok, "passed", [])` placeholder replaced in Task 4), `crates/oikos-os/src/flows/hub.rs` `check_house` (`TriggerSilent` names `params.entity`; `Limit` → `None`), and the `oikos-web` files the compiler lists (map to the Delay/State look until Task 5).
- Test: in-file tests of `model.rs` and `validate.rs`

**Interfaces:**
- Produces in `oikos_core::flow`:
  - `NodeKind::TriggerSilent(TriggerSilentParams)` with `#[serde(rename = "trigger.silent")]`; `NodeKind::Limit(LimitParams)` with `#[serde(rename = "limit")]`.
  - `pub struct TriggerSilentParams { pub entity: String, #[serde(rename = "for")] pub for_: String, extra }` — doc comments as the spec words them (they feed the MCP schema).
  - `pub struct LimitParams { pub max: u32, pub per: String, extra }`.
  - `pub const MIN_WINDOW: Duration = 60 s; pub const MAX_WINDOW: Duration = 168 h;` and `pub fn window(text: &str) -> Result<Duration, String>` (parse + bounds, message names the bounds) used for both `for` and `per`.
  - `pub const MAX_LIMIT: u32 = 100;`
  - `FlowError::BadDuration` reused for `for` / `per`; new `FlowError::BadLimit { node, reason }` for `max`.
  - `is_trigger()` true for `TriggerSilent`.

- [ ] **Step 1: Write the failing tests**
  - `silent_and_limit_round_trip`: `{"id":"t","type":"trigger.silent","params":{"entity":"binary_sensor.smoke","for":"50m"}}` and `{"id":"l","type":"limit","params":{"max":4,"per":"24h"}}` decode to the variants and encode back to the same JSON.
  - `silent_is_a_trigger`: a flow whose only trigger is `trigger.silent` validates.
  - `window_bounds`: `for` `"59s"` and `"169h"` → `BadDuration`; `"1m"` and `"168h"` → Ok; same for `per`.
  - `limit_max_bounds`: `max` 0 and 101 → `BadLimit`; 1 and 100 → Ok.
  - `limit_edge_has_no_port`: an edge from a `limit` with `"port": "x"` is refused like for `delay`.
- [ ] **Step 2:** `cargo test -p oikos-core flow::` → FAIL (unknown variants).
- [ ] **Step 3:** Implement the interfaces above; validation arms next to `NodeKind::Delay`.
- [ ] **Step 4:** `cargo test -p oikos-core flow::` → PASS; `cargo check --workspace --all-targets` → green.
- [ ] **Step 5: Hub check** test in `flows/hub.rs` tests: `silent_names_an_existing_entity` — `set_flow` with `trigger.silent` on an unknown id → `InvalidFlow("unknown entity `…` in node `t`")`. Run `cargo test -p oikos-os flows::hub` → PASS.
- [ ] **Step 6: Commit** `feat(flows): trigger.silent and limit in the model`.

### Task 2: Engine — `trigger.state` fires on a confirmed unknown → value

**Files:**
- Modify: `crates/oikos-os/src/flows/engine.rs` (`Engine` field, `state_seen`, `state_changed`, `reload`, link-drop and `events_lagged` clears, test `engine_at` constructor)
- Test: `engine.rs` `mod tests`

**Interfaces:**
- Produces: `Engine.confirmed_unknown: BTreeSet<Watched>`.

- [ ] **Step 1: Write the failing tests** (use an entity with a field that can be absent, like the vacuum helpers, or a binary sensor whose payload omits the key; flow `trigger.state` `{"entity": id, "to": true}`):
  - `unknown_then_value_fires_after_a_confirming_report`: reload, report without the key (value unknown), report with `true` → 1 run.
  - `first_report_with_the_value_is_silent`: reload with no report, first report `true` → 0 runs.
  - `restore_of_the_to_value_does_not_fire`: reload, `hub.restore_recorded_states(vec![(id, 0, OnOff(true))])` and feed the resulting `StateChanged` → 0 runs.
  - `from_or_no_to_stays_silent`: same sequence as the first test with `{"to": true, "from": false}` and with `{}` → 0 runs each.
  - `vanish_and_return_fires_again`: confirmed unknown → `true` (1 run, finished) → report without the key → report `true` → 2 runs.
  - `link_drop_clears_the_mark`: confirmed unknown, link Offline → Online, report `true` → 0 runs.
- [ ] **Step 2:** `cargo test -p oikos-os flows::engine` → the new tests FAIL.
- [ ] **Step 3:** Implement. In `state_seen` (current epoch only): after seeding, for each watched key of `id` whose `known` value is `Some(None)`, insert it into `confirmed_unknown`. In `state_changed`: when `previous` is `None` and `next` is `Some(v)`, if the key is in `confirmed_unknown`, remove it and fire every `trigger.state` on that key with `to` matching `v` and `from: None` (same one-fire-per-flow rule); then `continue` as today. Remove the key from `confirmed_unknown` whenever it gets a value. Clear the set wherever `known` is cleared; `retain` it to `watched` in `reload`.
- [ ] **Step 4:** `cargo test -p oikos-os flows::engine` → PASS (old tests included).
- [ ] **Step 5: Commit** `fix(flows): a trigger with to fires when its value first appears`.

### Task 3: Engine — silent clocks

**Files:**
- Modify: `crates/oikos-os/src/flows/engine.rs` (`Engine` field, `arm_silents`, `next_deadline`, `tick_silents`, `state_seen`, `handle_event` link changes, `StateSeen` `Lagged` branch in `run_with`, `run_node` detail)
- Test: `engine.rs` `mod tests` (add a movable test clock: `Arc<Mutex<DateTime<Utc>>>` behind `now`)

**Interfaces:**
- Consumes: `TriggerSilentParams`, `oikos_core::flow::window` (Task 1).
- Produces: `Engine.silents: BTreeMap<String /* "flow/node" */, Silent>`, `struct Silent { node: String, entity: String, for_: String, period: chrono::Duration, since: DateTime<Utc>, fired: bool }`; `fn arm_silents(&mut self)` called from `reload`; `fn tick_silents(&mut self)` called next to `tick_suns`.

- [ ] **Step 1: Write the failing tests** (flow: `trigger.silent` `for: "50m"` on a sensor → `notify`-free node such as `delay 1s`, so a run is countable):
  - `silent_fires_once_after_for`: reload at t0; advance to t0+49m, tick → 0 runs; t0+50m → 1 run; t0+3h → still 1.
  - `report_restarts_the_clock`: report at t0+40m (unchanged value, i.e. a `StateSeen` only); t0+50m → 0 runs; t0+90m → 1 run.
  - `fires_again_after_a_report_and_new_silence`: after the first fire, report at t0+2h; t0+2h50m → 2 runs.
  - `silent_counts_from_arming_not_last_update`: entity's `last_update` far in the past; reload at t0; t0+1m → 0 runs.
  - `silent_starts_at_enable`: flow disabled, then enabled at t0+5h; t0+5h+49m → 0 runs; t0+5h+50m → 1 run.
  - `silent_frozen_while_link_down_and_restarts_on_link_up`: Offline at t0+10m, t0+2h tick → 0 runs; Online at t0+2h; t0+2h49m → 0; t0+2h50m → 1.
  - `silent_clock_survives_unrelated_edit`: at t0+30m re-ingest the flow with a new `name`; t0+50m → 1 run. Changing `for` to `"1h"` at t0+30m → nothing at t0+50m, 1 run at t0+1h30m.
  - `seen_lag_restarts_every_clock`: call the lag handler at t0+30m; t0+50m → 0; t0+80m → 1.
  - `skipped_fire_stays_latched`: flow `single` with an active run when the clock expires → a skipped run is recorded, no later fire until a report.
- [ ] **Step 2:** `cargo test -p oikos-os flows::engine` → new tests FAIL.
- [ ] **Step 3:** Implement. `arm_silents` mirrors `arm_schedules`: keep an armed entry whose node, entity and `for` are unchanged; arm new or changed ones with `since = now, fired = false`; drop the rest. `next_deadline` chains `since + period` of unfired entries, only while online. `tick_silents` fires due unfired entries through `fire(..)` with `TriggerInfo { entity: Some(entity), field: None, value: json!(for_), previous: Value::Null }` and sets `fired = true`. `state_seen` (current epoch) sets `since = now, fired = false` on entries whose `entity == seen.id`. Link Online and `StateSeen` `Lagged`: every entry `since = now, fired = false`. While not online nothing ticks. `run_node` detail: `"{entity} silent for {for}"`.
- [ ] **Step 4:** `cargo test -p oikos-os flows::engine` → PASS.
- [ ] **Step 5: Commit** `feat(flows): trigger.silent fires when an entity stops reporting`.

### Task 4: Limit node at run time

**Files:**
- Modify: `crates/oikos-os/src/flows/hub.rs` (counts + methods), `crates/oikos-os/src/flows/engine.rs` (`run_node` arm, `reload` prune, `execute` ends the run on a limited step)
- Test: `engine.rs` `mod tests`

**Interfaces:**
- Consumes: `LimitParams`, `oikos_core::flow::window` (Task 1).
- Produces on `Hub`: `pub fn limit_pass(&self, flow: &str, node: &str, max: u32, per: Duration, now: DateTime<Utc>) -> Result<(), usize>` (Ok records the pass; `Err(n)` = n passes already in the window, nothing recorded) and `pub fn limit_retain(&self, keep: &BTreeMap<(String, String), (u32, String)>)` (drops entries whose key is absent or whose `(max, per)` changed). Storage: `Mutex<BTreeMap<(String, String), (u32, String, VecDeque<DateTime<Utc>>)>>`, timestamps older than `now - per` popped on each call.
- `run_node` returns a new fourth signal for "stop here": implement as `ports = vec!["limited".into()]` on a non-branching node and, in `execute`, do not follow successors when `node.kind` is `Limit` and `ports == ["limited"]` (keep `shown` = `None`). Step detail: `"passed (n of max in the last per)"` or `"limited: max in the last per"`.

- [ ] **Step 1: Write the failing tests** (flow: `trigger.manual` → `limit {max: 2, per: "1h"}` → `delay 1s`; drive with `run_manual` and the movable clock from Task 3):
  - `limit_passes_max_times_then_stops`: runs 1–2 record a `delay` step; run 3 records the limit step with detail starting `limited:` and no `delay` step; run 3 status `done`.
  - `limit_window_rolls`: passes at t0 and t0+30m; at t0+61m a third run passes.
  - `limit_count_survives_flow_save`: two passes, re-ingest the flow with a new `name`, third run limited.
  - `limit_count_dropped_when_node_removed_or_changed`: two passes, re-ingest with `max: 3` → next run passes.
- [ ] **Step 2:** `cargo test -p oikos-os flows::engine` → FAIL.
- [ ] **Step 3:** Implement the interfaces above; `reload` builds the keep map from enabled *and* disabled definitions (disabling keeps the count) and calls `limit_retain`.
- [ ] **Step 4:** `cargo test -p oikos-os flows` → PASS.
- [ ] **Step 5: Commit** `feat(flows): limit lets a run through at most max times per window`.

### Task 5: Web flow editor — Silent for and Limit cards

**Files:** the same set the off-peak trigger touched in commits `d57e2267` and `54cd284c`: `crates/oikos-web/src/flow_picker.rs` (library entries + default params), `flow_text.rs` (canvas titles/summaries), `flows.rs`, `flow_list.rs`, `flow_edit.rs`, `icons.rs`, `i18n/flows.rs`, `screens/flow_canvas/settings.rs` (sheets); test `crates/oikos-web/tests/flow_canvas.rs`.

**Interfaces:**
- Consumes: Task 1 types. Defaults when added from the library: silent `{entity: "", for: "50m"}` (entity chosen in the sheet, like `trigger.state`); limit `{max: 4, per: "24h"}`.
- Library: "Silent for" / "Silencieux depuis" under triggers; "Limit" / "Limite" under logic next to Delay. Canvas title: silent `"<entity name> silent for 50m"` / `"<nom> silencieux depuis 50m"`; limit `"At most 4 every 24h"` / `"Au plus 4 fois par 24h"`.
- Sheets: silent = entity picker (as `trigger.state`) + duration field (as Delay `for`); limit = number field `max` (1–100) + duration field `per`.

- [ ] **Step 1: Write the failing test** in `tests/flow_canvas.rs`: `library_lists_silent_and_limit` (both entries present with the defaults above) and `limit_and_silent_titles` (en and fr strings above).
- [ ] **Step 2:** `cargo test -p oikos-web --test flow_canvas` → FAIL.
- [ ] **Step 3:** Implement, reusing the Delay duration field and the State entity picker components.
- [ ] **Step 4:** `cargo test -p oikos-web` → PASS; `cargo check --workspace --all-targets` → green.
- [ ] **Step 5: Commit** `feat(web): Silent for and Limit cards in the flow editor`.

### Task 6: MCP and docs

**Files:** `crates/oikos-os/src/mcp/mod.rs` (instructions + its test asserting the new names), `crates/oikos-os/src/mcp/params.rs:191` (set_flow description: two bullets and the `trigger.state` unknown → value sentence), `docs/reference/flow-nodes.md` (sections `trigger.silent`, `limit`; note under `trigger.state`), `docs/users/flows.md` (section "Alive checks": silent 50m → limit 4/24h → notify; generic entity names), `docs/reference/mcp-tools.md` if it lists node types.

- [ ] **Step 1:** Extend the MCP instructions test: `assert!(instr.contains("trigger.silent")); assert!(instr.contains("limit"));` → `cargo test -p oikos-os mcp` FAIL.
- [ ] **Step 2:** Write the texts (wording from the spec; mention rolling window, once per silence, frozen while the broker is down, counts reset on hub restart).
- [ ] **Step 3:** `cargo test -p oikos-os mcp` → PASS.
- [ ] **Step 4: Commit** `docs: silent trigger, limit and first-value triggers`.
