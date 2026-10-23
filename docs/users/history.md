---
title: History
sidebar_position: 11
description: Record every entity state change in TimescaleDB, enable it in oikos.toml, query it and back it up with pg_dump.
---

The hub records every entity state change in TimescaleDB for years, so the web UI and MCP can answer "what did this sensor read" and "how much energy today". History is off by default; point the hub at a TimescaleDB instance by adding `[history]` to `oikos.toml`:

```toml
[history]
url = "postgres://oikos:<password>@<host>:<port>/oikos"
buffer_rows = 100000       # states queued when the database is down
```

Any TimescaleDB 2.17+ on Postgres 17 works, as long as the role can log in and `CREATE EXTENSION timescaledb` has been run inside the target database (the extension itself must be in `shared_preload_libraries` on the server). The hub applies its own schema migrations on first connect, writes every state change inline from `Hub::ingest` (never on the hot MQTT path — the writer runs in a bounded, try-send queue) and rolls each hour up into `history_hourly` every five minutes. A down database never blocks the house: up to `buffer_rows` states wait, past that the oldest are dropped with one warning per outage.

## Querying

Routes under `/api/v1/history/*` (see the [HTTP API](../reference/http-api.md)): `series` for charts, `daily` for kWh per day over 7, 30 or 365 days, `today` for kWh since local midnight, and `last_change` for 1 to 50 entities. Errors are `400 bad_request`, `404 not_recorded` (the entity has no rows) and `503 history_unavailable` (`[history] url` not set, or the database is down). MCP exposes the same through `get_history`.

## Backup

Back up with `pg_dump` from any client that can reach the database:

```sh
pg_dump -h <host> -p <port> -U oikos -Fc oikos > oikos-history-$(date +%F).dump
```
