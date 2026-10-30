# Flow math card: compute a number from the house and use it

Status: design, awaiting review (2026-10-30).

## Why

The user is moving a Jeedom scenario to Oikos: a solar-surplus regulator for the water
heater. Every reading it reads the grid power and the heater power, works out how many
watts the heater should draw (`heater + (-100 - grid)`, clamped to 0..1650 W), smooths
that against the previous target (fast when the change is over 300 W, slow otherwise),
turns watts into a slider percentage through a calibration table (linear interpolation
between measured points) and sends the percentage to the heater. It also has a 72 °C
cut-off and a "force" mode whose output drops 4 % per degree above 40 °C.

A flow cannot do this today. Every card compares a value with a constant or sends a
constant (`crates/oikos-os/src/flows/engine.rs`, `run_node`); nothing computes a number,
nothing carries a value from one card to the next, and nothing remembers a value between
runs except a virtual entity set to a constant.

The user wants this solved generically ("I want to be able to manage everything I can"),
and chose **one math card with a formula** over wiring small arithmetic cards together.

## Goals

- A new flow card, **Math** (`type: "math"`), that evaluates a formula over named inputs
  read from entities and produces one number, the card's **result**.
- The result can be:
  - **saved** into a `number` entity (a device number, a virtual number or a number group)
    by the card itself, which is also how a value is remembered between runs;
  - **used by later cards of the same run**: a `command` sends it, an `if` / `switch`
    compares it, a `notify` prints it.
- Built-in functions enough for regulation and everyday maths, including a
  piecewise-linear lookup table (`interp`).
- The same evaluator on the hub and in the web editor, so the editor checks the formula as
  it is typed and previews the result with the house's current values.
- Available through MCP `set_flow` and documented, so the assistant can build such flows.
- Acceptance: the water-heater scenario above can be written as one flow (appendix).

## Non-goals

- Several results per card, local `let` bindings, loops, user-defined functions, strings
  or dates in formulas. A formula is numbers in, one number out; chain cards for more steps.
- Values that live between runs other than through an entity (no hidden per-flow memory).
- A new trigger kind. Existing triggers (state, schedule, …) start the runs.
- Formulas inside other cards' fields (an `if` value, a command's JSON). Those read a math
  result by name instead.
- Unit conversion or unit tracking; the result has no unit unless it is saved into an
  entity that has one.

## Anti-goals

- A missing or nonsensical reading silently turning into 0 and driving a device. A math
  card that cannot compute a finite number **fails the step**, the run stops there, and
  the run history says why. (For the water heater, the firmware drops power to 0 after
  15 min without a command, which is the safe outcome.)
- A formula the editor accepts and the hub refuses, or the other way round.
- A preview that disagrees with what the hub computes.

## The card

### Parameters

```json
{ "id": "target", "type": "math", "params": {
    "inputs": [
      { "name": "grid",   "entity": "0xa4c1…_power_a_zigbee2mqtt" },
      { "name": "heater", "entity": "0xa4c1…_power_b_zigbee2mqtt", "average": "1m" }
    ],
    "formula": "clamp(heater - 100 - grid, 0, 1650)",
    "name": "desired",
    "save_to": "virtual_heater_target" } }
```

| Param | Type | Description |
| --- | --- | --- |
| `inputs` | list, 0 to 10 | Each `{name, entity, average?}`. `name` is how the formula refers to the entity's value; `average` (same format and limit as the `if` card's, at most `24h`) reads the mean over that window instead of the current value. |
| `formula` | string, 1 to 500 characters | The expression, grammar below. |
| `name` | string | The result's name, used by later cards. Same identifier rules as an input name. |
| `save_to` | string, optional | A `number`-kind entity (device, `virtual_*` or `group_*`) the result is written to with `set_number`. |

Names: `[a-z][a-z0-9_]{0,31}`, not a function name or a keyword (`and`, `or`, `not`,
`trigger`). Input names are unique within the card.

### What a formula can read

- its own inputs, by name;
- the result of any **other** math card of the same flow, by that card's `name` (see
  "Results in the run");
- `trigger`: the run's trigger value, when it is a number (a bool reads as 1 / 0).

An input's value: a number reads as itself; an on/off (bool) reads as 1 / 0. Unknown,
unavailable, text, or an `average` with no numeric reading, fails the step
(`grid is unknown`). An `average` input is the mean on exactly the `if` card's terms (same
readings kept, same history fallback), and the engine keeps readings for it the same way
(`averaged_windows` gains the math inputs).

### Grammar

- Numbers (`12`, `0.5`, `-3`), names, parentheses.
- Arithmetic `+ - * / %`, unary `-`; usual precedence, left-associative.
- Comparisons `< <= > >= == !=` give 1 or 0; `and`, `or`, `not` treat non-zero as true and
  give 1 or 0.
- Functions:
  - `min(a, b, …)`, `max(a, b, …)` (one or more arguments)
  - `abs(x)`, `floor(x)`, `ceil(x)`
  - `round(x)` and `round(x, n)` (n decimals, 0..6; halves away from zero)
  - `clamp(x, low, high)` (refused at save time when both bounds are literals and
    `low > high`; at run time a crossed pair fails the step)
  - `if(condition, then, else)` (only the taken branch is evaluated, so
    `if(b != 0, a / b, 0)` never divides by zero)
  - `interp(x, [x0, x1, …], [y0, y1, …])`: linear interpolation in a lookup table; below
    `x0` gives `y0`, above the last x gives the last y. The two lists are written as
    literals, have the same length, 2 to 50 points, and the x list strictly increases;
    all of this is checked when the flow is saved.
- Whitespace anywhere; no operator needs spaces (names are plain identifiers, unlike the
  notify placeholders, which hold raw entity ids).
- Limits so parsing and evaluation are bounded: 500 characters, nesting depth 32.

Errors at run time fail the step: division or `%` by zero, a result that is NaN or
infinite, an unknown value as above, a math result that has not been computed in this
run (below).

### Results in the run

A run now carries a map of results, `name → number`. A math card that succeeds puts its
result in the map (overwriting an earlier value of the same name in that run, e.g. in a
loop path). Results do not outlive the run.

- `command`: new optional param `value_from: "<result name>"`. It replaces the single
  number of the command: `set_number.value`, `set_temperature.temperature`,
  `set_position.position` (rounded, kept to 0..100) and `turn_on.brightness` (0..255,
  rounded, kept in range). Any other command type with `value_from` is refused at save
  time. The command's literal number stays in the JSON (it is what the web editor shows
  when the option is switched off) and is ignored while `value_from` is set.
- `if` and `switch`: new optional param `result: "<result name>"`, which takes the place
  of `entity` and is refused together with `entity`, `average` or `missing`. The
  comparison is the one they already do on a number. A result missing from the run (the
  card never ran on this path) fails the step; `missing` does not apply, since a computed
  result is never "unknown".
- `notify`: `{name}` in a title or body prints a math result, with `:N` rounding as for
  entity values. A math card's `name` that is also an entity id of the house is refused at
  save time, so the placeholder is never ambiguous.

Save-time checks: a `value_from` / `result` / placeholder name must be the `name` of a math
card in the same flow. Whether that card is on the path before the reader is a run-time
check (the step fails with `no result desired in this run`); a static path check is not
required.

- **One map per run** (my call). `execute` walks the graph depth-first with one stack; the
  map is threaded through it, not cloned per branch. A card after a fork sees results set
  by a sibling branch that ran before it, in edge order. Flows that need a result on a path
  compute it on that path.
- **`if` / `switch` value source** (my call): exactly one of `entity`, `result`, or neither
  (the existing "the trigger's entity" default). `averaged_windows` ignores nodes reading
  a `result`.
- **`trigger` in a formula** (my call): read at run time only; a run whose trigger carries
  no number (schedule, sun, a Manual card without a value, text) fails the step with
  `trigger has no number`. No save-time check against the flow's triggers.
- **Where checks run** (my call). Checks that need only the flow (grammar, names, `interp`
  tables, `value_from` on a command type, `result` with `entity`) live in
  `oikos-core::flow::validate`, shared by the hub and the editor. Checks that need the house
  (input and `save_to` entities exist, `save_to` is a `number` kind, a math `name` is not
  an entity id) go where the hub already validates a flow against the house before saving
  it. The editor gets them through the hub's refusal on save, like today's unknown-entity
  errors; its **Save to** picker only offers number entities anyway.

### Saving the result

With `save_to`, after computing, the card sends `set_number` to that entity:

- rounded to the entity's `step` (`capabilities.step`, else unrounded) and kept inside
  `min` / `max` when the entity announces them; the step detail shows when it was
  clamped;
- the card's step succeeds only when the command is accepted, like a `command` card; a
  refused command fails the step;
- the result put in the run map is the computed value, before rounding to the step. (My
  call: later cards then see the exact number; a card that needs the saved value reads the
  entity.)

Saving into a virtual number and reading it back as an input on the next run is how a
flow remembers a value (the smoothing in the appendix). Both targets exist today: virtual
numbers announce `min` / `max` / `step` and echo their new state when commanded, and a
`number` group fans `set_number` out to its members (`docs/users/groups-and-virtual-entities.md`).
The save waits for the accepted echo like any command, so the next run reads the saved
value. A target that announces no step or bounds is written unrounded and unclamped.

### Run history

The step detail lists the inputs read, then the result and the save, for example:
`grid=-412, heater=820 → desired = 1132` and
`… → slider = 47.6 → group_water_heater_power = 48`. A failure reads like
`grid is unknown` or `division by zero`.

## The web editor

- The card library gets **Math** next to If / Switch.
- Its settings drawer:
  - **Inputs**: rows of name + entity (the generic entity picker the other cards use) +
    optional average window; add / remove rows; a new row proposes a name from the entity
    (`grid`, `heater`…), editable.
  - **Formula**: a text field, checked as you type (unknown name, syntax error, bad
    `interp` table), with a **preview** line computed from the current values of the
    inputs (results of other math cards preview as `?`). An `average` input previews with
    its current value and the line says so ("now, not the 1m average"); the editor does
    not fetch history for it (my call).
  - **Result name**.
  - **Save to** (optional): the entity picker limited to number entities.
- The canvas sentence for the card reads like the others, e.g.
  "desired = clamp(heater - 100 - grid, 0, 1650) → Heater target".
- Command card: for the four commands above, a **Value from result** choice listing the
  flow's math results, replacing the number field when chosen.
- If / Switch cards: the value picker lists the flow's math results alongside entities.
- Notify: the **＋ Value** button lists the flow's math results too.
- EN and FR strings.

## Interfaces

- Core (`oikos-core::flow`): the model (`NodeKind::Math`, `MathParams`, the new optional
  params), the formula parser/evaluator, and save-time validation, shared by the hub and the
  web editor (`oikos-web` already depends on `oikos-core`).
- Hub engine: the run map threaded through `execute` → `run_node`; the math step; the
  `value_from`, `result` and placeholder reads; `average` inputs added to the windows the
  engine keeps readings for (`averaged_windows`).
- MCP: the `set_flow` description gains the math card, `value_from`, `result` and the
  placeholder; the JSON schema follows from the model.
- Docs: `docs/reference/flow-nodes.md` (a `## math` section, the new params on
  `command`, `if`, `switch`, `notify`), `docs/users/flows.md` (a short how-to with the
  water-heater example), `docs/reference/mcp-tools.md` if it lists node types.

## Left to the builder

- How the parser is written (a small hand-written parser in the style of
  `flow/template/parse.rs`, or `minijinja`'s expression compiler with registered functions,
  which `oikos-core` already depends on), as long as the grammar, the limits, the errors
  and the number semantics above hold, identically in the hub and in wasm.
- The exact wording of step details and editor messages (EN/FR).
- Canvas placement and icon of the card.

## Coordination

`feat/flow-value-of` (local, plan not yet executed) adds a `field` param and a "Value of"
menu to `if` / `switch` / `trigger.state`. Both features touch the same params and pickers;
whichever lands second rebases onto the other. `result` and `field` do not combine.

## Appendix: the water-heater scenario as a flow

Entities: grid `0xa4c13898428fae3a_power_a_zigbee2mqtt` (Réseau), heater power
`0xa4c13870d8ba6a60_power_b_zigbee2mqtt` (Chauffe eau), tank `water_heater_tank_temp`,
force flag `virtual_force_heating`, the three power channels `water_heater_power_0/1/2`.
Created with the flow: a number group `group_water_heater_power` over the three channels
(so one save drives all three) and a virtual number `virtual_heater_target` (0..2200 W,
step 1) to remember the smoothed target.

- Trigger: `trigger.state` on the grid power; mode `single`.
- `if` tank > 72 → `command` set_number 0 on the group.
- else `if` force flag on → math
  `force = clamp(if(tank < 40, 45, 45 - (tank - 40) * 4), 0, 60)` save to the group.
- else math `desired = clamp(heater - 100 - grid, 0, 1650)` →
  math `smoothed = if(abs(desired - prev) > 300, 0.1 * prev + 0.9 * desired, 0.6 * prev + 0.4 * desired)`
  (input `prev` = `virtual_heater_target`), save to `virtual_heater_target` →
  math `slider = round(interp(smoothed, [0, 38, 81, 160, 274, 438, 621, 843, 1062, 1270, 1450, 1650, 1947, 2100, 2180, 2200], [0, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 70, 80, 90, 100]))`
  save to the group.

Which Oikos temperature matches each of the scenario's two (Jeedom commands 4811 "cmdTemp"
for force mode and 4812 "tempInside" for the cut-off) is settled with the user when the
flow is built, after this feature lands.
