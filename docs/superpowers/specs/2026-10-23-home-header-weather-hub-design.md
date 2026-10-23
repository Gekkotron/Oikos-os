# Home header, hub side: weather, home settings and select virtual entities

Status: design, 2026-10-23. Project 3 of the Ivoire redesign, hub part.

## 1. Intent

The Ivoire redesign (project 1, `docs/superpowers/specs/2026-10-23-ivoire-theme-and-sidebar-design.md` on `feat/ivoire-redesign`) puts a **home header** on top of the house overview: a date, a greeting sentence, summary chips (lights on, indoor temperature, heating in eco, house mode), today's sunset and the outdoor weather. Like every feature of the redesign, it exists in every theme. A theme only changes the look.

Most of the header can be computed in the browser from what the web UI already receives: entities, areas, alarm state, and the sun from `[flows]` via `oikos_core::flow::sun`. Three things are missing on the hub, and this design covers them:

1. **Outdoor weather.** Nothing in the repo provides it. The user wants the hub to fetch it from Open-Meteo, for the header **and for flows**: "donne aussi toutes les données qui pourraient être utiles pour les flows".
2. **Home settings.** The header needs a few per-house choices: the eco setpoint, the house mode entity and the indoor temperature sensor. They must be settable from the web Settings screen and must not depend on the climate engine, since the climate screen only exists when the engine is enabled.
3. **A house mode the user owns.** The user wants a virtual entity whose value (home, away, night) **their flows manage**. The hub derives nothing: "juste un virtual d'alarme et c'est des flows qui géreront l'état". That needs a new `select` kind of virtual entity.

The web header itself is a later project. It waits until project 1 is merged, because project 1 owns `app.rs`, `shell/`, `styles/`, `theme.rs`, `i18n.rs` and `home.rs`. §7 records what the user decided for it, so the next design starts from those decisions.

The project is generic: nothing below names a machine, an address or a room of the user's house.

## 2. Goals, non-goals, anti-goals

**Goals**
- The hub fetches current weather, today's figures, the next hour and a 5-day forecast from Open-Meteo for a city named in `oikos.toml`, opt-in (§3).
- Weather values are **hub-published entities** (`weather_…`), so flows can trigger on them and test them, history records them, and `[climate] outdoor_entity` can point at the outdoor temperature (§4).
- `GET /api/v1/weather` returns everything in one object for the header, including the forecast days, with freshness information (§5).
- A retained **home settings** document holds `eco_setpoint`, `mode_entity` and `indoor_entity`. It is read and written over REST, WS and MCP (§6).
- Virtual entities gain a **`select`** kind (a fixed list of options), usable as the house mode (§6.3).
- The `docs/` pages are updated (§8).

**Non-goals**
- Any web code: the header, the Settings fields, and showing the weather or mode. That is the next project.
- Other weather providers (met.no, an MQTT weather entity as a source). The provider is a single enum value for now, so another one can be added later.
- Imperial units. Values are metric (°C, km/h, mm, hPa). This is my call, since the user is metric and the rest of the hub is too.
- Hub-computed house mode, eco state or indoor temperature. The hub stores the settings, and the client derives the values (§7).
- A per-house alarm or presence engine. Presence logic stays in the user's flows.
- Forecast days as entities. They exist only in the API, for display.
- Weather alerts and warnings (storm warnings and similar).

**Anti-goals** (failure even if it technically works)
- The hub refusing to start, or the house UI breaking, because Open-Meteo is unreachable or the city cannot be resolved. Weather is optional, like history and push.
- House coordinates or the city leaving the LAN while `[weather]` is not enabled. No request goes out unless the user opted in.
- The browser calling Open-Meteo itself. All traffic goes through the hub.
- Stale weather shown or acted on as if it were current. A failed source must turn into offline entities and a `stale` flag, never into silently frozen values.
- Touching `[flows] latitude/longitude/timezone`. Weather and flows are two separate things (user: "two different things, a timezone for flows and a city for the weather").

## 3. Weather source (decided with the user)

**Provider: Open-Meteo**, free tier, no API key. Two endpoints:
- Geocoding: `https://geocoding-api.open-meteo.com/v1/search?name=<city>&count=10&format=json`, plus `&countryCode=<CC>` when `country` is set.
- Forecast: `https://api.open-meteo.com/v1/forecast?latitude=…&longitude=…&timezone=auto&…`, with the `current`, `minutely_15`, `hourly` and `daily` variables that §4 and §5 need.

Open-Meteo data is CC BY 4.0. The docs page credits it (§8).

**Config** (new section, all keys optional except `city` when enabled):

```toml
[weather]
enabled = false          # opt-in; nothing leaves the LAN while false
city = "Paris"           # required when enabled
country = "FR"           # optional ISO 3166-1 alpha-2 filter (sent uppercased), avoids Paris, Texas
refresh_minutes = 15     # optional, default 15, minimum 10
```

- `enabled = true` with an empty or missing `city`, or a `country` that is not two letters, or `refresh_minutes < 10`, **stops the hub at start**. This matches how `[flows] latitude/longitude` mistakes are handled today.
- `OIKOS_WEATHER__…` environment overrides work as for the other sections.

**Resolving the city.** On start, the hub resolves the city once with the geocoding endpoint. It takes the first result, which Open-Meteo ranks by relevance and population, and logs the place it chose at `info` (name, admin area, country, coordinates). The coordinates stay in memory. Nothing is persisted, so every restart resolves again.
- No result: one `error` log naming the city, and weather stays unavailable for this run (entities offline, `GET /api/v1/weather` → 503). The hub runs on.
- Network or HTTP error: retry with exponential backoff (30 s doubling up to 15 min). Entities stay offline meanwhile.

**Polling.** The first fetch happens right after the city is resolved, then every `refresh_minutes`, with a small random jitter (±10 %) so many hubs do not hit the API in step. Each fetch is a single forecast request. A failed fetch is logged at `warn`, once per outage rather than once per try, and the last good value is kept. The HTTP client (`open_meteo::client()` returns a `Result`, so a failed build is reported, not a panic) uses short timeouts (connect 5 s, total 15 s) and an identifying `User-Agent` (`oikos-os/<version>`).

**Freshness** (my call, from the research draft):
- `stale` becomes `true` once the last success is older than **2 × refresh_minutes**.
- After **1 hour** without a success, the weather entities go **offline** through their availability topic, so flows see "unavailable" instead of an old number. `GET /api/v1/weather` keeps serving the last value with `stale: true`, and the web decides when to hide it.
- When a fetch succeeds again, the entities come back online.

**Disabled.** With `enabled = false`, the hub makes no request at all. If retained weather discovery configs from an earlier run exist on the broker, the hub clears them on start (empty retained payloads), so the Weather device disappears. History rows are kept. This is my call: turning it off means it is gone.

## 4. Weather entities

One device, **Weather** (`identifiers: ["oikos_weather"]`), published by the hub under the first discovery prefix `P`, following the virtual-entity pattern (`crates/oikos-os/src/virtual_entities.rs`):

- Configs: `P/<component>/weather/<key>/config`, retained.
- One retained topic per key, `P/weather/<key>`, with a plain payload (a number, an enum name, or `ON`/`OFF`). A value that disappears from the source is published as `None`, which the registry reads as unknown, and every key is republished once after each hub start so a restart never leaves a stale retained value.
- Availability: `P/weather/availability`, retained `online`/`offline`. The hub publishes `offline` on start until the first successful fetch.
- `weather` is added to the reserved sub-trees (`RegistryConfig::default_reserved`, and the mqtt-topics doc), so the state topic is never read as discovery.

Unique ids are `weather_<key>`. Names are in English, like the other hub entities, and the web can translate them later. No area is assigned. The user can move an entity to a room with the usual override.

| key | component | unit / class | meaning |
|---|---|---|---|
| `temperature` | sensor | °C, temperature, measurement | Current air temperature (2 m) |
| `apparent_temperature` | sensor | °C, temperature, measurement | Feels-like temperature |
| `humidity` | sensor | %, humidity, measurement | Relative humidity |
| `condition` | sensor | enum | One of `clear`, `partly_cloudy`, `cloudy`, `fog`, `drizzle`, `rain`, `freezing_rain`, `snow`, `showers`, `thunderstorm`, mapped from the WMO weather code |
| `daylight` | binary_sensor | — | Open-Meteo `is_day` |
| `cloud_cover` | sensor | %, measurement | Cloud cover |
| `precipitation` | sensor | mm, precipitation, measurement | Precipitation over the current interval |
| `wind_speed` | sensor | km/h, wind_speed, measurement | Mean wind (10 m) |
| `wind_gusts` | sensor | km/h, wind_speed, measurement | Gusts (10 m) |
| `wind_direction` | sensor | °, measurement | Wind direction, the direction the wind comes from |
| `pressure` | sensor | hPa, atmospheric_pressure, measurement | Sea-level pressure |
| `uv_index` | sensor | measurement | UV index |
| `today_min` | sensor | °C, temperature | Today's minimum |
| `today_max` | sensor | °C, temperature | Today's maximum |
| `today_precipitation` | sensor | mm, precipitation | Today's precipitation total |
| `today_precipitation_probability` | sensor | % | Today's highest precipitation probability |
| `today_wind_gusts_max` | sensor | km/h, wind_speed | Today's strongest gust |
| `rain_next_hour` | sensor | mm, precipitation | Sum of the next four 15-minute precipitation slots |
| `tonight_min` | sensor | °C, temperature | Lowest hourly temperature from now until 09:00 local the next morning (frost watch) |

These cover the user's examples and the usual ones: close the awning above a gust threshold, warn when rain is coming within the hour and a window is open, close the shutters on a hot day, frost alert tonight.

The `condition` mapping from WMO codes is part of the hub (`oikos-api` or `oikos-core`, the builder decides), so the web and the entity share one definition:
- 0 → `clear`; 1–2 → `partly_cloudy`; 3 → `cloudy`; 45, 48 → `fog`;
- 51–55 → `drizzle`; 56, 57, 66, 67 → `freezing_rain`; 61–65 → `rain`;
- 71–77 → `snow`; 80–82 → `showers`; 85, 86 → `snow`; 95–99 → `thunderstorm`;
- anything else → `cloudy`, with a `debug` log.

An entity's state changes only when its value changes, so the history writer's dedupe and the flow engine see real changes, not one every 15 minutes.

## 5. `GET /api/v1/weather`

Returns `200` with a `WeatherView` (in `oikos-api`, JSON Schema derived like the other views):

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

- `fetched_at` is Unix seconds, like `Snapshot.generated_at`. Every number under `current`, `today`, `rain_next_hour`, `tonight_min` and in `days` is nullable, because Open-Meteo can return `null`.
- `days` holds the next 5 days, starting tomorrow. Today is in `today`.
- No coordinates in the response. `place` is enough for display.
- `503` (`weather unavailable`, as climate answers `climate disabled`) when `[weather]` is disabled, the city did not resolve, or no fetch has succeeded yet. The error code style follows `history_unavailable`.
- **WS:** a new bump event `WeatherChanged` (no payload, like `ClimateChanged`) after every successful fetch that changed the view. The web refetches the endpoint on it. This is my call. In the conversation I said the entity changes would be enough, but the forecast days are not entities, so the header needs a signal to refresh them.
- **MCP:** a read-only `get_weather` tool returns the same view. My call, for parity with REST, since assistants are a first-class client here.

## 6. Home settings and the house mode

### 6.1 The document (decided with the user: "un réglage dans les paramètres")

A small per-house document, retained on the broker like dashboards and climate settings, so the broker holds the truth and it survives restarts:

- Topic: `<ui_prefix>/_home` (default `oikos/ui/_home`), retained JSON, next to `_areas` and `_types`. The hub intercepts it before the registry, so it never becomes an override and no new reserved sub-tree is needed. An empty payload means defaults; an undecodable one is logged and the previous document is kept.
- Fields, all optional, `null` meaning unset:
  - `eco_setpoint`: number, °C, finite, 5 to 30. The header shows "heating in eco" when every heating thermostat is off or set at or below this value (§7).
  - `mode_entity`: entity id of a `select` entity, usually a `virtual_<id>` select. The header shows its current option as the house mode pill, and changes it when the pill is tapped.
  - `indoor_entity`: entity id of a temperature sensor (`sensor` with `device_class: temperature`). When set, the header shows it as the indoor temperature.
- Field names are snake_case, like `HouseSettings` and `Dashboard`.

### 6.2 API

- `GET /api/v1/home/settings` → the document. Defaults are all `null` when nothing is retained.
- `PUT /api/v1/home/settings` → validates, publishes retained, applies, and returns the stored document. Validation:
  - `eco_setpoint` within 5–30 and finite;
  - `mode_entity` must currently exist and be a `select`;
  - `indoor_entity` must currently exist and be a temperature `sensor`.
  - A rejected value is `422` with a message naming the field (`Rejected`), and `503` while the broker link is down. An entity that vanishes later is not an error. The web treats it as unset.
- WS: `HomeSettingsChanged { settings }` on every change. The snapshot does not carry the document (a new field would break a web test literal): the web fetches `GET /api/v1/home/settings` at boot and listens for `HomeSettingsChanged`, so a change from another client reaches every open UI.
- MCP: `get_home_settings` and `set_home_settings` (the whole document, like `set_climate_settings`: read, change, send back).

### 6.3 `select` virtual entities (decided with the user)

`VirtualKind` gains `Select`, and `VirtualSpec` gains `options: Option<Vec<String>>`:
- Required for a select: 1 to 32 options, each 1 to 64 characters after trimming, no duplicates. Refused for the other kinds.
- `initial` must be one of the options, and defaults to the first.
- Discovery: a `select` component with `command_topic` = `state_topic` = `P/virtual/<id>`, `options`, and `retain: true`. The command is echoed back retained, exactly as switches and numbers are, so the existing select capability in `oikos-core` makes it controllable by `send_command`, the flow `command` node and the web.
- Re-running `set_virtual` with a new options list republishes the config. If the current state is no longer one of the options, the hub publishes the first option. This is my call.
- A command naming a value outside the options is rejected before publishing, like an out-of-range number.

The house mode is then just a select virtual the user creates (`virtual_house_mode` with options `home`, `away`, `night`) and names as `mode_entity`. The user's flows set it, for example an alarm-management flow that arms the alarm after a long absence and switches the mode to `away`. The hub has no built-in link between the mode, the alarm or presence.

## 7. Decisions for the web header (later project, recorded here)

These need no hub code beyond §3–§6. They are the user's answers to the open questions, so the web design can start from them:

- **Placement:** only on the house overview, not on every dashboard (user: "seulement sur la vue d'ensemble").
- **Eco chip:** computed in the browser from the climate entities and `eco_setpoint`, not from the climate engine, so it works with the engine disabled (user: "pour moi c'est si le chauffage est éteint ou si la température de consigne est de 15 °C"). A thermostat counts as eco when it is off or its target temperature is at or below `eco_setpoint`. My proposal, to be confirmed in the web design: the chip says "heating in eco" when every heating thermostat is eco, and the chip is hidden when `eco_setpoint` is unset. What to show in summer, when units are cooling, is left to the web design.
- **Indoor temperature:** `indoor_entity` when set. Otherwise, the mean of the primary temperature sensors that have an area (my call, since the user's "ok" did not pick one, and it works without the climate engine).
- **House mode pill:** shows `mode_entity`'s current option, and lets you pick another option. Hidden when unset.
- **Weather block:** from `GET /api/v1/weather`, refreshed on `WeatherChanged`. Hidden when the endpoint answers 503, or when `stale` has lasted long enough (the web decides, around 6 h).
- **Sunset:** computed in the browser from `[flows]` via `oikos_core::flow::sun`, as the flow canvas already does, with the existing Paris-fallback hint. Not from Open-Meteo, so that `[flows]` stays the only source of sun times for automations and UI alike.

## 8. Docs

- `docs/reference/configuration.md`: a new `[weather]` section.
- `docs/reference/http-api.md`: `GET /api/v1/weather`, `GET`/`PUT /api/v1/home/settings`, and the `select` fields of `PUT /api/v1/virtual/{id}`, plus the new WS events.
- `docs/reference/mqtt-topics.md`: the weather and home topics, the `select` virtual config topic, and the reserved sub-tree `weather`.
- `docs/reference/mcp-tools.md`: `get_weather`, `get_home_settings`, `set_home_settings`, the `set_virtual` select spec, and the tool count.
- `docs/users/weather.md` (new): enabling, the city and country, what each entity means, freshness and offline behaviour, flow examples (gust threshold, rain within the hour, frost tonight), privacy (the city and the coordinates of the resolved place go to Open-Meteo, and only when enabled), and the Open-Meteo CC BY 4.0 credit.
- `docs/users/groups-and-virtual-entities.md`: the `select` kind and the house-mode example.
- `docs/users/flows.md`: a pointer to the weather entities.

The README is not touched.

## 9. Constraints and code map

- Workspace crates only: `oikos-api` (views, events, `VirtualSpec`), `oikos-core` (reserved sub-trees and, if it fits there, the WMO mapping), and `oikos-os` (config, weather module, home settings, hub wiring, HTTP, MCP). **No changes under `crates/oikos-web`**, which belongs to project 1 until it merges. If an `oikos-api` change breaks the web build (a new `WsEvent` variant in an exhaustive match, for example), the builder makes the smallest compile fix in the web crate and says so in the commit. No behaviour change there.
- `reqwest` is already a workspace dependency. Tests use a local stub HTTP server (an `axum` router, as `oikos-push` tests do) with the provider's base URLs injectable. No real network in tests.
- Pure functions get unit tests: WMO mapping, Open-Meteo response → view, view → entity states, freshness rules, select validation, home settings validation, and config validation.
- The hub stays generic. Examples use public places only.

## 10. Left to the builder

- The module layout inside `oikos-os` (`weather/` with `client`, `poller`, `entities`, for example) and where the WMO mapping lives.
- The exact Open-Meteo variable list, as long as it yields §4 and §5. Whether `uv_index` comes from `current` or the nearest hourly slot.
- How the poller is wired into the hub's task set and shutdown, following the existing pollers and tasks.
- The exact log wording.
