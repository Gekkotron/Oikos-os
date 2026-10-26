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
- **My rooms**: one entry per room that has something on House, with the number of cards House shows in that room. A room opens at `/rooms/<id>`; the old room chips on House are gone, and the rooms list lives here instead.
- **Home**: Energy, Security, Climate (beta) and Alerts (with an unseen-count pill).
- **Settings** at the bottom, above the connection badge.

The header above the screen carries a breadcrumb, the theme button and the **FR · EN** button. The theme button cycles Auto, Night Glass and Ivoire, and the choice is remembered in the browser. FR · EN switches the shell (sidebar, header and breadcrumb) between French and English; the screens themselves are still in English for now, and the choice is remembered too.

The themes are **Night Glass** (dark), **Ivoire** (a light, warm ivory with sage and honey accents) and **Auto**, which follows the system. The former Warm Minimal theme is gone: a browser that had saved it opens in Ivoire.

On a phone or a narrow window (below 900 px) the sidebar becomes a drawer. The ☰ button at the left of the header opens it over the page; tapping the dimmed page, choosing an entry or widening the window closes it.

## Home header

The house overview (**House**, first under Overview) opens with a header above its cards: the date, a greeting, one sentence about the house, a detail line, a row of chips and, when weather is on, a weather card. Only the overview has it; your own dashboards do not.

On a wide window the text sits on the left and the weather card on the right. On a phone everything stacks, and the weather card becomes one compact row under the chips. Without a weather card, the sentence takes the full width and the sunset moves to the detail line.

The header is computed in the browser from what the screen already has: entities, rooms, alerts, the sun from `[flows]`, the weather and the home settings. Nothing is stored for it. When a part has no data, that part hides.

### Greeting

The date is written in the UI language, for example "Vendredi 23 octobre" or "Friday 23 October", using the house's local time. The greeting follows the hour:

| Hours | French | English |
| --- | --- | --- |
| 05:00 to 11:59 | Bonjour | Good morning |
| 12:00 to 17:59 | Bon après-midi | Good afternoon |
| 18:00 to 04:59 | Bonsoir | Good evening |

### The sentence

The sentence has two halves joined by a comma: the **activity** first, then the **temperature**. The first activity row that matches wins. The temperature half is left out when there is no indoor temperature.

| Half | When | French | English |
| --- | --- | --- | --- |
| Activity | An alert you have not seen yet (same count as the Alerts badge) | Quelque chose demande ton attention | Something needs your attention |
| Activity | One door or window open | Ouverture : Fenêtre (Cuisine) | Open: Window (Kitchen) |
| Activity | Several doors or windows open | 2 ouvertures | 2 doors or windows open |
| Activity | Dark outside, between 22:00 and 05:59, and no light on | La maison dort | The house is asleep |
| Activity | No light on | La maison est calme | The house is quiet |
| Activity | Anything else | La maison est éveillée | The house is awake |
| Temperature | Indoor temperature below the cold threshold | il fait froid | it's cold |
| Temperature | Indoor temperature above the hot threshold | il fait chaud | it's warm |
| Temperature | Anything else | il fait bon | it's comfortable |

Examples:

- « La maison est calme, il fait bon. » / "The house is quiet, it's comfortable."
- « La maison dort, il fait froid. » / "The house is asleep, it's cold."
- « Ouverture : Fenêtre (Cuisine), il fait froid. » / "Open: Window (Kitchen), it's cold."

The rules behind the table:

- The door or window line uses the entity's name. The room is added in brackets when the entity has one.
- "Dark" means between dusk and dawn at the location set in `[flows]`, as the flow canvas computes it.
- A light counts when it is on and not hidden. A light inside a group counts on its own; the group entity does not count, so no light is counted twice.
- The indoor temperature is the sensor you pick in [Settings](#settings), or else the median of the rooms: each room counts as the mean of its temperature sensors, offline sensors are left out, and so is any sensor that does not read in °C. One outdoor sensor cannot skew it. Outdoor weather sensors are never used. Offline lights, doors, windows and thermostats are ignored everywhere in the header. The sentence and the indoor chip show the same value.
- The cold and hot thresholds come from Settings › Home. They default to 15 °C and 26 °C.
- The same house state always gives the same sentence. It changes only when something in the house changes.

### Detail line

Under the sentence, in a muted style, the detail line holds up to two fragments joined by " · ". Each one appears only when it applies, in this order:

1. Rain expected within the hour (more than 0.1 mm): « Pluie attendue dans l'heure » / "Rain expected within the hour".
2. Frost tonight, shown from 15:00: « Gel cette nuit » / "Frost tonight".
3. The sunset time, shown from two hours before sunset until sunset, or at all times when the weather card is hidden: « Coucher du soleil à 18 h 42 » / "Sunset at 6:42 pm". When the house location is not set in `[flows]`, this fragment reads « Coucher du soleil : position de la maison non réglée » / "Sunset: house location not set" instead of a time.

### Chips

Chips sit in a row under the detail line. Each one hides when its input is missing.

- **Lights**: the number of lights on, for example « 3 lumières allumées » / "3 lights on". Shown in amber when at least one light is on, and hidden when none is.
- **Indoor**: the indoor temperature to one decimal, for example « 19,6 °C à l'intérieur » / "19.6 °C inside". Hidden without an indoor temperature.
- **Heating**: taken from the climate units that can heat. A unit counts as heating when its mode is heat, auto or heat_cool. A cooling-only unit is ignored.
  - « Clim en marche » / "Air conditioning on" when a unit is in cool mode. This case wins over the others.
  - « Chauffage éteint » / "Heating off" when no unit is heating.
  - « Chauffage en éco » / "Heating in eco" when every heating unit has a target at or below the eco setpoint. This case needs an eco setpoint in Settings › Home; without one, the chip hides.
  - Hidden in every other case, for example when a heating unit targets above the eco setpoint.
- **House mode**: the current option of the mode selector, shown as a pill with ▾. Tap it to open the list of the selector's options and pick one. Hidden when no mode selector is set in Settings › Home.

### Weather card

When weather is on, the card sits on the right of the header (or under the chips on a phone). It shows the condition icon, the current temperature, the condition, the "feels like" temperature, today's low and high, and the sunset time. Tap the card to open the [weather page](#weather-page).

The card hides when:

- weather is off, or the hub has not fetched it yet;
- the last fetch failed;
- the last good data is more than six hours old.

When the data is late but less than six hours old, the card stays and adds a muted "Data from 9:02 pm" line.

### Weather page

The weather page is at `/weather`, opened from the weather card. The Back button returns to the screen you came from. It is not in the sidebar.

- **Top**: the condition icon, the current temperature to one decimal, the condition, the feels-like temperature, today's low and high, tonight's minimum and the rain expected in the next hour.
- **Now**: eight tiles, four across on a wide window and two on a phone. Wind (speed, an arrow for the direction the wind comes from, a compass word and gusts), humidity, pressure, cloud cover, today's rain (amount and chance), the strongest gust today, the UV index with a word (Low, Moderate, High, Very high, Extreme), and the sun (sunrise and sunset).
- **Next 5 days**: one card per day with the weekday and date, the condition icon, the high, the low and the chance of rain.
- **Footer**: the place, the time of the last update, a stale notice when the data is late, and the Open-Meteo credit.

When weather is off or not fetched yet, the page says that the weather is unavailable and tells you to enable `[weather]` in `oikos.toml`.

## Dashboards

![The House dashboard: room chips and the Favorites cards](/img/screens/house.webp)

*This screenshot predates the sidebar: the rooms are now listed in the sidebar instead of as chips.*

Dashboards (`/dashboards/<id>`): the sidebar lists the dashboards under Overview. House is always the first entry, and the page the web UI opens on: the comprehensive rooms overview, synthesized by the hub (id `"house"`, non-editable, non-deletable), rendered on the client with the existing rooms-overview screen. Below House are user-authored named dashboards built from widgets. A widget is one of: `light`, `cover`, `climate`, `lock`, `sensor`, `room-group`, `flow-button`, `energy-stat`, `history-chart`, `camera`, `water-heater`, each with a kind-specific config and a `span` from a kind-specific set (`1x1`, `2x1`, `2x2`, `1x2`). The grid adapts phone (2 columns) ↔ tablet (4) ↔ desktop (6).

On a user-authored dashboard the header's pencil enters edit mode: click a widget to open its config sheet (title, size, delete; a history chart also has its range: 1 h, 6 h, 24 h, 7 d or 30 d), use move left/right to reorder, cycle the span with the resize button, Save or Discard. In edit mode a panel on the right (at the bottom on a phone) lists what can be added, grouped by kind, each row with its room and device; search narrows it, and clicking a row adds the widget at the end of the grid. The gear opens the dashboard settings sheet (name, a Show header switch that puts the House greeting, chips and weather card above the widgets, move up/down in the sidebar, delete), and `＋ New dashboard` in the sidebar creates a new one at `max(order) + 1`.

Dashboards persist as MQTT retained payloads under `<ui_prefix>/dashboards/<id>`. The Home section's placement rule still applies on House: an entity with an energy role or shown on Security leaves House unless its override pins it with `homePinned`; cover/light rooms still fold into group cards exactly as before. A room's section shows only when the room has at least one card under that rule, so a room whose entities are all hidden or all elsewhere drops off House and disappears from the sidebar too. Its `/rooms/<id>` page still opens and says "Nothing here yet.", the room pickers still offer it, and it comes back once something visible lands in it.

## Moods

A mood (in the sidebar, **My moods**) is a named list of actions that runs once, top to bottom, when you apply it. See [Ambiances](ambiances.md) for what an action does and how the hub runs it.

### Home

Home shows a row titled **One mood, one tap**, under the header. It holds up to four pinned moods, four across on a wide window and two by two on a phone. A tap applies the mood. While it runs, its tile is pressed, and a toast says **Good night: done**, or how many devices did not respond (**Good night: 2 devices did not respond**). The toast is the only report you get on Home; the full report is on the mood's page.

- A mood with no actions shows **Add a lamp** under its name. Tapping it opens the editor instead of applying it.
- When no mood is pinned, the row shows **Choose your moods**, which opens the page. When there are no moods at all, it shows **Create a mood** and **Add the four examples**.
- **How it works** at the end of the row opens a short explanation.
- The row stays hidden until the list of moods has loaded once.

### My moods

The page (`/ambiances`) lists every mood as a card, in order.

- **Show on Home**: pins the mood to the row. Only the first four pinned moods, by order, show on Home. The page says so when you pin more.
- **↑** and **↓** change the order.
- **Apply** runs the saved mood. **Edit** opens the editor. **Delete** removes it after a confirmation.
- **Last run** opens the report of the last run, if the mood ran since this page was opened.
- When the page is empty, it says **No mood yet.** and offers **New mood** or **Add the four examples**. The examples are *Good night*, *I'm heading out*, *Quiet evening* and *A little reading*; you can edit them like any other mood.
- A mood the hub cannot read shows its reason on its card. It cannot be applied, so only **Delete** is offered.

### Editor

The editor (`/ambiances/new`, or **Edit** on a card) has:

- **Name** (required), **Subtitle** and an **Icon**.
- **Actions**, in the order they run. Each one can be moved up or down, edited or deleted. **Add an action** offers four types:
  - **Every light, plug or shutter**: pick the kind, then **on** or **off** (lights can also get a brightness; shutters open or close). **Where** is the whole house or one room. Categories include devices you add later, so you do not have to edit the mood when you add a lamp. **See which** lists the devices in the category, grouped by room, with a checkbox on each; untick a device to leave it out.
  - **One device**: search for a device, then pick one of its commands. A command that takes a value shows a number field.
  - **Heating**: only shown when the climate engine is on. Pick a zone, or every zone. **Hold** sets a temperature and/or a mode for some **Hours**; **Back to the schedule** removes the hold.
  - **Automation**: pick a flow, then **Switch on**, **Switch off** or **Run**.
- **Save** stores the mood. It stays disabled until the name is filled in.
- **Try it** applies the saved version, so it is disabled while the form has unsaved changes. Its tooltip says **Save first**.

### Report

After a run, **Last run** opens the report (`/ambiances/<id>/report`), with the time of the run. Each action has an outcome: **Done**, **Failed** or **Skipped**. Failed and skipped actions list the devices concerned and the reason, such as offline. If the mood has not run since the page was opened, the report says **Not run since this page was opened.**

### How it works

**How it works** (`/ambiances/about`) explains the same rules as this page. A mood runs its actions once, in order. "Every light", "every plug" and "every shutter" include devices you add later. Only switches shown as a plug count as plugs, so set a switch to show as a plug to include it.

## Energy

![The Energy screen: live power flow between solar, grid, house and consumers](/img/screens/energy.webp)

Energy (`/energy`): the live power flow as in the app. Solar on top, the house in the centre, the grid on the left, consumers on an arc to the right, the battery or the water meter bottom-left; lines thicken and run faster with power and follow import or export. Under the diagram, one card per node with the live value and the lifetime kWh of its paired energy counter.

Roles come from the `energyRole` override (`solar`, `grid`, `grid_export`, `battery`, `house`, `consumer`, `water`), set from the entity sheet or over MCP.

Between the diagram and the cards, the **grid day chart** shows what crossed the grid meter on one day, in half-hour bars. What you bought from the grid sits above the zero line, off-peak (light blue) under peak (dark blue); what you sold, the solar export, sits below it in yellow. The off-peak periods are shaded behind the bars. Above the chart, the day's totals: bought with its off-peak and peak split, and sold. ‹ and › move to the previous or next day, from the first day with data up to today; today's bars fill up as the day goes on, refreshed about once a minute while the page is open. Hover or tap a bar for its half hour and values.

The chart reads the lifetime counters paired with the entities that carry the `grid` role (bought) and their export twin or a `grid_export` entity (sold), so it needs [history](history.md) and a grid meter with an energy counter. The split comes from the off-peak hours in Settings › Home; without them every bar is one colour ("Bought"). A day keeps the off-peak hours that were in force on it, so changing your contract does not recolour past days.

## Security

Security (`/security`): the alarm panel as a hero card (arm modes, Disarm, inline code when the panel asks for one), the opted-in actions (`securityAction`, asking first unless `securityAsk` is off), a summary line ("1 smoke · 2 open" or "All clear"), the safety and intrusion sensors in sections (Openings, Motion, Fire, Water, Other) as two-column tiles, and the recent alerts. `securityHidden` takes a sensor off the tab.

## Climate

Climate (`/climate`, beta) spans the page. The rooms are listed on the left (a row of chips on a phone), each with its reading, a dot in the colour of its target and where it stands. The selected room shows its status (reading, target, state, a quick hold of a setpoint for a while, and any hold with Clear), then its week as a calendar: seven day columns with the hours going down, each block coloured by its temperature (yellow below 17 °C, orange to 20 °C, red from 20 °C) and a line at the current time on today. Tapping a block shows its times. Below come the pending proposal, learning, the room thermometer field (the [entity picker](#picking-an-entity), opened on the thermometers) and the recent decisions; Settings opens the house settings sheet, and the engine's banners sit under the title. While editing the rules or a proposal, a block's ends drag up and down, tapping an empty spot on a day adds a two-hour block there (base + 2 °C), and a block's editor has day chips to add it to, or remove it from, other days. A room without rules offers **Create schedule**: an empty heating week (base 16 °C) to fill in; saving it approves it and a pending room becomes active. While editing, the base (the temperature outside the blocks) can be changed or set to Off. Switching rooms keeps an edit in progress. See [Climate](climate.md).

## History

History (`/history/<id>`): a full-screen chart of one entity over 1 h, 6 h, 24 h, 7 d, 30 d or 1 y, picked with range chips. It needs `[history] url`; see [History](history.md).

## Groups

Groups (`/groups`): the hand-made groups, with a wizard to create one (`/groups/new`) or edit it, and a group sheet (`/group/<id>`) with its controls. See [Groups and virtual entities](groups-and-virtual-entities.md).

## Cards

Every device card has the same frame: an icon tile, the room (left out under a room's own heading and in a sheet, where the room is already plain), the name, a short state line ("On · 14 %" / "Allumé · 14 %") and, at the top right, the card's main control (a toggle, a badge or a ⋯ menu). The words follow the FR/EN switch. A tile shows what you use every day; its sheet shows the same card with every control the device has.

- **Light**: toggle at the top right, brightness slider and colour swatch at the bottom.
- **Switch**: toggle and its state.
- **Climate**: the target sits in a ring. Drag the ring, or focus it and use the arrow keys (Page Up/Down: 1 °C, Home/End: the ends); the value is sent when you let go, or 0.6 s after the last key. − and + step at once. The ring follows the mode's colour and is greyed out while the mode is off. The mode is a badge you can change in place; the fan speed is in the sheet.
- **Temperature**: the value large, the humidity of the same device, and a small curve of the last 24 hours (it needs the history database; without it the card simply has no curve). The badge reads "Live" while the sensor reports, and "3 h ago" / "2 d ago" once it has been quiet for 3 hours, which is how a flat battery shows.
- **Vacuum**: two columns wide (a full row on a phone), a sentence for what the robot is doing ("At home.", "Cleaning.", "Needs help."), its battery, Start / Pause / Resume and Send home. Other actions and the fan speed are in the sheet.
- **Water heater**: the tank, the temperature and its caption; ⋯ opens the sheet with the target and the mode.

## Entity sheet

Entity sheet (`/entity/<id>`): the controls, the device info, the raw state and the override editor. The controls are the device's card with all its controls (see [Cards](#cards)).

Back, on every sheet and detail page, returns to the page it was opened from, one step at a time: dashboard → group → member → history unwinds in that order. A form you saved (new or edited group, new flow) is skipped. A page opened directly, from a link or after a reload, goes back to its parent: Alerts for an alert, Flows for a flow, Groups for the group wizard, the entity for its history, the House dashboard for the rest.

## Devices

Devices: the administration view of the Zigbee devices. Each row shows the device's room (the rooms of its entities, or "No room"), its model and vendor, and badges for firmware updates, battery, link quality and entity count. The search matches the name, room, model, vendor and IEEE address.

## Settings

Settings (`/settings`): a short list, one row per page, with a status on the right. A row opens its page at `/settings/<page>`, which has a Back link and its own breadcrumb (Settings › page):

- **Home** (**Maison**, `/settings/home`): the home header settings and the off-peak hours, described below.
- **Alarm** (`/settings/alarm`), status Off, Disarmed, or the armed mode: the optional native alarm. An Enable switch turns it on and off. The page lists the sensors (with their kind, the three mode ticks and instant or delayed), the sirens (an entity, or an MQTT topic with its payloads and a **Test chirp** button), the remotes with their signal names, presence (the trackers and the arm and disarm switches) and the timing (the delays and the motion rule). See [Alarm](alarm.md).
- **Appearance** (`/settings/appearance`), status the current theme: the theme switch (Auto, Night Glass, Ivoire), the same choice as the header's theme button.
- **Zigbee2MQTT** (`/settings/zigbee2mqtt`), status the number of firmware updates waiting, or the bridge state when there are none: the bridge (version, coordinator, permit join, state), the updates waiting, the counts (devices, entities, primary entities, areas, ignored topics) and the way to the [devices](#devices) list.
- **Broker** (`/settings/broker`), status Online, Connecting… or Offline: the MQTT connection, with the reason when it is down.
- **AI assistants (MCP)** (`/settings/mcp`): the endpoint and copy-ready connection snippets for Claude Code, Codex and Claude Desktop, built from the page's origin, so they always carry the hub's real address. The snippets are also in [Talking to the house (MCP)](assistants-mcp.md).
- **About** (`/settings/about`), status the version: the product, its version and the hub's address.

The **Home** page (**Maison** in French, `/settings/home`) holds the settings of the home header and the off-peak hours, with one Save button for the whole panel. Its fields:

- **Eco setpoint (°C)**: from 5 to 30. Leave it empty to turn off the "Heating in eco" case.
- **Cold below (°C)** and **Hot above (°C)**: the thresholds of the sentence, from 5 to 35. Each one shows its default (15 and 26) as a placeholder when empty. The cold threshold must stay below the hot one; an invalid value shows its error next to the field.
- **Mode selector**: the select entity behind the house mode pill. Choose "None" to hide the pill.
- **Indoor sensor**: the temperature sensor for the indoor value, chosen with the [entity picker](#picking-an-entity), which opens on the thermometers. "Automatic (median of rooms)" uses the median of the room values.
- **Off-peak hours**: the off-peak periods of your electricity contract, as start–end times (**Add a period**, **Remove**). A period may cross midnight (22:00–06:00); periods must not overlap. The same hours apply every day. A change applies from the start of the day you save it; earlier days keep the hours they had, and the first hours you set also cover the days recorded before them.

## Picking an entity

Every field that asks for an entity (the climate and home settings, the alarm's sensors, sirens, remotes and trackers, a flow card's entity, a mood's device action, a power sensor's energy counter, a group's members) opens the same picker. The closed field shows the chosen entity with its device, its room and its current value.

- **Search**: type part of a name, a device, a room or a type (`temperature`, `interrupteur`), in either language and without accents; the id works too.
- **Chips**: a field that needs one kind of entity opens on it (thermometers for the outdoor temperature, for example); **All** lists everything the field accepts, and **+ Diagnostic** adds the diagnostic, configuration and hidden entities.
- **Rows**: each row shows the entity, its device on the second line, and on the right its value and how long ago it changed, so two "Backlight mode" switches from two plugs are told apart.
- **On a computer** it is a window with the rooms on the left, each with its number of matches, and the entities on the right. **On a phone** it fills the screen and groups the entities under room headings.
- **Keys**: ↑ ↓ move, Enter picks, Esc closes. The browser's Back button also closes it.
- Fields that take several entities keep the picker open: tick each one, then **Done**. The chosen ones stay as chips under the field; tap a chip to remove it.
- With **All** and no search, the first 200 entities are shown; type or pick a room to see the rest.
- An entity the house no longer has stays in the field, marked "missing", until you replace or remove it.

## Alerts

Alerts (`/alerts`): the push alerts the hub keeps in `oikos/push/history`, with channel chips and day groups, and a "Recent alerts" section on Home. The detail (`/alerts/<id>`) shows the snapshot, plays the clip, offers the alert's buttons and "Open device". These are the same screens as the Android app; the body stays in the producer's language. See [Push notifications](push-notifications.md) and [Cameras (Frigate)](cameras.md).

## Flows

Flows (`/flows`, `/flows/<id>`): the flows list, grouped by room, with search, status and trigger filters, sorting and a List/Cards view; an enable switch per flow and a **New flow** button; and a full-bleed canvas per flow. See [Flows](flows.md) for what the canvas shows and how to edit it.
