# Flows: "Value of" — every value a device reports

Date: 2026-10-30. Status: design approved in chat, pending the written review.

## Why

A flow can only compare an entity's **headline value**: on/off, a measurement, or a state word
(`Scalar::of_state`). Everything else a device reports is out of reach:

- The Roborock vacuum is one entity. Its battery (62 %) and fan speed live inside its state
  (`VacuumState { state, battery, fan_speed }`), so an If card on it only offers Cleaning /
  Docked / Paused … The user wanted "battery ≤ 55 %" and could not build it.
- The same is true of a light's brightness, a cover's position, a climate's current and target
  temperature, a water heater's current and target, and a generic entity's non-primary fields.
- Zigbee devices expose each value as its own entity, but battery, link quality and the like are
  `category: diagnostic`, and the flow card search (`flow_picker.rs`) leaves diagnostic entities
  out, so they are hard to reach too.

The user's words: "each device needs to expose all its fields", "I want access to diagnostic
values".

## Goal

In a flow, any value a device reports can be compared or can fire a trigger, through one menu.

## The design (decided with the user)

### "Value of" menu

The If card, the Switch card and the When (state trigger) card get a **"Value of"** menu right
under their entity. It lists every value the **device** reports:

1. the entity's own headline value — the default, labelled as today ("State", "Temperature" …);
2. the entity's other fields (vacuum: Battery, Fan speed; light: Brightness …);
3. the device's other entities, **diagnostic ones included** (smoke detector: Smoke, Battery,
   Battery low, Link quality …).

Each entry shows its current value, e.g. "Battery (62 %)". Choosing an entry changes what the
card reads; the comparison input follows the chosen value's type (number with unit, list of
choices, on/off, text), exactly as the card does today for a whole entity.

Picking another entity in the card resets "Value of" to that entity's headline value.

### The user's decisions

- Option B (pick the device, then which value) over a search list with one line per value.
- The menu lists the device's values wherever they are stored: fields inside one state **and**
  sibling entities, diagnostic included.
- When (trigger) cards get the same menu as If cards.

## How it is stored (builder's design, my calls)

- **A sibling entity** is stored as today: choosing "Battery" on the smoke detector sets the
  card's `entity` to `0x…_battery_zigbee2mqtt`. The menu is a UI convenience; no new param. The
  card shows it as the device + value ("smoke · Battery").
- **A field inside a state** adds one optional param, `field`, to `if`, `switch` and
  `trigger.state`: e.g. `{"entity": "roborock_…", "field": "battery", "op": "lte", "value": 55}`.
  Absent means the headline value, so every existing flow keeps its meaning and its JSON.
- The fields per kind (names as in `oikos_core::model::state`):
  - vacuum: `battery` (number, %), `fan_speed` (text; choices = `capabilities.fan_speeds`)
  - light: `brightness` (number), `color_temp` (number), `effect` (text; choices = `capabilities.effects`)
  - cover: `position` (number, %)
  - climate: `current`, `target` (numbers, the entity's unit), `fan_mode` (choices = `capabilities.fan_modes`), `action` (text)
  - water heater: `current`, `target` (numbers)
  - generic: each field of its type definition other than the primary one
  - every other kind: no fields (the menu shows only the headline and the sibling entities)
- **Brightness unit:** compared in percent, the way the light card shows it, so "brightness ≥ 50"
  means 50 %. The hub converts from the 0–255 it stores. (My call; one rule shared by the engine
  and the card.)
- **Validation:** a `field` the entity's kind (or generic type) does not report makes the flow
  invalid with a clear reason, like an unknown entity does today. `field` without `entity`
  (comparing the trigger value) is invalid.
- **Trigger semantics with `field`:** `trigger.state` fires when that field's value changes (a
  state message that leaves it unchanged does not fire), `from` / `to` compare that field, and
  the run's trigger value is the field's value — so a following If without an entity compares
  the battery, not the vacuum's state.
- **Change tracking per value:** the engine remembers the last value per (entity, field), not
  per entity. Two flows on the same entity — one on its state, one on its battery — fire
  independently: a message that changes only the battery fires only the battery trigger. The
  headline trigger keeps today's behaviour exactly. (My call.)
- **Brightness rounding:** one shared rule, copied from the light card's existing percent
  conversion, used by the engine and the card alike. (My call.)
- **Changing the entity** clears `field`, the comparison value, `to` and `from`, as the cards
  already clear the value today. (My call.)
- **Missing value:** an absent field (e.g. battery not reported yet) is a missing value and
  follows the card's existing `missing` rule.
- **Average:** "average over a window" reads the history database, which keeps one number per
  entity (`history::num_of`), not one per field. So with a `field`, average is offered only
  when that field is the entity's recorded number (climate `current`, water heater `current`,
  cover `position`; not light brightness, whose recorded number is on/off); otherwise the average option is hidden in the card and a flow sent with
  both is invalid. Sibling entities (Zigbee battery …) keep average as today. (My call.)

## Everything that reads a flow follows

- **Engine** (`oikos-os` flow engine): reads the field for If / Switch, fires and carries the
  field for `trigger.state`.
- **Validation** (`oikos-core/src/flow/validate.rs`).
- **Web flow editor**: the "Value of" menu on the three cards; the card titles and the canvas /
  list sentences (`flow_text.rs`) name the value ("Roborock · Battery ≤ 55 %").
- **Flow card search** (`flow_picker.rs`): unchanged — diagnostic entities stay out of the plain
  search; they are reached through "Value of". (My call, so the list stays short.)
- **MCP**: `set_flow` takes `field` (schemars picks it up); the server instructions' paragraph on
  `if` / `trigger.state` mentions it.
- **Docs**: `docs/users/flows.md` explains "Value of" with the battery example.

## Non-goals

- No new entities: the hub does not split a vacuum's battery into its own entity.
- No per-field history (average stays limited as above).
- Raw MQTT `attributes` that are not typed fields are not listed (empty on this house today).
- Command cards (actions) are unchanged: they already set fields through their own controls.
- The Android app does not edit flows' `field`.

## Anti-goals

- Existing flows must not change meaning or JSON.
- The plain flow card search must not fill up with diagnostic lines.
- No value is offered in the menu that the engine cannot read.

## Left to the builder

- Where the field list lives (a core helper next to `vocabulary::reports`, so engine, validation
  and web share one source).
- The menu's exact control (a select is fine) and the wording of field labels in FR/EN.
- Test layout, following the crates' existing patterns.
