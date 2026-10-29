# Energy page: Flux (Sankey) live view — design

Date: 2026-10-28. Status: approved by the user.

## Intent

The live diagram on the web `/energy` page (house in the centre, solar, grid, battery and three
consumer slots as circles around it, joined by dashed animated lines) works but "is not very
beautiful". The user wants other looks for the real-time view, picked in **Settings ›
Appearance**, the same way the theme is picked.

The user brought three references: an "Orbite" double ring, a Sankey diagram, and the Home
Assistant power-flow-card circles. A throwaway mockup of all three
(`.superpowers/brainstorm/…/energy-styles.html`, untracked) was shown. The user chose to start with
**Flux (Sankey)**. The current diagram stays available as **Classic**. A later round will move
Classic towards the mockup's style C (circles, animated dots, house ring showing the source mix);
in that version solar is wired to the house only, never straight to the grid. That later round is
not part of this design.

## Goals

- A new **Energy view** setting on `/settings/appearance`, under the theme switch: a segmented
  choice **Classic** / **Flux** (FR: **Classique** / **Flux**), with a one-line hint.
- Persisted per browser in `localStorage`, exactly like the theme (`oikos.theme` →
  `oikos.energy_view`); unknown or missing value = Classic; every storage access fallible and
  silent.
- Classic is the default and is unchanged.
- With Flux selected, the `/energy` page replaces the circle diagram with a Sankey of the live
  power, fed by the same `EnergyUi` the Classic diagram uses (`EnergyFlow::build`).
- Everything else on the page is unchanged and in the same order: title, subtitle
  (`live_subtitle`), the check-sign hint, the grid day chart, the cards, the "all consumers" list.

### What the Sankey shows

Three columns, left to right (wide screens):

1. **Sources**: Solar (production), Grid (import, only while importing), Battery (only while
   discharging, `Flow::In`).
2. **House** and the surplus sinks: **Export** (grid, while exporting, `Flow::Out`) and **Battery
   charging** (`Flow::Out`).
3. **Consumers**: every consumer of `EnergyUi::consumers` with a known value > 0, in the
   `consumers` order (already throttled by `REORDER_MS`, so the order does not flicker), then
   **Not itemised** (FR **Non détaillé**) = house − sum of shown consumers, when > 0.
   When more than **8** consumers have power, the first 7 are shown and the rest fold into one
   **+N others** node (FR **+N autres**), like Classic's "more".

Links (band thickness ∝ watts, same scale for the whole drawing):

- House → each consumer, → +N others, → Not itemised.
- Sources → house and sinks: see **Data and balancing rules** below.

### Data and balancing rules

Where the figures come from:

- Role nodes (solar, grid, battery, house) come from `EnergyUi::node(id)`; consumers from
  `EnergyUi::consumers` (every consumer, in display order), never from the folded `diagram`.
- The house value is the one `EnergyFlow::build` already gives (a measured house sensor, or the
  derived sum of the inputs). Only when that is `None` does the Sankey fall back to the sum of the
  known flows into the house.

Node identity (keys for DOM and animation): the grid is two nodes, `grid` (import, column 1) and
`export` (column 2); the battery is two, `battery` (discharge, column 1) and `battery-charge`
(column 2). When the direction flips, one fades out and the other fades in; no bar moves between
columns.

Every drawn node is labelled with its **measured** value. Links are allocated from the measured
values, sinks first, because the grid and battery meters say exactly where the surplus went:

1. **Export** is fed by solar first, then by battery discharge.
2. **Battery charging** is fed by what solar has left, then by grid import.
3. **The house** gets what is left of solar, grid import and battery discharge.

If the sources cannot cover a sink (meters disagree), the link stays at what the sources have;
power is never invented. A node's bar is as tall as the larger of what flows in and what flows out
of it (sources: the sum of their links, capped at the measured value; the house: max(inflow,
outflow)). Bands attach from the top of the bar, so a bar may be taller than its bands, never
shorter.

`IDLE_W` (10 W): a link under it is not drawn; a source or sink under it is not drawn. A consumer
under it is not drawn either, and its watts join **Not itemised**, so the house's outflow still
adds up. Not itemised = house − shown consumers − +N others, and is never negative.

**Solar share** (chip): the solar → house link ÷ the house value, in %. It is a new figure, not
`self_consumption` (which is solar used ÷ solar produced and already in the subtitle). Hidden when
there is no solar node or the house is unknown or zero.

Look (from the approved mockup, "B · Flux"):

- Nodes are rounded vertical bars (16 px wide) in the role colour: solar `--solar`, grid
  `--accent-violet`, battery `--accent-mint`, export `--accent-mint`, house `--ink`, consumers a
  fixed rotation of the theme accents, Not itemised a muted neutral. Colours come from
  `tokens.css`, so both themes (Night Glass, Ivoire) work without extra code.
- Bands are filled with a left-to-right gradient from the source colour to the target colour at
  ~42 % opacity (~80 % on hover/focus).
- A soft highlight sweeps along all bands left to right (~3 s loop) so the view reads as live.
  Off under `prefers-reduced-motion: reduce`.
- When values change, node heights, band shapes and label positions glide (~0.9 s ease) instead
  of jumping.
- Labels: the value in bold then the name, muted (e.g. **1,84 kW** Maison). Sources to the left
  of their bar, consumers and the surplus sinks to the right, the house above its bar. Values use
  the existing `power_text` (W / kW, app language).
- Above the chart, four stat chips: consumed now (house), solar production, from the grid (or
  "↗ to the grid" while exporting), solar share of the house (solar used by the house ÷ house, %).
  A chip whose figure is unknown is left out.

### Interaction

- A source, sink or consumer node (bar or label) with exactly one sensor opens that sensor's
  entity page, like Classic (`open` callback, `/entity/<id>`). +N others toggles the existing
  "all consumers" list (the same `expanded` signal Classic's More node uses). A clickable node's
  label is its one tab stop: a button named "name, value", reacting to Enter and Space, with a
  visible focus ring; the bar takes clicks only.
- The SVG is `role="group"` (an image would hide the buttons inside it) with the same
  `aria_sentence` label/title as Classic.

### Narrow screens

Below the width where Classic switches to `Layout::NARROW` (600 px), the Sankey runs top to
bottom: sources on top, the house in the middle, consumers at the bottom; labels under each bar,
names truncated like Classic (`LABEL_CHARS`). The width is read once when the screen is built,
as Classic does. On phones the fold threshold drops to **4** consumers (3 shown + +N others),
so the bottom row has at most 5 bars plus Not itemised; nothing scrolls sideways. The chips wrap
as they already do. Build the narrow layout as a separate step after the wide one.

### Edge cases

- A node whose value is unknown (`None`) or unavailable is not drawn; its card below still says
  so.
- Nothing to draw (no known power at all): a short muted line in place of the chart (EN "No live
  power reading yet", FR "Pas encore de mesure de puissance").
- Water is not power: it is not in the Sankey (its card stays).

## Non-goals (this round)

- Changing Classic, or building style A (Orbite) or C (circles).
- The Android app.
- The dashboard `energy-stat` widget.
- Syncing the choice across devices through the hub.

## Anti-goals

- Bands that do not add up: for every drawn node, in = out (within the meter-disagreement rule
  above). The drawing must never invent power.
- Anything specific to this house in tracked code, docs or defaults (no device names).
- A slower Energy page with Classic selected: nothing new runs unless Flux is chosen.

## Constraints

- Leptos web crate (`crates/oikos-web`), inline SVG like the Classic diagram, no new JS
  dependency, no chart library.
- New text through `i18n` (EN + FR), following `i18n/settings.rs` and `i18n/energy.rs`.
- Styles in `crates/oikos-web/styles/main.css` next to the `.energy-*` rules, theme colours from
  `tokens.css`.
- Docs: update `docs/users/web-ui.md` (Energy section and the Appearance line).

## Decisions made by Claude (not explicitly by the user)

- Classic stays the default (public repo: other installs keep their look until they opt in).
- Vertical layout on phones, folding at 4 consumers there.
- Fold threshold of 8 consumers on wide screens.
- Measured values label the nodes; links are allocated sinks first (export, then battery
  charging, then the house); grid and battery are split into two keyed nodes each.
- Sub-10 W consumers join Not itemised; solar share is a new figure (solar → house ÷ house).
- `localStorage` key `oikos.energy_view`, values `classic` / `flux`.
- Labels in the app language, not always French as in the mockup.
- The settings list's Appearance status line still shows only the theme name.

## Builder notes

- Split the work in two: a pure, testable model (columns, nodes, links, values, the allocation,
  folding, Not itemised, the solar share) built from `EnergyUi`, with unit tests for import,
  export, battery charge/discharge, folding, unknown values and disagreeing meters; and the
  Leptos view that lays the model out in SVG.
- Animations: CSS transitions on SVG geometry work in Chrome/Safari when geometry is set as CSS
  (`d: path(...)`, `x`/`y`/`height` on rects, `transform` on labels). Keep the band path's command
  structure fixed so `d` interpolates. Key DOM elements by node/link id so they survive updates.
- The setting module can mirror `theme.rs` (enum, `parse`, `install_*` returning an
  `RwSignal`, provided as context from `App`).
- Left to the builder: exact spacing, label font sizes, the consumer colour rotation, how the
  narrow layout lays out labels, the sweep implementation (SVG gradient animation or CSS).
