---
title: The web UI
sidebar_position: 4
description: The screens of the browser app the hub serves, from dashboards and energy to security, devices, settings, alerts and flows.
---

The web UI is a Leptos app that runs in the browser as wasm. The hub serves it on its port for every path that is not `/api`, `/ws` or `/mcp`. Unknown paths fall back to `index.html`, so a deep link such as `/entity/<id>` loads the app. `index.html` is never cached; the hashed assets are. To build and develop the UI, see [Development](../contributors/development.md).

## Dashboards

![The House dashboard: room chips and the Favorites cards](/img/screens/house.webp)

Dashboards (`/dashboards/<id>`): the top nav opens a `Dashboards ▾` picker. House is always the first entry: the comprehensive rooms overview, synthesized by the hub (id `"house"`, non-editable, non-deletable), rendered on the client with the existing rooms-overview screen. Below House are user-authored named dashboards built from widgets. A widget is one of: `light`, `cover`, `climate`, `lock`, `sensor`, `room-group`, `flow-button`, `energy-stat`, `history-chart`, `camera`, `water-heater`, each with a kind-specific config and a `span` from a kind-specific set (`1x1`, `2x1`, `2x2`, `1x2`). The grid adapts phone (2 columns) ↔ tablet (4) ↔ desktop (6).

On a user-authored dashboard the header's pencil enters edit mode: click a widget to open its config sheet (title, size, delete), use move left/right to reorder, cycle the span with the resize button, Save or Discard. The gear opens the dashboard settings sheet (name, delete, move up/down in the picker), and `＋ New dashboard` at the bottom of the picker creates a new one at `max(order) + 1`.

Dashboards persist as MQTT retained payloads under `<ui_prefix>/dashboards/<id>`. The Home section's placement rule still applies on House: an entity with an energy role or shown on Security leaves House unless its override pins it with `homePinned`; cover/light rooms still fold into group cards exactly as before.

## Energy

![The Energy screen: live power flow between solar, grid, house and consumers](/img/screens/energy.webp)

Energy (`/energy`): the live power flow as in the app. Solar on top, the house in the centre, the grid on the left, consumers on an arc to the right, the battery or the water meter bottom-left; lines thicken and run faster with power and follow import or export. Under the diagram, one card per node with the live value and the lifetime kWh of its paired energy counter.

Roles come from the `energyRole` override (`solar`, `grid`, `grid_export`, `battery`, `house`, `consumer`, `water`), set from the entity sheet or over MCP.

## Security

Security (`/security`): the alarm panel as a hero card (arm modes, Disarm, inline code when the panel asks for one), the opted-in actions (`securityAction`, asking first unless `securityAsk` is off), a summary line ("1 smoke · 2 open" or "All clear"), the safety and intrusion sensors in sections (Openings, Motion, Fire, Water, Other) as two-column tiles, and the recent alerts. `securityHidden` takes a sensor off the tab.

## Climate

Climate (`/climate`): one card per zone with its reading, target, state, overrides, learning and recent decisions, a quick hold of a setpoint for a while, the house settings sheet and the engine's banners. See [Climate](climate.md).

## History

History (`/history/<id>`): a full-screen chart of one entity over 24 h, 7 d, 30 d or 1 y, picked with range chips. It needs `[history] url`; see [History](history.md).

## Groups

Groups (`/groups`): the hand-made groups, with a wizard to create one (`/groups/new`) or edit it, and a group sheet (`/group/<id>`) with its controls. See [Groups and virtual entities](groups-and-virtual-entities.md).

## Entity sheet

Entity sheet (`/entity/<id>`): the controls, the device info, the raw state and the override editor.

## Devices

Devices: the administration view of the Zigbee devices.

## Settings

Settings: the hub status, an MCP panel with the endpoint and copy-ready connection snippets for Claude Code, Codex and Claude Desktop (built from the page's origin, so they always carry the hub's real address), and the theme switch (Auto, Night, Warm). The snippets are also in [Talking to the house (MCP)](assistants-mcp.md).

## Alerts

Alerts (`/alerts`): the push alerts the hub keeps in `oikos/push/history`, with channel chips and day groups, and a "Recent alerts" section on Home. The detail (`/alerts/<id>`) shows the snapshot, plays the clip, offers the alert's buttons and "Open device". These are the same screens as the Android app; the body stays in the producer's language. See [Push notifications](push-notifications.md) and [Cameras (Frigate)](cameras.md).

## Flows

Flows (`/flows`, `/flows/<id>`): the flows list, grouped by room, with search, status and trigger filters, sorting and a List/Cards view; an enable switch per flow and a **New flow** button; and a full-bleed canvas per flow. See [Flows](flows.md) for what the canvas shows and how to edit it.
