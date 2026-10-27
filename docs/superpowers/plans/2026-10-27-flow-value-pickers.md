# Flow value pickers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the raw JSON boxes of the flow editor's If card (value) and Command card (command) with dropdowns and typed inputs, drawn from lists core builds from each entity's discovery data.

**Architecture:** A new wasm-safe core module `oikos_core::vocabulary` says what an entity accepts (`actions`) and reports (`reports`) from `Kind`, `component`, `Capabilities`, `unit` and the type table; a fixture test proves the command mapper accepts every action. The web draws those lists generically (a `CommandFields` component shared by the Command card and the ambiance form, an `IfValueField` for the If card) and keeps only wording per kind. The web's own per-kind command lists are deleted.

**Tech Stack:** Rust, Leptos (wasm), `oikos_core` / `oikos_api`, native `cargo test`.

**Spec:** `docs/superpowers/specs/2026-10-27-flow-value-pickers-design.md` (revised version, commit 9763655). Read it first; its "Settled calls" are binding.

## Global Constraints

- The web holds no list of commands or values per device type. Per-kind web code is allowed only for wording (`command_label`, `i18n::states::state_word`).
- No change to how a command maps to MQTT, to the flow model, the engine, MCP tools, `flow_text.rs` sentences, `cards/*` or `groups.rs`.
- New `Capabilities` fields are `#[serde(default)]` so the API and the Android app keep working.
- A saved flow or ambiance opens, displays and saves unchanged; opening a card never rewrites its value or command.
- Every new user-visible word exists in English and French (`i18n/flows.rs` `FlowText`: add the variant, its entry in the all-variants list near line 346, its `(en, fr)` pair near line 593).
- Brightness param: shown 1–100 %, `factor` 2.55, stored `round(shown × 2.55)` as 0–255.
- Colour swatches store `Hs`: white (0, 0), red 0, orange 30, yellow 55, green 120, cyan 180, blue 225, purple 270, magenta 300, pink 335 (s 100).
- Never name the user's house, rooms, people or hosts in code, tests or docs.
- Commits as `Gekkotron` (`git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`), message ending `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Before building, `df -h ~`; if `~/dev-scratch/cargo-target` is over 5 GB, `cargo clean -p oikos-web`.
- Checks: `cargo test -p oikos-core -p oikos-web`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo check -p oikos-core -p oikos-api -p oikos-web --target wasm32-unknown-unknown`.

## Review Focus

1. **An old If value that is not in the list** (`"closed"` on a switch, `"ON"` text) shows selected as is and is not rewritten on open → Task 5 `unlisted_current_value_is_kept_first`.
2. **Changing only a light's effect** keeps its brightness and colour → Task 3 `setting_one_param_keeps_the_others`.
3. **An If fed by two state triggers on different entities, or by a schedule** falls back to a text box → Task 5 `upstream_entity_is_none_for_mixed_triggers`.
4. **"Other value…" typing `21,5`, `true`, ` on `** stores 21.5, `true`, `"on"` → Task 5 `other_value_parses_numbers_bools_and_text`.
5. **An alarm panel that requires a code** offers Arm with a code input and the mapper accepts it with the code filled → Task 1 `every_fixture_action_maps` (code param filled) and `alarm_with_code_has_a_secret_code_param`.

---

## File Structure

- Create `crates/oikos-core/src/vocabulary.rs` (or `vocabulary/mod.rs` + `actions.rs` + `reports.rs` if it passes ~400 lines) — `Action`, `Param`, `Reports`, `actions`, `reports`. `pub mod vocabulary;` in `crates/oikos-core/src/lib.rs`.
- Modify `crates/oikos-core/src/model/entity.rs` (`Capabilities`), `crates/oikos-core/src/discovery/capabilities.rs` (cover facts), `crates/oikos-core/src/state/kinds/{cover,alarm,vacuum}.rs` (state word lists `pub(crate)`).
- Create `crates/oikos-core/tests/vocabulary.rs` — fixture round-trip.
- Create `crates/oikos-web/src/command_form.rs` — param ↔ command JSON helpers, labels, swatches, `CommandFields`. `pub mod command_form;` in `lib.rs`.
- Create `crates/oikos-web/src/if_value.rs` — If choices, "Other" parsing, keep-or-reset, upstream trigger walk. `pub mod if_value;` in `lib.rs`.
- Create `crates/oikos-web/src/screens/flow_canvas/value_field.rs` — `IfValueField`.
- Modify `crates/oikos-web/src/flow_picker.rs` (drop `commands`, `alarm_commands`, `generic_controls`), `crates/oikos-web/src/ambiances/action_forms.rs` (drop `device_commands`, `command_key`, `command_label`, `ValueSpec`, `value_spec`, `with_value`; `DeviceForm` uses `CommandFields`), `crates/oikos-web/src/screens/flow_canvas/settings.rs` (Command and If branches), `crates/oikos-web/src/i18n/flows.rs`.
- Modify `docs/users/flows.md`, `docs/contributors/adding-a-device.md`.

---

### Task 1: Core `vocabulary::actions` + cover facts + fixture round-trip

**Files:**
- Create: `crates/oikos-core/src/vocabulary.rs`, `crates/oikos-core/tests/vocabulary.rs`
- Modify: `crates/oikos-core/src/lib.rs`, `crates/oikos-core/src/model/entity.rs`, `crates/oikos-core/src/discovery/capabilities.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  pub struct Action { pub command: Command, pub params: Vec<Param> }

  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum Param {
      Number { key: String, min: f64, max: f64, step: f64, unit: Option<String>, factor: f64, optional: bool },
      Choice { key: String, options: Vec<String>, optional: bool },
      Colour { key: String },
      Text { key: String, secret: bool, optional: bool },
  }

  pub fn actions(kind: Kind, component: &str, caps: &Capabilities) -> Vec<Action>;
  ```
  `Param::key(&self) -> &str` accessor.
- New `Capabilities` fields (`#[serde(default)]`): `open_close: bool` (cover has a string `command_topic`), `stop: bool` (cover has a `command_topic` and `payload_stop` is not JSON null). Filled in `discovery/capabilities.rs` `Kind::Cover`.

Action rules, in this order per kind (spec "Core" section is the source):
- Light: `TurnOn` (params: brightness `Number{key:"brightness",min:1,max:100,step:1,unit:Some("%"),factor:2.55,optional:true}` when `brightness`; `Colour{key:"color"}` when `color_modes` ∩ {hs,xy,rgb,rgbw,rgbww}; `Choice{key:"effect",options:effects,optional:true}` when non-empty), `TurnOff`, `Toggle`. Switch: `TurnOn`, `TurnOff`, `Toggle` (no params).
- Cover: `Open`, `Close` when `open_close`; `Stop` when `stop`; `SetPosition{position:0}` with `Number{key:"position",0,100,1,"%",1,false}` when `position`.
- Climate: `SetTemperature{temperature: min_temp.unwrap_or(5.0)}` with `Number{key:"temperature", min_temp|5, max_temp|35, temp_step|0.5, "°C", 1, false}`, one `SetHvacMode` per `hvac_modes`, one `SetFanMode` per `fan_modes`. WaterHeater: same temperature action (its own min/max/step), one `SetHvacMode` per `hvac_modes`.
- Number: `SetNumber{value: min|0}` with `Number{key:"value", min|0, max|100, step|1, unit: None, factor 1, false}` (the unit is filled by the caller only if needed — keep `None` here; the web shows `EntityView.unit`).
- Select: one `SelectOption` per `options`. Button: `Press`.
- AlarmPanel: `Disarm{code:None}` then one `Arm{mode, code:None}` per `alarm_modes`; each gets `Text{key:"code",secret:true,optional:false}` when the matching `code_disarm_required` / `code_arm_required` is set.
- Vacuum: one `VacuumAction` per `vacuum_actions`, one `SetFanSpeed` per `fan_speeds`.
- Generic: per announced control (`caps.generic.controls`, defs from `types::type_def(component)`): press → `Control{value:None}`; OnOff → two actions `Some(json!(true))`, `Some(json!(false))`; Choice → one per `generic.options[field]`; Number → one with `Number{key:"value", range from generic.ranges[field] else 0/100/1, unit None, factor 1, false}` and value = min; Text → one with `Text{key:"value",secret: generic.secret,optional:false}` and value `""`.
- Sensor, BinarySensor: none.

- [ ] **Step 1: Write the failing unit tests** in `vocabulary.rs` `mod tests`:

```rust
#[test]
fn cover_without_stop_payload_offers_no_stop() {
    let caps = Capabilities { open_close: true, stop: false, position: true, ..Default::default() };
    let cmds: Vec<_> = actions(Kind::Cover, "cover", &caps).into_iter().map(|a| a.command).collect();
    assert_eq!(cmds, vec![Command::Open, Command::Close, Command::SetPosition { position: 0 }]);
}

#[test]
fn colour_light_turn_on_has_brightness_colour_and_effect() {
    let caps = Capabilities { brightness: true, color_modes: ["xy".into()].into(), effects: vec!["blink".into()], ..Default::default() };
    let on = &actions(Kind::Light, "light", &caps)[0];
    assert_eq!(on.params.iter().map(Param::key).collect::<Vec<_>>(), ["brightness", "color", "effect"]);
}

#[test]
fn alarm_with_code_has_a_secret_code_param() {
    let caps = Capabilities { alarm_modes: [AlarmMode::Away].into(), code_arm_required: true, ..Default::default() };
    let list = actions(Kind::AlarmPanel, "alarm_control_panel", &caps);
    assert_eq!(list[0].params, vec![]); // Disarm, no code required
    assert_eq!(list[1].params, vec![Param::Text { key: "code".into(), secret: true, optional: false }]);
}

#[test]
fn select_gives_one_action_per_option() {
    let caps = Capabilities { options: vec!["a".into(), "b".into()], ..Default::default() };
    assert_eq!(actions(Kind::Select, "select", &caps).len(), 2);
}
```

and in `discovery/capabilities.rs` tests: `cover_stop_follows_payload_stop` — config with `command_topic` and `"payload_stop": null` → `stop == false`, `open_close == true`; without `payload_stop` → `stop == true`; without `command_topic` → both false.

- [ ] **Step 2: Write the failing fixture test** `crates/oikos-core/tests/vocabulary.rs`:

```rust
mod common;
// For every entity of common::all_entities(): for every action of
// vocabulary::actions(e.kind, &e.component, &e.capabilities), build the command with each
// param at its default (Number → min × factor rounded for brightness; Choice → first option
// unless optional (then absent); Colour → Hs{h:0,s:100}; Text code → the config's literal
// `code` when it is not REMOTE_CODE/REMOTE_CODE_TEXT, else "1234"; other Text → "x") and
// assert map_command(&e, &EntityState::default(), &cmd, &Templates::new()) is Ok,
// panicking with the entity id and command otherwise.
#[test]
fn every_fixture_action_maps() { /* as described */ }
```

Param application in this test goes through JSON: `serde_json::to_value(&action.command)`, set `obj[key] = value`, `serde_json::from_value::<Command>`.

- [ ] **Step 3: Run** `cargo test -p oikos-core vocabulary` and `cargo test -p oikos-core --test vocabulary` → FAIL (module missing).
- [ ] **Step 4: Implement** the `Capabilities` fields, the discovery reads, and `actions`. If the fixture test shows a refused action, fix the list (not the mapper) unless the mapper is plainly wrong, and say so in the commit message.
- [ ] **Step 5: Run** `cargo test -p oikos-core` → PASS; `cargo check -p oikos-core --target wasm32-unknown-unknown` → OK.
- [ ] **Step 6: Commit** `feat(core): vocabulary::actions lists what an entity accepts, from discovery`.

---

### Task 2: Core `vocabulary::reports`

**Files:**
- Modify: `crates/oikos-core/src/vocabulary.rs`, `crates/oikos-core/src/state/kinds/{cover,alarm,vacuum}.rs`, `crates/oikos-core/src/state/mod.rs` (re-export the word lists `pub(crate)`)

**Interfaces:**
- Consumes: Task 1 module.
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum Reports {
      Values { values: Vec<Scalar> },
      Number { unit: Option<String>, min: Option<f64>, max: Option<f64> },
      Text,
      Nothing,
  }
  pub fn reports(kind: Kind, component: &str, caps: &Capabilities, unit: Option<&str>) -> Reports;
  ```
  Cover words: a new `pub(crate) const STATES: &[&str] = &["open", "closed", "opening", "closing", "stopped"]` in `state/kinds/cover.rs`, used by `reports` (and by the cover state code where it matches those words, if it reads cleaner). Alarm: existing `STATES` filtered — keep `disarmed`, `armed_<mode>` only for modes in `alarm_modes`, then `arming`, `disarming`, `pending`, `triggered`. Vacuum: existing `STATES`. Other kinds per the spec "Reports" rule.

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn cover_reports_its_five_words() {
    let Reports::Values { values } = reports(Kind::Cover, "cover", &Capabilities::default(), None) else { panic!() };
    assert_eq!(values, ["open", "closed", "opening", "closing", "stopped"].map(|w| Scalar::Text(w.into())));
}

#[test]
fn switch_reports_booleans() {
    assert_eq!(reports(Kind::Switch, "switch", &Capabilities::default(), None), Reports::Values { values: vec![Scalar::Bool(true), Scalar::Bool(false)] });
}

#[test]
fn alarm_reports_only_announced_armed_states() {
    let caps = Capabilities { alarm_modes: [AlarmMode::Away].into(), ..Default::default() };
    let Reports::Values { values } = reports(Kind::AlarmPanel, "alarm_control_panel", &caps, None) else { panic!() };
    assert!(values.contains(&Scalar::Text("armed_away".into())));
    assert!(!values.contains(&Scalar::Text("armed_home".into())));
}

#[test]
fn sensor_with_unit_is_a_number() {
    assert_eq!(reports(Kind::Sensor, "sensor", &Capabilities::default(), Some("°C")), Reports::Number { unit: Some("°C".into()), min: None, max: None });
    assert_eq!(reports(Kind::Sensor, "sensor", &Capabilities::default(), None), Reports::Text);
}

#[test]
fn generic_lock_reports_its_primary_on_off() {
    // a `lock` component with GenericCaps for its primary field → Values([true, false])
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-core vocabulary` → FAIL.
- [ ] **Step 3: Implement** `reports`.
- [ ] **Step 4: Run** `cargo test -p oikos-core` → PASS; wasm check → OK.
- [ ] **Step 5: Commit** `feat(core): vocabulary::reports lists what an entity can report`.

---

### Task 3: Web command helpers on top of core; remove the web's per-kind lists

**Files:**
- Create: `crates/oikos-web/src/command_form.rs`; modify `lib.rs`, `flow_picker.rs`, `ambiances/action_forms.rs`, `i18n/flows.rs`, `flow_text.rs` (`colour_name` → `pub(crate)`)

**Interfaces:**
- Consumes: `oikos_core::vocabulary::{Action, Param, actions}`.
- Produces (all `pub`):
  - `fn entity_actions(e: &EntityView) -> Vec<Action>` — `vocabulary::actions(e.kind, &e.component, &e.capabilities)`; for a `SetNumber` action, the value defaults to the entity's current number when it has one (the flow picker's current behaviour).
  - `fn command_key(c: &Command) -> String` — moved from `action_forms.rs`, extended: `fan:<mode>`, `speed:<speed>`, `vacuum:<action>`, `arm:<mode>`, `control:<name>:<value>` for OnOff/Choice controls (value included so Lock and Unlock differ).
  - `fn command_label(lang: Lang, c: &Command) -> String` — moved and extended for every `Command` variant (wording only; vacuum actions and modes through `state_word`/`format::word`; `Control` with OnOff value uses the flow library's Lock/Unlock words when the control is `locked`, else "<Control>: On/Off").
  - `fn command_options(current: &Command, actions: &[Action]) -> Vec<Action>` — `actions`, with `Action { command: current, params: vec![] }` first when `command_key(current)` is not offered.
  - `fn retarget(current: &Command, actions: &[Action]) -> Command` — `current` when offered, else the first action's command, else `current`.
  - `fn param_value(c: &Command, key: &str) -> Option<Value>` and `fn with_param(c: &Command, key: &str, value: Option<Value>) -> Command` — through the command's JSON; `None` removes the key; a result that does not parse back into `Command` returns `c` unchanged.
  - `const SWATCHES: [(FlowText, Hs); 10]`, `fn swatch_of(hs: Hs) -> Option<usize>`.
- Removed: `flow_picker::{commands, alarm_commands, generic_controls}` (the picker now iterates `entity_actions(e)` and uses each `action.command`), `action_forms::{device_commands, command_key, command_label, ValueSpec, value_spec, with_value}`.

- [ ] **Step 1: Write the failing tests** in `command_form.rs`:

```rust
#[test]
fn setting_one_param_keeps_the_others() {
    let on = Command::TurnOn { brightness: Some(100), color: Some(SWATCHES[1].1), effect: None };
    assert_eq!(with_param(&on, "effect", Some(json!("blink"))), Command::TurnOn { brightness: Some(100), color: Some(SWATCHES[1].1), effect: Some("blink".into()) });
    assert_eq!(with_param(&on, "color", None), Command::TurnOn { brightness: Some(100), color: None, effect: None });
}

#[test]
fn a_param_that_does_not_parse_is_ignored() {
    let pos = Command::SetPosition { position: 10 };
    assert_eq!(with_param(&pos, "position", Some(json!("abc"))), pos);
}

#[test]
fn unlisted_command_is_offered_first() {
    let list = vec![Action { command: Command::TurnOff, params: vec![] }];
    let odd = Command::SetPosition { position: 10 };
    assert_eq!(command_options(&odd, &list)[0].command, odd);
    assert_eq!(command_options(&Command::TurnOff, &list).len(), 1);
}

#[test]
fn retarget_keeps_offered_command_else_first() {
    let cover = vec![Action { command: Command::Open, params: vec![] }, Action { command: Command::Close, params: vec![] }];
    assert_eq!(retarget(&Command::Close, &cover), Command::Close);
    assert_eq!(retarget(&Command::TurnOff, &cover), Command::Open);
}

#[test]
fn swatches_read_back_as_their_own_colour_name() {
    for (name, hs) in SWATCHES { assert_eq!(crate::flow_text::colour_name(hs), name); }
    assert_eq!(swatch_of(Hs { h: 12.0, s: 80.0 }), None);
}

#[test]
fn lock_and_unlock_are_two_choices() {
    let lock = Command::Control { control: "locked".into(), value: Some(json!(true)) };
    let unlock = Command::Control { control: "locked".into(), value: Some(json!(false)) };
    assert_ne!(command_key(&lock), command_key(&unlock));
}
```

Move the label tests from `action_forms.rs` (`command_labels_follow_the_language`, `brightness_value_is_a_percentage` becomes a `with_param` test on `"brightness"`). Update `flow_picker.rs` tests that called `commands(...)` to call `entity_actions(...)`; the alarm test whose panel announces no modes now expects core's five modes (spec: the old fallback disagreed with core).

- [ ] **Step 2: Run** `cargo test -p oikos-web` → FAIL.
- [ ] **Step 3: Implement**, delete the removed functions, switch the picker and `DeviceForm` (still with its own select + number input for now, fed by `entity_actions`/`command_options`/`retarget`).
- [ ] **Step 4: Run** `cargo test -p oikos-web` → PASS; `grep -nE "Kind::(Light|Cover|Climate|Select|AlarmPanel|Vacuum|Number|Button|Generic) *=>" crates/oikos-web/src/flow_picker.rs crates/oikos-web/src/ambiances/action_forms.rs crates/oikos-web/src/command_form.rs` → no line that builds a `Command`.
- [ ] **Step 5: Commit** `refactor(web): flow library and ambiances read core's action list`.

---

### Task 4: `CommandFields` on the Command card and in ambiances

**Files:**
- Modify: `crates/oikos-web/src/command_form.rs`, `screens/flow_canvas/settings.rs` (Command branch ~line 1000; make `JsonField` `pub(crate)`), `ambiances/action_forms.rs` (`DeviceForm`), `i18n/flows.rs`

**Interfaces:**
- Consumes: Task 3.
- Produces:
  ```rust
  #[component]
  pub fn CommandFields(
      command: Signal<Command>,
      entity: Signal<Option<EntityView>>,
      write: Callback<Command>,
      /// Show the last "Advanced (JSON)" action (flow card only).
      advanced: bool,
      /// Class of each `<label>` wrapper: "node-editor__field" (flow card) or "field" (ambiances).
      field_class: &'static str,
  ) -> impl IntoView
  ```
  Action `<select>` from `command_options(command, entity_actions(entity))`, labels `command_label`. For the selected action, one input per `Param`, chosen by the param type only:
  `Number` → `<input type="number">` with min/max/step, value `param_value ÷ factor` (rounded to `step`), writes `round(shown × factor)` when `factor ≠ 1` else the number, plus the unit (param `unit`, else `EntityView.unit`); `Choice` → `<select>` with a first "None" entry when `optional`; `Colour` → `<select>` of `SWATCHES` names with "None" first, and "Custom" selected when the stored colour has no swatch; `Text` → text box (`type="password"` when `secret`). Empty optional inputs write `None`. With `advanced`, a last action "Advanced (JSON)" shows `JsonField` with the current command; it writes only values that parse into `Command`.
  New `FlowText` labels: `Action` ("Action"/"Action"), `NoneChoice` ("None"/"Aucun"), `ColourCustom` ("Custom"/"Personnalisée"), `AdvancedJson` ("Advanced (JSON)"/"Avancé (JSON)"), and one label per param key: `position` ("Position"), `temperature` ("Temperature"/"Température"), `value` ("Value"/"Valeur"), `brightness` ("Brightness"/"Luminosité"), `color` ("Colour"/"Couleur"), `effect` ("Effect"/"Effet"), `code` ("Code"); an unknown key is shown prettified.

- [ ] **Step 1: Implement** `CommandFields` (state of its own: only whether "Advanced" is open).
- [ ] **Step 2: Wire the Command card:** replace the `CommandJson` `JsonField` with `CommandFields { advanced: true, field_class: "node-editor__field" }`. `entity` from `House.entities` by the node's entity; the entity picker's `on_change` also sets `c.command = retarget(&c.command, &entity_actions(new))` when the new entity is known. With no entity: only the Advanced JSON box.
- [ ] **Step 3: Wire ambiances:** `DeviceForm` renders `CommandFields { advanced: false, field_class: "field" }` instead of its select + number input; keep `ambiance-device__command` on the action select and `ambiance-device__value` on a number input.
- [ ] **Step 4: Run** `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown && cargo clippy -p oikos-web --all-targets -- -D warnings` → pass.
- [ ] **Step 5: Commit** `feat(web): Command card picks an action instead of raw JSON`.

---

### Task 5: If value helpers (`if_value.rs`, pure)

**Files:**
- Create: `crates/oikos-web/src/if_value.rs`; modify `lib.rs`.

**Interfaces:**
- Consumes: `oikos_core::vocabulary::{Reports, reports}`.
- Produces (all `pub`):
  ```rust
  pub struct Choice { pub value: serde_json::Value, pub label: String }
  pub enum ValueInput { List(Vec<Choice>), Number { unit: Option<String> }, Text }
  pub fn value_input(lang: Lang, e: Option<&EntityView>) -> ValueInput;
  pub fn with_current(input: ValueInput, current: Option<&serde_json::Value>, lang: Lang, e: Option<&EntityView>) -> ValueInput;
  pub fn parse_other(text: &str) -> Option<serde_json::Value>;
  pub fn after_entity_change(current: Option<&serde_json::Value>, input: &ValueInput) -> Option<serde_json::Value>;
  pub fn upstream_trigger_entity(flow: &Flow, node_id: &str) -> Option<String>;
  ```
  - `value_input`: `reports(e.kind, &e.component, &e.capabilities, e.unit.as_deref())` → `Values` → `List` (labels: `state_word(lang, ctx, word)` for text, `on`/`off` words for booleans, with `ctx` from the kind / device class — wording only), `Number` → `Number { unit }`, `Text`/`Nothing` → `Text`, except `Text` with a live `StateValue::Number` → `Number { unit }`. `None` entity → `Text`.
  - `with_current`: a `List` not containing `current` gets `Choice { value: current, label: <word or JSON text> }` first.
  - `parse_other`: trimmed empty → `None`; `parse_decimal` → number; `true`/`false` → bool; else string.
  - `after_entity_change` (Settled call 2): keep `current` if the list holds it, or `Number` and it is a number, or `Text` and it is a string; else the list's first value; `None` otherwise.
  - `upstream_trigger_entity` (Settled call 1): walk `flow.edges` backwards from `node_id` with a visited set; collect trigger nodes reached; `Some(entity)` only when all are `TriggerState` on one entity.

- [ ] **Step 1: Write the failing tests:**

```rust
#[test]
fn cover_lists_core_words_in_french() {
    let ValueInput::List(l) = value_input(Lang::Fr, Some(&ent(Kind::Cover))) else { panic!() };
    assert_eq!(l[1].value, json!("closed"));
    assert_eq!(l[1].label, "Fermé");
}

#[test]
fn door_sensor_is_worded_by_device_class() {
    let mut door = ent(Kind::BinarySensor); door.device_class = Some("door".into());
    let ValueInput::List(l) = value_input(Lang::En, Some(&door)) else { panic!() };
    assert_eq!((l[0].value.clone(), l[0].label.as_str()), (json!(true), "Open"));
}

#[test]
fn text_sensor_with_a_numeric_state_gets_a_number_box() {
    let mut s = ent(Kind::Sensor); s.state.value = StateValue::Number(3.0);
    assert!(matches!(value_input(Lang::En, Some(&s)), ValueInput::Number { .. }));
}

#[test]
fn unlisted_current_value_is_kept_first() {
    let sw = ent(Kind::Switch);
    let ValueInput::List(l) = with_current(value_input(Lang::En, Some(&sw)), Some(&json!("closed")), Lang::En, Some(&sw)) else { panic!() };
    assert_eq!((l[0].value.clone(), l.len()), (json!("closed"), 3));
}

#[test]
fn other_value_parses_numbers_bools_and_text() {
    assert_eq!(parse_other("21,5"), Some(json!(21.5)));
    assert_eq!(parse_other("true"), Some(json!(true)));
    assert_eq!(parse_other(" on "), Some(json!("on")));
    assert_eq!(parse_other("  "), None);
}

#[test]
fn entity_change_keeps_an_offered_value_else_resets() {
    let cover = value_input(Lang::En, Some(&ent(Kind::Cover)));
    assert_eq!(after_entity_change(Some(&json!("closed")), &cover), Some(json!("closed")));
    assert_eq!(after_entity_change(Some(&json!(true)), &cover), Some(json!("open")));
}

#[test]
fn upstream_entity_follows_the_feeding_state_trigger() {
    // t1 = trigger.state(a) -> if1 ; t2 = trigger.state(b) -> d1 (delay)
    assert_eq!(upstream_trigger_entity(&two_branches(), "if1"), Some("a".into()));
}

#[test]
fn upstream_entity_is_none_for_mixed_triggers() {
    assert_eq!(upstream_trigger_entity(&state_and_schedule_into_if(), "if1"), None);
    assert_eq!(upstream_trigger_entity(&two_states_into_if(), "if1"), None);
}
```

(`ent(kind)` builds an `EntityView` as the `command_form` tests do; the flow fixtures are `Flow` literals with `Node`/`Edge`.)

- [ ] **Step 2: Run** `cargo test -p oikos-web if_value` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p oikos-web if_value` → PASS.
- [ ] **Step 5: Commit** `feat(web): value choices for the flow If card from core`.

---

### Task 6: `IfValueField` on the If card

**Files:**
- Create: `crates/oikos-web/src/screens/flow_canvas/value_field.rs`; modify `screens/flow_canvas/mod.rs`, `settings.rs` (If branch ~line 888), `i18n/flows.rs`.

**Interfaces:**
- Consumes: Task 5.
- Produces:
  ```rust
  #[component]
  pub fn IfValueField(
      value: Signal<Option<serde_json::Value>>,
      entity: Signal<Option<EntityView>>,
      write: Callback<serde_json::Value>,
  ) -> impl IntoView
  ```
  `List` → `<select class="field">` of `with_current(...)` plus a last "Other value…" (`FlowText::OtherValue`, "Other value…"/"Autre valeur…"); Other swaps to a text box writing `parse_other` (no write on `None`) with a "Back to the list" link (`FlowText::BackToList`, "Back to the list"/"Revenir à la liste"). `Number { unit }` → `<input type="number" step="any">` + unit span, writing `parse_decimal` values. `Text` → text box writing `parse_other`. Label `FlowText::Value` ("Value"/"Valeur").

- [ ] **Step 1: Implement** `IfValueField`.
- [ ] **Step 2: Wire the If card:** replace the `ValueJson` `JsonField` with `IfValueField`; `entity` = the card's own entity, else `upstream_trigger_entity(&draft, &id)`, looked up in `House.entities`. In `entity_write`, after setting `i.entity`, set `i.value = after_entity_change(i.value.as_ref(), &value_input(lang, resolved_new_entity))`. Leave `is_dark` and `IfStash` alone. Delete `FlowText::ValueJson` if unused.
- [ ] **Step 3: Run** all Global Constraints checks → pass.
- [ ] **Step 4: Commit** `feat(web): If card picks a value instead of raw JSON`.

---

### Task 7: Docs and live check

**Files:**
- Modify: `docs/users/flows.md`, `docs/contributors/adding-a-device.md`

- [ ] **Step 1:** `docs/users/flows.md`: the If value is picked from what the device can report ("Other value…" for anything else, a number box for measurements); the Command card picks an Action and its values, with "Advanced (JSON)" for a command typed by hand. Generic wording.
- [ ] **Step 2:** `docs/contributors/adding-a-device.md`: a new kind or type-table entry shows in the flow editor through `oikos_core::vocabulary`; `crates/oikos-core/tests/vocabulary.rs` checks the mapper accepts every listed action.
- [ ] **Step 3:** `cd crates/oikos-web && trunk serve`; check on screen: If on a cover (French words), If with no own entity under a state trigger, If on a temperature sensor (number + °C), Command on a colour light (brightness, colour, effect), Command on a cover (position %), an ambiance device action. Headless Chrome over CDP if the extension times out.
- [ ] **Step 4:** Run all Global Constraints checks → pass.
- [ ] **Step 5: Commit** `docs(flows): value and action pickers`.
