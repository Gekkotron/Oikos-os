# Entity picker — design

Date: 2026-10-25 · Status: approved shape, awaiting review · Scope: `crates/oikos-web` only

## Why

Every web screen that asks for an entity uses a plain browser `<select>` holding every
entity in the house, sorted by `list_label` alone. On a real house this means hundreds of
rows and no search. Many rows also read the same: seven "Backlight mode · Switch" rows from
seven different plugs, with no device and no room to tell them apart. The trigger was the
climate settings' **Outdoor temperature** field: the weather module's temperature is in
the list, but nobody can find it. The flow card settings are worse still: the user has to
type a raw `entity.id` by hand.

The goal is one shared, good-looking picker that makes any entity findable in a few
keystrokes. Each row shows enough (device, room, live value) to tell look-alikes apart.

## Goals

- One shared picker component replaces every entity choice in the web UI (list below).
- Search across entity name, device name, room name, kind words (EN + FR) and id. Matching
  ignores case and accents, using `fold_accents`, the same matching as the flow card library.
- Each row shows: an icon, the entity name, then the device name on a second line, the
  live value, and how long ago it was updated.
- Opens pre-filtered to what the field needs (a thermometer for Outdoor temperature). One
  chip switches to every entity.
- Layout follows the screen width (the user's choice):
  - **Computer (> 640 px):** a centred popup. Rooms are listed in a left column, each with
    a count; entities fill the right.
  - **Phone (≤ 640 px):** a full-screen sheet. One scrolling list grouped under room
    headings, with a back arrow in the header.
- Single-pick and multi-pick. In a multi-pick field the popup stays open, and the user
  ticks several entities before closing it (the user's choice).
- The closed field shows the chosen entity as a full row: icon, name, device, room, value.
  It does not show a bare label.

## Non-goals

- No hub or API change. Everything the picker needs is already in `EntityView`: `name`,
  `device_name`, `area`, `kind`, `device_class`, `unit`, `category`, `is_primary()`,
  `state.value` and `state.last_update`.
- No change to the Android app.
- Room pickers (`<select>` of areas) stay as they are; this is about entities.
- The flow canvas "＋Add" card library and the dashboard widget library panel keep their
  own UI. They are libraries of cards, not entity fields.
- No favourites or recently-used list in v1.

## Anti-goals (would be a failure even if it works)

- The list jumps around while it is open because a value ticked. Rows keep their order;
  only the value text updates.
- Opening the picker feels slower than the old dropdown on a house with ~1 000 entities.
- A field whose stored entity has vanished from the house loses its value or hides it.
  The old selects kept an unknown id visible, and the picker must too.
- On a phone, a popup that doesn't fit the screen or can't be closed.
- Any house-specific name (rooms, devices, hosts) in code, tests or docs. Use generic
  fixtures.

## Where it goes (every entity choice in the web UI)

| Screen | Today | Mode | Pre-filter (the "suggested" chip) | Pinned first row |
|---|---|---|---|---|
| Climate settings · Outdoor temperature | `EntityField` select | single | thermometers (`Sensor` + `device_class = temperature`) | None |
| Climate settings · Presence | `EntityListField` select + chips | multi | entities that report presence: `binary_sensor` presence/occupancy/motion and trackers (`is_tracker`) *(my call)* | — |
| Climate settings · Holiday | `EntityField` select | single | on/off entities: switches, binary sensors, virtual switches *(my call)* | None |
| Room panel + home settings · Room thermometer | `ThermometerField` (select grouped by room) | single | thermometers | Automatic (+ what it resolves to now) |
| Home settings · Mode selector | select of `Select` entities | single | `Select` kind, hub virtuals first | None |
| Alarm settings · sirens / remotes / trackers | `AddPicker` selects | multi | `is_siren` / `is_remote` / `is_tracker`, as today | — |
| Flow card settings · `trigger.state`, `if` | free text `entity.id` | single | anything with a state | — (`if`: None) |
| Flow card settings · `trigger.signal` | free text `entity.id` | single | `device_automation.*`, `event.*`, `tag.*` | — |
| Flow card settings · `command` | free text `entity.id` | single | entities that take a command (`flow_picker::commands` non-empty) | — |
| Ambiance device action | search box + result list | single | primary, non-group, commandable (as today) | — |
| Device sheet · Energy counter | select of counters | single | `EnergyMeters::counters` | Automatic |
| Group wizard · members | inline searchable checklist by room | multi, inline | the group's kind (as today) | — |

The group wizard already has an inline searchable list. It keeps its inline place in the
wizard step, but uses the picker's list body (the same rows, search and room grouping)
instead of its own `members_by_room` rendering. *(My call: the user asked for every
screen, and a popup inside a wizard step would be a popup over a sheet.)*

Every existing filter keeps its current meaning. The picker only changes how the choice is
shown and found.

## Behaviour

**Opening.** Tapping the closed field opens the picker with the search box focused. The
"suggested" chip (named after the field's need: *Températures*, *Sirènes*, *Télécommandes*
…) is on. The **All** chip switches to every entity. Diagnostic, config, disabled-by-default,
folded and hidden entities (`!is_primary()`) only appear when the **+ Diagnostic** chip is
on, or when one of them is already the current value. A field with no sensible
pre-filter opens on All, with no suggested chip.

**Search.** It filters the current chip's set as the user types. A query matches when its
folded form is a substring of any of: the list label, `name`, `device_name`, the room name,
`kind_search_words` (both languages) or the id. An empty result shows "No match" plus a
one-tap "Search all entities" when the suggested chip is on.

**Grouping and order.** Rooms come in the hub's room order. Entities with no room sit under
"No room", last, which is consistent with `group_by_room`. Inside a room, rows are
sorted by label, folded. On a computer the left column lists "All rooms" and then each room
that has matches, each with its match count. Picking a room narrows the right side, and
typing a query resets the room back to All rooms. On a phone, rooms are section headings in
one list.

**Rows.** Each row has:

- an icon (the same one the entity's card uses)
- line 1: the entity name (`short_name`, plus the `title_suffix` when it has one)
- line 2: the device name, with the room added in the "All rooms" view on a computer
- on the right: the live value (`display_value`) above the time since `last_update`, using
  the alerts' `relative_time`

An unavailable entity shows its row dimmed with "unavailable". Values update live, and row
order never changes while the picker is open.

**Picking (single).** Clicking a row, or pressing Enter on the highlighted row, picks it and
closes the picker. The pinned row (None / Automatic) sits above the list and stays visible
while searching.

**Picking (multi).** Each row has a tick box. Ticking adds the entity and unticking removes
it, both straight away (`on_change` per tick, as the chips do today). A **Done (n)** button
closes the picker. In the closed field the chosen entities stay as removable chips, each now
reading name + device.

**Keyboard (computer).** Esc closes. ↑/↓ moves the highlight across visible rows (skipping
headings), Enter picks, Tab reaches the chips and the room column. A click outside closes
the picker too.

**Missing value.** A stored id the house no longer has shows in the closed field as a row
reading the raw id with "missing". When the picker is open it sits at the top, ticked or
selected, so it can be removed or replaced.

**Closed field.** It shows the chosen entity's row (icon, name, device · room, value) and a
chevron, or the field's "None"/"Automatic" text. It's keyboard-focusable and opens on
Enter or Space.

## Look

The look follows the active theme tokens (Night Glass, Ivoire). It must not hard-code the
mockup colours. The approved layout mockup is
`.superpowers/brainstorm/…/picker-both.html` (not tracked): computer popup, rooms on the
left, entities on the right; phone full-screen sheet grouped by room. The selected row has
an accent outline. Chips use the app's existing `.chip` / `.chip--selected` styles.
Breakpoint: the app's existing 640 px.

## Text

All new strings go through the i18n modules in EN and FR: placeholder "Search a device, a
room, a type…", All, + Diagnostic, No room, No match, Search all entities, Done (n),
unavailable, missing, plus each suggested-chip name.

## Settled during review

Answered from the code:

- **Flow card fields hold a bare id.** `TriggerState.entity`, `TriggerSignal.entity` and
  `Command.entity` are `String`; `If.entity` is `Option<String>`. The picker writes the id
  through the existing `entity_write` callbacks. The "missing" row covers ids that are not in
  the house yet. Fixing how the Command JSON field silently drops bad input is out of scope.
- **Per-tick `on_change` is safe.** Every multi-pick host edits a local draft and has its
  own Save button: climate settings, alarm settings, home settings and the device sheet. The
  group wizard keeps local state until its last step. No draft-and-commit mode is needed in
  the picker.
- **Room order** is `AreaView.order` (then name), the same as `group_by_room`. Rooms with no
  order and "No room" come last. The Room thermometer field moves from name order to this
  order. *(My call: one order everywhere.)*

My calls:

- **Density.** The closed single-pick field shows the full row (icon, name, device · room,
  value). Multi-pick chips and the flow card side panel use a compact form: name, then the
  device, with no value. One component with a `compact` prop.
- **Layering and Back.** The picker renders in a portal above any open sheet or panel and
  traps focus while open. On a phone, opening it pushes an entry on the nav `BackStack`, so
  browser or Android Back closes the picker and leaves the screen underneath where it was.
- **Pinned rows and the current value.** Order at the top of the list: the pinned row (None or
  Automatic) first, then a missing id. The current value is always listed, whatever the chip.
- **Automatic** takes the resolved entity id as an `Option<String>` prop, as
  `ThermometerField`'s `auto` already does. Its row reads "Automatic", followed by that
  entity's name and value when there is one. The energy counter's Automatic has no
  resolution today, so it shows the word alone.
- **Size.** No virtual list. The chip pre-filter keeps most lists short. On All with no query,
  it renders the first 200 rows in order, then a line "200 of N shown, type to narrow".
  A query or a room choice lifts the cap.

## Shape of the code (left mostly to the builder)

- The pure part is a function from (entities, areas, chip, query, room, lang) to the grouped
  rows. It sits in its own module, takes plain data and is tested natively, as
  `flow_picker` and `group_wizard::members_by_room` already are. The per-field pre-filters
  are plain predicates passed in. The existing `is_siren`, `is_remote`, `is_tracker`,
  thermometer and counter predicates move or are reused, not rewritten.
- The view part is a picker shell (popup / sheet) plus a list body. The body is reusable on
  its own for the group wizard.
- Two public field components, single and multi, replace `EntityField`, `EntityListField`,
  `ThermometerField`, alarm `AddPicker`, `CounterField`, the home-settings mode select, the
  ambiance search and the flow `TextField`s for entities.
- Tests: native tests for filtering, search (accents, FR/EN kind words, device and room
  names), grouping/order, pre-filters, missing-id handling and the "diagnostic only when
  current" rule. Existing tests for the replaced components are ported, not deleted.
  Rendering is checked on screen in both themes at computer and phone widths before merge.
- Docs: the user docs pages that describe these settings fields get a line saying the
  picker is searchable (update `docs/`, not the README).
