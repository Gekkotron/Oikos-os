# Manual Trigger Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Node-RED-style `trigger.manual` flow card with a ▶ button on the canvas that starts a run from that card, optionally carrying a fixed value as the trigger value.

**Architecture:** A new `NodeKind::TriggerManual` in `oikos-core` (model + validation). The hub's `run_flow` grows an optional starting card (`Hub::run_flow_from`); the engine starts there, carries the card's value as a `TriggerInfo` without an entity, and honours `restart` for card presses. HTTP and MCP pass the card id through; the web adds the card to the library, the settings drawer, the value picker and the canvas (with the ▶ button).

**Tech Stack:** Rust 2024 workspace: `oikos-core`, `oikos-api`, `oikos-os` (tokio, axum 0.8, rmcp), `oikos-web` (Leptos, wasm-bindgen-test).

**Spec:** `docs/superpowers/specs/2026-10-28-flow-manual-trigger-design.md`

## Global Constraints

- Start only once PR #77 (`feat/flow-switch-time-of-day`, the `switch` and `time.between` nodes) is on `main`; rebase `feat/flow-manual-trigger` onto `origin/main` first. Not stacked.
- Wire tag `trigger.manual`; params `{ "value"?: bool | number | string }` (`Scalar`), unknown params kept in `extra`.
- **Run now** without a card keeps today's refusals (offline, unknown, invalid, disabled, a run is active) in both modes.
- A card press on an active flow: `single` refuses with `"a run is active"` (no skipped run recorded); `restart` cancels the active run and starts the new one.
- Generic: no house-specific names, rooms, hosts or IPs in tracked files, tests or examples.
- Every new web string in English and French, through `crates/oikos-web/src/i18n/flows.rs`.
- Commits authored as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Before builds check `du -sh ~/dev-scratch/cargo-target`; over 5 GB run `cargo clean` first. Prefer per-crate tests (`cargo test -p <crate>`) while iterating.
- CI gate (run all at the end): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test --workspace`, `wasm-pack test --headless --chrome crates/oikos-web`.

## Review Focus

1. **Press on an unsaved draft**: in edit mode the ▶ button must not post anything → pinned in Task 6 (`manual_button_is_inert_in_edit_mode`).
2. **Restart race**: pressing during a `restart` run must end the old run `cancelled` and leave exactly one active run that finishes → pinned in Task 3 (`card_press_restarts_an_active_restart_flow`).
3. **Old run JSON**: runs recorded before this change (`"entity": "x"`) and new ones without `entity` must both parse in the web → pinned in Task 2 (`trigger_info_entity_is_optional`).
4. **A card id that is not a Manual card** (a state trigger, a command, a missing id) must be refused, not start mid-graph → pinned in Task 3 (`run_flow_from_rejects_a_card_that_is_not_manual`) and Task 4 (HTTP 422).
5. **`{trigger}` in a notify after a valued Manual card** prints the bare value, and `?` without one → pinned in Task 3 (`manual_value_reaches_notify_text`).

---

### Task 1: `trigger.manual` in the core model and validation

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (enum `NodeKind`, `type_name`, `is_trigger`, new params struct)
- Modify: `crates/oikos-core/src/flow/mod.rs` (re-export `TriggerManualParams`)
- Modify: `crates/oikos-core/src/flow/validate.rs` (errors, `check_ifs_after_valueless_triggers`, tests)

**Interfaces:**
- Produces: `NodeKind::TriggerManual(TriggerManualParams)` with `#[serde(rename = "trigger.manual")]`; `pub struct TriggerManualParams { #[serde(default, skip_serializing_if = "Option::is_none")] pub value: Option<Scalar>, #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")] pub extra: BTreeMap<String, Value> }` deriving the same traits as `TriggerScheduleParams` (incl. `Default`, schemars cfg). `type_name()` → `"trigger.manual"`; `is_trigger()` → true; `ports()` → empty.
- Produces: `FlowError::AverageNeedsEntity { node: String }`, message ``node `{node}`: an average after a manual trigger must name an entity``.
- Changes messages: `IfNeedsEntity` → ``node `{node}`: an if after a schedule, sun or manual trigger without a value must name an entity``; `SwitchNeedsEntity` likewise with "a switch".

- [ ] **Step 1: Write the failing tests** in `validate.rs` `mod tests` (use the existing `decode_flow(nodes, edges)` helper; a command node `act` = `{"id":"act","type":"command","params":{"entity":"l","command":{"type":"turn_on"}}}`):
  - `manual_alone_is_a_trigger`: nodes `[{"id":"m","type":"trigger.manual","params":{}}, act]`, edge m→act → `Ok(())`.
  - `manual_round_trips`: `Flow::decode` of `{"id":"m","type":"trigger.manual","params":{"value":21.5}}` then `encode` contains `"type":"trigger.manual"` and `"value":21.5`; `params` with `"value":null` decodes to `value: None`.
  - `if_without_entity_after_valueless_manual_is_rejected`: m (no value) → `if {"op":"eq","value":true}` id `check` → act on `true` → `Err(IfNeedsEntity{node:"check"})`, and `to_string()` equals the new message.
  - `switch_without_entity_after_valueless_manual_is_rejected`: same with a one-rule switch → `SwitchNeedsEntity`.
  - `if_without_entity_after_valued_manual_is_fine`: m `{"value":true}` → check → act → `Ok(())`.
  - `average_after_valued_manual_needs_entity`: m `{"value":20}` → `if {"op":"gt","value":19,"average":"15m"}` id `avg` → act → `Err(AverageNeedsEntity{node:"avg"})`.
  - `average_with_entity_after_manual_is_fine`: same with `"entity":"t"` → `Ok(())`.
  - `mixed_paths_valueless_manual_and_state_is_rejected`: state trigger and a no-value m both into an entity-less `if` → `IfNeedsEntity`.
  - Update `if_without_entity_after_a_schedule_is_rejected` (and any switch twin) to the new message text.

- [ ] **Step 2: Run** `cargo test -p oikos-core flow::validate` — Expected: compile error (`TriggerManual` unknown).

- [ ] **Step 3: Implement.** Add the variant/params/matches above. In `check_ifs_after_valueless_triggers`, seed with schedule, sun and `TriggerManual` whose `value` is `None` (unchanged walk). Add a second walk seeded with `TriggerManual` whose `value` is `Some`: an `If`/`Switch` with `entity: None` and `average: Some(_)` → `AverageNeedsEntity`. Factor the walk into `fn reachable<'a>(flow: &'a Flow, seeds: Vec<&'a Node>) -> Vec<&'a Node>` (same port handling as today) and use it for both. Update the doc comment above the function.

- [ ] **Step 4: Run** `cargo test -p oikos-core` — Expected: PASS. Fix only `oikos-core`'s own non-exhaustive `NodeKind` matches here; `oikos-os` and `oikos-web` matches are fixed in Tasks 3 and 5 (`cargo test -p oikos-core` builds the core alone).

- [ ] **Step 5: Commit** `feat(flows): trigger.manual node and its validation`.

---

### Task 2: `TriggerInfo` without an entity

**Files:**
- Modify: `crates/oikos-api/src/flows.rs` (`TriggerInfo`, `RunView.trigger` doc, tests)
- Modify: `crates/oikos-os/src/flows/engine.rs` (three `TriggerInfo { entity: … }` constructors, `node_left`, `Notify` arm)
- Modify: `crates/oikos-web/src/flows.rs` (run label at ~146)

**Interfaces:**
- Produces: `pub struct TriggerInfo { #[serde(default, skip_serializing_if = "Option::is_none")] pub entity: Option<String>, pub value: Value, pub previous: Value }`. Doc: "`None` for a run started from a Manual card that carries a value."
- Produces (engine): `node_left` with `entity: None` and a trigger whose `entity` is `None` returns `Ok(Scalar::from_json(&trigger.value))`; with `average: Some` in that case returns `Err("no entity to average".to_owned())` (validation keeps this unreachable).

- [ ] **Step 1: Write the failing test** `trigger_info_entity_is_optional` in `oikos-api/src/flows.rs` tests: `{"entity":"x","value":true,"previous":false}` parses to `entity: Some("x")`; `{"value":true,"previous":null}` parses to `entity: None`; serialising the latter has no `entity` key.
- [ ] **Step 2: Run** `cargo test -p oikos-api` — Expected: FAIL (type mismatch).
- [ ] **Step 3: Implement** the field change; wrap the engine constructors in `Some(..)`; in the `Notify` arm look up `hub.reading` only when `t.entity` is `Some`; in `node_left` handle the entity-less trigger as above. In the web run label, use the entity branch only when `trigger.entity` is `Some` (Task 5 adds the manual wording).
- [ ] **Step 4: Run** `cargo test -p oikos-api -p oikos-os` and `cargo check -p oikos-web --target wasm32-unknown-unknown` — Expected: PASS / clean (the web may still fail on the non-exhaustive `NodeKind` matches from Task 1; that is Task 5's job — if so, check only that no `TriggerInfo` error remains).
- [ ] **Step 5: Commit** `refactor(flows): a trigger value may come without an entity`.

---

### Task 3: Hub and engine start a run from a Manual card

**Files:**
- Modify: `crates/oikos-os/src/flows/mod.rs` (`EngineRequest::Run`)
- Modify: `crates/oikos-os/src/flows/hub.rs` (`run_flow`, new `run_flow_from`)
- Modify: `crates/oikos-os/src/flows/store.rs` (`begin_manual_run` takes a trigger)
- Modify: `crates/oikos-os/src/flows/engine.rs` (`run_manual`, `run_node` arm, helper)
- Test: `crates/oikos-os/tests/flows.rs`

**Interfaces:**
- Consumes: `NodeKind::TriggerManual`, `TriggerInfo.entity: Option<String>`.
- Produces: `pub fn manual_trigger(flow: &Flow, node: &str) -> Option<TriggerInfo>` in `engine.rs` (`pub(crate)`): `Some(TriggerInfo { entity: None, value: scalar.to_json(), previous: Value::Null })` when `node` is a `TriggerManual` with a value, else `None`.
- Produces: `Hub::run_flow_from(&self, id: &str, node: Option<&str>) -> Result<RunView, HubError>`; `Hub::run_flow(id)` becomes `self.run_flow_from(id, None)` (keeps `ambiances/apply.rs` unchanged).
- Produces: `EngineRequest::Run { flow_id: String, run_id: String, node: Option<String> }`.
- Produces: `FlowStore::begin_manual_run(&mut self, flow_id: &str, trigger: Option<TriggerInfo>, at: u64) -> Option<RunView>`.

- [ ] **Step 1: Write the failing tests** in `tests/flows.rs` (reuse `engine_hub`, `echo`, `settle`, `next_run`, `next_run_event`, `step_nodes`; build flows as JSON like `reference_json`, ids generic):
  - `card_press_runs_from_the_card_with_its_value`: flow `living` = reference flow plus node `{"id":"test_on","type":"trigger.manual","params":{"value":true}}` and edge test_on→moving. `hub.run_flow_from("living", Some("test_on"))` → `Done` run with `manual == true`, `step_nodes == ["test_on","moving","on"]`, `trigger.value == json!(true)`, `trigger.entity == None`, `steps[0].detail == "manual: true"`.
  - `run_now_carries_the_first_manual_card_value`: flow whose only trigger is `{"id":"go","type":"trigger.manual","params":{"value":false}}` → moving → … ; `hub.run_flow("x")` → steps `["go","moving","wait"]` (false port).
  - `valueless_card_step_detail_is_manual`: `params: {}` → `steps[0].detail == "manual"`, `run.trigger.is_none()`.
  - `run_flow_from_rejects_a_card_that_is_not_manual`: `Some("motion")` and `Some("ghost")` → `Err(HubError::Rejected(m))` with `m == "`motion` is not a manual trigger of this flow"` / `"`ghost` …"`.
  - `card_press_restarts_an_active_restart_flow`: reference flow (`restart`) plus a manual card wired to `wait`; press once (run parks on the 5 min delay), press again → first run ends `Cancelled`, second is `Running` with the card as step 0, and the flow view's `active_run` (read the way the existing run tests read it) equals the second id.
  - `card_press_refused_while_single_flow_is_active`: same with `"mode":"single"` → second press `Err(HubError::Rejected("a run is active"))`, no skipped run added.
  - `run_now_still_refused_while_restart_flow_is_active`: second `run_flow("living")` (no card) on the parked restart flow → `Err(Rejected("a run is active"))`.
  - `manual_value_reaches_notify_text`: Manual card `{"value":"hello"}` → notify `{"title":"{trigger}", …}`; assert the published push title is `hello` (follow how existing notify tests capture the push); with `params: {}` the title is `?`.
  - Update `run_flow_executes_without_trigger_data` only if its expectations change (they should not).
- [ ] **Step 2: Run** `cargo test -p oikos-os --test flows manual card_press run_now` — Expected: FAIL (no `run_flow_from`).
- [ ] **Step 3: Implement.**
  - `run_flow_from`: same offline/unknown/invalid/disabled checks. With `Some(node)`: refuse unless the definition has that id as a `TriggerManual`. Active run: refuse with `"a run is active"` when `node` is `None` or `definition.mode` is `Single`; otherwise (restart card press) continue. Start = `node` or the first trigger; `trigger = manual_trigger(&definition, start)`; `begin_manual_run(id, trigger, now)`; send `EngineRequest::Run { flow_id, run_id, node }`. The store needs no other change: `end_run` only clears `active` when the id matches, so the old run ending later leaves the new active slot alone.
  - `run_manual(flow_id, run_id, node)`: start = `node` or first trigger; trigger = `manual_trigger(&flow, &start)`; if the engine has an active run for the flow and (`node.is_some()` and `flow.mode == Restart`) → `self.cancel(flow_id)` then spawn; if active otherwise → end the new run `Cancelled` (today's race path).
  - `run_node` arm: `TriggerManual(p)` → `(Ok, match &p.value { Some(v) => format!("manual: {}", v.to_json()), None => "manual".into() }, vec![])` (`"manual: true"`, `"manual: \"hello\""` — follow `Scalar`'s JSON form).
  - Leave the state/press/signal/schedule/sun loops alone; they match concrete kinds, so a Manual card never fires by itself.
- [ ] **Step 4: Run** `cargo test -p oikos-os` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(flows): start a run from a manual card`.

---

### Task 4: HTTP body and MCP `node`

**Files:**
- Modify: `crates/oikos-os/src/http/api.rs` (`run_flow` handler, ~708)
- Modify: `crates/oikos-os/src/mcp/params.rs` (`RunFlowParams`), `crates/oikos-os/src/mcp/flow_tools.rs` (tool doc + call), `crates/oikos-os/src/mcp/mod.rs` (server instructions naming `trigger.manual`)
- Modify: `docs/reference/mcp-tools.md` (`run_flow`)
- Test: `crates/oikos-os/tests/http.rs`, `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: `Hub::run_flow_from`.
- Produces: HTTP `POST /api/v1/flows/{id}/run` with optional JSON body `{"node": "<card id>"}`; handler takes `body: Option<Json<RunBody>>` (`#[derive(serde::Deserialize)] struct RunBody { #[serde(default)] node: Option<String> }`; axum 0.8's `Option<Json<_>>` yields `None` without a JSON content type). Refusals map like today (`Rejected` → 422).
- Produces: `RunFlowParams { id, #[serde(default, skip_serializing_if = "Option::is_none")] node: Option<String> }` with doc "The id of a `trigger.manual` card to start from; the run carries its value. Absent: start at the first trigger."

- [ ] **Step 1: Write the failing tests:** in `http.rs` next to the existing run tests (~1514): post `{"node":"test_on"}` to a flow with that card → 202 and `steps`/`manual` as expected once done (or just 202 + `manual: true`); post `{"node":"motion"}` → 422; post `{}` → 202 as today. In `mcp.rs` next to `set_flow_enabled_and_run_flow`: `run_flow` with `{"id":…, "node":"test_on"}` returns a run; with a non-manual node returns the tool error text.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test http --test mcp run_flow` — Expected: FAIL.
- [ ] **Step 3: Implement** handler and tool; mention `trigger.manual` (params `value`) wherever the MCP flow text lists trigger types (`grep -n "trigger.sun" crates/oikos-os/src/mcp`). Update `docs/reference/mcp-tools.md`.
- [ ] **Step 4: Run** `cargo test -p oikos-os` — Expected: PASS (the MCP tool-list snapshot test, if any, updated deliberately).
- [ ] **Step 5: Commit** `feat(flows): run_flow takes the manual card to start from`.

---

### Task 5: Web model, library, settings and value picker

**Files:**
- Modify: `crates/oikos-web/src/i18n/flows.rs` (new `FlowText` keys)
- Modify: `crates/oikos-web/src/flow_text.rs` (`role`, summaries/titles), `crates/oikos-web/src/flows.rs` (node name ~66, summary ~98, run label ~146), `crates/oikos-web/src/icons.rs` (~307), `crates/oikos-web/src/flow_edit.rs` (`next_node_id` prefix `"manual"`), `crates/oikos-web/src/flow_list.rs` (`trigger_kinds`), `crates/oikos-web/src/flow_picker.rs` (generic WHEN card + search words), `crates/oikos-web/src/if_value.rs`, `crates/oikos-web/src/screens/flow_canvas/settings.rs` (Value field for the card; `IfValueField` callers), `crates/oikos-web/src/screens/flow_canvas/value_field.rs` (`IfValueField` fallback input)

**Interfaces:**
- Consumes: `NodeKind::TriggerManual`, `TriggerManualParams`, `TriggerInfo.entity: Option<String>`.
- Produces: `FlowText::NodeManual` ("Manual", "Manuel"), `FlowText::ManualSub` (library subtitle: "Start by hand from the ▶ button", "Lancer à la main avec le bouton ▶"), `FlowText::ManualValue` ("Value", "Valeur"), `FlowText::ManualNoValue` ("No value", "Aucune valeur"), `FlowText::ManualPress` ("Run from here", "Lancer depuis ici"), `FlowText::ManualSaveFirst` ("Save to run from here", "Enregistrez pour lancer d'ici").
- Produces: `pub fn upstream_manual_value(flow: &Flow, node_id: &str) -> Option<Scalar>` in `if_value.rs` — the value of the one Manual card every path back from the node starts at (same walk as `upstream_trigger_entity`; `None` if paths start elsewhere, at several cards with different value kinds, or at a card without a value).
- Produces: `pub fn manual_value_input(lang: Lang, value: &Scalar) -> ValueInput` — `Bool` → the on/off `List` `value_input` builds for a binary sensor; `Number` → `Number { unit: None }`; `Text` → `Text`.
- Produces: `IfValueField` prop `#[prop(optional, into)] fallback: Option<Signal<Option<ValueInput>>>`, used when `entity` is `None`.

- [ ] **Step 1: Write the failing tests** (native `#[cfg(test)]` unit tests in each module):
  - `flow_list.rs`: `kinds_manual_card_is_manual` — a valid flow whose only trigger is a Manual card → `{TriggerKind::Manual}`; a flow with state + Manual → `{State, Manual}`. Adjust `kinds_per_trigger_type_manual_and_invalid` to the new meaning.
  - `flow_edit.rs`: `next_node_id` for a Manual card in an empty flow → `"manual_1"`.
  - `flow_picker.rs`: searching `"inject"`, `"button"`, `"manuel"` lists the Manual card; with no query the WHEN group contains it.
  - `if_value.rs`: `upstream_manual_value` → `Some(Scalar::Bool(true))` for card→if; `None` for a valueless card, for card+state into one if, and for two cards with `true` and `21.5`; `manual_value_input(Lang::En, &Scalar::Number(1.0))` is `Number { unit: None }`.
  - `flows.rs`: run label for `manual: true` with `trigger: Some(TriggerInfo{entity: None, value: json!(true), ..})` reads `"manual · on"` in English (use `value_words` without an entity) and `"manual"` without a trigger.
- [ ] **Step 2: Run** `cargo test -p oikos-web --lib` — Expected: FAIL / compile errors on non-exhaustive matches.
- [ ] **Step 3: Implement.** Fix every non-exhaustive `NodeKind` match the compiler names (`role` → `Role::Trigger`; icon: a new play-in-circle icon in `icons.rs`; card title "Manual" or "Manual · <value words>"; summary none). `trigger_kinds`: `TriggerManual` → `TriggerKind::Manual`, drop the "no trigger → Manual" fallback (validation forbids such flows). `upstream_trigger_entity`: treat `TriggerManual` like schedule/sun (inserts `None`). Settings drawer for `TriggerManual`: a select None / On / Off / Number / Text plus a number or text box, writing `value: Option<Scalar>` into the draft. For `if`/`switch` value fields, pass `fallback` = `upstream_manual_value(...).map(|v| manual_value_input(lang, &v))` when the card has no own entity.
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib` and `cargo check -p oikos-web --target wasm32-unknown-unknown` — Expected: PASS / clean.
- [ ] **Step 5: Commit** `feat(web): manual card in the library, settings and value picker`.

---

### Task 6: ▶ button on the canvas card

**Files:**
- Create: `crates/oikos-web/src/screens/flow_canvas/manual.rs` (`ManualRunButton`)
- Modify: `crates/oikos-web/src/screens/flow_canvas/mod.rs` (`mod manual;`), `crates/oikos-web/src/screens/flow_canvas/graph.rs` (render the button inside the node pill near `flow-node__title` ~692 when the kind is `TriggerManual`), `crates/oikos-web/src/api/core.rs` (`run_flow_from`), `crates/oikos-web/styles/main.css` (`.flow-node__run`)
- Test: `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `FlowText::ManualPress`, `FlowText::ManualSaveFirst`, HTTP body `{"node"}`.
- Produces: `pub async fn run_flow_from(id: &str, node: &str) -> Result<RunView, ApiError>` in `api/core.rs` (POST JSON `{"node": node}` to the same URL as `run_flow`).
- Produces: `#[component] pub fn ManualRunButton(flow_id: Signal<String>, node_id: String, disabled: Signal<bool>, title: Signal<&'static str>) -> impl IntoView` — a round `button.flow-node__run` with a ▶ glyph, `aria-label` = `title`; click calls `run_flow_from` and pushes a refusal to `Toasts` exactly like `run_now` in `toolbar.rs`; the click must `stop_propagation` so it does not select the card or open settings.
- Graph wiring: `disabled` = `mode == Mode::Edit || !flow.enabled`; `title` = `ManualSaveFirst` in edit mode, else `ManualPress`.

- [ ] **Step 1: Write the failing wasm tests** in `tests/flow_canvas.rs`, using the file's existing harness and fetch stubs (add `{"id":"go","type":"trigger.manual","params":{"value":true}}` → `moving` to a copy of `REFERENCE`):
  - `manual_card_shows_run_button`: the view-mode canvas has exactly one `.flow-node__run`, enabled.
  - `manual_button_posts_the_card`: clicking it sends one POST to `/api/v1/flows/<id>/run` whose JSON body is `{"node":"go"}`, and the card does not become selected.
  - `manual_button_is_inert_in_edit_mode`: after entering edit mode the button has `disabled` and a click sends no request.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome crates/oikos-web --test flow_canvas` — Expected: FAIL.
- [ ] **Step 3: Implement** the component, the API call, the graph render and the CSS (sized to sit inside the pill on phones; both themes via existing tokens).
- [ ] **Step 4: Run** the same command — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): run a flow from its manual card`.

---

### Task 7: User docs and full verification

**Files:**
- Modify: `docs/users/flows.md` (trigger list ~34; run section ~104: pressing a card, mode rules, the value as trigger value, `{trigger}`; editor section ~137–139: ▶ button, greyed in edit mode, the library card and its Value field)

- [ ] **Step 1: Write the docs** in the page's existing style; example JSON uses generic ids only.
- [ ] **Step 2: Run the full CI gate** from Global Constraints — Expected: all green.
- [ ] **Step 3: Commit** `docs(flows): manual trigger card`.
- [ ] **Step 4: Push** the branch and open a PR against `main` (body ends with the Claude Code line); note "rebuild hub + web after merge".
