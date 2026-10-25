---
title: HTTP API
sidebar_position: 2
description: Every route of the hub's REST API and WebSocket, with bodies, replies and status codes.
---

The hub listens on `[http] bind` (default `0.0.0.0:8081`). It answers JSON, with CORS open to any origin and no authentication in v1: keep it on the LAN. The router is in [`crates/oikos-os/src/http`](https://github.com/Gekkotron/Oikos-os/tree/main/crates/oikos-os/src/http) and the wire types in [`crates/oikos-api`](https://github.com/Gekkotron/Oikos-os/tree/main/crates/oikos-api).

Besides the JSON API, the same port serves:

- `GET /ws`, the [WebSocket](#websocket);
- `/mcp`, the MCP server (see [MCP tools](mcp-tools.md));
- the web UI, for every other path that is not under `/api`. An unknown path under `/api` answers a JSON `404`.

Ids in paths are the entity, flow, group or dashboard ids the API returns. Bodies are JSON; send `Content-Type: application/json`.

## Health and snapshot

| Route | What it does |
| --- | --- |
| `GET /health` | `{status, connection, entities}`: `status` is `"ok"`, `connection` the broker link (`{"type": "connecting" \| "online"}` or `{"type": "offline", "reason": "…"}`), `entities` the entity count. |
| `GET /api/v1/snapshot` | The whole house: connection, bridge, areas, devices, entities with their state, ignored topics, alerts, flows and `generated_at` (unix seconds). The same document is the MCP resource `oikos://snapshot`. |
| `GET /api/v1/ignored` | The discovery topics that did not become an entity, each `{topic, reason}`. |

## Entities

| Route | What it does |
| --- | --- |
| `POST /api/v1/entities/{id}/command` | Sends a command (`{"type": "turn_on", …}`) and reports `{accepted, reason, state}`: `accepted` is `true` once the entity's state echo confirms it within `[commands] confirm_timeout_ms`, `false` with a `reason` when nothing came back in time. `404` for an unknown entity, `422` for a command the entity cannot take or an offline entity, `503` while the broker link is down. |
| `PATCH /api/v1/entities/{id}/override` | Changes fields of the entity's `oikos/ui/<id>` override and returns the entity as it now appears. The body is a partial override; `null` clears a field. `400` for an unknown field or a bad value (the error names the valid ones), `404`, `503`. The fields are listed in [Entity types](entity-types.md#overrides). |
| `DELETE /api/v1/entities/{id}/override` | Removes the override (`204`). |
| `GET /api/v1/entities/{id}/picture` | The picture of a camera or image entity: the stored bytes, or the bytes of its URL fetched by the hub now (10 s timeout, 5 MiB cap, no redirects followed). The reply is served as an image or as an opaque download, with `X-Content-Type-Options: nosniff`. `404` when the entity has no picture, `502` when the URL cannot be fetched. |

## Areas

| Route | What it does |
| --- | --- |
| `PUT /api/v1/areas` | Replaces the declared rooms (`oikos/ui/_areas`) and returns the resulting list. |
| `PATCH /api/v1/areas/{id}` | Renames a room: body `{"name": "…"}`; the room's other fields are kept. Returns the area list. `422` for a blank name or for `Unassigned`, `404` for an unknown area, `503` while the broker link is down. |

## Home settings

| Route | What it does |
| --- | --- |
| `GET /api/v1/home/settings` | The home settings document `{eco_setpoint, mode_entity, indoor_entity, cold_below, hot_above, off_peak, off_peak_history}` (numbers and ids `null`, lists empty until saved). |
| `PUT /api/v1/home/settings` | Replaces the document and returns it as stored (retained on `oikos/ui/_home`). `eco_setpoint` is a number from 5 to 30 (°C), `mode_entity` the id of a `select` entity, `indoor_entity` the id of a temperature sensor; `cold_below` and `hot_above` are numbers from 5 to 35 (°C) with `cold_below` lower than `hot_above` (the web reads cold below 15 and warm above 26 when unset); `null` clears a field. `off_peak` is a list of `{start, end}` local `HH:MM` windows (`end` before `start` crosses midnight; none empty or overlapping; empty for a single tariff). `off_peak_history` (`[{since: "YYYY-MM-DD", windows}, …]`) is kept by the hub: a change of `off_peak` adds an entry dated today, and a value sent by the client is ignored. `422` for a value that does not validate (the error names the field), `503` while the broker link is down. |

## Groups

Groups need `[groups] enabled = true`; while it is `false` every route here answers `503` with `groups disabled`. See [Groups and virtual entities](../users/groups-and-virtual-entities.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/groups` | The stored groups, sorted by id, each with its members, its entity and the names of the flows that use it. |
| `PUT /api/v1/groups/{id}` | Creates or replaces a group: body `{"kind", "name", "members": […], "hideMembers"?}`. Returns the group. `422` for a bad id, an unsupported kind, a member that is itself a group or a member of another kind than the group's; `503` while groups are off or the broker link is down. |
| `DELETE /api/v1/groups/{id}` | Removes the group and clears what the broker retained for it (`204`); `404` for an id that is not a group. |

## Virtual entities

| Route | What it does |
| --- | --- |
| `PUT /api/v1/virtual/{id}` | Creates or replaces a virtual switch, number, button or select (`{"kind", "name", …}`) and returns the entity `virtual_<id>`; the initial state is published only when it is new, except that redefining a select whose current state is no longer among its options resets it to the first option. `422` for a bad id or spec, `503` while the broker link is down. |
| `DELETE /api/v1/virtual/{id}` | Removes a virtual entity and clears its retained topics (`204`), or `404`. |

The body takes `kind` (`switch`, `number`, `button` or `select`) and `name`; for a number also `min` (0), `max` (100), `step` (1) and `unit`; for a select `options` (1 to 32 distinct strings of up to 64 characters, required); `initial` is the state published when the entity is first created. The id is 1 to 48 characters of `a-z`, `0-9` and `_`.

## Dashboards

| Route | What it does |
| --- | --- |
| `GET /api/v1/dashboards` | The dashboards as `{dashboard, synthesized}`: `house` first (synthesized by the hub), then the authored ones sorted by `order`. |
| `GET /api/v1/dashboards/{id}` | One dashboard; `404` for an unknown id. |
| `PUT /api/v1/dashboards/{id}` | Creates or replaces a dashboard (`{"id", "name", "order", "widgets": […]}`) and returns it. The body `id` must equal the path id. An `order` of 0 or one already taken is replaced by the next free one. `403` for the reserved id `house`, `422` when the ids differ or a widget does not validate, `503` while the broker link is down. |
| `DELETE /api/v1/dashboards/{id}` | Removes a dashboard (`204`); `403` for `house`, `404` for an unknown id. |
| `POST /api/v1/dashboards/order` | `{"order": ["<id>", …]}` reassigns contiguous orders by position (`204`). The list must name every authored dashboard exactly once (`house` is ignored); otherwise `422`, and nothing changes. |

A widget `kind` is one of `light`, `cover`, `climate`, `lock`, `sensor`, `room-group`, `flow-button`, `energy-stat`, `history-chart`, `camera` or `water-heater`, with a `span` from the kind's allowed set (`1x1`, `2x1`, `2x2`, `1x2`). A dashboard id is 1 to 64 characters of letters, digits, `_` and `-`, and not `house`.

## Ambiances

See [Ambiances](../users/ambiances.md) for the action types and the report. An ambiance id is 1 to 48 characters of `a-z`, `0-9` and `_`. The body of `PUT` is `{"id", "name", "subtitle"?, "icon"?, "pinned", "order", "actions": […]}`; an `id` in the body may be left out (the path id is used) and must equal the path id otherwise. Unknown top-level fields are kept.

| Route | What it does |
| --- | --- |
| `GET /api/v1/ambiances` | Every stored ambiance as `{id, ambiance, invalid}`: valid ones first, by `order`, then the invalid ones with the reason in `invalid`. |
| `PUT /api/v1/ambiances/{id}` | Creates or replaces an ambiance and returns its view. The shape is checked, then the house: unknown entities, areas, flows, zones and `except` ids are refused, and a climate action is refused while the climate engine is off. An action equal to one of the stored version is not checked again. `422` for a body or a reference the house refuses (the error names the action by its index, e.g. `action 2: …`), `503` while the broker link is down. |
| `DELETE /api/v1/ambiances/{id}` | Removes an ambiance (`204`), valid or not. `404` for an id that is not an ambiance; `503` while the broker link is down. |
| `POST /api/v1/ambiances/{id}/apply` | Runs the actions once, in order, and returns the [apply report](../users/ambiances.md#reading-the-report) (`200`). `404` for an unknown id; `422` for an invalid ambiance or one that is already running; `503` while the broker link is down. The run finishes and broadcasts `ambiance_applied` even if the request is dropped. Categories leave out group entities (the hub's and Zigbee2MQTT groups), virtual entities, the Zigbee2MQTT bridge, covers whose device class is not a window covering, and switches whose `showAs` is not `plug` (an untyped switch is a plain switch, not a plug). A flow `run` action is `failed` when the flow is disabled, invalid or already running; an action whose every target is offline is `skipped` with the reason `every target is offline`. |

## Flows

See [Flow nodes](flow-nodes.md) for the document format.

| Route | What it does |
| --- | --- |
| `GET /api/v1/flows` | Every flow with its engine state: definition, why it is invalid, the active run and the recent runs. |
| `GET /api/v1/flows/{id}` | One flow; `404` for an unknown id. |
| `PUT /api/v1/flows/{id}` | Creates or replaces a flow (`oikos/automation/<id>`). The path id wins: a body without `id` takes it, another one is `400 id mismatch`. Returns `{flow, warning}`; `warning` is `null`, or a message when the flow reads the sun while the house point is the Paris fallback. `422` for a flow that does not validate against the house, `503` while the broker link is down. |
| `DELETE /api/v1/flows/{id}` | Removes the flow (`204`), or `404`. |
| `POST /api/v1/flows/{id}/enabled` | `{"enabled": bool}`; returns the flow. |
| `POST /api/v1/flows/{id}/run` | Starts a run by hand; `202` with the run, `422` when the flow is disabled, invalid or already running. |
| `POST /api/v1/flows/{id}/area` | `{"area": "<id>" \| null}` sets the room the flow is listed under (`null` clears it; the key is required); returns the flow. `422` for an unknown room, a body without `area` or an invalid flow, `404` for an unknown flow. |
| `GET /api/v1/schedule?cron=…&count=…` | Previews a 5-field cron in the house time zone: `{timezone, next: […]}` with the next `count` ticks as RFC 3339 (default 3, at most 10). `422` for a bad cron. |
| `GET /api/v1/config/flows` | The `[flows]` settings as the UI needs them: `{latitude, longitude, timezone, fallback_active}`. |

## Alerts and push

| Route | What it does |
| --- | --- |
| `GET /api/v1/alerts` | The push alerts the hub keeps in its retained history (`oikos/push/history`), newest first. |
| `POST /api/v1/alerts/{id}/actions/{action}` | Publishes the reply the app publishes when a button is tapped (`{"id","action","install_id":"oikos-web","ts"}` on `oikos/push/responses`); `202`, or `404` for an unknown alert or action, `503` while the broker link is down. |
| `GET /api/v1/push` | Push status: `{"state","installs","queued"}`. `state` is `off` (no usable Firebase key: neither `[push] credentials` nor `secrets/firebase.json`), `syncing` (rebuilding the token map after a connect) or `ready`; `installs` counts the registered app installs, `queued` the alerts waiting in the retry queue. |

## Media

| Route | What it does |
| --- | --- |
| `GET /api/v1/media?url=…` | A Frigate snapshot or clip fetched by the hub with the Frigate login (`[frigate]`). `Range` is passed through so clips seek. `400` without a valid absolute `url`, `403` off the Frigate origin or without a `[frigate] url`, `502` when Frigate is unreachable or refuses the login. |

When alerts point at Frigate's login port (8971) the browser cannot load them, so media on that origin is rewritten to `/api/v1/media?url=…`. The hub logs in on demand, and once more on a `401`.

## History

Available when `[history] url` is set. See [History](../users/history.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/history/series?entity=&range=` | A chart series over `24h`, `7d`, `30d` or `1y`: `{window, points: [[t, mean, min, max], …]}`, times in unix seconds. |
| `GET /api/v1/history/daily?entity=&days=&kind=` | kWh per calendar day for the last `days` days (7, 30 or 365), with `kind` (`counter` or `power`): `{days: [[t, kwh], …]}`. |
| `GET /api/v1/history/today?entity=&kind=` | kWh since local midnight: `{kwh, since}`. |
| `GET /api/v1/history/last_change?entity=&entity=…` | Last change time of 1 to 50 entities, in request order: `{items: [{changed_at, recorded}, …]}`. |
| `GET /api/v1/history/grid_day?date=YYYY-MM-DD` | What crossed the grid meter on one house-local day (today when `date` is absent): `{date, today, first_date, day_slots, slots: [{start, local, off_peak, peak, export}, …], bands: [[from, to], …], totals: {off_peak, peak, export}, has_export, tariffs}`. Slots are half hours of real time from local midnight (46, 48 or 50 a day, `day_slots`; today stops at the current one), `start` in unix seconds and `local` as `HH:MM`; kWh are counter increases, bought split by the off-peak windows in force that day (`bands`, unix seconds). The counters are those paired with the `grid` role (bought) and their export twin or `grid_export` (sold). `400 bad_request` for a bad or future date, `404 no_grid_meter` without a grid counter, `503 history_unavailable`. |

Errors are `400 bad_request` (a missing entity, an unknown range, `days` other than 7, 30 or 365, an unknown `kind`, or no `entity` at all), `404 not_recorded` (the entity has no rows) and `503 history_unavailable` (`[history] url` not set, or the database is down).

## Climate

Available when `[climate] enabled = true`; otherwise these routes answer `503`. See [Climate](../users/climate.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/climate` | The climate engine: house settings (with `eco`) and every zone with its reading, decision and why, overrides, its eco (`eco: {until, set_at}` or `null`), pending proposal and learner state. |
| `PUT /api/v1/climate/house` | Replaces the house settings (`204`); `422` when they do not validate. |
| `PUT /api/v1/climate/zones/{id}` | Replaces a zone's settings (`204`); `404` for an area that is not a zone, `422` when they do not validate. |
| `POST /api/v1/climate/zones/{id}/override` | `{setpoint?, mode?, minutes?, until?}`: holds a target and/or mode, with exactly one of `minutes` and `until` (`204`, else `422`). |
| `DELETE /api/v1/climate/zones/{id}/override` | Clears the zone's overrides (`204`). |
| `PUT /api/v1/climate/zones/{id}/eco` | `{until}` (unix seconds): holds the zone in eco until then (`204`; a past `until` or an `off` zone is `422`). |
| `DELETE /api/v1/climate/zones/{id}/eco` | Ends the zone's eco (`204`). |
| `POST /api/v1/climate/zones/{id}/proposal/approve` | Approves the pending proposal, optionally with edited `Rules` as the body (`204`); `422` when there is none. |
| `POST /api/v1/climate/zones/{id}/proposal/reject` | Drops the pending proposal (`204`); `422` when there is none. |
| `POST /api/v1/climate/zones/{id}/learn` | Learns the zone now; `200` with the learner state. |
| `GET /api/v1/climate/zones/{id}/decisions?limit=` | `{recent, older}`: the latest decisions from memory, then older rows from history when it is on (`limit` 50 by default, 500 at most; `400` for a `limit` that is not a number). |

## Alarm

The native alarm is off until its settings have `enabled` true; the routes answer as usual meanwhile. See [Alarm](../users/alarm.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/alarm` | The alarm as one document (`200`): the settings, the panel state, mode, source, `deadline`, `cause`, `open_at_arm`, any refused settings and the run-time warnings. |
| `PUT /api/v1/alarm/settings` | Replaces the settings document (`204`); `422` when it does not validate or names an entity the house does not have, `503` while the broker link is down. |
| `POST /api/v1/alarm/test-chirp` | `{"siren": <index>}`, the index into `settings.sirens`: sounds that siren's chirp (`204`). `422` when the index is out of range, that siren has no chirp, or the alarm is ringing; `503` while the broker link is down. |

## Weather

Available when `[weather] enabled = true`. See [Weather](../users/weather.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/weather` | The outdoor weather as one document (below). `503` while `[weather]` is off, the city was not found, or no fetch has succeeded yet. |

```json
{
  "place": { "name": "Paris", "admin": "Île-de-France", "country": "FR" },
  "fetched_at": 1792746902,
  "stale": false,
  "current": {
    "temperature": 13.4, "apparent_temperature": 11.9, "humidity": 81,
    "condition": "cloudy", "daylight": true, "cloud_cover": 92,
    "precipitation": 0.0, "wind_speed": 14.0, "wind_gusts": 31.0,
    "wind_direction": 230, "pressure": 1012.3, "uv_index": 1.2
  },
  "today": {
    "min": 9.1, "max": 15.0, "precipitation": 2.4,
    "precipitation_probability": 70, "wind_gusts_max": 45.0
  },
  "rain_next_hour": 0.3,
  "tonight_min": 7.8,
  "days": [
    { "date": "2026-10-24", "condition": "rain", "min": 8.0, "max": 13.5, "precipitation_probability": 85 }
  ]
}
```

Units are metric: °C, km/h, mm, hPa and %. `fetched_at` is unix seconds. Any number may be `null` when Open-Meteo has no value. `days` holds the next five days, starting tomorrow. There are no coordinates in the response. `stale` is `true` once the last fetch is older than twice `refresh_minutes`.

## Solar

Available when `[solcast] enabled = true`. See [Solar forecast](../users/solar-forecast.md).

| Route | What it does |
| --- | --- |
| `GET /api/v1/solar` | The solar forecast as one document (below). `503` with `solar unavailable` while `[solcast]` is off or before the first fetch. |

```json
{
  "fetched_at": 1792746902, "stale": false, "next_refresh_at": 1792754100,
  "calls_today": 6, "daily_limit": 8,
  "now": { "power": 1.6, "remaining_today": 5.2 },
  "today": {
    "date": "2026-10-23", "estimate": 12.4, "low": 9.8, "high": 14.1,
    "peak": { "power": 2.4, "at": 1792758600 },
    "best_hours": { "from": 1792749600, "to": 1792764000 }
  },
  "days": [ { "date": "2026-10-24", "estimate": 8.1, "low": 5.0, "high": 11.0 } ],
  "sites": [ { "index": 0, "today": 5.1 }, { "index": 1, "today": 7.3 } ],
  "periods": [ { "end": 1792746000, "minutes": 30, "estimate": 0.0, "low": 0.0, "high": 0.0 } ],
  "history": {
    "forecasts": [
      { "date": "2026-10-22", "estimate": 11.6, "low": 8.9, "high": 13.0, "made_at": 1792680000 }
    ]
  }
}
```

Energies are kWh (the day figures), powers kW (the period values, the means over each period), times Unix seconds and dates `YYYY-MM-DD` in the house's time zone. Any number may be `null`. `next_refresh_at` is `null` when no call is planned (budget spent, key refused). `days` holds the next seven days, starting tomorrow. `periods` are the 30-minute forecast periods from local midnight today to the end of tomorrow, each with its mean power in kW (`estimate`, `low`, `high`) and its `minutes` length. `sites` never carries the resource id; `index` is its position in `[solcast] sites`. `best_hours` is the longest run today where the estimate is at least 60 % of the peak, or `null` on a day with no production. `history.forecasts` lists the morning forecast of each of the 7 local days before today, oldest first, and is empty when history is off. `stale` is `true` when a planned refresh has failed for over an hour or the last success is older than 18 hours.

## WebSocket

`GET /ws` upgrades to a WebSocket. The hub sends a `snapshot` frame first, then every hub event as a JSON text frame tagged by `type`. It subscribes before it takes the snapshot, so no event falls between the two. A client that falls behind gets a fresh `snapshot` instead of being dropped. Incoming frames are ignored.

| `type` | Payload |
| --- | --- |
| `snapshot` | The whole [snapshot](#health-and-snapshot). |
| `entity_upserted` | An entity view. |
| `entity_removed` | `{id}`. |
| `state_changed` | `{id, state}`. |
| `areas_changed` | `{areas}`. |
| `devices_changed` | `{devices}`. |
| `alerts_changed` | `{alerts}`. |
| `flows_changed` | `{flows}`. |
| `flow_run_changed` | `{flow_id, run}`. |
| `bridge_changed` | The Zigbee2MQTT bridge view. |
| `connection_changed` | The broker link status. |
| `command_failed` | `{id, reason}`. |
| `button_pressed` | `{id}`: a virtual or discovered button was pressed. An event, not a state. |
| `signal_received` | `{id, signal}`: a remote, event entity or tag sent a signal. An event, not a state. |
| `weather_changed` | No payload: the weather was refreshed with new data, refetch `GET /api/v1/weather`. |
| `solar_changed` | No payload: the solar forecast was refreshed or its current figures moved, refetch `GET /api/v1/solar`. |
| `home_settings_changed` | `{settings}`: the home settings document changed. |
| `climate_changed` | `{zone}`: one zone's document changed, or the house settings when `zone` is `null`. |
| `alarm_changed` | No payload: the alarm's settings, state or warnings changed, refetch `GET /api/v1/alarm`. The entity's own state arrives as `state_changed`. |
| `ambiances_changed` | `{ambiances}`: the stored ambiances (the same list as `GET /api/v1/ambiances`) after one was saved, deleted or arrived from the broker. |
| `ambiance_applied` | `{report}`: the [apply report](../users/ambiances.md#reading-the-report) of a run, whichever client started it. |

## Errors

Errors are `{"error": "…"}` with one of these statuses:

| Status | When |
| --- | --- |
| `400` | A bad override patch, a flow body whose `id` differs from the path (`id mismatch`), a bad query parameter, or a body that is not valid JSON of the expected shape. |
| `403` | The reserved `house` dashboard id, or media off the Frigate origin. |
| `404` | An unknown entity, alert, action, flow, area, dashboard, group, virtual entity, ambiance or route. |
| `422` | A command refused, a bad virtual or group id or spec, a flow that is invalid, disabled or already running, an unknown room, a dashboard that does not validate, or an ambiance that does not validate, names something the house does not know, or is already running. |
| `502` | A picture URL or Frigate media that cannot be fetched. |
| `503` | The broker link is down, so the write was refused and nothing was applied; or the feature is off (`[groups]`, `[climate]`, `[history]`). The same publish refusal is an in-band tool error over MCP. |

History routes use short codes as the message: `bad_request`, `not_recorded` and `history_unavailable`.
