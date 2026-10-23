---
title: Weather
sidebar_position: 14
description: Outdoor weather from Open-Meteo as entities and a REST document, how to turn it on, what each entity means, and how it behaves when the network is down.
---

The hub can fetch the outdoor weather for your city and publish it as a **Weather** device. Its entities behave like any other sensor: flows can trigger on them, the history records them, and you can place them in a room. The same data is served as one document at `GET /api/v1/weather` and by the `get_weather` MCP tool.

Weather is **off by default**. While it is off, the hub makes no request to anyone.

## Turn it on

Add a `[weather]` section to `oikos.toml` and restart the hub:

```toml
[weather]
enabled = true
city = "Lyon"
country = "FR"
refresh_minutes = 15
```

- `city` is required. `country` is optional: two letters (`FR`), it keeps the lookup to one country, so `Paris` is not Paris, Texas.
- `refresh_minutes` is 15 by default and must be between 10 and 1440.
- A mistake (an empty city, a three-letter country, a refresh below 10) stops the hub at start with a message naming the key.

At start the hub looks the city up once and logs the place it chose (name, region, country and coordinates). If it finds nothing, it logs an error naming the city and the weather stays unavailable until you fix the name and restart. If the lookup fails because of the network, the hub retries after 30 seconds, then waits twice as long each time, up to 15 minutes between tries. The rest of the house keeps running either way.

See [Configuration](../reference/configuration.md#weather) for every key and environment variable.

## The entities

All units are metric. Entity ids are `weather_<key>`, for instance `weather_wind_gusts`.

| Key | Meaning |
| --- | --- |
| `temperature` | Air temperature now, °C |
| `apparent_temperature` | Feels-like temperature, °C |
| `humidity` | Relative humidity, % |
| `condition` | `clear`, `partly_cloudy`, `cloudy`, `fog`, `drizzle`, `rain`, `freezing_rain`, `snow`, `showers` or `thunderstorm` |
| `daylight` | On while the sun is up |
| `cloud_cover` | Cloud cover, % |
| `precipitation` | Precipitation over the current interval, mm |
| `wind_speed` | Mean wind, km/h |
| `wind_gusts` | Gusts, km/h |
| `wind_direction` | The direction the wind comes from, degrees |
| `pressure` | Sea-level pressure, hPa |
| `uv_index` | UV index |
| `today_min`, `today_max` | Today's lowest and highest temperature, °C |
| `today_precipitation` | Today's precipitation total, mm |
| `today_precipitation_probability` | Today's highest chance of precipitation, % |
| `today_wind_gusts_max` | Today's strongest gust, km/h |
| `rain_next_hour` | Precipitation expected in the next hour, mm |
| `tonight_min` | The lowest temperature from now until 09:00 the next morning, °C (frost watch) |

The Weather device has no room. Move an entity to a room with the usual override if you want it there. A value Open-Meteo does not provide reads as unknown.

## Freshness and offline

- The hub fetches every `refresh_minutes`, with about 10 % random jitter. A failed fetch is logged once per outage and the last good values stay.
- When the last success is older than twice `refresh_minutes`, `stale` is `true` in `GET /api/v1/weather`.
- After one hour without a success, whatever `refresh_minutes` is, the weather entities go **unavailable**, so a flow sees "unavailable" and not an old number. `GET /api/v1/weather` keeps serving the last document, with `stale` set by the rule above.
- The entities come back as soon as a fetch succeeds.
- Turning weather off removes the Weather device: on the next start the hub clears what it left on the broker. History rows are kept.

## Flow ideas

These are built in the flow editor, or with `set_flow` through [MCP](assistants-mcp.md). Use the ids from your own house.

- **Close the awning in the wind:** trigger on `weather_wind_gusts`, check it is above 50, then close the awning cover.
- **Rain is coming, a window is open:** trigger on `weather_rain_next_hour`, check it is above 0.2, check the window sensor is open, then send a notification.
- **Frost tonight:** a schedule trigger at 18:00, check `weather_tonight_min` is below 1, then notify.
- **Hot day:** check `weather_today_max` is above 30 at 09:00, then close the shutters.

See [Flows](flows.md) for the nodes.

## Privacy

While `[weather] enabled` is `true`, the hub sends the **city name** (and the country, if set) to Open-Meteo's geocoding service, then the **coordinates of the place it found** with each forecast request. Open-Meteo sees your public IP address, as any web service does. Nothing else leaves the house: no device names, no entity states. While weather is disabled, nothing is sent.

## Credit

Weather data is provided by [Open-Meteo.com](https://open-meteo.com/) and licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Open-Meteo's free service is meant for light, non-commercial use; the default 15-minute refresh stays far below its limits.
