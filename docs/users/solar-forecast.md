---
title: Solar forecast
sidebar_position: 15
description: The Solcast solar forecast as entities and a REST document, how to get a key and a rooftop site, how to share an account, when the hub calls Solcast, and how the forecast behaves when it is stale or unreachable.
---

The hub can fetch the expected solar production of your roof from [Solcast](https://solcast.com) and publish it as a **Solar forecast** device. Its entities behave like any other sensor: flows can read them ("run the water heater if today looks sunny"), the history records them, and you can read the whole forecast as one document at `GET /api/v1/solar`, or with the `get_solar` MCP tool.

The forecast is an estimate, not a meter. The entities carry kWh and kW, but no energy or power device class, so the Energy tab and the energy-role pickers never take them for a measured reading. Measured production comes from your own meter, as before.

Solar is **off by default**. While it is off, the hub makes no request to anyone.

## What you need

- A Solcast account with a **hobbyist** plan, which allows 10 requests per UTC day and up to 2 rooftop sites.
- A **rooftop site** configured in the Solcast Toolkit. Its resource id, shown for the site in the Toolkit, is what the hub asks for.
- An **API key** from the same account.

## Turn it on

1. Put the API key, on one line, in `secrets/solcast.key` next to `oikos.toml`. The file is git-ignored; nothing else needs the key. Docker reads the same folder through `compose.yml`.
2. Add a `[solcast]` section to `oikos.toml` and restart the hub:

```toml
[solcast]
enabled = true
sites = ["<rooftop resource id>"]
daily_limit = 10
```

- `sites` lists one or more rooftop resource ids. Only letters, digits, `-` and `_` are accepted in an id.
- `daily_limit` is this hub's share of the account's daily requests. Keep the default of 10 when the hub is the only tool using the account.
- `api_key_file` is optional. It replaces `secrets/solcast.key` with another path.

An empty `sites`, an id with other characters, a `daily_limit` below the number of sites, or a missing or empty key file stops the hub at start with a message naming the key, as for `[weather]`.

See [Configuration](../reference/configuration.md#solcast) for every key and environment variable.

### Sharing the account with another tool

If another tool polls the same Solcast account, its calls count against the same daily limit. Set `daily_limit` to the account limit minus what the other tool uses. For example, if another tool makes 2 calls a day, use `daily_limit = 8`: the hub then has 8 calls, of which 2 stay in reserve, so it refreshes 6 times a day. With two rooftop sites the same budget gives 3 refreshes a day, because each refresh calls both sites.

The hub always keeps 2 calls in reserve for restarts and surprises. If the numbers are off anyway and Solcast answers "quota exceeded", the hub stops calling until 00:05 UTC.

## When the hub calls Solcast

The refreshes are spread over daylight:

- One refresh about 30 minutes before sunrise, so the day starts with a morning forecast.
- The others evenly spaced until about an hour before sunset.
- None at night.

Sunrise and sunset come from the latitude and longitude in `[flows]`, the same place your flows use for sun triggers. Without them the hub uses Paris.

Each refresh keeps the periods of the day that have already passed, so "forecast today" stays the energy of the whole day, not just what is left. The latest forecast and the call count are kept as a retained document on the broker (`oikos/solar/cache`). A restart reads it back, so restarting the hub does not spend calls.

If Solcast refuses the key (401 or 403), the hub logs one error and stops calling until it restarts. Fix the key file, then restart.

:::warning
Enable `[solcast]` on one hub only per Solcast account and broker. A second hub with it off clears the first one's retained cache, and two enabled hubs share the daily budget badly.
:::

## The entities

Entity ids are `solar_<key>` for the keys below, for instance `solar_forecast_today`. The device has no room; move an entity to one with the usual override if you want it there.

| Key | Unit | Meaning |
| --- | --- | --- |
| `forecast_today` | kWh | Expected energy for the whole of today |
| `forecast_today_low` | kWh | Today, pessimistic (10th percentile) |
| `forecast_today_high` | kWh | Today, optimistic (90th percentile) |
| `forecast_remaining_today` | kWh | Expected energy from now until the end of today |
| `forecast_tomorrow` | kWh | Expected energy for tomorrow |
| `forecast_tomorrow_low` | kWh | Tomorrow, pessimistic |
| `forecast_tomorrow_high` | kWh | Tomorrow, optimistic |
| `forecast_power_now` | kW | Expected power for the current half hour |
| `forecast_peak_today` | kW | Today's highest expected power |
| `forecast_peak_time_today` | time | When that peak is |

`forecast_remaining_today` and `forecast_power_now` are recalculated every 30 minutes from the forecast already held, without calling Solcast, so they keep moving through the day.

## Freshness and offline

- `stale` is `true` in `GET /api/v1/solar` when a planned refresh has been failing for more than an hour, or when the last success is older than 18 hours. There are no refreshes at night, so a forecast from the evening is not stale the next morning until the 18 hours have passed.
- The entities go **unavailable** when the last success is older than 24 hours, or when no forecast period covers the current time. A flow then sees "unavailable" and not an old number.
- The entities come back on the next successful refresh.
- Turning solar off removes the device on the next start, and the hub clears the retained cache and discovery it left on the broker.

## In the web UI

**The card.** On the house overview, the **Solar forecast** card sits beside the weather card. It shows:

- measured and forecast production for today, in kWh;
- a small day curve. The forecast is a light area with an outline. Measured production is an amber area up to now, and a dashed line marks now;
- a bottom line with what is left today and tomorrow's forecast, with its low–high range.

Click the card to open the solar page. The card is hidden when the hub has no forecast or the forecast is more than 24 hours old. When the forecast is stale but less than 24 hours old, the bottom line says "Data from HH:MM". On a phone the two cards stack, and the curve becomes a thin strip.

**Measured or forecast.** The amber area is measured production. It comes from the entities whose energy role is **Solar**, set in the entity's settings as for the Energy tab. Several are added together, and **Invert** is honoured. When nothing is measured, the area shows the forecast so far in a lighter shade and the title starts with "≈". When their kWh counters exist, the card's "today" figures are the same as on the Energy page.

When there is no solar entity, or history is off, the card fills the curve with the forecast up to now, and the title starts with "≈". That figure is the forecast so far, not a measurement.

**The solar page.** Open it from the card, or at `/solar`. It has:

- a top card with measured and forecast production today (with the range), power now measured against forecast, what is left today, and tomorrow;
- a curve with **Today** and **Tomorrow** tabs. Tomorrow shows the forecast and its range only;
- tiles:
  - **Forecast peak**: today's highest expected power.
  - **Best hours**: the longest run at or above 60 % of today's peak.
  - **Gap**: measured against forecast so far.
  - **Sun**: sunrise and sunset, from the location in `[flows]`.
  - **Yesterday**: measured against the morning forecast. A forecast made more than an hour after sunrise is shown as "forecast at HH:MM".
  - **7-day accuracy**: needs at least 3 days with a morning forecast above 0.5 kWh.
  - **Roofs**: only with two or more rooftop sites.
  - **This month**.
- the next days as bars;
- a footer with the update time, the next refresh and the Solcast calls used today.

Each tile hides when its data is missing. **Yesterday**, **7-day accuracy** and **This month** need history.

With solar disabled, the page says that the forecast is unavailable, and how to enable it: `[solcast]` in `oikos.toml`, and the API key in `secrets/solcast.key`.

## Flow idea

"If the forecast for today is above 15 kWh, turn the water heater on at 11:00":

1. A **schedule** trigger at 11:00.
2. An **if** check: `solar_forecast_today` is above 15.
3. A **command** on the water heater's switch, to on.

Build it in the flow editor, or with `set_flow` through [MCP](assistants-mcp.md). See [Flows](flows.md) for the nodes.

## Privacy

While `[solcast] enabled` is `true`, the hub sends the **rooftop resource ids** and the **API key** to Solcast, once per planned refresh. Solcast sees your public IP address, as any web service does. Nothing else leaves the house: no device names, no entity states, no address. While solar is disabled, nothing is sent.

## Credit

Forecasts by [Solcast](https://solcast.com).
