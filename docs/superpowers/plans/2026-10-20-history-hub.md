# History in TimescaleDB, hub side: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The hub records every entity state change in TimescaleDB, rolls it up hourly, and serves history over `/api/v1/history/*` and the MCP tool `get_history`; TimescaleDB runs from a compose file in this repo.

**Architecture:** All history maths (dedupe, carried-forward buckets, kWh) are pure functions in a new `oikos_core::history` module. `oikos-os` gains a `history` module: a `Db` over `sqlx` (Postgres), a writer task fed without awaiting from `Hub::ingest`, a rollup task that fills a plain `history_hourly` table, and the read queries behind the HTTP routes and MCP. TimescaleDB is used for the hypertable and compression only (no Toolkit, no continuous aggregates).

**Tech Stack:** Rust 2024, tokio, axum 0.8, rmcp, `sqlx` 0.8 (`runtime-tokio`, `postgres`, `chrono`, `json`, `migrate`, `macros`; runtime `sqlx::query`, never `query!`), chrono + chrono-tz, `timescale/timescaledb` image (PostgreSQL 17).

**Spec:** `docs/superpowers/specs/2026-10-20-timescale-history-design.md`. The web UI part of the spec (history screen, Energy "kWh today", Security "since X") is a second plan, written after this one lands.

## Plan decisions beyond the spec

The spec leaves the rollup mechanism and the Toolkit to the builder. This plan:

- computes rollups in Rust (`oikos_core::history`), stored in a plain table `history_hourly`. A hub task maintains it, so there is no Toolkit and no continuous aggregate, and the maths is unit-tested without a database;
- stores no `history_daily`. Daily windows and daily kWh are summed from hourly rows (8 760 rows per entity per year). This assumes the house zone's UTC offsets are whole hours, which holds for `Europe/Paris`;
- adds two small lookup tables, `entity_latest` and `availability_latest`. They seed the dedupe cache at startup and answer `/last_change` without scanning years of compressed chunks;
- skips `Availability::Unknown` as well as `StateValue::Unknown`, and skips numbers that are not finite. All three are "nothing known", for the same reason the spec gives for `Unknown`.

## Global Constraints

- History must never break the house: no `.await` on the database from `Hub::ingest` or any MQTT, command or flow path; `History::offer` is `try_send` only.
- Only `Change::StateChanged` feeds the recorder, never `StateReported`; fed inline from `Hub::ingest` (`crates/oikos-os/src/hub.rs:184`), never from the `WsEvent` broadcast.
- Rows are stamped with the hub's clock in microseconds; order is always `(time, seq)`.
- Values hold until the next row (carried forward). Chart means are time-weighted; min/max include the carried-in value; quiet windows show the carried value; only windows before an entity's first row are omitted. Power → kWh is a step integral / 1000 with no cap; counter → kWh is the sum of non-negative increases.
- Ranges and windows: `24h` → 600 s from raw rows; `7d` → 3 600 s, `30d` → 18 000 s, `1y` → 86 400 s, all from hourly rows. Window starts are UTC-aligned unix seconds. Values are rounded to 3 decimals.
- `daily` days ∈ {7, 30, 365}; calendar days in the house zone (`Hub::zone()`, `[flows] timezone`), DST-safe, last day partial.
- Error bodies: `{"error":"bad_request"}` 400, `{"error":"not_recorded"}` 404, `{"error":"history_unavailable"}` 503 (database down or `[history] url` empty).
- No authentication on the history routes (same as every v1 route).
- `[history] url` (string, in `STRING_ENV_KEYS`, env `OIKOS_HISTORY__URL`, password masked in `Debug`), `[history] buffer_rows` default `100000`. An empty `url` means history is off.
- Schema changes only through embedded `sqlx` migrations applied at startup.
- Commits as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never stage the unrelated uncommitted `crates/oikos-web` and `styles/main.css` changes.

## Review Focus

1. **Restart or broker reconnect replays every retained state.** Expected: no new rows unless a value really differs from what is stored. Covered by Task 6 `replay_after_restart_writes_nothing`.
2. **Database unreachable at boot or mid-run.** Expected: ingest never blocks; up to `buffer_rows` states wait and flush when the database returns; beyond that the oldest are dropped with one warning. Covered by Task 6 `offer_never_blocks_when_full` and Task 1 `pending_drops_oldest_past_capacity`.
3. **NaN or infinite numbers, `Unknown` value or availability.** Expected: never a row, and the cache is not disturbed. Covered by Task 1 `unknown_and_non_finite_write_nothing`.
4. **DST days (2026-10-25 is 25 h, 2026-03-29 is 23 h in Europe/Paris).** Expected: one day per local calendar day, stamped at local midnight. Covered by Task 7 `day_starts_cross_dst`.
5. **Rows arriving late (buffer flushed after an outage) for an hour already rolled up.** Expected: that hour is recomputed. Covered by Task 6 `late_rows_recompute_their_hour`.

---

### Task 1: Pure recorder model (`oikos_core::history`)

**Files:**
- Create: `crates/oikos-core/src/history/mod.rs`
- Modify: `crates/oikos-core/src/lib.rs` (add `pub mod history;`)
- Test: unit tests in `crates/oikos-core/src/history/mod.rs`

**Interfaces:**
- Produces:
  - `pub type Micros = i64;` and `pub const HOUR: Micros = 3_600_000_000;`
  - `pub fn num_of(value: &StateValue) -> Option<f64>`: `Number(x)` → x; `OnOff(b)` → 1/0; `Light` → 1/0 from `on`; `Cover` → `position`; `Climate` → `current`; `WaterHeater` → `current`; otherwise `None`. A result that is not finite → `None`.
  - `pub struct Incoming { pub at: Micros, pub id: String, pub state: EntityState }`
  - `pub struct ValueRow { pub at: Micros, pub id: String, pub value: StateValue, pub num: Option<f64> }`
  - `pub struct AvailabilityRow { pub at: Micros, pub id: String, pub availability: Availability }`
  - `#[derive(Default)] pub struct Rows { pub values: Vec<ValueRow>, pub availability: Vec<AvailabilityRow> }` with `fn is_empty(&self) -> bool`
  - `#[derive(Clone, Default)] pub struct LastWritten` with `seed_value(&mut self, id: String, value: StateValue)`, `seed_availability(&mut self, id: String, a: Availability)`, `diff(&mut self, incoming: &Incoming, rows: &mut Rows)`
  - `pub struct Pending` (a `VecDeque<Incoming>` with a capacity) with `new(capacity: usize)`, `push(&mut self, i: Incoming) -> bool` (false when it dropped the oldest), `take(&mut self, n: usize) -> Vec<Incoming>`, `requeue_front(&mut self, batch: Vec<Incoming>)`, `len()`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn first_state_writes_value_and_availability() {
    // Number(21.5), Online → values: [num 21.5], availability: [Online]
}
#[test] fn same_state_again_writes_nothing() { /* diff twice; second Rows is empty */ }
#[test] fn attributes_only_change_writes_nothing() { /* same value, different attributes map */ }
#[test] fn availability_change_alone_writes_availability_row() { /* Online → Offline, same value */ }
#[test] fn light_brightness_change_is_a_value_row_with_same_num() {
    // on:true brightness 10 → on:true brightness 200: one row, num 1.0 both times
}
#[test] fn unknown_and_non_finite_write_nothing() {
    // Number(20.0) written; then Unknown → empty and cache unchanged; then Number(20.0) → empty;
    // Number(f64::NAN) → empty; Number(f64::INFINITY) → empty; Availability::Unknown → no availability row
}
#[test] fn num_of_each_kind() {
    // OnOff(true)=1, Cover position 40 = 40, Climate current 19.5, WaterHeater current 55, Text → None, Alarm → None
}
#[test] fn pending_drops_oldest_past_capacity() {
    // capacity 2: push a,b → true,true; push c → false; take(10) yields [b, c]
}
#[test] fn requeue_front_keeps_order() { /* take 2 of [a,b,c], requeue → take(3) == [a,b,c] */ }
```

- [ ] **Step 2: Run** `cargo test -p oikos-core history` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** in `crates/oikos-core/src/history/mod.rs`. `diff` compares the full `StateValue` with `PartialEq` against the cached value, writes a value row only when the value differs and is not `Unknown` and is not a non-finite `Number`, then updates the cache; it does the same for availability, skipping `Unknown`. Attributes and `last_update` are ignored.
- [ ] **Step 4: Run** `cargo test -p oikos-core history` — Expected: PASS. Also `cargo clippy -p oikos-core --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(core): history recorder model (dedupe, pending buffer)`.

### Task 2: Pure buckets and kWh (`oikos_core::history::buckets`)

**Files:**
- Create: `crates/oikos-core/src/history/buckets.rs` (declared `pub mod buckets;` in `history/mod.rs`)
- Test: unit tests in the same file

**Interfaces:**
- Consumes: `Micros`, `HOUR` (Task 1)
- Produces:
  - `pub struct Sample { pub at: Micros, pub num: f64 }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct Bucket { pub start: Micros, pub covered: Micros, pub integral: f64, pub min: f64, pub max: f64, pub last: f64, pub increase: f64 }`. `integral` is in value-hours; `fn mean(&self) -> f64` returns `integral / (covered as f64 / HOUR as f64)`.
  - `pub fn rollup(carry: Option<f64>, samples: &[Sample], start: Micros, end: Micros, size: Micros) -> Vec<Bucket>` builds the buckets of `size` from `start` (aligned) to `end` (may be mid-bucket) that hold at least one sample. `carry` is the value before `start`; `samples` are sorted and within `[start, end)`.
  - `pub fn fill(carry: Option<f64>, buckets: &[Bucket], start: Micros, end: Micros, size: Micros) -> Vec<Bucket>` returns every bucket from `start` to `end`, the given ones as-is. A quiet bucket gets the carried value: `min = max = last = carry`, `increase = 0`, `integral = carry × span`, where `span` is cut at `end`. Buckets before the first known value are omitted.
  - `pub fn merge(buckets: &[Bucket], window: Micros) -> Vec<Bucket>` merges consecutive buckets into UTC-aligned windows. It sums `covered`, `integral` and `increase`, takes min of `min` and max of `max`, and keeps the last bucket's `last`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] pub enum EnergyKind { Counter, Power }`
  - `pub fn kwh(buckets: &[Bucket], from: Micros, to: Micros, kind: EnergyKind) -> Option<f64>` uses the buckets with `start` in `[from, to)`. Counter is Σ`increase`; power is Σ`integral` / 1000. It returns `None` when no bucket is in range.
  - `pub fn round3(x: f64) -> f64`

Inside one bucket, the segments are `[bucket start or first known, s1)` with the carry, `[s_i, s_i+1)` with `v_i`, and `[s_n, min(bucket end, end))` with `v_n`. `covered` is the total length of those segments. `increase` sums `max(0, b − a)` over the consecutive pairs carry → s1 and s_i → s_i+1. The carry into the next bucket is that bucket's `last`.

- [ ] **Step 1: Write the failing tests** (times in minutes via `const M: Micros = 60_000_000;`)

```rust
#[test] fn hour_with_carry_and_two_samples() {
    let b = rollup(Some(10.0), &[s(15*M, 20.0), s(45*M, 0.0)], 0, HOUR, HOUR);
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].integral, b[0].covered), (12.5, HOUR));
    assert_eq!((b[0].min, b[0].max, b[0].last, b[0].increase), (0.0, 20.0, 0.0, 10.0));
    assert_eq!(b[0].mean(), 12.5);
}
#[test] fn first_sample_without_carry_covers_from_the_sample() {
    let b = rollup(None, &[s(30*M, 4.0)], 0, HOUR, HOUR);
    assert_eq!((b[0].covered, b[0].integral, b[0].mean()), (30*M, 2.0, 4.0));
}
#[test] fn counter_reset_adds_nothing() {
    let b = rollup(Some(100.0), &[s(10*M, 105.0), s(20*M, 0.0), s(30*M, 3.0)], 0, HOUR, HOUR);
    assert_eq!(b[0].increase, 8.0);
}
#[test] fn partial_last_bucket_stops_at_end() {
    let b = rollup(Some(2.0), &[s(10*M, 2.0)], 0, 30*M, HOUR);
    assert_eq!((b[0].covered, b[0].integral), (30*M, 1.0));
}
#[test] fn fill_quiet_buckets_with_carry_and_cut_at_end() {
    let f = fill(Some(5.0), &[], 0, HOUR + 30*M, HOUR);
    assert_eq!(f.len(), 2);
    assert_eq!((f[0].integral, f[0].increase, f[0].last), (5.0, 0.0, 5.0));
    assert_eq!((f[1].covered, f[1].integral), (30*M, 2.5));
}
#[test] fn fill_omits_buckets_before_first_value() {
    let b = rollup(None, &[s(HOUR + 10*M, 1.0)], 0, 2*HOUR, HOUR);
    assert_eq!(fill(None, &b, 0, 2*HOUR, HOUR).len(), 1);
}
#[test] fn day_mean_is_time_weighted() {
    // 23 quiet hours at 1.0 then one hour at 0.0 (sample at hour 23), merged into one day
    let raw = rollup(Some(1.0), &[s(23*HOUR, 0.0)], 0, 24*HOUR, HOUR);
    let day = merge(&fill(Some(1.0), &raw, 0, 24*HOUR, HOUR), 24*HOUR);
    assert_eq!(round3(day[0].mean()), 0.958);
    assert_eq!((day[0].min, day[0].max), (0.0, 1.0));
}
#[test] fn kwh_counter_and_power() {
    let hours = fill(Some(2000.0), &[], 0, 2*HOUR, HOUR);
    assert_eq!(kwh(&hours, 0, 2*HOUR, EnergyKind::Power), Some(4.0));
    assert_eq!(kwh(&hours, 0, 2*HOUR, EnergyKind::Counter), Some(0.0));
    assert_eq!(kwh(&[], 0, HOUR, EnergyKind::Power), None);
}
#[test] fn round3_rounds_half_away() { assert_eq!(round3(1.23456), 1.235); }
```

- [ ] **Step 2: Run** `cargo test -p oikos-core buckets` — Expected: FAIL.
- [ ] **Step 3: Implement** `rollup`, `fill`, `merge`, `kwh`, `round3` and `EnergyKind` as specified.
- [ ] **Step 4: Run** `cargo test -p oikos-core` — Expected: PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(core): carried-forward buckets and kWh maths`.

### Task 3: Wire types (`oikos-api`)

**Files:**
- Create: `crates/oikos-api/src/history.rs`
- Modify: `crates/oikos-api/src/lib.rs` (`mod history;` + re-exports)
- Test: unit tests in `history.rs`

**Interfaces:**
- Produces (all `Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq`):
  - `pub enum HistoryRange { #[serde(rename="24h")] Day, #[serde(rename="7d")] Week, #[serde(rename="30d")] Month, #[serde(rename="1y")] Year }` with `fn window_secs(self) -> i64` (600, 3600, 18000, 86400) and `fn span_secs(self) -> i64` (86 400, 604 800, 2 592 000, 31 536 000), plus `fn parse(s: &str) -> Option<Self>`
  - `pub use oikos_core::history::buckets::EnergyKind;` (re-exported, with `parse(&str) -> Option<EnergyKind>` added in core if needed)
  - `pub struct HistorySeries { pub window: i64, pub points: Vec<(i64, f64, f64, f64)> }`
  - `pub struct HistoryDaily { pub days: Vec<(i64, f64)> }`
  - `pub struct HistoryToday { pub kwh: Option<f64>, pub since: i64 }`
  - `pub struct LastChangeItem { pub changed_at: Option<i64>, pub recorded: bool }` (`changed_at` serialises as `null` when absent, as OikosHistory does) and `pub struct LastChange { pub items: Vec<LastChangeItem> }`

- [ ] **Step 1: Write the failing test** `series_serialises_points_as_arrays`, which asserts `serde_json::to_value(HistorySeries{window:600, points: vec![(1, 2.0, 1.0, 3.0)]})` == `json!({"window":600,"points":[[1,2.0,1.0,3.0]]})`. Also `range_parses_wire_names`, which asserts `HistoryRange::parse("1y") == Some(Year)` and `parse("2d") == None`.
- [ ] **Step 2: Run** `cargo test -p oikos-api history` — FAIL.
- [ ] **Step 3: Implement** the types. If `schemars` needs it, give `EnergyKind` its derive behind core's `schemars` feature.
- [ ] **Step 4: Run** `cargo test -p oikos-api` and `cargo build -p oikos-web --target wasm32-unknown-unknown` (the web crate must still compile) — PASS.
- [ ] **Step 5: Commit** `feat(api): history wire types`.

### Task 4: Config and the TimescaleDB container

**Files:**
- Modify: `crates/oikos-os/src/config.rs` (add `pub history: HistoryConfig` to `Config`; `"history.url"` in `STRING_ENV_KEYS`)
- Modify: `oikos.example.toml` (a commented `[history]` block)
- Create: `deploy/timescaledb/docker-compose.yml`, `deploy/timescaledb/.env.example`
- Modify: `.gitignore` (add `deploy/timescaledb/.env`)
- Test: `crates/oikos-os/tests/config.rs`

**Interfaces:**
- Produces: `#[derive(Clone, Serialize, Deserialize, PartialEq)] #[serde(default)] pub struct HistoryConfig { pub url: String, pub buffer_rows: usize }`, default `url: ""` and `buffer_rows: 100_000`. `impl HistoryConfig { pub fn enabled(&self) -> bool }` returns true when `url` is non-empty. A manual `Debug` replaces the password in `url` with `***` (parse with `url::Url`; if parsing fails, print `"<unparsable>"`).

- [ ] **Step 1: Write the failing tests** in `tests/config.rs`, following that file's existing figment-jail style:
  - `history_defaults_off`: `!cfg.history.enabled()`, `buffer_rows == 100_000`;
  - `history_url_from_env_stays_a_string`: `OIKOS_HISTORY__URL=postgres://oikos:s3cret@192.168.1.91:5432/oikos` → `cfg.history.url` equals it;
  - `history_debug_masks_password`: `format!("{:?}", cfg.history)` contains `***` and not `s3cret`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test config history` — FAIL.
- [ ] **Step 3: Implement** the config. Write the compose file:

```yaml
# TimescaleDB for the Oikos-os history. Run on the home server:
#   cp .env.example .env   # set a real password
#   docker compose up -d
services:
  timescaledb:
    image: timescale/timescaledb:2.17.2-pg17   # Step 4 checks the tag exists
    container_name: oikos-timescaledb
    restart: unless-stopped
    env_file: .env
    ports:
      - "5432:5432"
    volumes:
      - oikos-history:/var/lib/postgresql/data
volumes:
  oikos-history:
    name: oikos-history
```

  Write `.env.example` with `POSTGRES_USER=oikos`, `POSTGRES_PASSWORD=change-me` and `POSTGRES_DB=oikos`.
- [ ] **Step 4: Verify the image and bring it up locally (the Mac has Docker):**
  - Run `cd deploy/timescaledb && cp .env.example .env && docker compose up -d && docker exec oikos-timescaledb psql -U oikos -d oikos -c "select extversion from pg_available_extensions where name='timescaledb'"`. Expected: one version row.
  - If the tag does not exist, pin the newest `2.x-pg17` tag that does, and note it in the commit message.
- [ ] **Step 5: Run** `cargo test -p oikos-os --test config` — PASS.
- [ ] **Step 6: Commit** `feat(history): [history] config and TimescaleDB compose file`.

### Task 5: Database layer (`oikos_os::history::db`)

**Files:**
- Modify: `Cargo.toml` (workspace dep `sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "postgres", "chrono", "json", "migrate", "macros"] }`), `crates/oikos-os/Cargo.toml` (`sqlx.workspace = true`)
- Create: `crates/oikos-os/migrations/0001_history.sql`, `crates/oikos-os/src/history/mod.rs` (for now only `pub mod db;`), `crates/oikos-os/src/history/db.rs`
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod history;`)
- Test: `crates/oikos-os/tests/history_db.rs`

**Interfaces:**
- Consumes: `Rows`, `ValueRow`, `AvailabilityRow`, `LastWritten`, `Sample`, `Bucket`, `Micros` (Tasks 1–2)
- Produces (`#[derive(Clone)] pub struct Db { pool: PgPool }`):
  - `pub fn connect_lazy(url: &str) -> Result<Db, sqlx::Error>` (pool `max_connections(4)`, `acquire_timeout(5 s)`)
  - `pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError>` (`sqlx::migrate!("./migrations")`)
  - `pub async fn insert(&self, rows: &Rows) -> Result<(), sqlx::Error>`: one transaction. It does `UNNEST` inserts into both hypertables, then upserts `entity_latest` and `availability_latest` with `ON CONFLICT (entity_id) DO UPDATE … WHERE excluded.time >= <table>.time`.
  - `pub async fn load_latest(&self) -> Result<LastWritten, sqlx::Error>`
  - `pub async fn samples(&self, from: Micros, to: Micros, entity: Option<&str>) -> Result<Vec<(String, Sample)>, sqlx::Error>`: rows with `num IS NOT NULL` in `[from, to)`, ordered by `entity_id, time, seq`
  - `pub async fn hourly_carry(&self, before: Micros, entities: &[String]) -> Result<HashMap<String, f64>, sqlx::Error>`: `last` of each entity's latest `history_hourly` row with `bucket < before`
  - `pub async fn upsert_hourly(&self, rows: &[(String, Bucket)]) -> Result<(), sqlx::Error>` (`ON CONFLICT (entity_id, bucket) DO UPDATE`)
  - `pub async fn hourly(&self, entity: &str, from: Micros, to: Micros) -> Result<Vec<Bucket>, sqlx::Error>`
  - `pub async fn done_until(&self) -> Result<Option<Micros>, sqlx::Error>` and `pub async fn set_done_until(&self, at: Micros) -> Result<(), sqlx::Error>`
  - `pub async fn first_time(&self) -> Result<Option<Micros>, sqlx::Error>` (oldest `entity_state.time`, for the first rollup)
  - `pub async fn last_changes(&self, ids: &[String]) -> Result<HashMap<String, Micros>, sqlx::Error>` (from `entity_latest`)
  - `pub async fn is_recorded(&self, id: &str) -> Result<bool, sqlx::Error>`
  - Conversions `Micros` ↔ `chrono::DateTime<Utc>` live here (private helpers).

`migrations/0001_history.sql`:

```sql
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE entity_state (
    time      timestamptz      NOT NULL,
    seq       bigserial,
    entity_id text             NOT NULL,
    num       double precision,
    value     jsonb            NOT NULL
);
SELECT create_hypertable('entity_state', by_range('time', INTERVAL '7 days'));
CREATE INDEX entity_state_entity_time ON entity_state (entity_id, time DESC, seq DESC);
ALTER TABLE entity_state SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'entity_id',
    timescaledb.compress_orderby = 'time DESC, seq DESC'
);
SELECT add_compression_policy('entity_state', INTERVAL '7 days');

CREATE TABLE entity_availability (
    time         timestamptz NOT NULL,
    seq          bigserial,
    entity_id    text        NOT NULL,
    availability text        NOT NULL
);
SELECT create_hypertable('entity_availability', by_range('time', INTERVAL '30 days'));
CREATE INDEX entity_availability_entity_time ON entity_availability (entity_id, time DESC, seq DESC);
ALTER TABLE entity_availability SET (timescaledb.compress, timescaledb.compress_segmentby = 'entity_id');
SELECT add_compression_policy('entity_availability', INTERVAL '30 days');

CREATE TABLE entity_latest (
    entity_id text PRIMARY KEY,
    time      timestamptz NOT NULL,
    value     jsonb       NOT NULL
);
CREATE TABLE availability_latest (
    entity_id    text PRIMARY KEY,
    time         timestamptz NOT NULL,
    availability text        NOT NULL
);

CREATE TABLE history_hourly (
    entity_id  text             NOT NULL,
    bucket     timestamptz      NOT NULL,
    covered_us bigint           NOT NULL,
    integral   double precision NOT NULL,
    min        double precision NOT NULL,
    max        double precision NOT NULL,
    last       double precision NOT NULL,
    increase   double precision NOT NULL,
    PRIMARY KEY (entity_id, bucket)
);

CREATE TABLE rollup_state (
    id         smallint PRIMARY KEY CHECK (id = 1),
    done_until timestamptz NOT NULL
);
```

Test harness in `tests/history_db.rs`: `async fn fresh_db() -> Option<Db>` reads `TEST_DATABASE_URL`. When it is unset, it prints `skipping: TEST_DATABASE_URL unset` and returns `None`, and each test returns early. When it is set, it creates `oikos_test_<pid>_<n>` on that server, connects to it, runs `migrate()`, and drops the database at the end of the test. Use `postgres://oikos:change-me@localhost:5432/oikos` against the Task 4 container.

- [ ] **Step 1: Write the failing tests:**
  - `insert_then_load_latest_round_trips`: insert two value rows for `a` (21.0, then 22.0, 1 µs apart) and one availability row. Then `load_latest()` followed by `diff` of `Number(22.0)` Online yields empty `Rows`.
  - `same_microsecond_keeps_insert_order`: two rows with the same `at` (1.0, then 2.0). `samples(..)` returns them in insert order.
  - `last_changes_and_recorded`: `is_recorded("a")` is true and `is_recorded("ghost")` is false. `last_changes(["a","ghost"])` has only `a`, at the second row's time.
  - `hourly_upsert_replaces`: upsert a bucket, then upsert it again with other numbers. `hourly(..)` returns the second.
  - `hourly_carry_takes_latest_before`: buckets at hour 0 (last 1.0) and hour 2 (last 3.0). `hourly_carry(2h, ["a"])` gives `{a: 1.0}`.
- [ ] **Step 2: Run** `TEST_DATABASE_URL=postgres://oikos:change-me@localhost:5432/oikos cargo test -p oikos-os --test history_db`. Expected: FAIL (no module).
- [ ] **Step 3: Implement** `db.rs` and the migration.
- [ ] **Step 4: Run** the same command. Expected: PASS. Also run without the variable: PASS with the skip line printed.
- [ ] **Step 5: Commit** `feat(history): Postgres schema and database layer`.

### Task 6: Recorder, rollup task, hub wiring

**Files:**
- Modify: `crates/oikos-os/src/history/mod.rs` (the `History` handle, `HistoryError`)
- Create: `crates/oikos-os/src/history/writer.rs`, `crates/oikos-os/src/history/rollup.rs`
- Modify: `crates/oikos-os/src/clock.rs` (`pub fn now_micros() -> i64`)
- Modify: `crates/oikos-os/src/hub.rs` (field `history: OnceLock<Arc<History>>`, `set_history`, `history`, and the feed in `ingest`)
- Modify: `crates/oikos-os/src/main.rs` (start history when `cfg.history.enabled()`)
- Test: `crates/oikos-os/tests/hub.rs`, `crates/oikos-os/tests/history_db.rs`

**Interfaces:**
- Consumes: `Db` (Task 5); `Incoming`, `Pending`, `LastWritten`, `rollup`, `HOUR` (Tasks 1–2); `HistoryConfig` (Task 4)
- Produces:
  - `pub struct History`, holding the sender, `Option<Db>`, `zone: Tz`, a drop counter and `dirty_from: AtomicI64`.
  - `impl History`:
    - `pub fn start(cfg: &HistoryConfig, zone: Tz) -> Result<Arc<History>, sqlx::Error>` connects lazily and spawns the writer and the rollup tasks.
    - `pub fn detached(capacity: usize, zone: Tz) -> (Arc<History>, mpsc::Receiver<Incoming>)` builds one with no database, for tests.
    - `pub fn offer(&self, incoming: Incoming)` uses `try_send` only. When the channel is full it counts the drop and logs one warning per run of drops.
    - `pub fn db(&self) -> Result<&Db, HistoryError>` and `pub fn zone(&self) -> Tz`.
  - `pub enum HistoryError { Unavailable(String), NotRecorded, BadRequest(String) }` with `pub fn code(&self) -> &'static str`, returning `history_unavailable`, `not_recorded` or `bad_request`.
  - `Hub`:
    - `pub fn set_history(&self, history: Arc<History>)` (`OnceLock::set`; a second call is ignored)
    - `pub fn history(&self) -> Option<&Arc<History>>`
  - `pub async fn rollup_once(db: &Db, now: Micros, dirty_from: Micros) -> Result<(), sqlx::Error>` in `rollup.rs` is public so tests can drive it.

Behaviour:
- **Feed (`hub.rs` `ingest`).** Take `clock::now_micros()` once per message. Inside the existing registry-lock block, if `self.history.get()` is set, collect `(id, reg.state(id).cloned())` for each `Change::StateChanged(id)`. After the lock is released, call `history.offer(Incoming { at, id, state })` for each.
- **Writer.**
  - The channel has capacity 4 096. The task drains it into `Pending::new(cfg.buffer_rows)` at all times, including while waiting on the database (`tokio::select!`).
  - Once connected, it runs `migrate()` and then `load_latest()`. It never writes before the cache is seeded.
  - Every 1 s, or when 500 states are pending, it `take`s a batch and runs `diff` on a clone of the cache. It inserts the batch, then replaces the cache. On error it calls `requeue_front` and backs off 1 s, doubling up to 30 s. The URL is never logged; the error is.
  - After a successful insert whose oldest row is before the current hour, it lowers `dirty_from` to that row's hour.
  - Each time `Pending::push` drops a state, it logs one warning per outage with the running count.
- **Rollup** runs every 5 min and once 30 s after start.
  - Range: `from = min(done_until, dirty_from, hour_floor(now) − HOUR)`, where `done_until` falls back to `hour_floor(first_time)`, and `to = hour_floor(now)`.
  - It walks the range in steps of at most 24 h. For each step it loads the `samples`, takes `hourly_carry` for the entities present, runs `rollup` per entity with `size = HOUR`, and calls `upsert_hourly`.
  - Then it calls `set_done_until(to)` and resets `dirty_from` to `i64::MAX`.
- **main.rs.** When `History::start` errors, log it and run without history (the house keeps working). Abort the history tasks on shutdown, like `engine`.

- [ ] **Step 1: Write the failing tests:**
  - `tests/hub.rs` `state_change_feeds_history`: use `History::detached(8, UTC)` and `hub.set_history`. Ingest the switch fixture config and then an `{"state":"ON"}` state message. The receiver gets one `Incoming` with that id and `OnOff(true)`. Ingesting the same payload again yields nothing (no `StateChanged`).
  - `tests/hub.rs` `offer_never_blocks_when_full`: `detached(1, UTC)`, and offer 3 times without receiving. The call returns immediately (wrap it in `tokio::time::timeout(10 ms)`), and the receiver holds 1.
  - `tests/history_db.rs` `replay_after_restart_writes_nothing`: write `Number(20.0)` through `Db::insert`. Then `load_latest`, and `diff` of an `Incoming` with `Number(20.0)` gives empty `Rows`, while `Number(21.0)` gives one value row.
  - `tests/history_db.rs` `rollup_fills_hourly_with_carry`: insert `a` = 10.0 at hour 0 + 15 min and 20.0 at hour 1 + 30 min. Run `rollup_once(now = hour 3, dirty_from = MAX)`. `hourly("a", 0, 3h)` then has two buckets. The second has `min 10.0`, `max 20.0` and `integral 15.0` (10 × 0.5 + 20 × 0.5).
  - `tests/history_db.rs` `late_rows_recompute_their_hour`: after the previous rollup, insert `a` = 30.0 at hour 1 + 45 min. Run `rollup_once(now = hour 3, dirty_from = hour 1)`. Bucket 1 now has `max 30.0` and `last 30.0`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test hub history` and `TEST_DATABASE_URL=… cargo test -p oikos-os --test history_db`. Expected: FAIL.
- [ ] **Step 3: Implement** the pieces above.
- [ ] **Step 4: Run** `cargo test -p oikos-os` (whole crate, with `TEST_DATABASE_URL` set) and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Smoke run.**
  - Put `[history] url = "postgres://oikos:change-me@localhost:5432/oikos"` in the local `oikos.toml` and start the hub with `tools/restart.sh`. After a minute run `docker exec oikos-timescaledb psql -U oikos -d oikos -c "select count(*) from entity_state"`. Expected: a non-zero count.
  - Restart the hub and run the count again. Expected: unchanged, apart from real changes.
- [ ] **Step 6: Commit** `feat(history): recorder and hourly rollup fed from the hub`.

### Task 7: Read queries and HTTP routes

**Files:**
- Create: `crates/oikos-os/src/history/query.rs`
- Modify: `crates/oikos-os/src/http/api.rs` (routes `/history/series`, `/history/daily`, `/history/today`, `/history/last_change`)
- Test: unit tests in `query.rs`, `crates/oikos-os/tests/http.rs`, `crates/oikos-os/tests/history_db.rs`

**Interfaces:**
- Consumes: `History`, `HistoryError`, `Db` (Tasks 5–6); `rollup`, `fill`, `merge`, `kwh`, `round3`, `EnergyKind` (Task 2); the wire types (Task 3)
- Produces:
  - `pub fn day_starts(zone: Tz, now: Micros, days: u32) -> Vec<Micros>`. It returns local midnights from today minus `days − 1` through today, then `now` as the closing boundary, so the length is `days + 1`.
  - `pub async fn series(h: &History, entity: &str, range: HistoryRange, now: Micros) -> Result<HistorySeries, HistoryError>`
  - `pub async fn daily(h: &History, entity: &str, days: u32, kind: EnergyKind, now: Micros) -> Result<HistoryDaily, HistoryError>`
  - `pub async fn today(h: &History, entity: &str, kind: EnergyKind, now: Micros) -> Result<HistoryToday, HistoryError>`
  - `pub async fn last_change(h: &History, ids: &[String]) -> Result<LastChange, HistoryError>` (1–50 ids, else `BadRequest`)
  - `pub(crate) async fn hours(h: &History, entity: &str, from: Micros, now: Micros) -> Result<Vec<Bucket>, HistoryError>`. It returns filled hourly buckets from `from` to `now`: stored rows before `done_until`, plus a live `rollup` from raw `samples` for `[done_until, now)` using the stored carry. The helper is shared by series 7d/30d/1y, daily and today.

Behaviour:
- `series` 24h:
  - The carry at `start` is the last raw sample in `[hour_floor(start), start)`, or `hourly_carry(hour_floor(start))`.
  - Then `rollup(size = 600 s)` on `samples(start, now, entity)`, followed by `fill`.
- `series` for the other ranges: `merge(hours(..), window)`.
- Points are `(start / 1e6, round3(mean), round3(min), round3(max))`, with `start = align_down(now − span, window)`.
- An entity that is not recorded gives `NotRecorded`; a database error gives `Unavailable`.
- `daily` and `today` use `kwh` over `day_starts`, and omit days that return `None`. `today` is `since = local midnight / 1e6`.
- Routes:
  - All four parse `RawQuery` with `url::form_urlencoded`: `entity` (repeatable for `last_change`), `range`, `days` (7|30|365), `kind` (counter|power). Anything missing or invalid → 400 `bad_request`.
  - No history configured → 503 `history_unavailable`.
  - `HistoryError` maps to its status with body `ApiError { error: code() }`.

- [ ] **Step 1: Write the failing tests:**
  - `query.rs` `day_starts_cross_dst`: zone `Europe/Paris`, now = 2026-10-26T10:00 local, days 2. Expected `[2026-10-25T00:00+02:00, 2026-10-26T00:00+01:00, now]`, and the first day is 25 h long. Also 2026-03-30T10:00 local with days 2: the first day is 23 h.
  - `tests/http.rs` `history_routes_without_history_are_503`: `GET /api/v1/history/series?entity=x&range=24h` → 503 with `{"error":"history_unavailable"}`.
  - `tests/http.rs` `history_bad_params_are_400`: a hub with `History::detached` set and `range=2d` → 400 `bad_request`. `last_change` with no `entity` → 400. `daily` with `days=8` → 400. Parameters are checked before the database is touched.
  - `tests/history_db.rs` `series_24h_carries_into_quiet_windows`:
    - Insert `a` = 5.0 at now − 2 h and run `rollup_once`.
    - `series(a, Day, now)` has its points from the window holding now − 2 h onward. Every later point is `(5.0, 5.0, 5.0)`, and no point comes before it.
  - `tests/history_db.rs` `today_power_and_unrecorded`:
    - Insert a power entity at 1000.0 at local midnight + 1 h, with now = midnight + 3 h. `today(Power)` gives `kwh ≈ 2.0`.
    - `series("ghost", …)` → `NotRecorded`.
    - `last_change(["a","ghost"])` → `[{changed_at: Some(_), recorded: true}, {changed_at: None, recorded: false}]`.
- [ ] **Step 2: Run** the three test targets. Expected: FAIL.
- [ ] **Step 3: Implement** `query.rs` and the routes.
- [ ] **Step 4: Run** `cargo test -p oikos-os` (with `TEST_DATABASE_URL`) and clippy. Expected: PASS.
- [ ] **Step 5: Smoke.**
  - With the hub from Task 6 running, `curl -s 'localhost:8081/api/v1/history/series?entity=<a sensor id from /api/v1/snapshot>&range=24h'` returns points.
  - `curl -s 'localhost:8081/api/v1/history/last_change?entity=<id>'` returns one item.
- [ ] **Step 6: Commit** `feat(history): series, daily, today and last_change routes`.

### Task 8: MCP `get_history` and docs

**Files:**
- Modify: `crates/oikos-os/src/mcp/params.rs` (`GetHistoryParams`, `HistoryAnswer`), `crates/oikos-os/src/mcp/tools.rs` (the tool; update the module doc's tool count), `crates/oikos-os/src/mcp/mod.rs` (server instructions: one sentence on `get_history`)
- Modify: `README.md`:
  - a "History" section covering the compose file, `[history] url`, the four routes with their parameters, and the `pg_dump` backup line `docker exec oikos-timescaledb pg_dump -U oikos -Fc oikos > oikos-history-$(date +%F).dump`;
  - rows in the routes table;
  - the MCP tool list.
- Modify: `docs/superpowers/plans/2026-10-19-web-energy-security-deferred.md` (point the history items at the new routes)
- Test: `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: `query::{series, daily, today, last_change}` (Task 7)
- Produces:
  - `GetHistoryParams { entity: String, what: HistoryWhat, range: Option<HistoryRange>, days: Option<u32>, kind: Option<EnergyKind> }`, where `HistoryWhat` is `series | daily | today | last_change` (snake_case).
  - `HistoryAnswer { series: Option<HistorySeries>, daily: Option<HistoryDaily>, today: Option<HistoryToday>, last_change: Option<LastChangeItem> }` with `skip_serializing_if = "Option::is_none"`, so the result is always an object.
  - A missing `range`, `days` or `kind` for the chosen `what` is a tool error that names the field.

- [ ] **Step 1: Write the failing test** `get_history_without_history_reports_unavailable` in `tests/mcp.rs`, following the file's client pattern. It calls `get_history {entity:"x", what:"series", range:"24h"}` on a hub with no history, and the tool error text contains `history_unavailable`. Also `get_history_is_listed`: `list_tools` includes `get_history` with `read_only_hint`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test mcp history`. Expected: FAIL.
- [ ] **Step 3: Implement** the tool and update the docs.
- [ ] **Step 4: Run** `cargo test --workspace` (with `TEST_DATABASE_URL`) and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(mcp): get_history tool; docs: history`.

### Task 9: Bring it up in the house (needs the user)

**Files:** none (deploy only). The README is updated if a step differs.

- [ ] **Step 1:** Ask the user for SSH access to the Geekom, and check that nothing is already on port 5432 (`ss -ltnp | grep 5432`).
- [ ] **Step 2:** Copy `deploy/timescaledb/` to the Geekom, create `.env` with a real password, and run `docker compose up -d`. Then `docker exec oikos-timescaledb psql -U oikos -d oikos -c 'select 1'` should print `1`.
- [ ] **Step 3:** Set `[history] url = "postgres://oikos:<password>@192.168.1.91:5432/oikos"` in the hub's `oikos.toml` and restart with `tools/restart.sh`. The log should show the migrations applied and no history errors.
- [ ] **Step 4:** After 10 minutes, `select count(*), count(distinct entity_id) from entity_state` is non-zero. One real sensor's `/api/v1/history/series?range=24h` returns points.
- [ ] **Step 5:** Record the live state in project memory, and in the README if the deploy steps changed. No commit is needed unless docs changed.
