# Flow list redesign — design

Date: 2026-10-22 · Status: draft, awaiting approval

## Why

The web Flows page (`crates/oikos-web/src/screens/flows.rs`) shows one large card per flow.
That works for four flows; the house will have dozens. The page must stay fast to use at
50+ flows. The two jobs it serves:

1. **Find one flow quickly** to open or change it.
2. **Spot problems at a glance**: invalid flows, flows whose last run failed, flows switched off.

Mockups were reviewed in the browser (`.superpowers/brainstorm/…/flow-list-layouts.html`,
options A, B, C). The user liked all three and chose **B (list grouped by room)** as the default.

## Goals

- A top bar with search, filters, sort, grouping and a List / Cards toggle.
- A dense list view (default), grouped by room, plus an ungrouped list and a card view.
- A hand-set **room** per flow, settable from the flow editor, from the list, and via MCP.
- Works for any house (project rule: generic, the live house is only a test bed).

## Non-goals

- Automatic room detection from the devices a flow uses (the user chose hand-set).
- Folders, tags or free-form categories beyond the room.
- Bulk actions (multi-select enable/disable/delete).
- Android app UI changes. The app is single-user and may adopt the field later; the hub
  contract change is additive.

## Anti-goals

- A page that is slower to scan than today at 4 flows.
- A broken or invalid flow hidden inside a folded section with nothing pointing at it.
- Saving a flow failing because of the room field (a stale room must never block a save).

## Data: `Flow.area`

- New optional field on `oikos_core::flow::Flow`: `area: Option<String>`, an area id
  (`Area.id`). Serialized as `"area"`, omitted when `None`. Stored with the flow in the
  retained `oikos/automation/<id>` payload, so every reader sees it.
- Flows written before this change have no `area` and land in "No room". Older readers keep
  it through `Flow::extra`.
- **Hub validation** on write (HTTP save and MCP `set_flow`): an `area` that names no existing
  area is rejected with a clear error — *unless it equals the area already stored for that
  flow* (an unchanged stale id passes, so a `get_flow` → `set_flow` round-trip after a room
  deletion still saves). *[builder-check call]*
- Loading retained flows (startup, MQTT) stays lenient: an unknown `area` never makes a flow
  invalid. *[builder-check call]*
- **A room deleted later**: the flow keeps the stale id; every reader treats an unknown id as
  "No room". The web editor shows "No room" for it and writes `null` when the flow is next
  saved (anti-goal above).
- Renaming a room changes nothing (the id is stable).
- **Invalid flows**: today `FlowView.definition` is a stub (id, name = id, no nodes). The hub
  fills the stub's `name` and `area` from the raw stored JSON when those two keys are
  readable, so an invalid flow is listed under its room with its real name. "Move to room" is
  disabled for an invalid flow (fix it first). *[builder-check call]*

## MCP

- `set_flow` accepts `area` (area id or absent).
- `list_flows` returns `area` for each flow; `get_flow` returns it as part of the definition.
- The server instructions mention the field in one sentence ("a flow's `area` is the room it
  is listed under in the UI; it does not change what the flow does").

## Web: top bar

Left to right, wrapping on narrow screens:

- **Title "Flows"** and **＋ New flow** (unchanged).
- **Search box**, placeholder "Search flows, devices, rooms…". Case- and accent-insensitive
  substring match on: flow name, description, the trigger summary text (which names the
  trigger devices), and the room name. Filters live as you type.
- **Sort**: Name (A→Z, default) or Last run (newest `started_at` first, a running run
  included; never-run flows last). When grouped, the sort applies inside each room section;
  sections keep the room order. *[builder-check call]*
- **Group**: Room (default) or None.
- **View toggle**: List (default) / Cards.

Below it, a row of filter chips in two sets:

- **Status** (single choice): All · On · Off · ⚠ Needs attention. Each shows a count.
  *Needs attention* = invalid, or the newest run that finished as `Done` or `Failed` is
  `Failed` (`runs` is newest first; `Running`, `Cancelled` and `Skipped` runs neither raise
  nor clear it). *[builder-check call]*
- **Trigger type** (multi choice, none selected = all): Device state (`trigger.state`) ·
  Schedule (`trigger.schedule`) · Sun (`trigger.sun`) · Button / event (`trigger.signal`) ·
  Manual (no trigger node). A flow with several triggers matches each of its types. Invalid
  flows have no readable nodes: they match no trigger type (they show only when no trigger
  chip is selected) and use a ⚠ icon. *[builder-check call]*

Filters combine with AND across sets. When nothing matches: "No flows match" with a
"Clear filters" link. The existing empty state ("No flows yet…") stays for a house with zero
flows.

## Web: list view

One row per flow, in a rounded panel matching the existing card tokens:

| icon | name + mode tag | trigger summary | last run | switch |

- **Icon**: by first trigger type (state, schedule, sun, signal, manual), tinted with the
  existing accent tokens.
- **Name** in bold; `restart` shown as a small tag (single is the default, no tag).
- **Trigger**: `trigger_summary` (extend it to describe `trigger.sun`, currently missing).
- **Last run**: "Done · 2 min ago" as today; a failed run in the danger colour.
- **Switch**: the existing `FlowSwitch`.
- Disabled flows are dimmed; invalid flows get the red "Invalid" tag with the reason as tooltip.
- A **⋯ menu** at the row end with "Move to room…" (a room list plus "No room") and "Open".
- Clicking the row (outside the switch and menu) opens the flow, as today.

**Grouped by room** (default): one foldable section per room, header "LIVING ROOM · 4".
Sections follow the areas' `order` (then name), with **No room** last. Empty rooms are not
shown. A folded section whose flows include one needing attention shows a ⚠ in its header
(anti-goal: nothing broken hidden).

**Phone width**: a row becomes two lines: name + switch on the first, trigger · last run on
the second. Same top bar, wrapping.

## Web: card view

A grid of smaller cards than today (`repeat(auto-fill, minmax(230px, 1fr))`, like Home
tiles): icon, name, switch on top; trigger; last run and mode tag at the bottom. Invalid or
failed flows get a danger-tinted border. Grouping applies here too (same section headers).
One column on phone.

## Web: setting the room

- **Flow editor**: in edit mode, a room picker sits next to the name and mode in the canvas
  toolbar (`flow_canvas/toolbar.rs`); part of the draft, saved with Save, one undo step.
- **From the list**: the row/card ⋯ menu "Move to room…" saves immediately and the flow moves
  to its new section.
- New flows start with no room.

## Remembered between visits

View, grouping, sort, status/trigger filters and folded sections are kept per browser in
`localStorage`, with the same fail-silent pattern as `theme.rs`. The search text is not kept.
Folded sections are keyed by area id, with a fixed sentinel for "No room"; stored keys for
rooms that no longer exist are ignored. *[builder-check call]*

## Calls made without the user

Marked *[builder-check call]* above: stale-area validation rule, lenient load, invalid-flow
stub carrying name + area, the "needs attention" rule, invalid flows vs trigger filters, sort
inside sections, fold-state keys. Also mine from the brainstorm: grouping applies to the card
view, the ⋯ row menu as the place for "Move to room", the phone two-line row, what is
remembered in `localStorage`.

## Left to the builder

- How the list's "Move to room" reaches the hub: a small endpoint like the existing
  `POST /flows/{id}/enabled`, or reusing the full-flow save. Either is fine; it must not
  race an open editor any worse than the enable switch does today.
- Exact icon glyphs, spacing and colours, within the existing tokens in `styles/tokens.css`.
- Whether chip counts reflect the search text (suggested: yes, counts follow the search and
  the other chip set).
- Splitting `flows.rs` into smaller modules as it grows.
- Tests: core (serde round-trip with and without `area`, validation of unknown area), hub
  (save rejected on unknown area), web pure functions (search match, filters, grouping/order,
  needs-attention).
