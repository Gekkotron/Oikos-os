# Flow value pickers — design

Date: 2026-10-27. Scope: web only (`crates/oikos-web`). No hub, core model or API change.

## Why

In the flow editor, two card sheets ask for raw JSON:

- the **If** card's "Value (JSON)" box, where a user must type `"closed"` (quotes included)
  to test a shutter;
- the **Command** card's "Command (JSON)" box, where a user must type
  `{"type":"set_position","position":40}`.

Someone building their own flow cannot guess these values. The sheet already knows the
entity, so it should offer what that entity can be (If) or accept (Command).

**Success:** a user builds "if the shutter is closed, then set the light to 60 % amber" in
the editor without typing JSON or knowing the stored words.

**Non-goals:** changing the flow model, the engine, the MCP tools, or how the canvas card
titles read (`flow_text.rs` already writes sentences).

**Anti-goals:** a flow saved before this change must open, display and save unchanged. The
picker must never silently replace a value the list does not contain.

## If card — Value field

Replaces the `JsonField` for `ValueJson` in `kind_fields` → `NodeKind::If`
(`screens/flow_canvas/settings.rs`). The `is_dark` branch is untouched.

**Which entity drives the list:** the card's own `entity` when set, else the flow's trigger
entity (the "trigger value" default). The trigger entity is found the same way the Notify
sheet finds it (`trigger_entity` in `notify_fields`). When neither resolves to a known entity,
the field is a plain text box (see Fallback).

**Shape per entity (choice A, a dropdown, approved by the user):**

| Entity | Field | Stored value |
|---|---|---|
| Light, Switch, BinarySensor | dropdown On / Off (binary sensors worded by device class, e.g. Open/Closed for a door, Detected/Clear for motion — `state_word` with `StateCtx::BinarySensor`) | `true` / `false` |
| Cover | dropdown open, closed, opening, closing, stopped | the word as a string |
| Climate | dropdown of `capabilities.hvac_modes` | string |
| WaterHeater | dropdown of its modes (`hvac_modes` capability, same vocabulary) | string |
| Select | dropdown of `capabilities.options` | string |
| AlarmPanel | dropdown: disarmed, armed_<mode> per announced mode (home/away/night when none announced), arming, pending, triggered | string |
| Vacuum | dropdown docked, cleaning, returning, paused, idle, error | string |
| Generic | the primary field of its type (`types::state::primary` / `TypeDef`): OnOff → On/Off dropdown (bool); Choice → dropdown of its options from `capabilities.generic.options`; Number → number box; Text → text box | per shape |
| Sensor / Number (numeric) | number box with the entity's unit shown beside it; accepts a decimal comma (`parse_decimal`) | number |
| Sensor with a non-numeric state | text box | string |

The stored value is what the engine compares (`Scalar::of_state` / `Scalar::of_generic`), so
the list must produce exactly those types: booleans for on/off kinds, strings for words,
numbers for measurements.

**Labels:** translated with the existing `i18n::states::state_word` (cover, lock, alarm,
hvac, vacuum, binary sensor vocabularies); unknown words fall back to its prettifier.

**"Other value…":** the last dropdown entry. Choosing it turns the field into a text box (a
value typed there is stored as a number when it parses as one, `true`/`false` as booleans,
anything else as a string). Builder's call: whether "Other" shows a small "back to list" link.

**Existing values not in the list:** the dropdown shows the current value as a selected
entry at the top (labelled with its translated word), never a different entry.

**Operator interplay:** with `gt/lt/gte/lte` on a numeric entity, the number box is shown
(already the case for numeric entities). With an ordering operator on a word entity the
dropdown stays; the hub's validator already reports the mismatch on save.

## Command card — Action + values

Replaces the `JsonField` for `CommandJson` in `kind_fields` → `NodeKind::Command`.

**Reuse:** the ambiance editor already has this form for its `device` action
(`ambiances/action_forms.rs`: `device_commands`, `command_key`, `command_label`,
`value_spec`, `with_value`, `DeviceForm`). Lift the pure helpers into a shared module (e.g.
`crate::command_form`) used by both the ambiance form and the Command card, so both
editors offer the same commands. The ambiance form keeps its look and behaviour.

**Action dropdown:** built from the entity (`device_commands`, extended):

| Entity | Actions |
|---|---|
| Light / Switch | Turn on, Turn off, Toggle |
| Cover | Open, Close, Stop, Set position (when `position`) |
| Climate | Set temperature, one "Mode: <mode>" per hvac mode, one "Fan: <mode>" per `fan_modes` (new) |
| Number | Set value |
| Select | one "Choose <option>" per option |
| Button | Press |
| AlarmPanel | Disarm, Arm <mode> per announced mode |
| Vacuum | one entry per `vacuum_actions`, one "Fan speed: <speed>" per `fan_speeds` (new) |
| Generic | one entry per announced control (`generic_controls`) |

**Value fields, only when the action needs them:**

- Set position: number 0–100 %.
- Set temperature: number with the entity's min/max/step, °C.
- Set value (Number): number with the entity's min/max/step and unit.
- Turn on (light): optional brightness % (stored 0–255, as `value_spec` does today) when the
  light has `brightness`; optional colour when it has a colour mode (hs/xy/rgb…), picked from
  a small palette of named colours matching `flow_text::colour_name` (white, red, orange,
  yellow, green, cyan, blue, purple, magenta, pink); optional effect dropdown from
  `capabilities.effects` when non-empty. Each optional field has a "none" entry that clears it.
- Arm / Disarm: optional code text field, shown when the panel's `code_arm_required` /
  `code_disarm_required` is set.
- Generic control: by its shape — OnOff → On/Off dropdown, Number → number box, Choice →
  dropdown of the field's options, Text → text box, press → nothing.

**"Advanced (JSON)":** the last Action entry. It shows today's JSON box with the current
command. A flow whose saved command the form cannot express (unknown shape, or a command
the entity doesn't list) opens on this entry, so nothing is lost or rewritten.

**Changing entity:** when the new entity does not offer the current action, the card switches
to that entity's first action (same rule as `DeviceForm::pick`). When the card has no entity
yet, only the entity field shows.

## Fallback and robustness

- No entity resolved (empty If entity and no state trigger, unknown id, or entity list still
  loading): plain text box for If; JSON box for Command.
- Undo: every field write goes through `Form::update`, so one change = one undo step, as now.
- Card titles on the canvas already follow the stored value; no change needed.

## Testing

- Pure helpers (choice lists per kind, value parsing for "Other", command lists and
  `with_value` round-trips, "keeps an unknown current value") are unit-tested natively like
  `flow_picker.rs` and `action_forms.rs` tests.
- The ambiance form's existing tests keep passing after the helpers move.
- A visual check in the running web UI on a cover, a light, a climate and a numeric sensor.

## Docs

Update `docs/users/flows.md` where it describes the If value and the Command card JSON.

## Settled after the builder check

Facts checked in the code:

- Every command the form writes already exists in `oikos_core::model::Command` (`TurnOn {
  brightness, color: Hs, effect }`, `SetFanMode`, `SetFanSpeed`, `VacuumAction`, `Arm/Disarm
  { code }`, `Control { control, value }`). No core task.
- `CommandParams.command` is the typed `Command`, so a saved command always parses; extra JSON
  keys are not kept by serde today. "Cannot express" therefore only means "not among the
  actions offered for this entity".
- Switches and binary sensors reach the engine as `StateValue::OnOff`, lights as `Light`, both
  compared as booleans: picking the shape from `Kind` is correct for those kinds.

Calls made by Claude (not discussed with the user):

1. **Trigger entity for the If card:** walk the card's incoming edges back to the
   `trigger.state` that feeds it (the value the engine actually compares). Several state
   triggers upstream with different entities, or only a signal/schedule/sun trigger
   upstream: no entity resolved → text box. A `trigger.signal` value is a signal name, so
   it is a text box too.
2. **Sensor shape:** a Sensor (and a Generic with no type def) picks number vs text from its
   live `StateValue` (`Number` → number box, else text box); with no state yet, number box
   when it has a unit, else text box.
3. **Changing the If card's entity:** keep the value when the new entity's list offers it (or
   the new field is a number box and the value is a number); otherwise reset to the new
   list's first entry, like the Command card. Opening a card never rewrites its value.
4. **An unlisted command on open:** shown as the selected top entry of the Action list (the
   `DeviceForm` rule), with its value fields when it has some. "Advanced (JSON)" is only for
   editing by hand.
5. **Colour palette:** stores `Hs` (what `TurnOn.color` holds). Swatches: white (h 0, s 0),
   red 0, orange 30, yellow 55, green 120, cyan 180, blue 225, purple 270, magenta 300,
   pink 335, all s 100, so each reads back through `colour_name` as its own name. A stored
   colour that is not exactly a swatch shows as a "Custom" entry kept selected. The colour
   field shows when `color_modes` holds any of hs, xy, rgb, rgbw, rgbww.
6. **Ambiances share it:** the action list, the value fields and their helpers move into one
   shared module/component, and the ambiance `DeviceForm` uses it too, so ambiances gain the
   same new actions (fan mode, fan speed, light colour/effect, alarm code). The hub already
   accepts any `Command` in an ambiance `device` action.
7. A number box with no unit shows no unit.

## Left to the builder

Module name and layout for the shared helpers; exact CSS (reuse `.field` / `node-editor__field`
styles); the precise French wording of new labels (follow `i18n/flows.rs` conventions); the
order of entries within a dropdown when not given above.
