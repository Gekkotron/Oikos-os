# Flow value pickers — design

Date: 2026-10-27. Revised the same day: the lists come from core, built from discovery.

## Why

In the flow editor, two card sheets ask for raw JSON:

- the **If** card's "Value (JSON)" box, where a user must type `"closed"` (quotes included)
  to test a shutter;
- the **Command** card's "Command (JSON)" box, where a user must type
  `{"type":"set_position","position":40}`.

Someone building their own flow cannot guess these values. The sheet already knows the
entity, so it should offer what that entity can report (If) or accept (Command).

**Success:** a user builds "if the shutter is closed, then set the light to 60 % amber" in
the editor without typing JSON or knowing the stored words.

**Generic (user requirement):** what an entity reports and accepts comes from its discovery
config, through core. The web holds **no list per device type**: it draws what core returns
and only translates words. A device type added to core or to the type table
(`types/table.json`) appears in the editor with no web change.

**Non-goals:** changing the flow model, the engine, the MQTT behaviour of commands, the MCP
tools, the canvas card titles (`flow_text.rs`), or the device cards (`cards/*`).

**Anti-goals:** a flow or ambiance saved before this change must open, display and save
unchanged. The picker must never silently replace a value the list does not contain. The web
must not grow a new per-kind table.

## Core: one description per entity (`oikos_core::vocabulary`)

A new wasm-safe module, next to `command` and `state`, built only from what discovery
recorded (`Kind`, `component`, `Capabilities`, `unit`) and the type table:

```rust
/// What an entity can be told: a command template plus the inputs it takes.
pub struct Action { pub command: Command, pub params: Vec<Param> }

/// One input of an action; `key` is the field inside the command's JSON
/// (`position`, `temperature`, `value`, `brightness`, `color`, `effect`, `code`).
pub enum Param {
    /// Shown value × `factor` = stored value (brightness: shown 1–100 %, factor 2.55, stored 0–255).
    Number { key: String, min: f64, max: f64, step: f64, unit: Option<String>, factor: f64, optional: bool },
    Choice { key: String, options: Vec<String>, optional: bool },
    Colour { key: String },
    Text { key: String, secret: bool, optional: bool },
}

/// What an entity can report, as the flow engine compares it (`Scalar::of_state` / `of_generic`).
pub enum Reports {
    Values(Vec<Scalar>),
    Number { unit: Option<String>, min: Option<f64>, max: Option<f64> },
    Text,
    Nothing,
}

pub fn actions(kind: Kind, component: &str, caps: &Capabilities) -> Vec<Action>;
pub fn reports(kind: Kind, component: &str, caps: &Capabilities, unit: Option<&str>) -> Reports;
```

Rules (each from data core already has):

- **Choices baked into a command become one action each**, as the ambiance form does
  today: one `SelectOption` per `options`, one `SetHvacMode` per `hvac_modes`, one
  `SetFanMode` per `fan_modes`, one `Arm` per `alarm_modes` (plus `Disarm`), one
  `VacuumAction` per `vacuum_actions`, one `SetFanSpeed` per `fan_speeds`, one generic
  `Control` per announced control (OnOff → two actions, `true` and `false`; Choice → one per
  option from `generic.options[field]`; Number → one action with a `Number` param from
  `generic.ranges[field]`; Text → one action with a `Text` param; press → no param).
- **Values a command carries are params:** cover position (when `position`), climate and
  water-heater temperature (`min_temp`/`max_temp`/`temp_step`), number value
  (`min`/`max`/`step`, the entity's unit), light brightness (when `brightness`), colour (when
  `color_modes` holds hs, xy, rgb, rgbw or rgbww), effect (`Choice` of `effects`, when
  non-empty), alarm code (`Text`, `secret`, when `code_arm_required` /
  `code_disarm_required`).
- **What discovery says but `Capabilities` does not record yet** is added to `Capabilities`
  in `discovery/capabilities.rs`. Known case: a cover offers Stop only when `payload_stop` is
  not null, and open/close only when it has a `command_topic`. New fields are serde-default,
  so the API and the Android app keep working.
- **Reports:** Light, Switch, BinarySensor → `Values([true, false])`. Cover → the cover
  state words of `state/kinds/cover.rs` (open, closed, opening, closing, stopped), exposed as
  a `pub const` there and reused. Vacuum → `state/kinds/vacuum.rs` `STATES`. Climate,
  WaterHeater → `hvac_modes`. Select → `options`. AlarmPanel → `disarmed`, `armed_<mode>`
  per `alarm_modes`, then `arming`, `pending`, `triggered` (a `pub const` in
  `state/kinds/alarm.rs`). Number → `Number` with `min`/`max` and unit. Sensor → `Number` when
  it has a unit, else `Text`. Generic → its primary field: OnOff → `[true, false]`, Choice →
  `generic.options[primary]` (else the field's canonical `values`), Number → `Number` with its
  range, Text → `Text`, otherwise `Nothing`. Button → `Nothing`.
- **Agreement guarantee:** a core test runs every action of every recorded fixture entity
  (`crates/oikos-core/tests/fixtures`), with each param at its default, through the command
  mapper and expects no error. A list that offers something the mapper refuses fails CI.

## Web

The web calls `vocabulary::actions` / `vocabulary::reports` with an `EntityView`'s fields. Its
only kind-aware code is wording: `command_label` per `Command` variant and
`i18n::states::state_word` for values (unknown words are prettified).

**Removed:** `flow_picker::commands` (and its alarm fallback, which already disagrees with
core's), `alarm_commands`, `generic_controls`, and `ambiances/action_forms.rs`
`device_commands`. The flow card library, the ambiance form and the Command card all use
`vocabulary::actions`.

### If card — Value field

Replaces the `JsonField` for `ValueJson` in `kind_fields` → `NodeKind::If`. The `is_dark`
branch is untouched. A dropdown (choice A, approved by the user):

- `Values` → a dropdown of the values, labelled with `state_word` (context picked from the
  entity's kind and device class — wording only), with a last entry "Other value…".
- `Number` → a number box with the unit beside it (decimal comma accepted).
- `Text`, `Nothing`, or no entity → a text box.
- A Sensor whose `reports` is `Text` but whose live state is a number gets the number box
  (generic rule on the live value, not per kind).

### Command card — Action + params

Replaces the `JsonField` for `CommandJson`. An "Action" dropdown of the entity's `actions`
(labelled with `command_label`), then one input per `Param` of the chosen action, drawn
from the param type alone: `Number` → number box (shown = stored ÷ `factor`), `Choice` →
dropdown with a "None" entry when optional, `Colour` → named colour dropdown, `Text` → text
box (password box when `secret`). An input writes its `key` into the command's JSON and the
result is parsed back into `Command`; clearing an optional input removes the key. A last
Action entry "Advanced (JSON)" shows today's JSON box.

The ambiance device action uses the same component, without "Advanced".

## Settled calls (Claude's, not discussed with the user)

1. **Trigger entity for the If card:** walk the card's incoming edges back to the triggers
   that feed it. `Some(entity)` only when every trigger reached is a `trigger.state` on the
   same entity; anything else (several entities, a schedule, sun or signal trigger) → text
   box.
2. **Changing the If card's entity:** keep the value when the new entity's list offers it (or
   the new field is a number box and the value is a number); otherwise reset to the first
   value. Opening a card never rewrites its value.
3. **Unlisted current value or command on open:** shown as the selected top entry, never
   replaced. The Command card switches to the new entity's first action only when the user
   changes entity and the new one lacks the current action.
4. **Colour palette:** stores `Hs`. White (h 0, s 0), red 0, orange 30, yellow 55, green
   120, cyan 180, blue 225, purple 270, magenta 300, pink 335 (s 100), so each reads back
   through `flow_text::colour_name` as its own name. Another stored colour shows as
   "Custom". The palette is wording/UI, not a device list.
5. **Ambiances share the component**, so they gain fan modes, fan speeds, colour, effect and
   alarm code. The hub accepts any `Command` in an ambiance `device` action.

## Fallback and robustness

- No entity resolved, or the entity list still loading: text box for If; the Advanced JSON
  box for Command.
- Undo: every write goes through `Form::update` (one change = one undo step).

## Testing

- Core: `actions`/`reports` per kind and per type-table shape, and the fixture round-trip
  through the command mapper.
- Web (native): applying a param to a command's JSON and back, "Other value…" parsing, the
  keep-or-reset rule, the upstream trigger walk, an unlisted current value kept first.
- The existing ambiance and flow-picker tests keep passing (updated where the alarm
  fallback now follows core).
- A visual check in the running web UI on a cover, a light, a climate and a numeric sensor.

## Docs

Update `docs/users/flows.md` (If value and Command card). Add a short note in
`docs/contributors/adding-a-device.md`: a new kind or type-table entry must be covered by
`vocabulary` to show in the flow editor.

## Follow-ups (out of scope, found in the audit)

- `groups.rs` `targets()` re-derives which members accept a command; it could ask core.
- Device cards (`cards/*`) draw their buttons per kind; they could read `vocabulary::actions`.

## Left to the builder

Module layout inside `oikos_core::vocabulary`; exact CSS (reuse `.field` /
`node-editor__field`); French wording of new labels (follow `i18n/flows.rs`); order of
entries within a list when not given above.
