# Flow Switch and Time of day cards

Date: 2026-10-27. Status: approved.

## Intent

The user moves their automations from Node-RED to Oikos by rebuilding them by hand in the
Oikos flow editor. Node-RED gives them two building blocks that Oikos lacks:

- a **switch** node: one value checked against a list of rules, each rule with its own
  output, so one card replaces a chain of `if` cards;
- a **time range** node: "is it between 07:00 and 22:30 right now?", with two outputs.

Today the `if` card has exactly two exits (`true`, `false`), and only `trigger.schedule`
and `is_dark` read the clock. This design adds two new node types, `switch` and
`time.between`. The `if` card stays as it is (the user's decision).

This is not a Node-RED import. Nobody pastes Node-RED JSON into Oikos.

## Goals

- A `switch` node with any number of rules, one exit per rule, and an optional
  "otherwise" exit.
- A `time.between` node with two exits, *inside* and *outside*, that works across
  midnight.
- Both nodes work everywhere a node works today: validation on save, the engine, run
  history and highlighting, the web editor (card library, canvas, parameter editor, card
  titles in EN and FR), MCP writers (through the flow JSON schema), and the docs.

## Non-goals

- Importing Node-RED flows.
- Changing, migrating or removing `if`. Saved flows keep working untouched.
- Days of the week, dates, or sun-relative bounds on `time.between` (my call: `is_dark`
  and `trigger.sun` cover the sun, `trigger.schedule` covers days; this can be added
  later as optional params without breaking anything).
- Text matching rules on `switch` (contains, regex) (my call: the house values are
  states, numbers and fixed words, which `eq`/`ne` cover; can be added later as new ops).
- `is_dark` as a switch rule (my call: a switch checks one value, and darkness is not a
  value of that entity; use `if` with `is_dark` or `time.between`).

## Anti-goals

- A saved flow that uses `if` behaves differently after this change.
- Reordering or deleting a rule in the editor silently moves a wire to a different rule.
- A switch run whose highlighted wires on the canvas don't match the exits it really took.

## Constraints

- PR #76 (value and action pickers for the `if` card, `crates/oikos-web/src/if_value.rs`)
  is merged on `main` (d56e4bec). The switch rule editor reuses those pickers. Work goes
  on a branch from `origin/main`.
- The project is generic: no house-specific names, rooms or hosts in tracked files.
- Unknown keys on nodes and params are kept, as for every node today.
- The web UI is EN and FR. Every new label goes through the i18n tables.

## The `switch` node

### Document shape

```json
{ "id": "temp", "type": "switch", "params": {
    "entity": "sensor_temperature",
    "average": "30m",
    "rules": [
      { "op": "lt", "value": 18 },
      { "op": "between", "value": 18, "to": 22 },
      { "op": "gt", "value": 22 }
    ],
    "otherwise": true,
    "all": false
} }
```

Edges leaving it carry `"port": "1"`, `"2"`, `"3"` (the rule's 1-based position) or
`"port": "otherwise"`.

| Param | Type | Description |
| --- | --- | --- |
| `entity` | string, optional | Same as `if`: the entity read, live. Absent: the entity that triggered the run. |
| `average` | duration, optional | Same as `if`: compare the mean over the window. Every rule's `value` must then be a number. |
| `rules` | array, 1 to 10 entries | The checks, in order. |
| `otherwise` | bool, default `false` | Adds the `otherwise` exit, taken when no rule matched. |
| `all` | bool, default `false` | `false`: only the first matching rule's exit fires (the Node-RED default "stop after first match"). `true`: every matching rule's exit fires, in rule order. |

Each rule is `{ "op", "value", "to"? }`:

- `op` is `eq`, `ne`, `gt`, `lt`, `gte`, `lte` (same meaning as `if`) or `between`.
- `between` matches when `value <= x <= to`, both numbers, both bounds included. `to`
  is required with `between` (and must be at least `value`) and refused with every other op.

### Behaviour

- The value is read once per step. Every rule compares that same value.
- An unknown value (unknown, unavailable, or no reading in the window) matches no rule
  except `ne`, the same typed rule `if` follows. If nothing matches, the run takes
  `otherwise`, or stops there when there is no `otherwise` exit. It never fails because the
  value is unknown (my call: the `missing` param of `if` is not carried over, because
  `otherwise` already plays that role).
- An ordering rule (`gt`, `lt`, `gte`, `lte`, `between`) on a known value that isn't a
  number fails the step, as it does for `if`.
- With `all: true` the successors of every fired exit run depth first, exit after exit in
  rule order, `otherwise` never fires when a rule matched. The run stays one depth-first
  walk, so a `delay` behind exit 1 holds back exit 2 until it ends (my call: no parallel
  branches inside one run; the run model, cancellation and the parked-delay view stay as
  they are).
- The step detail says what was compared and which exits fired, for example
  `21.4 → 2` or `average 30m = 17.2 → 1`, `unknown → otherwise`.

### Validation (on save and on load)

- `rules` holds 1 to 10 entries (my call: ten exits is far more than one card shows well).
- Each rule's `op`, `value` and `to` are checked as above.
- An edge from a switch must carry a port, and the port must be `"1"` to `"<rules count>"`,
  or `"otherwise"` when `otherwise` is true. Same error shapes as today's `UnknownPort`
  and missing-port errors.
- A switch reachable from a `trigger.schedule` or `trigger.sun` must name an `entity`,
  as for `if`.
- On save, the hub checks the `entity` exists (as for `if`).

### Port naming (my call)

Ports are the rule positions (`"1"`, `"2"`, …), like Node-RED's numbered outputs, and
not ids stored on each rule. This keeps the JSON short for MCP writers. In exchange, the
**web editor** owns keeping wires on the right rule. When a rule is moved, its edges move
with it. When a rule is deleted, its edges are removed and the later rules' edges are
renumbered. When `otherwise` is turned off, its edges are removed. The editor shows a
short note when this deletes a wire.

## The `time.between` node

### Document shape

```json
{ "id": "day", "type": "time.between", "params": { "from": "07:00", "to": "22:30" } }
```

Edges leaving it carry `"port": "in"` or `"port": "out"`.

| Param | Type | Description |
| --- | --- | --- |
| `from` | `HH:MM`, 24-hour | Start of the range, included. |
| `to` | `HH:MM`, 24-hour | End of the range, excluded. |

### Behaviour

- It reads the hub clock at the moment the step runs, in the house zone
  (`[flows] timezone`, the same zone schedules use).
- `from` < `to`: inside when `from <= now < to`.
- `from` > `to`: the range crosses midnight. `22:00` to `06:00` is inside from 22:00 until
  05:59.
- `from` == `to` is refused (my call: "always" or "never" is ambiguous; the user doesn't
  need a card for that).
- Daylight saving: it compares wall-clock time, so `07:00` means 07:00 on the clock on
  every day of the year.
- It reads no entity, so it works after any trigger, schedule and sun included.
- The step detail is `14:05 → in` or `23:10 → out`.

### Validation

- `from` and `to` are `HH:MM` with hours 00 to 23 and minutes 00 to 59, and they differ.
- Every edge leaving it carries `in` or `out`.

## Shared engine change

`NodeKind::ports()` returns a fixed `&'static` list today, and `run_node` returns at most one
port. Both have to change:

- the ports of a switch depend on its params (rule count, `otherwise`);
- a switch with `all: true` can fire several exits in one step.

The run view must also say which exits a step took, as data and not inside the `detail`
text. Today the web finds an `if`'s branch by comparing `detail` with the port name, which
already breaks for `if` with `average` (`average 30m = 18.6 → true`). Add to `StepView` an
optional list of the ports the step followed. The web highlight and the "parked on
delay" lookup (`crates/oikos-web/src/flows.rs`) read that list for `if`, `switch` and
`time.between`. Runs recorded before the change have no list, so the web falls back to
today's `detail` match for `if`.

## Web editor

- **Card library:** two new entries in the logic section, next to If. "Switch" (FR
  "Commuter", the user's choice) and "Time of day" (FR "Plage horaire", my call).
- **Canvas:** a switch card shows one exit dot per rule plus one for `otherwise`, each
  labelled with its rule in short form (`< 18`, `18–22`, `> 22`, `otherwise`). The card
  gets taller with the number of exits, so the dots don't crowd. The time card shows two
  dots labelled with the range (`07:00–22:30`) and "outside". Wires snap to the dot they
  were drawn from, like the If dots today (`out_anchor` in `flow_layout.rs`).
- **Parameter editor:** for the switch, the entity picker and the average field as on If,
  then the rule list. Each row is an op select, a value input (the #76 value picker for the
  chosen entity), a second value input for `between`, up and down buttons, and a delete
  button. Below that, "Add rule", an "Otherwise exit" toggle and a "Fire every matching
  rule" toggle. For the time card, two time inputs.
- **Card titles** (`flow_text.rs`), EN and FR. Switch: "Switch on <entity>" with the rules
  as exit labels. Time: "Between 07:00 and 22:30".
- **Drop card on wire** and "delete bridges neighbours": a dropped switch takes the
  wire on its rule 1 exit, a dropped time card on `in`. This is the same as If, which uses
  its first port.
- Phone stays view-only, as today.

## Docs

- `docs/reference/flow-nodes.md`: a `switch` section and a `time.between` section in the
  same shape as `if`. Update the edge rule "an edge leaving an `if` must carry a port" to
  cover the three branching nodes.
- `docs/users/flows.md`: a short "Branch on several values" part and a "Only at certain
  hours" part, with one example each.

## Left to the builder

- Rust shapes: how `ports()` becomes param-dependent (for example returning `Vec<String>` or
  `Cow`), and how `run_node` returns several ports.
- The exact `StepView` field name and how the web falls back for old runs.
- Card height and dot spacing on the canvas, and the exit label truncation.
- Test layout, as long as there are tests for validation errors, the engine's first-match
  and `all` modes, unknown values, midnight crossing, DST days, and the editor's wire
  renumbering on rule delete and move.
- Whether the time parse lives next to `duration.rs` in `oikos-core/src/flow`.
