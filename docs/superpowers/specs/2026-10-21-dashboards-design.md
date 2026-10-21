# Oikos Dashboards — Design

**Date:** 2026-10-21
**Worktree / branch:** `feat/dashboards`
**Status:** approved pending builder-check

## Intent

Give the Oikos web UI a dashboards system in the shape of Homey's. Replace the fixed Home tab with a `Dashboards ▾` picker in the top nav. The first entry in the picker is **House**, an auto-generated comprehensive view (every room, every entity) — fixed, not editable, not deletable. The remaining entries are user-authored named dashboards built from widgets.

## Why

Today's Home page is a comprehensive tour of every room and entity. It is useful as a catalog but does not fit the moments where the user wants a focused view (a morning glance, an energy-watch screen, a view for guests). The user wants to build curated views alongside the catalog, not replace it.

## Goals

- Multiple named dashboards, switchable from the top nav.
- The comprehensive catalog view survives, as the fixed **House** dashboard.
- Column-based grid layout that adapts phone ↔ desktop.
- Built-in widget types for every entity type the Oikos web already renders.
- An edit mode on each user-authored dashboard: drag, resize (span), add, remove, rename, reorder.
- Persistence as MQTT retained payloads under a per-dashboard topic, same shape as flows and areas.

## Non-goals

- **Kiosk Mode** (full-screen on a tablet as a wall controller). Homey ships this; Oikos does not in v1. The Homey-gap review marked it P5.
- **Third-party / community widget system.** Single-user hub — not worth the v1 extension surface.
- **Floor-plan widget.** Out of v1.
- **Multiple user accounts / per-user dashboards.** Oikos is single-user.

## Anti-goals

- The House dashboard must not be editable or deletable. Someone trimming it loses the catalog guarantee that every room and entity shows somewhere.
- Dashboards must not live in `oikos.toml`. Config holds configuration; user state belongs on the broker, like flows and areas.
- Adding a dashboard must not require a hub restart.

## Constraints

- Single user, generic for any house (not tied to the live test house).
- Must coexist with the existing Oikos web routes (Energy, Security, Flows, device sheets, flow canvas, areas rename).
- Must adapt phone ↔ desktop. No tablet-only tier.
- The edit mode UX must follow the pattern already landed in the flow canvas (pencil button toggles edit, drag, side sheet, Add picker, Save / Discard). See `project_flow_canvas_redesign_landed_2026-10-21`.
- The web client is Leptos (WASM); the hub is Rust. Widget rendering is on the client, data access via hub JSON APIs.

## How — the decisions

### Top nav

Replace the current Home tab with a `Dashboards ▾` picker:

```
[Dashboards ▾]  [Energy]  [Security]  [Flows]
```

The picker lists dashboards in `order`. The first entry is always **House** (the fixed comprehensive view). The last entry is `＋ New dashboard`. The default landing (`/`) redirects to the first entry.

### Dashboards

A dashboard is a named, ordered collection of widgets.

```
Dashboard {
  id:       opaque string         // "d_" + random; House has id "house"
  name:     string                 // user-visible, unique per hub
  order:    integer                // index in the picker; 0 = House
  widgets:  Widget[]               // ordered
}
```

House is **synthesized on read** from the current entity/area state. The hub never stores a House record; the client cannot `PUT` or `DELETE` it. All other dashboards are persisted.

**House rendering.** `GET /api/v1/dashboards` returns House as `{ id: "house", name: "House", order: 0, widgets: [] }`. The Leptos client detects `id == "house"` and renders it with the **existing rooms-overview renderer** (today's `screens::home::HomeScreen` in the web crate), not the generic dashboard grid. The widget list is empty on purpose — House is not built from widgets; it is the comprehensive catalog, kept verbatim. (Per "stay in app, but renamed".)

**Dashboard order.** House has fixed `order = 0`, not writable. Authored dashboards use integer `order ≥ 1`; the hub normalizes to contiguous 1-based integers on write. `＋ New dashboard` creates at `max(order) + 1`.

### Widgets

```
Widget {
  id:     opaque string              // stable per widget within a dashboard
  kind:   WidgetKind
  span:   "1x1" | "2x1" | "2x2" | "1x2"
  config: WidgetConfig               // kind-specific
}
```

Widget kinds for v1 (one per entity/role the Oikos web already renders):

| kind            | config                     | behavior on click                     |
|-----------------|----------------------------|---------------------------------------|
| `light`         | `{ entity_id, title? }`    | toggle (same as rooms overview)       |
| `cover`         | `{ entity_id, title? }`    | open device sheet                     |
| `climate`       | `{ entity_id, title? }`    | open device sheet                     |
| `lock`          | `{ entity_id, title? }`    | open device sheet                     |
| `sensor`        | `{ entity_id, title? }`    | display only                          |
| `room-group`    | `{ area_id, title? }`      | act on the group (existing contract)  |
| `flow-button`   | `{ flow_id, title? }`      | trigger the flow                      |
| `energy-stat`   | `{ metric, title? }`       | display only                          |
| `history-chart` | `{ entity_id, range }`     | display only                          |
| `camera`        | `{ entity_id }`            | open the live stream                  |
| `water-heater`  | `{ entity_id, title? }`    | open device sheet                     |

Widget runtime behavior mirrors how the entity already renders in the rooms overview and device sheet; dashboards do not introduce a second interaction model.

**Allowed spans per widget kind.**

| kind                                                                             | allowed spans         |
|----------------------------------------------------------------------------------|-----------------------|
| light, cover, climate, lock, sensor, flow-button, water-heater, energy-stat      | `1x1`, `2x1`          |
| room-group                                                                       | `1x1`, `2x1`, `2x2`   |
| history-chart                                                                    | `2x1`, `2x2`, `1x2`   |
| camera                                                                           | `2x1`, `2x2`, `1x2`   |

The resize handle cycles through only the kind's allowed spans.

**Widget enumerations.**

- `history-chart.range`: one of `1h`, `6h`, `24h`, `7d`, `30d`.
- `energy-stat.metric`: the set of metrics the existing Energy page already exposes (reused verbatim — do not invent new metrics for v1). Current set at the time of writing: `today_kwh`, `week_kwh`, `month_kwh`, `grid_power_now`, `pv_power_now`, `self_consumption_pct`. The builder reads the Energy page's metric list in code and reuses it; the exact names come from there.

**Missing or orphaned widget targets.** The hub accepts `PUT` without validating that widget target ids exist (lenient). On render, the Leptos client shows a greyed "unavailable" placeholder tile for a widget whose `entity_id` / `flow_id` / `area_id` is unknown; the user can delete it in edit mode. The hub does not auto-prune.

### Grid

Column-based, adapts by viewport:

- Phone (≤ ~600 px): 2 columns. Any widget declared wider than 2 collapses to full-width (span `2x1` and `2x2` become 2-wide; `1x2` stays tall and 1-wide).
- Tablet / small desktop: 4 columns.
- Large desktop: 6 columns.

Rows flow around spans (CSS Grid with explicit `grid-column` / `grid-row` per widget). Breakpoint pixel values and the exact large-desktop column count are the builder's call.

### Edit mode

Carries the flow canvas pattern:

- A pencil button in each user-authored dashboard header toggles edit mode. The House dashboard has no pencil.
- In edit mode:
  - Widgets become draggable to reorder.
  - A corner handle on each widget cycles through its allowed spans.
  - Clicking a widget opens a right-side sheet with its config (entity picker, title override, size, delete).
  - A `＋ Add widget` button at the end of the grid opens a picker (same shape as the flow canvas's Add picker), grouped by widget kind.
  - A gear icon in the header opens a dashboard settings sheet (name, delete, move up / move down in the picker).
  - `Save` commits via `PUT /api/v1/dashboards/{id}`; `Discard` reverts to the last-saved state.
- The House dashboard never enters edit mode and never shows the pencil or gear.

### Dashboard management

- `＋ New dashboard` at the bottom of the picker opens a sheet that takes a name; it creates an empty dashboard at the next free order and navigates to it in edit mode.
- Rename, delete, and reorder live in the gear's settings sheet in edit mode.
- Deleting a dashboard removes it from the hub immediately on confirm; the picker falls back to the first remaining entry.

### Persistence (hub)

Dashboards persist as MQTT **retained payloads**, same shape as flows and areas. One topic per dashboard id under `<ui_prefix>/dashboards/<id>`. The hub keeps a `DashboardStore` (in-memory decoded copy of the retained payloads, mirroring `FlowStore`); the broker is the source of truth, so a hub restart replays the retained payloads and the store rebuilds.

```
GET    /api/v1/dashboards          → Dashboard[]                 // includes House at index 0
GET    /api/v1/dashboards/{id}     → Dashboard
PUT    /api/v1/dashboards/{id}     ← Dashboard                   // create or replace; 403 for id = "house"
DELETE /api/v1/dashboards/{id}     → 204                         // 403 for id = "house"
POST   /api/v1/dashboards/order    ← [id, id, …]                 // reorder atomically; "house" ignored if present, must remain at index 0
```

- `GET /api/v1/dashboards` returns the ordered list, with House synthesized at index 0 and all authored dashboards after it.
- `PUT` or `DELETE` on `id = "house"` returns 403.
- `POST /api/v1/dashboards/order` takes the full ordered id list of authored dashboards; the hub rewrites `order` on each affected dashboard and republishes them retained in one batch. This is the single reorder endpoint; the client never issues N `PUT`s for a reorder.
- The retained payload shape is the serialized `Dashboard` struct. Deleting a dashboard means publishing an empty retained payload on its topic (same convention as flows).

### Opening the app

- The web client fetches `GET /api/v1/dashboards` on load.
- `/` redirects to `/dashboards/{first_id}`.
- Each dashboard has a stable `/dashboards/{id}` URL that is linkable from elsewhere.
- The current `/` Home route is removed. Any existing link to `/` lands on the first dashboard (House by default).

### Realtime

- Widget state (light on/off, current temperature, sensor reading, energy value, camera stream) uses the **existing entity WebSocket stream** the Oikos web client already consumes.
- Dashboard structure (widget list, order, name) is **not** pushed over WebSocket. Single-user hub: edits only come from the active tab. On route change, the client re-fetches `GET /api/v1/dashboards`. No WS/SSE channel is added for dashboards in v1.

### Opening the Dashboards ▾ picker

- Clicking the picker opens a dropdown listing dashboards in `order`, with the current one marked. The last entry is `＋ New dashboard`.
- If the list grows long, the builder may swap the dropdown for a side sheet; the picker's semantics do not change.

## What's left to the builder

- Exact phone / tablet / desktop breakpoint pixel values and the large-desktop column count.
- The resize interaction detail (corner-handle cycling spans vs an eight-way handle). Cycle is the recommended default.
- Picker UI (dropdown vs side sheet) once the dashboard list grows long.
- The config sheet UI for each widget kind — follow the device-sheet pattern already in the app.
- The exact MQTT topic prefix (`<ui_prefix>/dashboards/`) — follow the config convention used for flows.
- The widget `id` generation scheme (ULID, UUID, or hash-based).
- The exact Leptos routing update for `/` → first dashboard, and link-preserving redirects.
- Any migration from existing bookmarks of `/` (soft: the redirect handles it).

## Spec traceability

- Replaces: the fixed Home tab in the Oikos web UI.
- Related memories:
  - `project_web_ui_v1_landed_2026-10-19` — the current Home / rooms overview.
  - `project_web_room_groups_landed_2026-10-20` — the room-group card contract reused by the `room-group` widget.
  - `project_device_sheet_landed_2026-10-20` — the device sheet pattern reused by widget config.
  - `project_flow_canvas_redesign_landed_2026-10-21` — the edit mode UX precedent.
  - `project_homey_gap_worktrees_2026-10-21` — context for why Kiosk Mode is out of v1 (marked P5).
- Depends on existing surfaces: flows (flow-button widget), areas (room-group widget), history (history-chart widget), energy (energy-stat widget), Frigate camera routes (camera widget), water-heater controller (water-heater widget).
