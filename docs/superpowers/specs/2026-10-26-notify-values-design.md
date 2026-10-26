# Notify values: live values in a flow's push text

Status: design, awaiting review (2026-10-26).

## Why

A flow's `notify` card sends its `title` and `body` exactly as typed
(`crates/oikos-os/src/flows/engine.rs`, `NodeKind::Notify`). There is no way to say
"Rain today: 3 mm, now 10.1 °C": the text cannot read an entity. The user wants the
morning-weather kind of push: millimetres of rain, outdoor temperature, the condition, a
min/max spread, rounded and in their language.

## Goals

- `{…}` placeholders in a notify card's `title` and `body`, filled when the step runs.
- A placeholder reads an entity (any entity, not only weather) or the trigger value.
- Rounding (`:N` decimals), simple maths (`+ - * /`, parentheses, number literals),
  units carried automatically.
- Text values translated into the language of the web app the flow was saved from (EN/FR).
- In the web editor: a **＋ Value** button under the title and the body fields opens the
  generic entity picker and inserts the placeholder at the cursor; a preview line shows the
  message filled with the house's current values.
- The same engine runs on the hub and in the web preview, so the preview is what the phone
  gets.

## Non-goals

- Placeholders anywhere but the notify card's `title` and `body` (not `command` values,
  not the MCP `send_notification` tool, not `image_url`).
- Functions (`round()`, `max()`), comparisons or conditionals inside braces. Branching stays
  the job of `if` cards.
- Unit conversion (°C → °F), date/time placeholders, entity attributes (only the headline
  value `Scalar::of_state` already gives).
- A house-wide language setting on the hub.

## Anti-goals

- A push that is not sent because a value is missing. A run-time problem shows `?` in the
  text; the push still goes out.
- A preview that disagrees with the phone.
- Users typing entity ids by hand as the only way in.

## Syntax (decided with the user, details are my call where marked)

| Template | Sent (lang `en`) | Sent (lang `fr`) |
|---|---|---|
| `{weather_temperature}` | `10.1 °C` | `10,1 °C` |
| `{weather_temperature:0}` | `10 °C` | `10 °C` |
| `{weather_today_max - weather_today_min:1}` | `5.4 °C` | `5,4 °C` |
| `{weather_tomorrow_precipitation * 2}` | `0 mm` | `0 mm` |
| `{weather_condition}` | `Cloudy` | `Nuageux` |
| `{trigger}` | the trigger value | the trigger value |
| `{{` / `}}` | a literal `{` / `}` | same |

- **Operand**: an entity id, the word `trigger`, or a number literal (`2`, `0.5`, `-3`;
  always `.` as the decimal mark in the template). Live entity ids are not only
  `[a-z0-9_.]`: the house has ids with `-`, `,` and accents
  (`0x…_c,28-0a-18-…_zigbee2mqtt`, `double_take_rémi`). So an operand is any run of
  characters other than whitespace, `{`, `}`, `(`, `)`; a run that parses as a number is a
  number literal, else `trigger`, else an entity id. `trigger` is a reserved word; no live
  entity is named `trigger`. *(my call)*
- **Expression**: operands with `+ - * /` and parentheses; usual precedence. **Operators
  are separated by spaces** (`{a - b}`), because `a-b` is a valid entity id. There is no
  unary minus on an entity (write `0 - a`); `-3` is a literal. *(my call)*
- **`:N`** is recognised only as a final `:` plus digits after the last operand or `)`; no
  live entity id contains `:`. *(my call)*
- **`:N`**, N in 0..=6: exactly N decimals, half away from zero.
  *(my call)* Without `:N`, a number shows at most 2 decimals with trailing zeros dropped
  (`10.1`, `5.4`, not `5.3999999`). *(my call)*
- **Decimal mark** follows the language: `.` for `en`, `,` for `fr`. *(my call, the web
  app already does this)*
- **Units**: an entity operand carries its entity's `unit`. `+`/`-` keep the unit when both
  sides share it or one side is a plain number; `*`/`/` by a plain number keep it; any
  other mix drops it. A unit is written after a non-breaking space: `10.1 °C`, and `%`
  sticks to the number (`77 %` in `fr`, `77%` in `en`). *(my call)*
- **Text and on/off values** are allowed only as a bare operand (`{weather_condition}`).
  Maths on a text value gives `?`. *(my call)*
- **Where values come from**: an entity's value is `Scalar::of_state` of its state (and
  `Scalar::of_generic` with the type table for generic entities, on both hub and web so the
  preview matches); its unit and device class are the `unit` and `device_class` of the
  entity (`crates/oikos-core/src/model/entity.rs`), already on the hub registry and the
  web `House`. The lookup is built the same way on both sides. *(my call)*
- **`{trigger}`**: for a `trigger.state` run, the value the trigger carried, with the
  triggering entity's unit and translation (as `{that_entity}` would print it, but the
  value at trigger time). For a `trigger.signal` run, the signal name, tidied like other
  text. In the web preview, `{trigger}` shows the current value of the flow's first
  `trigger.state` entity, or `?` when there is none. This is the one place the preview can
  differ from the phone, by nature. *(my call)*
- **`?`**: the value is unknown, the entity is gone at run time, `trigger` in a run with no
  trigger (schedule, sun, manual), a division by zero, or maths on a non-number.

### Translation

Translated, keyed on the raw state word, in both languages:

- Weather conditions (`clear`, `partly_cloudy`, `cloudy`, `fog`, `drizzle`, `rain`,
  `freezing_rain`, `snow`, `showers`, `thunderstorm`), with the same words as the web
  app's `condition_label` (`crates/oikos-web/src/i18n/weather.rs`).
- On/off: `On`/`Off`, `Allumé`/`Éteint`. Binary sensors with a door/window/opening/garage
  device class say `Open`/`Closed`, `Ouvert`/`Fermé`. Motion/occupancy/presence say
  `Detected`/`Clear`, `Détecté`/`Rien`. *(my call)*
- Cover states (`open`, `closed`, `opening`, `closing`, `stopped`), alarm states
  (`disarmed`, `armed_home`, `armed_away`, `armed_night`, `arming`, `pending`,
  `triggered`), climate modes (`off`, `heat`, `cool`, `auto`, `dry`, `fan_only`,
  `heat_cool`).
- Any other text: underscores become spaces, first letter upper-cased (`eco_mode` →
  `Eco mode`). *(my call)*

Where the web app already has these words, it should use the shared table instead of a copy,
as long as that stays cheap. Otherwise keeping both is the builder's call.

### Language

`NotifyParams` gains an optional `lang` (`"en"` | `"fr"`; absent means `en`). The web
editor writes the web app's current language into the `lang` of **every** notify node of
the draft when the flow is saved, so the push follows the language the user last saved in.
*(my call: every node on each save, not only edited ones)* MCP `set_flow` may pass `lang`,
and the MCP instructions tell the assistant to pass the language the user writes in, so a
flow written by the assistant for a French speaker is in French. *(my call)* An unknown
`lang` is refused at save.

## Validation

- **Hub-independent** (`oikos-core` `validate`): a template that does not parse (an
  unclosed `{`, a stray `}`, a bad operator, `:N` out of range, an empty `{}`) is refused
  with `BadNotify { reason }` naming the field and the problem.
- **House check** (`check_house` in `crates/oikos-os/src/flows/hub.rs`): every entity id in
  a placeholder must exist, like the entity of a `command` card. `trigger` is always
  accepted.
- An existing flow with literal braces in its text would become invalid on load. Checked on
  the live house (2026-10-26): the only braces are `{weather_temperature}` in the body of
  one notify card, which is valid and starts working. No migration needed.

## Engine (hub)

- The template parser, the evaluator, the number formatting and the translation table live
  in `oikos-core` (new module under `flow/`), with no new dependency, so the hub and the
  wasm web app share them. The evaluator reads values through a caller-supplied lookup
  (id → scalar, unit, and device class for binary sensors), so it knows nothing about the
  registry or the web `House`.
- `NodeKind::Notify` in the engine renders `title` and `body` before building the alert,
  then sends as today.
- The run step text gains the rendered title so the Runs drawer shows what was sent
  (`sent <id>: <title>`). The rendered body is left out of the step text. *(my call)*

## Web editor

- Under the notify card's **Title** and **Body** fields
  (`notify_fields` in `crates/oikos-web/src/screens/flow_canvas/settings.rs`): a
  **＋ Value** button. It opens the generic entity picker (the same popup as
  `EntityPickField`: rooms | entities on a computer, full-screen on a phone) over every
  entity that has a value, with a pinned **Trigger value** row. Picking inserts `{id}` (or
  `{trigger}`) at the field's last caret position, or at the end when the field never had
  focus, then gives the field focus back with the caret after the placeholder.
- The fields remain plain text inputs; the placeholder is editable text. The user adds
  `:0` or `- …` by hand.
- Under the body: a **Preview** line rendering title and body with the house's live values
  and the web app's language, updating as values and text change. A parse error shows
  there instead, in the error colour, with the same reason the hub would give.
- Canvas card title: `Notify "…"` shows placeholders with the entity's display name instead
  of its id (`{Today's precipitation}`) *(my call)*.
- All new text exists in EN and FR, through the flow i18n tables.

## Docs and assistant

- `docs/users/flows.md`: a short "Values in a notification" section with the table above.
- The `oikos` MCP server instructions (`crates/oikos-os/src/mcp/mod.rs`) and the `set_flow`
  schema docs on `NotifyParams` explain the braces, `:N`, maths, `{trigger}` and `lang`.

## Testing

- `oikos-core` unit tests: parsing (good and bad, ids with `-`, `,` and accents, `a-b` as
  one id vs `a - b`), precedence, `:N` rounding, default
  formatting, decimal mark per language, unit rules, `?` cases, `{{`/`}}`, translation
  lookup and the fallback.
- `oikos-core` validate tests for the refused templates.
- Hub test (`crates/oikos-os/tests/flows.rs`): a flow whose notify names a missing entity is
  refused; a run sends the rendered text (weather-style sensors with units, FR).
- Web: the existing flow_text tests extended for the canvas title; preview rendering via the
  shared engine needs no separate logic test.

## Left to the builder

- Parser shape (hand-written recursive descent is expected; `minijinja`, already a
  dependency, is not the user-facing syntax).
- Exact module and type names, where the lookup trait lives, how the caret position is
  remembered in the Leptos input.
- Branch: one branch from `origin/main`, one PR.
