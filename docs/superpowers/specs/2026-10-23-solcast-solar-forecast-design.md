# Solar forecast from Solcast: crate, hub entities, header card and solar page

Status: design, 2026-10-23.

## 1. Intent

The user wants to know how much their solar panels will produce **today and tomorrow**, using their own Solcast rooftop site (they have an account and a configured site). The forecast should be visible at a glance **next to the weather card** in the overview header (home-header web project, `docs/superpowers/specs/2026-10-23-home-header-web-design.md`, being built on another branch), with the **actual production** filled in against it, and a full **solar page** opened from that card, built like the weather page.

Like weather, the forecast also becomes hub-published entities, so flows can use it ("run the water heater this afternoon if today is sunny") and history records it.

Solcast's own code lives in a new crate, `oikos-solcast` (user agreed: "is it a good idea to add a crate?" → yes, small, the same split as `oikos-push` / `oikos-frigate`: the crate knows the service, the hub does the wiring).

The project is generic: nothing names a machine, an address, a site id or a room of the user's house.

## 2. Goals, non-goals, anti-goals

**Goals**
- New crate `oikos-solcast`: Solcast rooftop-site client, response parsing, per-day totals in house local time, the daily call budget (§3).
- Hub: opt-in `[solcast]` config, a poller that never exceeds the account's daily request limit, a cache that survives restarts, `solar_…` entities, `GET /api/v1/solar`, a `SolarChanged` WS event, MCP access (§4–§6).
- Hub: one row per day of the forecast as it stood in the morning, kept in the history database, for the "yesterday vs forecast" and "accuracy" figures (§7).
- Web: a solar card beside the weather card in the overview header, layout "C2 with real production" (§8), and a `/solar` page (§9), both approved by the user from mockups.
- Docs pages (§11).

**Non-goals**
- Other providers (Forecast.Solar, Open-Meteo solar radiation). The crate split keeps that open; nothing is built for it now.
- Solcast's `estimated_actuals` endpoint. It costs requests from the same tiny budget, and the house measures its real production itself.
- Measuring production. The hub already has the user's meter as entities; the web reads the entities whose override `energyRole` is `solar` (§8.2).
- The Android app.
- Any automatic action (load shifting, water heater control). That is the user's flows' job, fed by the entities.
- Solcast's commercial/advanced PV endpoints (lat/lon PV power without a configured rooftop site).

**Anti-goals** (failure even if it technically works)
- Running out of Solcast requests. A restart loop, a retry storm or two sites must never spend more than the configured daily limit; past the limit the hub waits for the next UTC day, it does not retry.
- The hub refusing to run, or the overview breaking, because Solcast is unreachable, the key is wrong or the quota is spent. Solar is optional, like weather.
- An old forecast shown as current (stale handling §5.3).
- The API key in `oikos.toml`, in a log line, in an API response or on MQTT.
- A card or page that is taller than the header's left column on desktop (user: the card may grow, but not beyond the left data).
- Hard-coded French or English strings outside `i18n.rs`.

## 3. The `oikos-solcast` crate

Facts used here are from Solcast's hobbyist docs (`https://docs.solcast.com.au/docs/section/rooftop-sites-hobbyist`): up to 10 requests per **UTC** day on a hobbyist account, up to 2 rooftop sites, 30-minute periods, forecasts up to 14 days ahead, `Authorization: Bearer <key>` (or `api_key=` query), `GET https://api.solcast.com.au/rooftop_sites/{resource_id}/forecasts?format=json`. The parser is written against the documented shape with a hand-made fixture (my call: no live call is spent during the build). At the first live run with the user's key, the real response is checked against it and, if it differs, saved as the fixture **with the site id replaced**. Expected shape:

```json
{ "forecasts": [ { "pv_estimate": 1.23, "pv_estimate10": 0.8, "pv_estimate90": 1.6,
                   "period_end": "2026-10-23T14:30:00.0000000Z", "period": "PT30M" } ] }
```

`pv_estimate*` are mean kW over the period ending at `period_end`. Unknown fields are ignored. A `hours` query parameter, if Solcast accepts it, is set to cover today + 7 days (the page shows 7 days, §9); otherwise the whole response is kept and trimmed.

Contents (shape is the builder's call; behaviour is fixed):
- **Client**: `fetch_site(key, resource_id) -> Result<Vec<Period>, SolcastError>`, async, `reqwest` (workspace), short timeouts (connect 5 s, total 20 s), `User-Agent: oikos-os/<version>`. Errors distinguish: unauthorized (401/403), quota exceeded (429), other HTTP, network, parse. The key is never part of an error's `Display`/`Debug`.
- **Period**: `end` (UTC), `duration` (from `period`, default 30 min), `estimate`, `p10`, `p90` (kW).
- **Merge**: sites are summed period by period (two roofs, east/west), keeping the per-site daily totals for the page's "Roofs" tile.
- **Daily totals**: kWh per local calendar day (house time zone passed in, `chrono-tz`), for `estimate`, `p10`, `p90`: Σ kW × hours. A period is counted in the day of its start (`end − duration`) (my call). Also: kWh remaining today from `now`, a partial period prorated; the current forecast power (the period containing `now`); today's peak (kW and time); "best hours" (§9).
- **Budget**: a pure type that, given the configured daily limit, the number of sites, the calls already spent this UTC day and `now`, the sunrise/sunset of the day, answers "may I call now, and when is the next refresh". Rules in §5.2.
- Pure functions are unit-tested natively (no network): parsing the fixture, merging two sites, day totals across a DST change, period straddling midnight, remaining-today proration, the budget rules.

## 4. Config and secret

```toml
[solcast]
enabled = false                 # opt-in; nothing leaves the LAN while false
sites = ["xxxx-xxxx-xxxx-xxxx"] # rooftop resource ids from the Solcast Toolkit, 1 or more
daily_limit = 10                # the account's requests per UTC day (hobbyist: 10)
prefix = "oikos/solar"          # retained cache topic root (§5.4)
# api_key_file = "…"            # optional; default secrets/solcast.key next to the config
```

- The key lives in `secrets/solcast.key` (plain text, trimmed), like `secrets/firebase.json` (`config.rs` `PUSH_KEY_FILE`, `secrets/README.md`), my call so `oikos.toml` stays shareable. `api_key_file` overrides the path, like `[push] credentials`. Only the path is ever logged. `OIKOS_SOLCAST__…` environment overrides work as for the other sections, so `OIKOS_SOLCAST__API_KEY_FILE` works in Docker; `compose.yml` already mounts `secrets/`.
- `enabled = true` with no site, an empty site id, `daily_limit` of 0, or no readable key file **stops the hub at start** with a message naming the field, like `[weather]` mistakes.
- `secrets/README.md` gains a `solcast.key` line.
- **Shared keys.** `daily_limit` is the hub's share, not necessarily the account's. When another tool polls the same Solcast account (the user's Jeedom still makes 2 calls a day during the migration), the user sets `daily_limit` to the account limit minus that tool's calls (here 10 − 2 = 8, so 6 refreshes a day after the reserve of 2); the docs page says so. The 429 rule of §5.2 is the safety net when the numbers are off.
- The house time zone and the sun times come from `[flows]` (`timezone`, `latitude`/`longitude`, `oikos_core::flow::sun`), as everywhere else; the Paris fallback applies (my call). Solcast's own site location is not read.

## 5. Hub: poller, budget, cache, freshness

Module `crates/oikos-os/src/solar/`, modelled on `weather/` (`poller.rs`, `hub.rs`, `entities.rs`).

### 5.1 Fetch
One refresh = one `forecasts` request per site, then the merged forecast is stored, the entities republished, the cache retained (§5.4) and `SolarChanged` sent. If one site of two fails, the refresh counts as failed and the previous forecast stays (no half-merged forecast).

### 5.2 Budget (the anti-goal "never run out")
- Calls are counted per **UTC** day (Solcast's reset), every attempted request counts, success or not.
- Usable calls per day = `daily_limit − 2` (a reserve of 2 for restarts and the unexpected; my call), divided by the number of sites → refreshes per day; at least 1.
- Refreshes are spread over **daylight** (house sunrise to sunset, from `[flows]`): one shortly before sunrise (the morning forecast, §7), the rest evenly spaced until about an hour before sunset. No refresh at night. With the hobbyist limit and one site that is 8 refreshes a day; two sites, 4.
- Any time zone works (my call, the project is generic): slots are placed from local daylight, but each call is charged to the UTC day it happens in, and the planner never schedules more calls in a UTC day than that day has left. Where local daylight straddles UTC midnight, the day's slots simply draw on two budget days; tests cover a European, an American and an Oceanian zone.
- 429 (quota exceeded) → the current UTC day's budget is recorded as fully spent (the key may also be used elsewhere, or the cache was lost) and no more calls happen until the next UTC midnight + a few minutes, one `warn` log. 401/403 → one `error` log ("Solcast rejected the API key"), no more calls until the hub restarts. Network or 5xx → the next planned slot is the retry; never an immediate retry loop.
- The call count and the last call time are part of the retained cache (§5.4), so a restart in a loop does not reset the budget. On start, the hub reads the cache first; it calls Solcast only if the cache is missing, from another UTC day's plan, or a planned slot is due.

### 5.3 Freshness (my call)
- `stale` is `true` when the last success is older than 2 × the planned interval, or more than 6 hours old.
- The entities go **offline** (availability topic) when there is no forecast covering now, or the last success is older than 12 hours; they come back on the next success.
- `GET /api/v1/solar` keeps answering with the last forecast and `stale: true`; the web decides (§8.3).

### 5.4 Cache
Retained JSON document at `<prefix>/cache` (default `oikos/solar/cache`): the merged periods from local midnight today onward, per-site daily totals, `fetched_at`, the UTC day and call count, the last error kind. Never the key. `oikos/solar` is added to the reserved sub-trees so nothing reads it as discovery. Disabled → the hub deletes the retained cache and discovery configs on start (like weather).

## 6. Hub: entities, REST, WS, MCP

### 6.1 Entities
One device **Solar forecast** (`identifiers: ["oikos_solar"]`), published like the weather device (`weather/entities.rs`): configs `P/sensor/solar/<key>/config`, state `P/solar/<key>`, availability `P/solar/availability`; `solar` added to the reserved sub-trees. Unique ids `solar_<key>`. No area. State republished only on change.

| key | unit / class | meaning |
|---|---|---|
| `forecast_today` | kWh | Today's forecast (estimate) |
| `forecast_today_low` | kWh | Today, `p10` |
| `forecast_today_high` | kWh | Today, `p90` |
| `forecast_remaining_today` | kWh | From now to the end of today |
| `forecast_tomorrow` | kWh | Tomorrow's forecast |
| `forecast_tomorrow_low` | kWh | Tomorrow, `p10` |
| `forecast_tomorrow_high` | kWh | Tomorrow, `p90` |
| `forecast_power_now` | kW, measurement | Forecast power for the current period |
| `forecast_peak_today` | kW | Today's forecast peak |
| `forecast_peak_time_today` | timestamp (device class) | When that peak is |

The entities carry unit `kWh`/`kW` but **no `energy`/`power` device class and no `total_increasing` state class** (my call): they are forecasts, not meters, so the Energy tab, energy-role pickers and the climate learner must never take them for a meter reading. They use the `mdi:solar-power` icon. History records them like any sensor.

`forecast_remaining_today` and `forecast_power_now` are re-evaluated every 30 minutes from the stored forecast, without calling Solcast, as long as the entities are online (§5.3: a day-ahead solar forecast a few hours old is still the best estimate, so the values keep updating until the 12 h offline mark; only the REST view flags `stale`). The low/high values are separate entities rather than attributes because the registry has no free attributes on hub-published sensors (my call; the builder may fold them if that turns out wrong).

### 6.2 `GET /api/v1/solar`
`SolarView` in `oikos-api` (JSON Schema derived like `WeatherView`). 503 `solar_unavailable` when disabled or nothing fetched yet.

```json
{
  "fetched_at": 1792746902, "stale": false, "next_refresh_at": 1792754100,
  "calls_today": 6, "daily_limit": 10,
  "now": { "power": 1.6, "remaining_today": 5.2 },
  "today":    { "date": "2026-10-23", "estimate": 12.4, "low": 9.8, "high": 14.1,
                "peak": { "power": 2.4, "at": 1792758600 },
                "best_hours": { "from": 1792749600, "to": 1792764000 } },
  "days": [ { "date": "2026-10-24", "estimate": 8.1, "low": 5.0, "high": 11.0 } ],
  "sites": [ { "index": 0, "today": 5.1 }, { "index": 1, "today": 7.3 } ],
  "periods": [ { "end": 1792746000, "estimate": 0.0, "low": 0.0, "high": 0.0 } ],
  "history": { "forecasts": [ { "date": "2026-10-22", "estimate": 11.6, "low": 8.9, "high": 13.0 } ] }
}
```

- `days`: the next 7 days starting tomorrow. `periods`: 30-minute periods (kW) from local midnight today to the end of tomorrow, for the card and the page's curve.
- `sites` never carries the resource id; the page labels them "Roof 1", "Roof 2" (the user can't name them in v1, my call).
- `best_hours`: the longest contiguous run today where the estimate is ≥ 60 % of today's peak (my call; the threshold text on the page shows the kW value); `null` on a day with no production.
- `history`: from §7. Measured production is not in this view; the web reads it from the solar-role entities' history (§8.2), so the hub does not pick the meter.
- All numbers nullable; times Unix seconds.

### 6.3 WS and MCP
- `WsEvent::SolarChanged` (no payload, refetch), sent after each refresh and each 30-minute re-evaluation that changed `now`.
- MCP tool `get_solar` returning the `SolarView`, documented in `docs/reference/mcp-tools.md`; the server instructions mention the `solar_…` entities for flows.

## 7. Morning forecast kept per day

To show "yesterday: 10.9 kWh measured, 11.6 forecast" and the 7-day accuracy, the hub must remember what was forecast before the day began; the live forecast keeps changing during the day.
- When history is enabled, after the first successful refresh of each local day, the hub upserts one row: `solar_forecast_days(date, estimate_kwh, low_kwh, high_kwh, made_at)`, in a new migration `crates/oikos-os/migrations/0003_solar.sql` (plain table, not a hypertable; my call). Only the first refresh of the day writes it; later ones do not overwrite. A first refresh made after local sunrise (a restart, a failed early call) still writes the row, with its real `made_at`; rows made more than one hour after sunrise are **excluded from the accuracy figure** and the "yesterday" tile shows them as "forecast at HH:MM" (my call).
- `SolarView.history.forecasts` lists the stored rows for the last 7 local days before today (`[{date, estimate, low, high}]`, oldest first; empty when history is off). Measured daily kWh needs the solar-role entities, which only the web picks (§8.2), so the web computes "yesterday" and the accuracy from these rows and `/api/v1/history/daily?days=7` of the solar entities. Accuracy = mean of |measured − forecast| / forecast over the days that have both and a forecast > 0.5 kWh, shown as "± N %"; fewer than 3 such days → tile hidden (my call).
- No history → those two tiles hide.

## 8. Web: the solar card in the overview header

Depends on the home-header web project (branch of `sc-quantum-cryostat`), which must be merged first: the card lives in its header component, next to its weather card. Approved mockup: `.superpowers/brainstorm/…/solar-actual.html` (gitignored).

### 8.1 Layout (approved: "C2", then "with real production", then "more height but not beyond the left data")
- Desktop: the header's right column holds **two cards side by side, the same height**: weather (unchanged) and solar. The right column stretches to the left column's height (date → chips) and never beyond it.
- Solar card, top to bottom:
  - Title line: sun icon, **measured today** / **forecast today** kWh ("6,4 / 12,4 kWh").
  - The **day curve**, taking all the remaining height (min ~24 px): today's forecast as a light area with a thin outline over the whole day (sunrise→sunset span of the periods with production), the **measured** production as an amber filled area from the first period to now, a dashed vertical "now" line.
  - Bottom line: « reste ≈ 5,2 · Demain 8,1 kWh (5–11) » / "≈ 5.2 left · Tomorrow 8.1 kWh (5–11)".
- Phone: stacks with the weather card as in the header spec; the curve becomes a thin strip (~22 px).
- Click anywhere → `/solar`.
- Theme tokens only (Ivoire, Night Glass), sun icon from `icons.rs` (`SUN` exists).

### 8.2 Measured production
- The solar entities are the primary entities whose override `energyRole` is `solar` (as `energy.rs` already finds them), honouring `energyInvert`. Several are summed.
- Power-unit entities (W/kW): today's kWh from `/api/v1/history/today?kind=power`; the curve from `/api/v1/history/series?range=24h`, keeping today's points, resampled to 30-minute means to sit on the forecast's grid (my call). An energy-unit entity (kWh counter): `kind=counter` for the total; it gives no curve, so the card falls back to §8.3's forecast fill for the area.
- Refreshed on load, on each `SolarChanged`, and every 5 minutes while the header is visible (my call).

### 8.3 Degrading
- No `SolarView` (503, error) or `fetched_at` older than 12 h → the solar card is hidden; the weather card returns to its single-card layout from the header spec.
- `stale` under 12 h → a muted "data from HH:MM" line replaces the bottom line.
- No solar-role entity, or history off → the filled area is the **forecast** up to now and the title shows "≈ 7,2 / 12,4 kWh" with the forecast-so-far (the approved C2 behaviour before real production).
- Night: the card keeps showing today until local midnight (the whole day's measured area, no "now" line after the last producing period); tomorrow stays on the bottom line (my call).

## 9. Web: the solar page `/solar` (approved mockup `solar-page.html`)

Built like the weather page (header spec §6): Back follows `nav.rs` `BackStack`, breadcrumb « Solaire » / "Solar", not in the sidebar, a direct visit works.

- **Top card**: measured / forecast today with the low–high range; on the right, current power measured vs forecast, remaining today, tomorrow with its range.
- **Curve card**, tabs Today / Tomorrow: measured (amber area, today only), forecast (dashed line), low–high band (light area), "now" line, kW gridlines. Tomorrow: forecast and band only.
- **Tiles** (4 columns desktop, 2 phone), each hidden when its input is missing:
  1. Forecast peak — kW and time.
  2. Best hours — from–to, "above X kW" (§6.2).
  3. Gap — measured so far vs forecast so far, in %.
  4. Sun — sunrise · sunset from `[flows]`.
  5. Yesterday — measured kWh, "forecast N · ±%" (§7).
  6. 7-day accuracy — "± N %" (§7).
  7. Roofs — per-site forecast today, only with 2+ sites.
  8. This month — measured kWh since the 1st (`/history/daily?days=30`, summed from the 1st, my call).
- **Next 7 days**: one column per day: short weekday + day, a bar scaled to the week's max, estimate (bold), low–high (muted).
- **Footer**: "updated at HH:MM · next around HH:MM · Solcast calls N / limit today", stale notice when stale, and « Prévisions Solcast » / "Forecasts by Solcast".
- Unavailable (503): short explanation and the docs wording ("Enable `[solcast]` in oikos.toml and put the API key in secrets/solcast.key").
- Numbers formatted in the UI language (comma decimals in FR), kWh one decimal, kW one decimal.

## 10. Constraints

- Crates touched: new `oikos-solcast`; `oikos-api` (`SolarView`, `WsEvent::SolarChanged`); `oikos-os` (config, `solar/`, migration, HTTP, MCP); `oikos-web` (card, page, i18n, routes).
- Order: **two plans** (my call). Plan 1, crate + hub (§3–§7, docs for them), starts now in this worktree. Plan 2, web card and page (§8–§9), is written only after the home-header web branch is merged, against its real header component and weather page.
- Workspace conventions: `#![forbid(unsafe_code)]`, workspace dependencies, attribution `Gekkotron`.
- `cargo test --workspace`, clippy and fmt clean; pure logic unit-tested (§3); the web checked at desktop and ~360 px in both themes.
- No real site id, key or house data in tracked files (fixture anonymised).
- Check `~/dev-scratch/cargo-target` before builds (over 5 GB, `cargo clean`).

## 11. Docs

- `docs/users/solar-forecast.md` (new): what it does, the Solcast account and site, the key file, the budget and refresh times, the entities for flows (with an example flow), the card and the page.
- `docs/reference/configuration.md` (or the existing config reference): `[solcast]`.
- `docs/reference/http-api.md`, `mcp-tools.md`, `mqtt-topics.md`: `/api/v1/solar`, `get_solar`, `oikos/solar/cache`, the `solar` reserved sub-tree.
- `secrets/README.md`: `solcast.key`.

## 12. Left to the builder

- Module layout inside the crate and the hub; whether the budget planner lives in the crate (preferred) or the hub.
- The exact refresh times within the §5.2 rules.
- Drawing details of the curve, spacing and type sizes within the mockups and theme tokens.
- FR/EN wording of the tile labels, keeping the mockup's meaning.
