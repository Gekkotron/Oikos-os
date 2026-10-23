---
title: Configuration (oikos.toml)
sidebar_position: 1
description: Every oikos.toml section and key with its type, default and meaning, plus the environment overrides.
---

The hub reads `oikos.toml` from the working directory. `oikos-os run --config <path>` reads another file. A missing file is fine: every key has a default. [`oikos.example.toml`](https://github.com/Gekkotron/Oikos-os/blob/main/oikos.example.toml) is the annotated starting point, and the structs are in [`crates/oikos-os/src/config.rs`](https://github.com/Gekkotron/Oikos-os/blob/main/crates/oikos-os/src/config.rs).

## Layers and overrides

Settings are layered: built-in defaults, then `oikos.toml`, then environment variables. Each later layer wins.

Every key can be set from the environment as `OIKOS_<SECTION>__<KEY>`, with two underscores between nesting levels and the names in upper case:

```sh
OIKOS_MQTT__PASSWORD='<password>' cargo run -p oikos-os -- run
OIKOS_WATCHDOG__GATEWAY__ENABLED=true
OIKOS_HISTORY__BUFFER_ROWS=50000
```

String keys keep the raw text of the variable, so `OIKOS_MQTT__PASSWORD=007` stays `"007"` and is not read as a number. Numbers and booleans are parsed. For a list, use array syntax: `OIKOS_TOPICS__DISCOVERY_PREFIXES='["oikos", "homeassistant"]'`.

Logging has one more rule: if `RUST_LOG` is set and valid, it wins over `[log] level`. If it is unset or invalid, `[log] level` applies; if that is invalid too, the level is `info`.

At start the hub also checks `[flows]` (coordinates), `[watchdog]`, `[climate]` and `[weather]`. A bad value stops it with `invalid config: <reason>`.

Secrets (`[mqtt] password`, `[frigate] password`, the password inside `[history] url`) are masked when the config is logged. Keep `oikos.toml` out of git; it is git-ignored in this repo.

## `[mqtt]`

The broker connection. The hub keeps a persistent MQTT session under `client_id`, so a write queued just before a connection blip is retransmitted on reconnect. Two hubs on one broker must not share a `client_id`: the broker would bounce them off each other and hand one the other's session.

| Key | Default | Description |
| --- | --- | --- |
| `host` | `"localhost"` | Broker host name or address. |
| `port` | `1883` | Broker port. |
| `user` | none | Broker user. Leave out for an open broker. |
| `password` | none | Broker password. |
| `client_id` | `"oikos-os"` | MQTT client id and session key. |
| `send_ha_birth` | `false` | When `true`, publish `online` (not retained) on `ha_status_topic` each time the hub connects, so bridges re-publish their discovery. |
| `ha_status_topic` | `"homeassistant/status"` | Topic of the birth message. |

```toml
[mqtt]
host = "<host>"
port = 1883
user = "oikos"
password = "<password>"
```

## `[topics]`

Where the hub looks for discovery and where it keeps its own retained documents. See [MQTT topics](mqtt-topics.md) for what lives under each prefix.

| Key | Default | Description |
| --- | --- | --- |
| `discovery_prefixes` | `["oikos"]` | Home Assistant discovery prefixes the hub subscribes to (`<prefix>/#`). The first one is also where the hub publishes the discovery of its own virtual entities and groups. |
| `ui_prefix` | `"oikos/ui"` | Entity overrides (`<ui_prefix>/<id>`), rooms (`<ui_prefix>/_areas`), the type table (`<ui_prefix>/_types`) and dashboards (`<ui_prefix>/dashboards/<id>`). |
| `z2m_base` | `"zigbee2mqtt"` | Zigbee2MQTT base topic; the hub reads `<z2m_base>/bridge/devices`, `/info` and `/state`. |
| `automation_prefix` | `"oikos/automation"` | Flow definitions: one retained JSON document per flow at `<automation_prefix>/<flow id>`. |

Until Zigbee2MQTT publishes under `oikos`, list the Home Assistant prefix next to it so the discovery topics it still retains under `homeassistant/` are picked up too:

```toml
[topics]
discovery_prefixes = ["oikos", "homeassistant"]
```

## `[http]`

| Key | Default | Description |
| --- | --- | --- |
| `bind` | `"0.0.0.0:8081"` | Address the HTTP server listens on: the [HTTP API](http-api.md), the WebSocket, the MCP server and the web UI share it. 8081 sits next to Jeedom on 80 and clear of Zigbee2MQTT on 8080 on the same server. |

## `[commands]`

| Key | Default | Description |
| --- | --- | --- |
| `confirm_timeout_ms` | `5000` | How long a command waits for the entity's state echo before it reports `accepted: false`. Flow `command` nodes and `send_command` use it. |
| `subscription_debounce_ms` | `500` | How long the MQTT task waits after the set of state topics changes before it re-diffs its subscriptions. |

## `[automation]`

The flow engine.

| Key | Default | Description |
| --- | --- | --- |
| `runs_kept` | `20` | Recent runs remembered per flow, newest first. Runs live in memory only; a restart forgets them. A value of 0 is treated as 1. |

## `[flows]`

Flow scheduling and the house position used by sun nodes. See [Flow nodes](flow-nodes.md).

| Key | Default | Description |
| --- | --- | --- |
| `timezone` | system zone | IANA zone `trigger.schedule` runs in (`"Europe/Paris"`). Absent: the system's zone, or UTC if it cannot be read. An unknown name falls back to UTC with a warning. |
| `latitude` | none | House latitude in degrees, north positive, -90 to 90. |
| `longitude` | none | House longitude in degrees, east positive, -180 to 180. |

`latitude` and `longitude` go together: setting only one, or a value outside the range, stops the hub at start. With neither set, `trigger.sun` and `if` with `op: is_dark` use Paris (48.8566, 2.3522), and saving a flow that reads the sun reports a warning.

```toml
[flows]
timezone = "Europe/Paris"
latitude = 48.8566
longitude = 2.3522
```

## `[groups]`

| Key | Default | Description |
| --- | --- | --- |
| `enabled` | `false` | Kill switch for [flexible groups](../users/groups-and-virtual-entities.md). While `false`, `GET /api/v1/groups`, `PUT`/`DELETE /api/v1/groups/{id}` and the MCP group tools answer `groups disabled` (503 over HTTP), group commands are rejected, and the one-shot room-group migration is skipped. |

```toml
[groups]
enabled = true
```

## `[log]`

| Key | Default | Description |
| --- | --- | --- |
| `level` | `"info"` | A `tracing` filter: a level (`debug`) or a directive list (`oikos_os=debug,rumqttc=warn`). `RUST_LOG` wins. |
| `json` | `false` | `true` writes JSON lines; otherwise a human-readable format. Logs go to stderr. |

## `[push]`

Push notifications, sent by the hub through Firebase Cloud Messaging. When `credentials` is not set (in the file or as `OIKOS_PUSH__CREDENTIALS`), the hub uses `secrets/firebase.json` in the config file's folder if that file exists. Without a key the hub keeps the alert history and sends nothing, with one warning at start; a key file that is unreadable is the same, and the warning names its path. The hub starts either way. See [Push notifications](../users/push-notifications.md).

| Key | Default | Description |
| --- | --- | --- |
| `prefix` | `"oikos/push"` | Topic prefix: `<prefix>/notification/<source>`, `<prefix>/tokens/<installId>`, `<prefix>/responses`, `<prefix>/history`. |
| `legacy_prefix` | `"oikos-push"` | OikosPush's topics, still read during the switch-over. An empty string turns them off. |
| `credentials` | `secrets/firebase.json` next to the config file, when it exists | Path of the Firebase service-account JSON key. Only the path is ever logged. |
| `retry_interval` | `60` | Seconds between two passes of the retry queue. |
| `max_retries` | `10` | Attempts before a queued alert is given up. |
| `history_size` | `100` | Alerts kept in the history. |
| `history_days` | `7` | Days an alert stays in the history. |

```toml
[push]
prefix = "oikos/push"
legacy_prefix = "oikos-push"
credentials = "/path/to/firebase-service-account.json"
```

:::warning
`[push]` refuses unknown keys. The old `history_topic` and `responses_topic` keys stop the hub at start with an error that names the keys `[push]` accepts. Remove them: the history and responses topics are now always `<prefix>/history` and `<prefix>/responses`.
:::

## `[frigate]`

Frigate, for alert media and the event bridge. See [Cameras (Frigate)](../users/cameras.md).

| Key | Default | Description |
| --- | --- | --- |
| `url` | none | Frigate origin. With it set, media on that origin is served by the hub at `/api/v1/media`, so the browser never sees the login. Any other origin is refused. A blank string counts as unset. |
| `user` | none | Frigate login, for its login port (8971). Leave `user` and `password` out for the unauthenticated port (5000). |
| `password` | none | Frigate password. |
| `connect_timeout_ms` | `5000` | How long a connection to Frigate may take before the media request fails. |
| `bridge` | `false` | Turn `<topic_prefix>/events` into `camera` push alerts. Needs `url` or `media_url`. The topic is subscribed only when the bridge is on. |
| `media_url` | none | Base URL the phone fetches snapshots and clips from; `url` when unset. Use the unauthenticated port (5000): the app sends no login. |
| `labels` | `[]` | Object labels to alert on. Empty: all. |
| `cameras` | `[]` | Cameras to alert on. Empty: all. |
| `zones` | `[]` | Alert only once the object enters one of these zones. Empty: any. |
| `cooldown` | `0` | Seconds to ignore new events on a camera after an alert. |
| `severity` | `"warning"` | Severity of the alerts: `info`, `warning` or `critical`. |
| `ttl` | `43200` | Time to live of the alerts, in seconds. |
| `topic_prefix` | `"frigate"` | Frigate's MQTT `topic_prefix`. |

```toml
[frigate]
url = "http://<frigate-host>:8971"
user = "oikos"
password = "<password>"
bridge = true
media_url = "http://<frigate-host>:5000"
labels = ["person", "car"]
```

The broker user needs read on `<topic_prefix>/events`. Frigate needs `snapshots.enabled: true`, and event clips retained (`record.enabled`) for the clip update.

## `[watchdog.gateway]`

Alerts when a component under `<prefix>/` turns non-ok or recovers. Off by default.

| Key | Default | Description |
| --- | --- | --- |
| `enabled` | `false` | Subscribe to `<prefix>/+`. |
| `prefix` | `"encryption-gateway/health"` | Topic prefix where the gateway publishes one retained state per component. Must not be empty when enabled. |

## `[watchdog.acl]`

Alerts on Mosquitto `Denied PUBLISH/SUBSCRIBE/UNSUBSCRIBE` log lines. Off by default.

| Key | Default | Description |
| --- | --- | --- |
| `enabled` | `false` | Turn the watchdog on. |
| `source` | `"topic"` | `"topic"` reads `log_topic` (needs `log_dest topic` in `mosquitto.conf`); `"file"` tails `log_file`. |
| `log_topic` | `"$SYS/broker/log/#"` | Topic read when `source = "topic"`. Must not be empty in that case. |
| `log_file` | none | Mosquitto log path, required when `source = "file"`. |
| `cooldown` | `300` | Seconds during which repeats of one denial are held back; later denials are counted in the next alert. |

```toml
[watchdog.gateway]
enabled = true
prefix = "encryption-gateway/health"

[watchdog.acl]
enabled = true
source = "topic"
cooldown = 300
```

See [Watchdogs](../users/watchdogs.md) for the alert wording and the broker permissions each source needs.

## `[history]`

TimescaleDB connection for the entity-state history. See [History](../users/history.md).

| Key | Default | Description |
| --- | --- | --- |
| `url` | `""` | Postgres URL. Empty turns history off: the house keeps running and the `/api/v1/history/*` routes answer 503. |
| `buffer_rows` | `100000` | States the writer keeps while the database is unreachable, before it drops the oldest (one warning per outage). |

```toml
[history]
url = "postgres://oikos:<password>@<host>:5432/oikos"
buffer_rows = 100000
```

## `[climate]`

The climate engine (beta, still being tested). Everything except these two keys is edited through MCP, REST and the web Climate screen, as retained JSON under `prefix`. See [Climate](../users/climate.md).

| Key | Default | Description |
| --- | --- | --- |
| `enabled` | `false` | Turn the engine on. While `false`, `GET /api/v1/climate` answers 503 and the climate MCP tools answer `climate disabled`. |
| `prefix` | `"oikos/climate"` | Topic prefix of the climate documents. When enabled it must not be empty or only `/`, and must not contain `+` or `#`. |

```toml
[climate]
enabled = true
prefix = "oikos/climate"
```

## `[weather]`

Outdoor weather from [Open-Meteo](https://open-meteo.com), a free service with no API key. Off by default: while `enabled` is `false` the hub makes no request at all. See [Weather](../users/weather.md).

| Key | Default | Description |
| --- | --- | --- |
| `enabled` | `false` | Turn the weather on. While `false`, `GET /api/v1/weather` answers 503 and `get_weather` answers `weather unavailable`. |
| `city` | `""` | The city to look up. Required (not empty or blank) when enabled. |
| `country` | none | Optional ISO 3166-1 alpha-2 filter for the lookup, such as `FR`, so `Paris` is not Paris, Texas. Exactly two letters. |
| `refresh_minutes` | `15` | Minutes between two fetches, with about 10 % random jitter. At least 10 when enabled. |

When enabled, an empty `city`, a `country` that is not two letters or a `refresh_minutes` below 10 stops the hub at start with `invalid config: [weather] …`. `OIKOS_WEATHER__ENABLED`, `OIKOS_WEATHER__CITY`, `OIKOS_WEATHER__COUNTRY` and `OIKOS_WEATHER__REFRESH_MINUTES` work like the other sections; the city and country stay text even when they look like numbers.

```toml
[weather]
enabled = true
city = "Lyon"
country = "FR"
refresh_minutes = 15
```
