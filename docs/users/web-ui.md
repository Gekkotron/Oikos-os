---
title: The web UI
sidebar_position: 4
description: The screens of the browser app the hub serves, from dashboards and energy to security, devices, settings, alerts and flows.
---

The web UI is a Leptos app that runs in the browser as wasm. The hub serves it on its port for every path that is not `/api`, `/ws` or `/mcp`. Unknown paths fall back to `index.html`, so a deep link such as `/entity/<id>` loads the app. `index.html` is never cached; the hashed assets are. To build and develop the UI, see [Development](../contributors/development.md).

## Layout and themes

A sidebar on the left replaces the old top bar. From top to bottom it holds:

- **Overview**, with House first and your own dashboards under it, plus **＋ New dashboard**.
- **Automations** (flows) and **Groups**.
- **My rooms**: one entry per room, with the number of cards Home shows there. A room opens at `/rooms/<id>`; the old room chips on House are gone, and the rooms list lives here instead.
- **Home**: Energy, Security, Climate (beta), Alerts (with an unseen-count pill) and Settings.
- A connection badge at the bottom.

The header above the screen carries a breadcrumb, the theme button and the **FR · EN** button. The theme button cycles Auto, Night Glass and Ivoire, and the choice is remembered in the browser. FR · EN switches the shell (sidebar, header and breadcrumb) between French and English; the screens themselves are still in English for now, and the choice is remembered too.

The themes are **Night Glass** (dark), **Ivoire** (a light, warm ivory with sage and honey accents) and **Auto**, which follows the system. The former Warm Minimal theme is gone: a browser that had saved it opens in Ivoire.

On a phone or a narrow window (below 900 px) the sidebar becomes a drawer. The ☰ button at the left of the header opens it over the page; tapping the dimmed page, choosing an entry or widening the window closes it.

## Dashboards

![The House dashboard: the Favorites cards and the rooms](/img/screens/house.webp)

Dashboards (`/dashboards/<id>`): the sidebar lists the dashboards under Overview. House is always the first entry: the comprehensive rooms overview, synthesized by the hub (id `"house"`, non-editable, non-deletable), rendered on the client with the existing rooms-overview screen. Below House are user-authored named dashboards built from widgets. A widget is one of: `light`, `cover`, `climate`, `lock`, `sensor`, `room-group`, `flow-button`, `energy-stat`, `history-chart`, `camera`, `water-heater`, each with a kind-specific config and a `span` from a kind-specific set (`1x1`, `2x1`, `2x2`, `1x2`). The grid adapts phone (2 columns) ↔ tablet (4) ↔ desktop (6).

On a user-authored dashboard the header's pencil enters edit mode: click a widget to open its config sheet (title, size, delete), use move left/right to reorder, cycle the span with the resize button, Save or Discard. In edit mode a panel on the right (at the bottom on a phone) lists what can be added, grouped by kind, each row with its room and device; search narrows it, and clicking a row adds the widget at the end of the grid. The gear opens the dashboard settings sheet (name, delete, move up/down in the sidebar), and `＋ New dashboard` in the sidebar creates a new one at `max(order) + 1`.

Dashboards persist as MQTT retained payloads under `<ui_prefix>/dashboards/<id>`. The Home section's placement rule still applies on House: an entity with an energy role or shown on Security leaves House unless its override pins it with `homePinned`; cover/light rooms still fold into group cards exactly as before. A room's section shows only when the room has at least one card under that rule, so a room whose entities are all hidden or all elsewhere drops off House; it stays in the sidebar and the room pickers and comes back once something visible lands in it.

## Energy

![The Energy screen: live power flow between solar, grid, house and consumers](/img/screens/energy.webp)

Energy (`/energy`): the live power flow as in the app. Solar on top, the house in the centre, the grid on the left, consumers on an arc to the right, the battery or the water meter bottom-left; lines thicken and run faster with power and follow import or export. Under the diagram, one card per node with the live value and the lifetime kWh of its paired energy counter.

Roles come from the `energyRole` override (`solar`, `grid`, `grid_export`, `battery`, `house`, `consumer`, `water`), set from the entity sheet or over MCP.

## Security

Security (`/security`): the alarm panel as a hero card (arm modes, Disarm, inline code when the panel asks for one), the opted-in actions (`securityAction`, asking first unless `securityAsk` is off), a summary line ("1 smoke · 2 open" or "All clear"), the safety and intrusion sensors in sections (Openings, Motion, Fire, Water, Other) as two-column tiles, and the recent alerts. `securityHidden` takes a sensor off the tab.

## Climate

Climate (`/climate`, beta): one card per zone with its reading, target, state, overrides, learning and recent decisions, a quick hold of a setpoint for a while, the house settings sheet and the engine's banners. The room thermometer picker lists the temperature sensors grouped by room. Rules and proposals show on a week timeline; while editing, tapping an empty spot on a day adds a two-hour block there (base + 2 °C), and a block's popover has day chips to add it to, or remove it from, other days. See [Climate](climate.md).

## History

History (`/history/<id>`): a full-screen chart of one entity over 24 h, 7 d, 30 d or 1 y, picked with range chips. It needs `[history] url`; see [History](history.md).

## Groups

Groups (`/groups`): the hand-made groups, with a wizard to create one (`/groups/new`) or edit it, and a group sheet (`/group/<id>`) with its controls. See [Groups and virtual entities](groups-and-virtual-entities.md).

## Entity sheet

Entity sheet (`/entity/<id>`): the controls, the device info, the raw state and the override editor.

Back, on every sheet and detail page, returns to the page it was opened from, one step at a time: dashboard → group → member → history unwinds in that order. A form you saved (new or edited group, new flow) is skipped. A page opened directly, from a link or after a reload, goes back to its parent: Alerts for an alert, Flows for a flow, Groups for the group wizard, the entity for its history, the first dashboard for the rest.

## Devices

Devices: the administration view of the Zigbee devices. Each row shows the device's room (the rooms of its entities, or "No room"), its model and vendor, and badges for firmware updates, battery, link quality and entity count. The search matches the name, room, model, vendor and IEEE address.

## Settings

Settings: the hub status, an MCP panel with the endpoint and copy-ready connection snippets for Claude Code, Codex and Claude Desktop (built from the page's origin, so they always carry the hub's real address), and the theme switch (Auto, Night Glass, Ivoire). The snippets are also in [Talking to the house (MCP)](assistants-mcp.md).

## Alerts

Alerts (`/alerts`): the push alerts the hub keeps in `oikos/push/history`, with channel chips and day groups, and a "Recent alerts" section on Home. The detail (`/alerts/<id>`) shows the snapshot, plays the clip, offers the alert's buttons and "Open device". These are the same screens as the Android app; the body stays in the producer's language. See [Push notifications](push-notifications.md) and [Cameras (Frigate)](cameras.md).

## Flows

Flows (`/flows`, `/flows/<id>`): the flows list, grouped by room, with search, status and trigger filters, sorting and a List/Cards view; an enable switch per flow and a **New flow** button; and a full-bleed canvas per flow. See [Flows](flows.md) for what the canvas shows and how to edit it.
