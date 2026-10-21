# Web history visuals

Author: Gekkotron
Date: 2026-10-21
Status: approved in conversation, plan at `docs/superpowers/plans/2026-10-21-web-history-visuals.md`

## Why

The hub now owns history (`docs/superpowers/specs/2026-10-20-timescale-history-design.md`, landed `c819fd7..45f420e`). Four routes are live and recording — `/api/v1/history/series`, `/daily`, `/today`, `/last_change`. Nothing in the web UI reads them yet. Three features sit on the deferred list (`docs/superpowers/plans/2026-10-19-web-energy-security-deferred.md`), waiting for exactly these routes:

1. A visual history page for sensors and Energy nodes (24h / 7d / 30d / 1y).
2. `x.x kWh today` under each Energy node and card.
3. `since X` per sensor in Security.

The user wants all three in one wave: a full-screen history page as the chart home, and the two tile features added to Energy and Security the same release.

Success: tapping any entity opens a history page with a step-line chart over the four ranges, Energy shows live kWh-today under each node, Security shows "since X" per sensor — all served by the hub's existing routes, drawn by hand with SVG in the Oikos web aesthetic.

## Decisions taken in conversation (the user's)

- **Full-screen `/history/:entity` route**, not an inline panel inside the entity sheet. The sheet gets a chip-styled "View history →" entry in its Controls panel that pushes to it. Rejected alternative: inline panel.
- **Range chips: `24h · 7d · 30d · 1y`**, matching the four hub windows exactly. Default `24h`. No custom date range in v1.
- **Step line chart**, hand-drawn SVG, in the Oikos aesthetic (same module language as the Energy flow). Matches the hub's held-until-next-row storage model and Homey's "Insights" semantics.
- **Y-axis auto-scales to the data range** on web, with min/max labels. Explicit decision against Homey's mobile 0-anchor, which the user hated when seen in the recon.
- **Counter entities (energy) swap to a daily bar chart** for `7d / 30d / 1y`, with a kWh-today readout at the top of the page. `24h` on a counter still shows the step line of instantaneous power if the entity carries one, otherwise a bar for that day.
- **Multi-sensor compare, pan/zoom, custom date ranges — not in v1.** Homey ships compare, users complain, large code cost; the user chose to leave them out.
- **One wave.** History page, Energy kWh-today tiles, and Security "since X" ship together.
- **History is available for every recorded entity**: sensors (temperature, humidity, power…), climate and water-heater (current temperature), lights and switches (on/off as 0/1), covers (position 0–100), binary sensors (motion, door, window — as 0/1). The hub already stores `num` for all of these; the chip is universal, no per-kind predicate.
- **Derived-house kWh-today tile yes, no tap-through.** The Energy screen shows "x.x kWh today" under the derived-house node, computed client-side by fanning out `/today` across its plus/minus counters. The node is not tappable in v1 — single-entity history only, no aggregate chart.

## Goals

1. A `/history/:entity_id` route in `oikos-web` that fetches `/api/v1/history/*` and renders the four ranges as a step-line (series) or daily-bar (counter) chart.
2. A `View history →` chip in the entity sheet's Controls panel, pushing to the history route for that entity.
3. The entity sheet opens the history route also when an Energy node card is tapped.
4. `x.x kWh today` under each Energy node and card, refreshed on screen entry and on a 5-minute visibility-aware tick (`document.hidden`-gated so the tab in the background does not poll).
5. `since X` per sensor on the Security screen, loaded in one batched `/last_change` call for all visible sensor ids.
6. A new `oikos_web::components::history_chart` module that owns the SVG drawing for both flavors (step line and daily bars) and is reusable.
7. Dark and light themes handled from existing CSS tokens; no new color palette.

## Non-goals

- Pan, zoom, scrubbing, or custom date pickers. Range is one of the four presets.
- Compare multiple entities on one chart.
- A dedicated state-timeline UI with named-event labels ("turned on", "closed by Mia") or coloured bands. Discrete entities use the same step line as numeric ones — the line reads as a square wave for 0/1 series; richer event labelling is a follow-up.
- Downloading a chart as PNG/CSV.
- Showing history in the Android app. The app still talks to OikosHistory until its own migration lands (`project_timescale_history_landed_2026-10-21.md`).
- A chart library (chart.js, plotly, uplot). Hand-drawn SVG, same style as the Energy flow module.

## Anti-goals

- **The history page must not freeze the UI when the hub is down.** Loading, 404 and 503 are first-class states, drawn, not thrown.
- **No polling cliff.** The Energy screen's 5-min tick is paused when the tab is hidden and resumed on `visibilitychange`. The history page itself does not poll — it refetches when the range chip changes.
- **No horizontal scroll on narrow viewports.** The chart grows and shrinks with its container; axis labels remain readable at phone width (min 320 px).
- **No raw timestamps in UI copy.** All times rendered in the house's local timezone (`Europe/Paris` for this user), using relative labels where it reads better ("yesterday 14:20").

## How it works

### The history route

A new Leptos route `#/history/:entity_id?range=24h|7d|30d|1y` (hash routing matches the rest of the app). Query parameter is the source of truth, so the browser Back / Forward keeps the user's chosen range.

Layout, top to bottom:

- **Chrome row** — back arrow (returns to the sheet with scroll preserved), entity name + area/type/availability, chip row for ranges on the right. On narrow viewports the chip row wraps below the title.
- **Chart card** — full-width glass card holding the SVG. The chart takes about 60% of the viewport height, with a floor of ~240 px and a ceiling of ~460 px.
- **Legend row** — single line under the chart. Series flavor: `"Last 24 hours · held-until-next-row"` on the left, `"min 18.4° · max 23.8°"` on the right. Counter flavor: `"Last 7 days · daily total"` on the left, `"7-day total: 42.3 kWh"` on the right.
- **No chrome below.** The chart is the page.

The route decides series vs. counter from the entity's registered `kind` (sensor vs. energy counter). The Energy node tap uses the same route — the registry already carries the counter flag; the builder resolves this on page mount.

### Chart module

`crates/oikos-web/src/components/history_chart.rs`, exposing one component:

```
<HistoryChart entity_id=… range=… flavor=Series|Counter />
```

The component:

- fetches, memoized by `(entity_id, range, flavor)`;
- draws the chart with hand-written SVG;
- shows loading shimmer while fetching;
- shows the empty copy on `404 not_recorded`;
- shows the unavailable copy on `503 history_unavailable`;
- shows the error copy on anything else, with a `Retry` button;
- hover and tap both open the tooltip; the tooltip is a small glass pill with local time (relative on 24h, absolute from 7d up) and the value.

**Step-line chart (Series flavor)** — draws one path per range from the `points: [[t, mean, min, max]]` response, step-held between points. Min/max is drawn as a muted band behind the mean line. Y-axis is auto-scaled with 10% headroom top and bottom; three gridlines (min, mid, max) with their numeric labels. X-axis has 4–6 tick labels depending on the range: `00:00 · 06:00 · 12:00 · 18:00 · now` for 24h; day names for 7d; `01 · 08 · 15 · 22` dates for 30d; months for 1y.

Discrete entities (lights, switches, binary sensors, covers) use the same step-line path with Y-axis labels swapped to the entity's natural pair — `off / on` for OnOff, binary sensors and lights (0/1), and `0 / 100` for cover position. The line reads as a square wave; the min/max band collapses to the line itself because `mean == min == max` for a 0/1 series.

**Daily-bar chart (Counter flavor)** — one bar per day from the `days: [[t_local_midnight, kwh]]` response. Width autosizes from the day count (7, 30, or 365). For 1y the bars become hairlines; a hover shows the exact day and kWh.

### Entity sheet entry

In `crates/oikos-web/src/screens/entity_sheet.rs`, inside the Controls panel, add one row:

```
┌─────────────────────────────┐
│ 21.4 °C                    │
│ Humidity 48%                │
│                             │
│ View history →              │
└─────────────────────────────┘
```

Chip-styled, same vocabulary as the rest of the sheet. Visible on every entity — if the server returns 404, the user sees "no data recorded yet" rather than being blocked by a predicate. (Keeps the sheet simple; avoids a per-entity "has_history" probe.)

### Energy screen tiles

In `crates/oikos-web/src/screens/energy.rs`, under each energy node and each card, add a small readout:

```
Oven
  0.0 W · 1.3 kWh today
```

- Loaded in parallel with the existing node fetches on screen entry (one `/today` call per counter; batching is a hub follow-up if the fan-out hurts).
- Refreshed on a 5-minute tick. The tick is paused when `document.hidden` and resumed on `visibilitychange`.
- Tapping a named counter node opens the history route for its counter entity. The derived-house node is not tappable in v1.
- The derived-house kWh-today is computed client-side: fan out `/today?kind=counter&entity=…` across every counter in `house_meters().plus` and `.minus`, then sum plus − minus. The live `W` reading already comes from `oikos_core::energy`'s subtraction and keeps working unchanged.

### Security "since X"

In `crates/oikos-web/src/screens/security.rs`, under each sensor row, add a small secondary line:

```
Front door
  closed · since 14:20
```

- One batched `/last_change?entity=…&entity=…` call for all visible sensors on screen entry. The hub route supports 1–50 ids per call; the screen chunks into 50 if there are more.
- Relative labels: `just now` under a minute, `3 min ago` under an hour, `14:20` same day, `yesterday 14:20`, `3 days ago` beyond.
- No refresh timer — the Security screen already resubscribes to WebSocket events, which carry state changes that will update the sensor row directly; `since X` recomputes from the new event.

### Error and empty copy

- `404 not_recorded` → "No data recorded for this entity yet. The hub starts recording from the moment the entity first appears."
- `503 history_unavailable` → "History is unavailable right now. The database may be down."
- Other errors → "Could not load history" + `Retry` button.

### Code shape

New / changed files:

- `crates/oikos-web/src/components/history_chart.rs` — the chart module (new).
- `crates/oikos-web/src/components/mod.rs` — register it.
- `crates/oikos-web/src/api/history.rs` — typed wrappers over the four history routes (new). Returns `thiserror` variants for 400 / 404 / 503 / other.
- `crates/oikos-web/src/screens/history_detail.rs` — the full-screen page (new).
- `crates/oikos-web/src/screens/entity_sheet.rs` — add the `View history →` row.
- `crates/oikos-web/src/screens/energy.rs` — tiles + tap-to-history + 5-min tick.
- `crates/oikos-web/src/screens/security.rs` — batched `/last_change` + `since X` row.
- `crates/oikos-web/src/nav.rs` — new `History(entity_id, range)` target, Back behavior.
- `crates/oikos-web/style.css` or the equivalent, extending existing tokens — new classes `.history-page`, `.history-chart`, `.history-chip-row`, `.history-tooltip`, `.energy-tile-today`, `.security-since`.

No new dependencies. Hand-drawn SVG using Leptos `view!`. The viewBox strategy mirrors `energy_flow.rs`.

## Left to the builder

- The exact tooltip visual (shape, font sizes, padding) — pick to match the existing glass tiles.
- The loading shimmer style — reuse the one Energy and Security already have, if any; otherwise a soft gradient.
- The exact placement of the "View history →" chip inside Controls (last row, or its own mini-section) — pick what fits the current spacing.
- Bar width and spacing in the daily-bar chart — pick what reads well at 7, 30 and 365 bars.
- Whether `since X` text turns muted when the sensor is `offline` — pick what reads clearly against the existing Security styles.
- The exact `max` chunk size for the batched `/last_change` call if more than 50 sensors are visible (the route accepts 1–50; the user's house is nowhere near 50, so any chunking strategy is fine).
- Shimmer vs. placeholder during the kWh-today loading on the Energy screen — pick to not jitter the layout.

## Calls settled during the builder check

Small decisions the builder-check surfaced; I settled them so the plan can move forward. Flag any that feel wrong before the plan starts.

- **Series vs. Counter detection** on the history page: classify at the client via `oikos_core::energy::meters::is_counter(entity.unit)` — the same predicate the Energy screen already uses. No new field in `EntityView` or `oikos-api`.
- **Counter → paired power sensor** for the 24h view on an energy counter: add a reverse-lookup helper in `oikos_core::energy::meters` that scans `house_meters` for a `power` entry whose counter is this id. First-registered wins when two power sensors pair to the same counter. If none pairs, the 24h view falls back to a single-day bar for the counter. Legend/unit on the chart comes from the sensor being plotted (`W` on the step line, `kWh` on the bars).
- **Nav wiring**: no new enum variant. The route uses `path!("/history/:id")` + `use_query_map("range")` and reuses the existing `TrackReturnPath` / `ReturnPath` so Back returns to whatever opened the page (sheet, Energy screen, Security screen).
- **API error shape**: builder's call — adapt `api::check` to expose the HTTP status if the ripple across callers is small, otherwise a local helper in `api/history.rs`. Either way, `api/history.rs` returns typed `thiserror` variants for 400 / 404 / 503 / other.
- **Hub batching for `/today` on the Energy screen**: v1 fans out one call per counter and sums client-side. A `/today` batch endpoint on the hub is a follow-up if the fan-out ever hurts.

## Open points (none blocking)

None. All big calls are above.
