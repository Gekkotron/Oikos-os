# Home header, web side: greeting, mood sentence, chips, weather card and weather page

Status: design, 2026-10-23. Project 3 of the Ivoire redesign, web part. The hub part (weather, home settings, `select` virtuals) is merged (PR #13, `docs/superpowers/specs/2026-10-23-home-header-weather-hub-design.md`); its §7 holds decisions this document builds on.

## 1. Intent

The house overview should greet you like a calm, domestic home screen, not a control panel. When you open it, one glance says what state the house is in ("La maison est calme, il fait bon."), what is on, how warm it is inside and what the weather does outside. The user drew this in the Ivoire mockup; this project builds it for every theme (a theme only changes the look) and in both languages (FR/EN).

The user also wants to open a full weather view by clicking the weather card: wind and its direction, the other current figures, and the next days.

Everything is computed in the browser from what the web UI already receives or can fetch: entities and areas (WS), alerts, `flows_config` (sun), `GET /api/v1/weather` and `GET /api/v1/home/settings`. The only hub change is two new fields in the home settings (§7).

The project is generic: nothing names a machine, an address or a room of the user's house.

## 2. Goals, non-goals, anti-goals

**Goals**
- A header on top of the house overview: date, greeting, mood sentence, a detail line, summary chips, weather card (§3–§5), with the layout the user picked (option A, §3).
- A weather page (`/weather`) opened from the weather card (§6).
- A "Home" panel in Settings for the five home settings (§7).
- Two new home settings, the cold and hot thresholds, on the hub (§7).
- Every string in FR and EN through `i18n.rs`.

**Non-goals**
- The header on authored dashboards. Only the overview (`/dashboards/house`, which `DashboardScreen` renders as `HomeScreen`) gets it (hub spec §7, user: "seulement sur la vue d'ensemble").
- Hourly forecasts, per-day wind, or any new weather data. The weather page shows only what `GET /api/v1/weather` returns today (user approved the mockup as is).
- Any house-mode logic. The mode pill shows and sets the `select` the user named; their flows decide what the mode means.
- The Android app. It will reuse the same rules later, out of scope here.
- Ambiances (project 4) and rich cards (project 2).
- The app's `hero_weather` card style (`override_editor.rs`): unrelated, left as is.

**Anti-goals** (failure even if it technically works)
- A sentence that contradicts the chips (cold sentence, warm indoor chip). The sentence and the indoor chip use the same temperature (§4.2).
- A sentence that flickers or changes at random. Same inputs, same sentence; it only changes when the house changes.
- Stale weather shown as current. The card hides when the endpoint has no data or the data is old (§5).
- The overview breaking or waiting when weather, home settings, sensors or climate entities are absent. Every part of the header degrades by disappearing.
- Hard-coded French or English strings outside `i18n.rs`.

## 3. Layout (option A, approved with mockups)

Desktop (the mockup file is `.superpowers/brainstorm/…/header-a-phone.html`, gitignored):

```
VENDREDI 23 OCTOBRE                                   ┌──────────────────────┐
Bonsoir                                               │ 🌦 9°                 │
La maison est calme, il fait bon.                     │ Averses · ressenti 5° │
Pluie attendue dans l'heure                           │ Aujourd'hui 9° / 14°  │
(💡 3 lumières allumées) (🌡 19,6 °C à l'intérieur)    │ Coucher du soleil 18 h 42 │
(🍃 Chauffage en éco) (Maison ▾)                      └──────────────────────┘
```

- Left column on the page background: date (small caps, muted), greeting (Manrope, large), mood sentence, detail line (muted, small), chip row (wrapping).
- Right: the weather card, a regular card surface (theme tokens), clickable as a whole, opening `/weather`.
- Without the weather card (§5), the left column takes the full width, and the sunset moves into the detail line (§4.3).
- Phone width: everything stacks. Chips use short labels ("3 lumières", "19,6 °C", "Éco", "Maison ▾"). The weather card sits under the chips as one compact row (icon, temperature, "Averses · 9° / 14°", "Coucher 18 h 42"). The shell's ☰ drawer stays above.
- Styling uses only theme tokens (Ivoire and Night Glass both), icons from `icons.rs` (inline SVG, a weather condition set added there; the emoji in the mockups are placeholders).

## 4. Text: date, greeting, sentence, detail line

All text comes from the FR/EN table. A pure module (no signals) computes it from a `HouseSummary` input and the current time, so it is unit-tested natively. The header re-evaluates when its inputs change and once a minute (for the date, greeting and night band).

### 4.1 Date and greeting
- Date: weekday, day, month in the UI language ("Vendredi 23 octobre" / "Friday 23 October"), house local time.
- Greeting by hour (my call): 5:00–11:59 "Bonjour" / "Good morning"; 12:00–17:59 "Bon après-midi" / "Good afternoon"; 18:00–4:59 "Bonsoir" / "Good evening".

### 4.2 The mood sentence (approved by the user)

Two halves joined by a comma: **activity**, then **temperature**. Each half is one row of a table; the first matching activity row wins.

Activity:

| # | When | FR | EN |
|---|------|----|----|
| 1 | At least one unseen alert (same count as the Alerts badge, `alerts::unseen_count`) | « Quelque chose demande ton attention » | "Something needs your attention" |
| 2 | One door or window open | « La fenêtre de la cuisine est ouverte » (entity title, room when known) | "The kitchen window is open" |
| 2b | Several open | « 2 ouvertures » | "2 doors or windows open" |
| 3 | Dark (civil dusk to dawn, from `[flows]`) **and** 22:00–5:59 **and** no light on | « La maison dort » | "The house is asleep" |
| 4 | No light on | « La maison est calme » | "The house is quiet" |
| 5 | Otherwise | « La maison est éveillée » | "The house is awake" |

Temperature (indoor temperature T, see below; thresholds from §7, defaults cold 15 °C, hot 26 °C):

| When | FR | EN |
|------|----|----|
| T < cold | « il fait froid » | "it's cold" |
| T > hot | « il fait chaud » | "it's warm" |
| otherwise | « il fait bon » | "it's comfortable" |

- Examples: « La maison est calme, il fait bon. » « La maison dort, il fait froid. » « La fenêtre de la cuisine est ouverte, il fait froid. »
- No indoor temperature → the sentence stops after the activity half.
- **Indoor temperature T** (hub spec §7): the `indoor_entity` value when set and numeric; else the mean of the primary temperature sensors (`device_class == "temperature"`, `°C`) that have an area other than Unassigned and are not offline (`state.availability != Offline`, so `Unknown` counts, with a numeric value; no staleness check on `last_update`, my call). The weather entities (`weather_*`) are excluded: they are in Unassigned, and the builder also excludes the hub's weather device explicitly. Rounded to 0.1 °C for display; the comparison uses the unrounded value.
- "Light on": an entity whose `kind` is `Kind::Light` (the hub already sends the effective kind, so a switch with `showAs` light is included, see `SwitchType::effective_kind`), not a group (`group_*` id), not hidden by the user (`ui.hidden`), not a diagnostic or config entity, and on. A light folded into a group with `hideMembers` still counts; the group entity itself does not (my call, so a light is never counted twice nor missed).
- "Door or window open": a primary, non-hidden `binary_sensor` with `device_class` `door`, `window`, `garage_door` or `opening`, state on.
- Wording of the sentences in FR is the user's; the EN column and the table rows 1, 2, 2b, 5 are my proposals that the user accepted ("ok c'est top").

### 4.3 Detail line

Under the sentence, muted, at most two fragments joined by " · ", in this order, each only when it applies:
1. Rain in the next hour (`rain_next_hour` > 0.1 mm): « Pluie attendue dans l'heure » / "Rain expected within the hour".
2. Frost tonight (`tonight_min` ≤ 0 °C), shown from 15:00: « Gel cette nuit » / "Frost tonight".
3. Sunset, shown from 2 h before sunset until sunset, **or always when the weather card is hidden**: « Coucher du soleil à 18 h 42 » / "Sunset at 6:42 pm" (sun from `[flows]` via `oikos_core::flow::sun`, as the flow canvas does; when `fallback_active`, the sunset fragment, and the sun tile of the weather page, show the canvas's Paris-fallback hint wording instead of a time; `House::flows_config` is already fetched at start, `house.rs`). Row 3's "dark" test uses the same sun computation, Paris fallback included (my call: wrong by minutes at worst in France, and the hint already tells the user).

The line is absent when no fragment applies.

## 5. Chips and weather card

Chips, left to right; each one is hidden when its input is missing:

- **Lights**: count of lights on (§4.2 rule). Amber "lit" style when > 0; hidden when 0 (my call: the sentence already says "calme"). Click → nothing in v1 (my call).
- **Indoor**: T with one decimal and « à l'intérieur » / "inside". Hidden without T.
- **Heating** (cases approved with the user; the mapping onto the state is my call). Over primary, non-hidden climate entities whose `capabilities.hvac_modes` contains `heat` (a cool-only unit is ignored). `ClimateState.mode` decides (`oikos-core/src/model/state.rs`), not `action`:
  - a unit is **heating-on** when `mode` is `heat`, `auto` or `heat_cool`; **off** when `mode` is `off`, `dry` or `fan_only` (those do not heat);
  - any unit with `mode == cool` → « Clim en marche » / "Air conditioning on" (takes precedence);
  - else no unit heating-on → « Chauffage éteint » / "Heating off";
  - else every heating-on unit has a known `target` ≤ `eco_setpoint` → « Chauffage en éco » / "Heating in eco" (matches the `eco_setpoint` doc: at or below, an off unit counts as eco);
  - else (a target above eco, or a heating-on unit with no target) → chip hidden.
  - "Eco" needs `eco_setpoint`; when it is unset, the eco case hides the chip. The chip is hidden when no unit qualifies.
- **House mode**: the current option of `mode_entity`, styled as a pill with ▾. Click opens a small menu of the select's options (`capabilities.options`); picking one sends `{"type":"select_option","option":…}` through the existing command path. Hidden when `mode_entity` is unset or not found. A state that is not among the options is shown as is; the menu still works.

Weather card (right column, or compact row on phone), from `GET /api/v1/weather`, refetched on the `WeatherChanged` WS event (and on reconnect):
- Condition icon, current temperature (rounded), condition label, « ressenti » apparent temperature, today min/max, sunset (from `[flows]`, not Open-Meteo).
- Hidden when the endpoint answers 503 (weather off or not fetched yet), on any fetch error, or when `fetched_at` is more than 6 hours old. While `stale` is true but under 6 hours, the card shows with a muted "data from HH:MM" line (my call).
- A WMO condition → icon + FR/EN label table covers every `WeatherCondition` variant, with day/night icons where `daylight` matters (clear, partly cloudy).

## 6. Weather page (`/weather`, approved mockup)

Opened from the weather card; the header's Back follows `nav.rs` `BackStack` like every other screen. The breadcrumb names it « Météo » / "Weather". Not in the sidebar (my call); a direct visit works.

- Top card: big condition icon, current temperature (one decimal), condition and apparent temperature; on the right, today min/max, tonight's minimum, rain in the next hour (« Pas de pluie dans l'heure » when 0).
- "Now", 8 tiles in a grid (4 columns desktop, 2 on phone): wind (speed, an arrow rotated to the direction the wind comes **from** and a compass word, gusts), humidity, pressure, cloud cover, today's rain (mm and probability), today's max gusts, UV index (with a word: faible/modéré/élevé/très élevé/extrême by WHO bands), sun (sunrise and sunset from `[flows]`).
- "Next 5 days": one card per `days[]` entry: short weekday and day, icon, max (bold), min (muted), precipitation probability.
- Footer: place name and region, "updated at HH:MM", stale notice when stale, and « Données Open-Meteo (CC BY 4.0) » / "Data Open-Meteo (CC BY 4.0)".
- When weather is unavailable (503): a short explanation and a link to the docs page `docs/users/weather.md` wording ("Enable `[weather]` in oikos.toml").
- Units: metric as the hub sends them; numbers formatted in the UI language (comma decimals in FR).

## 7. Home settings: two new thresholds and the Settings panel

**Hub (the only hub change):** `HomeSettings` gains `cold_below` and `hot_above` (`Option<f64>`, °C). Validation: each in 5..=35 and, when both are set, `cold_below < hot_above`; `422` names the field otherwise. Unset means the web uses the defaults 15 and 26 (the user's 15 °C is the cold default; 26 °C is my proposal). They serialize as `null` when unset, like the other fields, and `#[serde(default)]` keeps older stored documents valid; the existing shape test in `oikos-api/src/home.rs` is updated. Validation goes in `oikos-os/src/home.rs` `validate`. Same document, topic, REST/WS/MCP paths as the other fields; docs (`http-api.md`, `mcp-tools.md`, the home settings user page) updated.

**Web Settings, new "Maison" / "Home" panel** (my call on its form):
- Eco setpoint: number field, °C, 5–30, empty = unset.
- Cold below / Hot above: two number fields with the defaults shown as placeholders.
- House mode entity: a picker of `select` entities (virtual selects, ids `virtual_…`, first), with "None".
- Indoor sensor: the existing room-grouped thermometer picker (`climate/entity_field.rs`), with "Automatic (mean of rooms)" as the empty choice.
- One Save button sends the whole document (`PUT` replaces it), built from the latest copy (kept fresh by `HomeSettingsChanged`); errors from `422` show next to the named field. The panel reloads on `HomeSettingsChanged`.

## 8. Data plumbing

- `House` gains the home settings (fetched once, refreshed on `HomeSettingsChanged`) and the weather (fetched on load, on `WeatherChanged`, and after reconnect). Both are `Option`: absent → the dependent parts hide.
- Both WS events are currently ignored in `house.rs:158` and `house_state.rs:79`; they get handled. `WeatherChanged` has no payload (refetch); `HomeSettingsChanged` carries the document. A refetch after WS reconnect is added if no hook exists (builder's call how).
- The header logic splits into pure, natively tested functions: `summary(entities, areas, settings, alerts_unseen) -> HouseSummary`, `mood(&HouseSummary, now, sun, lang) -> (sentence, detail)`, `heating_chip(...)`, `weather_visible(view, now)`, condition → icon/label. Components only wire signals to them.

## 9. Docs

- `docs/users/` page for the overview (or the existing home page): the header, what each sentence means and the rules behind it, the chips, the weather page.
- `docs/users/weather.md`: the header card and the weather page.
- Home settings: the two thresholds and the Settings panel.
- `docs/reference/http-api.md`, `mcp-tools.md`: the two new fields.

## 10. Constraints

- Workspace crates: `oikos-web` (most of the work), `oikos-api` (`HomeSettings` fields), `oikos-os` (validation, tests, MCP schema).
- Pure functions get unit tests, including every sentence row, the threshold edges, the heating chip cases, weather visibility (503, error, 6 h), and both languages.
- `cargo test --workspace`, clippy and fmt clean. The web is checked at desktop and ~360 px width in both themes.
- Check `~/dev-scratch/cargo-target` size before builds (over 5 GB, `cargo clean`).

## 11. Left to the builder

- Module and component layout inside `oikos-web` (for example `home_header.rs` + `mood.rs`, `screens/weather.rs`).
- The weather icon set's drawing, within the existing icon style.
- Exact spacing and type sizes, within the mockup and the theme tokens.
- The compass words (8 or 16 points) and exact FR/EN phrasing of tile labels, keeping the mockup's meaning.
