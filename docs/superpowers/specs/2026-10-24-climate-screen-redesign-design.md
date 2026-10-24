# Climate screen redesign: full width, the week at a glance

Date: 2026-10-24 · Scope: `crates/oikos-web` only (no hub, API or engine change)

## Why

On a wide screen the Climate page leaves empty space on both sides, and the
schedule is hard to read. Each zone is a card in a grid of cards at least
320 px wide, and the week timeline inside it gives each day about 250 px for
24 hours, with hour marks only at 00/06/12/18/24. The owner wants to **see the
whole week's schedule at a glance** and wants the page to **use the full width**.

## Goals

- The Climate screen fills the content area beside the sidebar. It is not held
  to the 1200 px `.screen` cap that other screens use.
- A room's week reads in one look: a calendar with days as columns and hours
  going down, each block showing its temperature (and its times when there is room).
- Block colour shows how warm the block is: yellow, orange, red.
- Everything the zone card does today still works: state switch, thermometer
  pick, holds and clearing them, learning, proposals, editing the approved
  rules, recent decisions, house settings.
- Both themes (Night Glass and Ivoire) take their colours from `styles/tokens.css`.

## Non-goals

- No change to the hub, `oikos-climate`, the API or the MCP tools.
- No change to what the settings sheet (`climate/settings_sheet.rs`) contains.
- No French translation pass. The climate strings stay in English, as now.
  The i18n project (Ivoire project 5, `feat/i18n-all-screens`) will cover them.
- No new schedule features (copy a day, templates and so on).

## Anti-goals (a failure even if it technically works)

- A calendar that loses an editing ability today's timeline has: dragging block
  ends, tapping an empty spot to add a block, the block editor, and Off blocks.
- Hard-coded colours that ignore the theme.
- A refetch (`climate_rev`) that resets what the person is doing: the selected
  room, an edit in progress, an open decision list or a hold being typed.

## Layout, wide screens (approved mockup "B")

Mockup: `docs/superpowers/specs/2026-10-24-climate-screen-redesign-mockup.html`
(see "Mockups" below).

```
┌ Climate [Beta] ───────────────────────────────────────── [Settings] ┐
│ banners (error, house invalid, history off)                         │
├──────────────┬──────────────────────────────────────────────────────┤
│ ROOM LIST    │ STATUS CARD                                          │
│ ● Salon 22.6°│  Now 22.6° · Target Heat 16° (Schedule, reason)      │
│   Heat 16° · │  [Off|Pending|Active]  Hold [19.5] [1 h][2 h][Until  │
│   Active     │  next change]   Held 19.5° until 21:05 · set on a    │
│ ● Julien …   │  device · [Clear]                                    │
│ ● Alicia …   ├──────────────────────────────────────────────────────┤
│ ● Rémi …     │ THIS WEEK                          [Heat|Cool] [Edit]│
│              │      Mon  Tue  Wed  Thu  Fri  Sat  Sun               │
│              │  00 ┌───┐┌───┐ …                                     │
│              │  03 │   ││   │                                       │
│              │  …  │18.5°  … blocks coloured by temperature         │
│              │  21 │16°│…  ── now line on today ──                  │
│              │  24 └───┘└───┘                                       │
│              │ Base 15° outside the blocks · legend ≤16 / 17–19 / ≥20│
│              ├──────────────────────────────────────────────────────┤
│              │ PROPOSAL (only when one is pending): same calendar   │
│              ├──────────────────────────┬───────────────────────────┤
│              │ LEARNING + THERMOMETER   │ RECENT DECISIONS          │
└──────────────┴──────────────────────────┴───────────────────────────┘
```

### Room list (left column, about 240 px)

- One row per zone, in the order the hub sends (`zone_ids`). Each row shows the
  name, the reading and a sub-line. A dot sits beside the name in the
  temperature colour of the decision's setpoint, or neutral when there is no
  temperature setpoint. Sub-line mapping *(designer's call)*, a pure, tested
  helper:

  | Zone                                   | Sub-line                     |
  |----------------------------------------|------------------------------|
  | orphan                                 | `No climate device`          |
  | state Off                              | `Off`                        |
  | Pending, no approved rules             | `Pending · no schedule`      |
  | Pending, with rules                    | `Pending`                    |
  | Active, no decision                    | `Active · no decision yet`   |
  | Active, decision                       | `Heat 16° · Active`          |
  | …and any override present              | append ` · held`             |
- Clicking a row selects that zone. The selected row uses the theme's
  selection surface.
- **Selection:** a signal on the screen. It defaults to the first zone, survives
  refetches and falls back to the first zone if the selected one disappears.
  It is not put in the URL. *(Builder's call: if adding `?zone=` is trivial
  with the existing nav helpers, it may be added.)*
- **Switching rooms keeps every room's state** *(designer's call)*. One panel is
  built per zone, keyed by zone id (the same `<For>` as today's cards), and only
  the selected one is shown (the others are hidden, not unmounted). An unsaved
  schedule draft, an open decision list or a half-typed hold in one room is
  still there when the person comes back. No dirty-state tracking or
  confirmation is needed.
- With no zones, show today's "No room has a climate device" message in place of
  the room list and the panel.

### Selected room panel (right, fills the rest)

1. **Status card:** the room name, a large reading with its age, the target
   (mode + setpoint) with its source chip and reason, the Off/Pending/Active
   switch, the hold input and its buttons (only when the zone can hold, as now)
   and the active overrides with Clear. The settings-invalid badge stays.
2. **"This week" card:** the vertical calendar (below), the Heat/Cool tabs when
   both modes have rules, "Edit schedule" / Save / Cancel (today's
   `RulesPanel` behaviour), the base line and a colour legend. With no approved
   rules it shows "No approved schedule yet", as now.
3. **Proposal card:** only when a proposal is pending. It shows today's
   `ProposalPanel` content (changes list, approve/reject/edit), with its
   timeline drawn as the same vertical calendar.
4. **Lower row, two columns:** on the left, learning status + "Learn now" and the
   thermometer field; on the right, recent decisions (today's collapsible list).

All of the panel's per-zone local state (hold text, open decisions, edit
draft, proposal edit) must survive refetches, as `ZoneCard` does today, and
room switches (see Selection).

## The vertical week calendar

Replaces the horizontal `WeekTimeline` everywhere: the approved rules and the
proposal.

- Grid: an hours gutter (labels every 3 h on wide screens, every 6 h on phones)
  plus 7 day columns of equal width (Mon→Sun, `Day::ALL`). Light hour lines
  every 3 h. Column height is about 420 px on wide screens.
- **Today:** the day header is bold with an accent underline, and the column
  border uses the accent. A **now line** (accent colour, small dot at the left)
  sits at the current minute in the house time zone (`week_minute`). It moves
  at least once a minute while the page is open.
- **Blocks:** absolutely positioned by from/to minutes. The block shows the
  temperature in bold, plus `HH:MM – HH:MM` underneath when the block is tall
  enough (builder picks the cut-off). Off blocks are drawn neutral grey with
  "Off".
- **Proposal evidence:** a proposal block that has `evidence` keeps today's
  `agree/of` count (for example `18.5° · 3/5`) *(designer's call)*. It goes
  beside or under the temperature when the block has room, and otherwise it
  shows in the tap-to-view popup. Approved-rules blocks show no evidence, as now.
- **Base:** the plain column background is the mode's base. The legend line
  reads "Base 15° outside the blocks".
- **Editing** (when "Edit schedule" is on): the same behaviour as today, turned
  vertical. The top and bottom edges of a block are drag handles in 15-minute
  steps (`move_edge`). Tapping an empty spot in a column adds a block there
  (`add_block`). Tapping a block opens `BlockEditor` (times, setpoint, Off,
  days, delete). It stays **inline under the calendar**, as today, not an
  anchored popover *(designer's call)*. The pure editing functions in `climate/timeline.rs` and their
  tests stay as they are. Only the pointer→minute mapping changes from X to Y
  (`minute_on` uses `client_y` against the column's top and height), and the
  CSS classes change from track to column.
- **Viewing on touch (not editing):** tapping a block shows its times and
  temperature (a small popover or tooltip), because on phones the block only
  shows the temperature.

### Temperature colours

Three bands, defined per theme in `styles/tokens.css` as new tokens (for
example `--temp-low`, `--temp-mid`, `--temp-high` and `--temp-ink` for the
text on them):

| Setpoint     | Colour |
|--------------|--------|
| below 17 °C  | yellow |
| 17 to <20 °C | orange |
| 20 °C and up | red    |
| Off          | neutral grey (existing surface/border tokens) |

*Owner's call:* yellow/orange/red by temperature. *Designer's call:* the
17/20 cut-offs, which the owner saw in the mockup and liked. The function that
maps a setpoint to a band is a pure function, unit-tested. The same bands are
used in Cool mode and for the room-list dots.

Mockup values: Night Glass yellow `#ffd95a`, orange `#ff9a3c`, red `#ff5a4e`,
ink `#1a1206`; Ivoire yellow `#f2cc55`, orange `#e58a3a`, red `#d04a36`, ink
`#2a1a08`. The builder may tune them for contrast but must keep the hues.

## Phone and narrow layout (approved mockup "A")

Below a breakpoint (builder's call, roughly when the room list plus seven
readable columns no longer fit), the page stacks:

1. Title + Settings.
2. **Room chips:** a horizontal, scrollable row (the existing `.chips`
   pattern), each with the dot, name and reading; the selected chip is filled.
3. **Compact status card:** reading and target side by side, state switch full
   width, hold buttons in one row ("Next change" as the short label), the
   override line.
4. **"This week":** the **same seven-column calendar**, narrower (about 360 px
   tall). Day headers are two letters. Blocks show only the temperature, and
   times show on tap. The whole week stays visible; there is no day picker.
5. Proposal (if any), learning + thermometer, decisions: stacked.

## Mockups

`2026-10-24-climate-screen-redesign-mockup.html`, next to this spec, holds the
brainstorm mockups: desktop B in Night Glass and Ivoire, then two phones. The
**left phone is the approved A**, and the right one (week strip + day) was
rejected. Open it in a browser. The schedules are real and the room names are
generic. It is the visual reference for the status card, room rows and chips,
block styling and the legend.

## Constraints

- Leptos (CSR/wasm) as in the rest of `oikos-web`. Follow the existing
  patterns: memos so refetches don't rebuild inputs, `act()` + toasts for API
  calls, `Callback` for change notifications.
- No real house names, hosts or IPs in tracked files.
- Keep the existing pure helpers in `climate/mod.rs` and `climate/timeline.rs`
  and their tests. New pure logic (temperature band, room selection fallback,
  vertical minute mapping) gets unit tests.
- Remove CSS for the old card grid and the horizontal timeline that nothing uses
  any more.

## Left to the builder

- How to split `ZoneCard` into a room-list row, a status card and the lower cards.
- Exact breakpoint, column heights, the block-label cut-off and spacing.
- The popover vs tooltip for "tap a block to see its times".
- Whether `?zone=` goes in the URL.
- How the now line ticks (a shared clock signal or an interval).
