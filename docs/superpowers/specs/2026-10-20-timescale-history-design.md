# History in TimescaleDB

Author: Gekkotron
Date: 2026-10-20
Status: approved in conversation, awaiting plan

## Why

The house keeps no memory of its own. Sensor history lives in InfluxDB 2.7 on the Geekom (org `geckostudio`, bucket `athena`), written by Athena-Core from raw MQTT and read by OikosHistory, a Python service on `:8090` that the Android app calls for charts, "kWh today" and "since X". The hub stores nothing: state is in memory, rebuilt from retained MQTT on each connect. The web Energy and Security tabs have features on hold waiting for history (`docs/superpowers/plans/2026-10-19-web-energy-security-deferred.md`).

The user wants the hub to own history in **TimescaleDB**, record **everything**, and keep it **for years**. OikosHistory goes away. The original hub design planned "InfluxDB reader first, then SQLite writer" for this phase (`2026-10-18-oikos-os-design.md` §roadmap 3). This design replaces that plan: no Influx reader and no SQLite.

Success: TimescaleDB runs in Docker on the Geekom; the hub writes every entity state change to it; the web UI shows 24 h / 7 d / 30 d / 1 y charts, "kWh today" on Energy and "since X" on Security, all served by the hub; the house keeps running normally when the database is down.

## Decisions taken in conversation (the user's)

- TimescaleDB, in a Docker container on the Geekom.
- Record everything the hub knows, kept for years.
- Start fresh: no import of the existing InfluxDB data. History begins when the hub's recorder goes live. The Influx data stays untouched.
- No Jeedom data (Jeedom is being retired). The hub only knows entities announced by discovery (Zigbee2MQTT, hand-published discovery such as the water heater) and its own virtual entities, so Jeedom stays out with no filtering.
- OikosHistory is retired.

## Goals

1. **Recorder.** Every entity's state changes are written to TimescaleDB, keyed by entity id.
2. **Long retention.** Raw rows are kept forever, compressed; hourly and daily rollups keep long-range charts fast.
3. **Hub read API** under `/api/v1/history/*`, keyed by entity id.
4. **Web UI**: a history detail screen (24 h / 7 d / 30 d / 1 y) for charted entities, "x.x kWh today" on Energy, "since X" on Security.
5. **MCP**: a `get_history` tool so the assistant can answer "how much energy yesterday" or "when did the garage door last open".
6. **Deployment**: a docker-compose file for TimescaleDB, kept in this repo, plus a `[history]` config section.

## Non-goals

- Importing InfluxDB data.
- Moving the Android app to the hub's API. That is a follow-up plan in the app repo (my call on ordering, see below). Until it lands, OikosHistory and Athena-Core's Influx writer keep running unchanged for the app.
- Stopping Athena-Core or deleting the Influx container. Done by hand after the app moves.
- Recovering what happened while the hub was down. A gap is a gap.
- Automated backups (see open points).
- Running the hub itself in Docker or as a systemd service. The hub still runs from `tools/restart.sh`, on the Mac for now, and connects to the database over the LAN.

## Anti-goals

- **History must never break the house.** A slow, full or unreachable database must not delay MQTT handling, commands or flows. No `await` on the database in the ingest path.
- Rows for nothing: Zigbee2MQTT repeats a device's full state on every command echo and link-quality report, and the hub replays retained states on every connect. Writing those repeats would multiply the data and fake "last changed" times.
- A database schema the hub cannot evolve: schema changes are versioned migrations applied at startup, never manual SQL on the Geekom.
- Losing years of data silently because the container volume was anonymous or inside the container.

## How it works

### What a row is

The recorder is fed inline from `Hub::ingest` (`crates/oikos-os/src/hub.rs:184`): right after `reg.apply(inbound)`, while the registry lock is still held, the hub takes each `Change::StateChanged(id)` (`crates/oikos-core/src/registry/mod.rs:82`, never `StateReported`), clones that entity's new `EntityState`, and `try_send`s it to the recorder's own queue. It does not subscribe to the `WsEvent` broadcast: a lagging broadcast receiver drops events, which would lose rows and fake "last changed" times. Every other path that changes state the same way (virtual entities written by flows or MCP) feeds the recorder too.

For each state it receives, the recorder compares it with the last one it wrote for that entity and writes only what differs:

- **Value changed** (full `StateValue` comparison, so a light's brightness or a climate setpoint counts even when `num` is the same) → one row in `entity_state`.
- **Availability changed** → one row in `entity_availability`. Kept separate so "last changed" means the value, not the device dropping off the network.
- **Attributes only** (`json_attributes_topic`, mostly `linkquality`, `last_seen`) → nothing. My call: those values are separate diagnostic entities in Z2M discovery and get recorded as their own series. Recording attribute maps per entity would copy every Z2M message about ten times.
- **`StateValue::Unknown`** → never written, and it does not update the "last written" cache. Otherwise every discovery and reconnect would write "unknown" rows and break "last changed".

"Everything" includes hidden, diagnostic, config, disabled-by-default and virtual entities, and every kind of value.

**Timestamps.** The recorder stamps each row with the hub's own clock at microsecond precision when it takes the state, because `State { at }` and `last_update` are whole seconds. Rows also carry a `seq bigint` that the database generates, so two changes in the same microsecond keep their order. Every "latest" or "previous" row query orders by `(time, seq)`. There is no unique constraint on `(entity_id, time)`.

**Entity ids.** The series key is the registry id, as-is. When an entity is removed (device unpaired, virtual entity deleted), its rows stay, so `/last_change` and charts still answer for it. A replaced device that comes back with a new id starts a new series. Moving history from an old id to a new one is a follow-up (an MCP tool), not part of this design.

`entity_state` (hypertable on `time`):

| column | type | meaning |
|---|---|---|
| `time` | `timestamptz` | when the hub took the state, microseconds |
| `seq` | `bigint`, generated | tie-break for equal `time` |
| `entity_id` | `text` | registry id |
| `num` | `double precision`, null | the chartable number, see below |
| `value` | `jsonb` | the full `StateValue`, serialised as the hub's wire type |

`num` per `StateValue`: `Number(x)` → x; `OnOff(b)` → 1/0; `Light` → 1/0 for on/off; `Cover` → position; `Climate` → current temperature; `WaterHeater` → current temperature; `Text`, `Alarm`, `Vacuum` and anything else → null. The builder may refine the composite kinds; the `value` column always has everything.

`entity_availability`: `time`, `entity_id`, `availability` (`online` / `offline` / `unknown`).

### Restarts and replays

On startup the recorder loads the last written value and availability per entity from the database (`DISTINCT ON (entity_id) … ORDER BY time DESC`) into its in-memory "last written" cache. The retained replay after a connect then produces no rows unless the state really differs. If it does (the value changed while the hub was down), the row is stamped with the replay time. That is accepted and documented.

### Write path

`StateChanged` events go into a bounded queue (no `await` from the ingest path; `try_send`). A writer task drains it in batches (about once a second, or every few hundred rows) with a multi-row insert. When the database is unreachable the writer reconnects with backoff and keeps up to a fixed number of rows (on the order of 100 000, about an hour of a busy house). Past that it drops the oldest and logs one warning per outage with a count. The "last written" cache is updated only when a row is enqueued, so a dropped row does not later produce a fake change.

The recorder is a hub component in `oikos-os` (I/O); `oikos-core` stays pure. Turning `StateValue` into `num` and the dedupe comparison are pure functions and live in `oikos-core` with unit tests.

### What a stored series means

Only changes are stored, so **a value holds until the next row** (last value carried forward). This differs on purpose from OikosHistory, which saw every sample and interpolated linearly between them. Every reading of the data follows this rule:

- **Chart windows.** The mean is time-weighted: a sensor that holds 1 for 23 h and 0 for 1 h averages about 0.96 over the day, not 0.5. Min and max include the value carried in from before the window. A quiet window with no rows shows the carried value, so it is not a hole. Only windows before the entity's first row are omitted.
- **Power → kWh.** A step integral: each value counts until the next row. There is no cap on long holds, because a load that sits at 0 W or 2 kW for hours is real. Across hub downtime the last value is carried as well. That is accepted and documented, like the replay stamping above.
- **Counter → kWh.** Unchanged by the rule: the sum of non-negative increases between consecutive rows, so a meter reset adds nothing.

### Long retention

- Raw rows: kept forever. Compression on chunks older than 7 days (segment by `entity_id`, order by `time, seq`).
- Rollups `history_hourly` and `history_daily` on `num`, per entity per bucket. They must give the time-weighted, carried-forward results above, and the builder picks the mechanism. Options include Toolkit `time_weight('LOCF', …)` aggregates with `interpolated_average` across bucket edges in continuous aggregates, or plain tables filled by a hub job. Daily buckets are aligned on the house time zone, not UTC.
- Image: `timescale/timescaledb-ha` (includes the Toolkit extension). My call: the Toolkit's `counter_agg` (counter resets) and `time_weight` (step integrals and weighted means) cover the maths here. The builder may drop the Toolkit and use plain SQL if it complicates things, as long as the results match the rules above.

### Read API (hub, `/api/v1/history/*`)

The shapes follow OikosHistory's (`OikosHistory/handlers.py`, the app's `HistoryApi.kt`) so the app migration later is a URL and parameter change, but series are named by entity id instead of `topic` + `key`.

- `GET /series?entity=<id>&range=24h|7d|30d|1y` → `{"window": s, "points": [[t, mean, min, max], …]}`. Windows: 10 min, 1 h, 5 h, 1 day. `t` is the window start, unix seconds; values rounded to 3 decimals; empty windows are omitted. 24 h reads raw rows; 7 d and 30 d read `history_hourly` (5 h windows re-bucketed from hourly); 1 y reads `history_daily`. Booleans chart as step lines on the client.
- `GET /daily?entity=<id>&days=7|30|365&kind=counter|power` → `{"days": [[t_local_midnight, kwh], …]}`. Calendar days in the house time zone (`flows::schedule::house_zone`, `[flows] timezone`), DST-safe, last day partial, days without data omitted. `counter`: sum of non-negative increases (a meter reset adds nothing and never goes negative). `power`: step integral in hours (see "What a stored series means"), divided by 1000 (W → kWh). The unit scaling (Wh, kW) stays on the client, as in the app's `EnergyScale`.
- `GET /today?entity=<id>&kind=counter|power` → `{"kwh": f|null, "since": t_local_midnight}`.
- `GET /last_change?entity=<id>&entity=<id>…` (1–50) → `{"items": [{"changed_at": t|null, "recorded": bool}, …]}` in request order. `changed_at` is the time of the latest `entity_state` row. Every row is a change of the full value, so no comparison with the previous row and no 30-day window are needed. `recorded: false` when the entity has no rows.
- No authentication, same as every other hub route in v1 (`crates/oikos-os/src/http/mod.rs:22`). MCP `get_history` calls the same code.
- Errors: 400 `bad_request`, 404 `not_recorded` (entity unknown to the database), 503 `history_unavailable` (database down, or `[history]` not configured).

### Web UI

- **History detail screen**: opened from a sensor or binary-sensor card (and an Energy node), with chips 24 h / 7 d / 30 d / 1 y, min / avg / max over the range, and a Power | kWh-per-day toggle for energy entities on 7 d and longer. Behaviour mirrors the app's `feature/history`; the look follows the existing web UI.
- **Energy**: "x.x kWh today" under each node and card, refreshed every 5 minutes, ported from the app's `EnergyToday.requests` (which counters to ask, the derived-house subtraction; `oikos_core::energy` already has the derived-house logic).
- **Security**: "since X" per sensor, from `/last_change`.
- These items come off the deferred list in `2026-10-19-web-energy-security-deferred.md`.

### MCP

`get_history` with `entity`, and either `range` (series) or `daily` / `today` with `kind`, or `last_change`. Returns the same JSON as the HTTP routes.

### Configuration

```toml
[history]
url = "postgres://oikos:<password>@192.168.1.91:5432/oikos"   # empty or absent = history off
buffer_rows = 100000
```

`history.url` goes in `STRING_ENV_KEYS` (`config.rs:213`), overridable as `OIKOS_HISTORY__URL`, and the password is masked in `Debug` and in logs like `MqttConfig`. The real URL lives in `oikos.toml`, which is git-ignored, or in the env var. `oikos.example.toml` shows a placeholder. The time zone reuses `[flows] timezone`. With history off, the hub runs as today and the history routes answer 503.

### Deployment

`deploy/timescaledb/docker-compose.yml`: `timescale/timescaledb-ha` pinned to a PostgreSQL major and a Timescale version, a **named volume** for the data, port 5432 on the LAN, user, password and database from a `.env` next to it (`.env.example` committed, `.env` git-ignored). Restart policy `unless-stopped`. The README gets a "History" section: start the container, set `[history] url`, and a one-line manual `pg_dump` backup command. Schema migrations are embedded in the hub and applied at startup. The hub connects as the image's `POSTGRES_USER`, which owns the database and is a superuser in that image. So the first migration can run `CREATE EXTENSION IF NOT EXISTS timescaledb` (and `timescaledb_toolkit`) and create hypertables, compression and refresh policies, with no init script. That is acceptable for a single-user home LAN.

## Left to the builder

- The Postgres client crate (`sqlx` or `tokio-postgres` + a pool) and the migration mechanism, provided migrations are embedded and run at startup.
- Exact batch size, flush interval and backoff.
- Whether 7 d / 30 d series read the hourly aggregate or raw rows, if raw proves fast enough.
- How integration tests reach a real TimescaleDB (a `TEST_DATABASE_URL` that skips when absent, or containers); pure parts are unit-tested in `oikos-core`.
- Chart rendering in Leptos (SVG by hand or a small crate), matching the existing look.

## My calls (for the user to check)

- Order: hub recorder and API, then the web UI, in this repo. The app moves to the hub in a separate follow-up plan; OikosHistory is switched off after that.
- Attribute-only changes are not recorded.
- Raw rows kept forever (compressed), rather than dropped after N months behind rollups.
- A 1 y range added to charts, since the data now lasts years.
- Timescale Toolkit image for the kWh maths.
- Rows are stamped with the hub's own clock (microseconds), not the device's time.
- Values hold until the next change: time-weighted chart means, step integral for power. This deliberately differs from OikosHistory's linear interpolation, so app numbers may shift slightly once it moves.
- `Unknown` states are never recorded.
- History stays with the registry id. Rows survive entity removal, and moving history to a replaced device is a follow-up.
- The recorder is fed inline from the hub's ingest path, not from the lossy WebSocket broadcast.
- The hub uses the database superuser from the compose file.
- Backups: a documented manual `pg_dump`, no automation.
