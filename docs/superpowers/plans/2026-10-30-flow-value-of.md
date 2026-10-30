# Flow "Value of" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** If, Switch and When (trigger.state) flow cards can read any value a device reports — a field inside an entity's state, or a sibling entity of the same device, diagnostic included.

**Architecture:** One core module lists an entity's fields and reads one from a state; the flow model gains an optional `field` on `if` / `switch` / `trigger.state`; the hub validates it, the engine reads it and tracks trigger changes per (entity, field); the web flow editor adds a "Value of" select under the entity of the three cards. A sibling entity is just a different `entity`.

**Tech Stack:** Rust workspace — `oikos-core` (model, flow), `oikos-api` (wire types), `oikos-os` (hub, flow engine, MCP), `oikos-web` (Leptos/wasm).

**Spec:** `docs/superpowers/specs/2026-10-30-flow-value-of-design.md`

## Global Constraints

- `field` is optional everywhere and `skip_serializing_if = "Option::is_none"`: an existing flow's JSON and meaning do not change.
- Field names are the struct field names of `oikos_core::model::state`: vacuum `battery`, `fan_speed`; light `brightness`, `color_temp`, `effect`; cover `position`; climate `current`, `target`, `fan_mode`, `action`; water heater `current`, `target`; generic: each `TypeDef.fields` key except `TypeDef.primary`. Every other kind: none.
- Brightness is compared in percent with the light card's rule: `round(level * 100 / 255)`, at least 1 when `level > 0` (today `oikos-web/src/cards/text.rs::brightness_percent`; it moves to core and the web re-uses it).
- `average` with a `field` is allowed only when the field is the entity's recorded history number: cover `position`, climate `current`, water heater `current`.
- `field` without `entity` on `if` / `switch` is invalid.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths; check `git branch --show-current` is `feat/flow-value-of` before each commit.
- Disk: before each build check `df -h ~` and `du -sh ~/dev-scratch/cargo-target`; over 5 GB → `cargo clean`. Build with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`. Browser tests only scoped: `wasm-pack test --headless --chrome -- --test <name>`.

## Review Focus

1. A vacuum message that changes only the battery must not fire a trigger on the vacuum's state, and a state change must not fire a battery trigger (Task 4 test `field_trigger_fires_only_on_its_field`).
2. Reconnect replay: a watched (entity, field) missing from `known` is seeded silently, never fired (Task 4 test `field_trigger_seeds_silently_after_reconnect`).
3. An If with no entity after a battery trigger compares the battery, not the vacuum state (Task 4 test `if_without_entity_reads_the_trigger_field`).
4. A light that is off still stores a brightness (`on == false`, `brightness: Some(n)`): the field reads the stored level in percent, it does not report 0 (Task 1 test `brightness_reads_percent_even_when_off`).
5. Changing the entity in a card drops a `field` the new entity does not report (Task 6 test `entity_change_clears_field_value_to_from`).

---

### Task 1: Core — the fields an entity reports, and reading one

**Files:**
- Create: `crates/oikos-core/src/flow/field.rs`
- Modify: `crates/oikos-core/src/flow/mod.rs` (export), `crates/oikos-core/src/vocabulary.rs` (generalise `generic_reports` to any field name), `crates/oikos-web/src/cards/text.rs` (use core's `brightness_percent`)

**Interfaces:**
- Produces:
  - `pub struct FieldInfo { pub name: String, pub reports: Reports, pub recorded: bool }`
  - `pub fn fields(kind: Kind, component: &str, caps: &Capabilities, unit: Option<&str>) -> Vec<FieldInfo>` — in the Global Constraints order; generic fields from `type_def(component)`, sorted by name; `recorded` true only for cover `position`, climate `current`, water heater `current`.
  - `pub fn field_scalar(value: &StateValue, field: &str) -> Option<Scalar>` — `None` for an unknown field or an absent value; generic reads `GenericState.fields[field]`.
  - `pub fn brightness_percent(level: u8) -> u8` (moved from the web, same body).
  - `vocabulary::field_reports(component, caps, unit, name) -> Reports` (the old `generic_reports` body with `name` instead of the primary).
- Reports per field: vacuum `battery` Number{unit "%"}, `fan_speed` Values(caps.fan_speeds); light `brightness` Number{"%", 0, 100}, `color_temp` Number{None}, `effect` Values(caps.effects); cover `position` Number{"%", 0, 100}; climate `current`/`target` Number{entity unit}, `fan_mode` Values(caps.fan_modes), `action` Text; water heater `current`/`target` Number{entity unit}.

- [ ] **Step 1: Write the failing tests** in `field.rs`:
  - `vacuum_lists_battery_and_fan_speed`: `fields(Kind::Vacuum, "vacuum", &caps_with_fan_speeds(["Silent","Turbo"]), None)` names == `["battery", "fan_speed"]`; battery reports `Number { unit: Some("%"), .. }`; fan_speed reports `Values` of `"Silent"`, `"Turbo"`.
  - `only_recorded_fields_are_recorded`: climate `current` recorded, `target` not; cover `position` recorded; vacuum `battery` not.
  - `kinds_without_fields_list_none`: `Kind::Sensor`, `Kind::Switch`, `Kind::BinarySensor` → empty.
  - `reads_vacuum_battery`: `field_scalar(&StateValue::Vacuum(VacuumState { state: Some("cleaning".into()), battery: Some(62), fan_speed: None }), "battery") == Some(Scalar::Number(62.0))`; `"fan_speed"` → `None`; `"nope"` → `None`.
  - `brightness_reads_percent_even_when_off`: light `{ on: false, brightness: Some(128) }` → `Some(Number(50.0))`.
  - `generic_field_reads_from_fields_map`.
- [ ] **Step 2: Run** `cargo test -p oikos-core flow::field` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `field.rs`, move `brightness_percent` into it, re-export from `oikos_core::flow`; web `cards/text.rs` calls `oikos_core::flow::brightness_percent`.
- [ ] **Step 4: Run** `cargo test -p oikos-core` and `cargo test -p oikos-web` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): list and read the fields an entity reports`.

### Task 2: Flow model and core validation

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (`IfParams`, `SwitchParams`, `TriggerStateParams`), `crates/oikos-core/src/flow/validate.rs`, `crates/oikos-api/src/flows.rs` (`TriggerInfo`)

**Interfaces:**
- Produces: `pub field: Option<String>` on the three params, doc comment "A value inside the entity's state (`battery`, `position` …); absent reads its headline value." `TriggerInfo` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub field: Option<String>`. New validate error variant `FieldWithoutEntity { node: String }` → "node `{node}`: a field needs an entity".
- Every existing `TriggerInfo { … }` literal gets `field: None` (12 sites).

- [ ] **Step 1: Failing tests** in `model.rs`: `if_without_field_serialises_as_before` (an `IfParams` with `field: None` serialises with no `field` key) and `if_field_round_trips` (`{"op":"lte","value":55,"entity":"r","field":"battery"}` parses and serialises back equal). In `validate.rs`: `field_without_entity_is_refused` for `if` and for `switch`.
- [ ] **Step 2: Run** `cargo test -p oikos-core flow::` — Expected: FAIL to compile / FAIL.
- [ ] **Step 3: Implement** the fields, the error, `field: None` at every `TriggerInfo` and params literal (`cargo check --workspace --all-targets` lists them).
- [ ] **Step 4: Run** `cargo test -p oikos-core && cargo check --workspace --all-targets` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(flow): optional field on if, switch and trigger.state`.

### Task 3: Hub — check the field against the house

**Files:**
- Modify: `crates/oikos-os/src/flows/hub.rs` (`check_house`, new `entity_value`)

**Interfaces:**
- Consumes: `fields`, `field_scalar` (Task 1); `field` params (Task 2).
- Produces: `pub fn entity_value(&self, id: &str, field: Option<&str>) -> Option<Scalar>` — `None` field = today's `entity_scalar` (keep `entity_scalar` as a thin call to it). Errors from `check_house`: "node `{node}`: `{entity}` reports no field `{field}`" and "node `{node}`: `{field}` of `{entity}` has no history to average".

- [ ] **Step 1: Failing tests** (hub tests module, using its existing registry helpers): `unknown_field_is_refused` (vacuum + `field: "brightness"`), `average_on_unrecorded_field_is_refused` (vacuum `battery` + `average: "15m"`), `average_on_recorded_field_is_accepted` (climate `current` + `average`), `entity_value_reads_the_field` (vacuum battery 62 → `Number(62.0)`).
- [ ] **Step 2: Run** `cargo test -p oikos-os flows::hub` — Expected: FAIL.
- [ ] **Step 3: Implement** in `check_house` after the entity is found: look up `fields(found.kind, &found.component, &found.capabilities, found.unit.as_deref())`.
- [ ] **Step 4: Run** `cargo test -p oikos-os flows::hub` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(hub): validate a flow card's field against its entity`.

### Task 4: Engine — read the field, fire per field

**Files:**
- Modify: `crates/oikos-os/src/flows/engine.rs`

**Interfaces:**
- Consumes: `Hub::entity_value` (Task 3), `field_scalar` (Task 1).
- Produces: `known: BTreeMap<(String, Option<String>), Option<Scalar>>`, `watched: BTreeSet<(String, Option<String>)>`; `node_left(hub, now, entity, field, average, trigger)` gains `field: Option<&str>`; with no entity it uses `trigger.entity` **and** `trigger.field`.
- `state_changed(&mut self, id: &str, state: &StateValue)` computes each watched key's scalar (`None` field → `Scalar::of_state`, else `field_scalar`) and applies today's rule per key: fire only between two present, different values; `TriggerInfo.field` = the key's field. A trigger fires when its `(entity, field)` matches the key. `state_seen` seeds every missing watched key of `id` via `entity_value`. `averaged_windows` is unchanged (average is only allowed on recorded fields, which are the entity's own number).

- [ ] **Step 1: Failing tests** (engine tests, existing fake hub/registry):
  - `field_trigger_fires_only_on_its_field`: two flows on `vacuum.r`, one plain, one `field: "battery"`; a message 62→55 with state unchanged fires only the battery flow, with trigger value `55`; a message cleaning→docked with battery unchanged fires only the plain one.
  - `field_trigger_seeds_silently_after_reconnect`: after a non-Online `ConnectionChanged`, the first battery report fires nothing; the next change fires.
  - `if_without_entity_reads_the_trigger_field`: battery trigger → `if` with no entity, `op: lte`, `value: 55` takes `true` when the battery is 55 while the state is `"cleaning"`.
  - `if_reads_entity_field`: `if` with `entity: vacuum.r, field: battery, op: lte, value: 55` → `true` at 50.
  - `switch_reads_entity_field`.
- [ ] **Step 2: Run** `cargo test -p oikos-os flows::engine` — Expected: FAIL.
- [ ] **Step 3: Implement** as in Interfaces; `handle_event` passes `&state.value` instead of a scalar.
- [ ] **Step 4: Run** `cargo test -p oikos-os` — Expected: PASS (existing engine tests unchanged).
- [ ] **Step 5: Commit** `feat(flows): if, switch and triggers read a field; changes tracked per field`.

### Task 5: Web — the "Value of" choices and the input for a field

**Files:**
- Modify: `crates/oikos-web/src/if_value.rs`, `crates/oikos-web/src/i18n/flows.rs`

**Interfaces:**
- Consumes: `fields`, `FieldInfo` (Task 1).
- Produces:
  - `pub enum ValueOf { Headline, Field(String), Sibling(String) }` and `pub struct ValueOfChoice { pub of: ValueOf, pub label: String }`.
  - `pub fn value_of_choices(lang: Lang, entities: &BTreeMap<String, EntityView>, e: &EntityView) -> Vec<ValueOfChoice>` — Headline first (label = the entity's type word as the card shows it today), then each field (label `field_label(lang, name)` + " (current value)"), then every other entity with the same `device_id` (diagnostic included, disabled-by-default excluded), labelled by its shown name; an entity without `device_id` has no siblings.
  - `pub fn value_input_of(lang: Lang, e: Option<&EntityView>, field: Option<&str>) -> ValueInput` — `None` field = today's `value_input`; a field maps its `Reports` the same way.
  - `pub fn average_allowed(e: Option<&EntityView>, field: Option<&str>) -> bool` — true for no field, or a `recorded` field.
  - `pub fn field_label(lang: Lang, name: &str) -> String` in `i18n/flows.rs`: EN/FR for battery (Battery/Batterie), fan_speed (Fan speed/Vitesse d'aspiration), brightness (Brightness/Luminosité), color_temp (Colour temperature/Température de couleur), effect (Effect/Effet), position (Position/Position), current (Current temperature/Température actuelle), target (Target temperature/Consigne), fan_mode (Fan mode/Mode de ventilation), action (Activity/Activité); a generic field falls back to its name with `_` as spaces, first letter capitalised.
  - `pub fn after_entity_change` keeps its signature; settings call it with the new input and also clear `field`, `to`, `from` (see Task 6).

- [ ] **Step 1: Failing tests** in `if_value.rs`: `vacuum_value_of_lists_fields_then_siblings` (Roborock with no siblings → [Headline, Field battery, Field fan_speed]; labels contain "Battery (62 %)"), `zigbee_value_of_lists_diagnostic_siblings` (smoke sensor with a diagnostic `…_battery` sibling on the same device → contains `Sibling("…_battery")`), `battery_field_input_is_a_percent_number`, `average_only_on_recorded_fields`.
- [ ] **Step 2: Run** `cargo test -p oikos-web if_value` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p oikos-web` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): the values a flow card can read from a device`.

### Task 6: Web — "Value of" select on the When, If and Switch cards; sentences

**Files:**
- Modify: `crates/oikos-web/src/screens/flow_canvas/settings.rs` (`kind_fields` arms `TriggerState`, `If`, and `switch_fields`), `crates/oikos-web/src/flow_text.rs` (card sentences/titles), `crates/oikos-web/src/i18n/flows.rs` (label "Value of" / "Valeur de")
- Test: `crates/oikos-web/tests/flow_canvas.rs` (browser)

**Interfaces:**
- Consumes: `value_of_choices`, `value_input_of`, `average_allowed`, `field_label` (Task 5).
- Behaviour: a `<select>` labelled "Value of" right under the entity field, shown once an entity is chosen. Choosing `Headline` sets `field = None`; `Field(n)` sets `field = Some(n)`; `Sibling(id)` sets `entity = id`, `field = None`. Any entity change clears `field`, `to`, `from` and resets `value` through `after_entity_change`. The compared-value input uses `value_input_of`. The average field is hidden when `!average_allowed` and clears `average`. Sentences name the value: `"Roborock · Battery ≤ 55 %"` (`flow_text` appends ` · {field_label}` after the entity name when `field` is set).

- [ ] **Step 1: Failing tests:** native `flow_text` test `if_sentence_names_the_field` (`Roborock · Battery ≤ 55 %`, FR `Roborock · Batterie ≤ 55 %`); browser test `if_card_value_of_sets_field` in `tests/flow_canvas.rs`: open an If node on the vacuum, select "Battery" in the "Value of" select → draft node has `field == Some("battery")` and the value input is a number box; browser test `entity_change_clears_field_value_to_from`: with `field: battery`, `to`/`from` set, picking another entity leaves `field`, `to`, `from` as `None`.
- [ ] **Step 2: Run** `cargo test -p oikos-web flow_text` and `wasm-pack test --headless --chrome -- --test flow_canvas` (crates/oikos-web) — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** both — Expected: PASS; then `cargo clippy -p oikos-web --all-targets -- -D warnings` and `cargo clippy -p oikos-web --target wasm32-unknown-unknown --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(web): Value of on the When, If and Switch cards`.

### Task 7: MCP instructions and user docs

**Files:**
- Modify: `crates/oikos-os/src/mcp/mod.rs` (server instructions paragraph on `if` / `trigger.state` / `switch`), `docs/users/flows.md`

- [ ] **Step 1:** MCP text: "`if`, `switch` and `trigger.state` take an optional `field` naming a value inside the entity's state (vacuum `battery`, `fan_speed`; light `brightness` in %; cover `position`; climate and water heater `current`, `target` …); `average` works on a field only when it is the entity's recorded number (cover position, current temperature). Diagnostic entities (battery, link quality) are ordinary entities: name them in `entity`." If an MCP test asserts the instructions text, update it.
- [ ] **Step 2:** `docs/users/flows.md`: a "Value of" subsection under the If / When cards with the Roborock battery ≤ 55 % example and the Zigbee battery example; mention average's limit.
- [ ] **Step 3: Run** `cargo test -p oikos-os mcp` — Expected: PASS.
- [ ] **Step 4: Commit** `docs: Value of in flows (MCP and user guide)`.

### Final gates (after Task 7)

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo check -p oikos-web --target wasm32-unknown-unknown`; `cargo test --workspace` (per crate if disk < 5 GB free); scoped browser tests `rich_cards` is not affected — run `flow_canvas` and `flows_list`.
- Remove the TODOS.md line? None exists for this feature; nothing to remove.
