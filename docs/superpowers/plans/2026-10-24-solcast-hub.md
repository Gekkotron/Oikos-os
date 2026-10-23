# Solcast solar forecast, crate and hub — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `oikos-solcast` crate and a hub `solar/` module that fetch the user's Solcast rooftop forecast inside a strict daily request budget and expose it as `solar_…` entities, `GET /api/v1/solar`, a `SolarChanged` WS event, the `get_solar` MCP tool and a per-day morning-forecast row in the history database.

**Architecture:** The crate is pure Solcast knowledge: parsing, merging sites, day figures in the house time zone, the call budget planner, and a small HTTP client. The hub mirrors `crates/oikos-os/src/weather/` (pure `entities.rs`, a `SolarSlot` in `hub.rs`, a `poller.rs` task) and persists the forecast plus the call ledger as a retained MQTT document so restarts never spend calls. Web work (card, `/solar` page) is Plan 2, after the home-header web branch merges.

**Tech Stack:** Rust 2024 workspace, tokio, reqwest 0.13, chrono + chrono-tz 0.10, serde, axum (tests), sqlx/TimescaleDB (history), rmcp (MCP).

**Spec:** `docs/superpowers/specs/2026-10-23-solcast-solar-forecast-design.md` (§3–§7, §10, §11 for this plan).

## Global Constraints

- New crate `crates/oikos-solcast`, `#![forbid(unsafe_code)]`, `edition/version/authors/license/repository.workspace = true`, workspace dependencies where they exist (`chrono-tz = "0.10"` as in `oikos-os`).
- Solcast: `GET {base}/rooftop_sites/{resource_id}/forecasts?format=json&hours=192`, header `Authorization: Bearer <key>`, base `https://api.solcast.com.au`; values are mean kW over the period ending at `period_end`.
- Budget: calls counted per **UTC** day, every attempted request counts; `RESERVE = 2`; refreshes per day = `max(1, (daily_limit − 2) / sites)`; no refresh at night; 429 → day's budget spent, wait until next UTC midnight + 5 min; 401/403 → no more calls until restart; never an immediate retry.
- Freshness: `stale` when a planned refresh has been failing for > 1 h or the last success is > 18 h old; entities **offline** when no period covers now or the last success is > 24 h old.
- Entities: device `Solar forecast`, `identifiers: ["oikos_solar"]`, unique ids `solar_<key>`, units `kWh`/`kW`, **no** `energy`/`power` device class, **no** `total_increasing`, icon `mdi:solar-power`; `forecast_peak_time_today` has device class `timestamp`.
- Topics (P = first discovery prefix): configs `P/sensor/solar/<key>/config`, states `P/solar/<key>`, availability `P/solar/availability`, cache `P/solar/cache` (retained); `solar` joins `RegistryConfig::default_reserved`.
- The API key: file `secrets/solcast.key` next to the config by default (`[solcast] api_key_file` overrides); never in a log, error `Display`/`Debug`, API response, MQTT payload or tracked file. No real site id in tracked files.
- Config errors with `enabled = true` stop the hub at start, message names the field (`[solcast] …`).
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Before any build: if `~/dev-scratch/cargo-target` is over 5 GB, `cargo clean`.
- Done = `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` clean.

## Review Focus

1. **Restart loop / lost cache** — a hub restarted 20 times in a day, or started with the retained cache missing, must never exceed `daily_limit` calls in a UTC day. Test in Task 3 (`restart_storm_never_exceeds_limit`) and Task 8 (`restart_reads_cache_and_makes_no_call`).
2. **Time zones where daylight straddles UTC midnight** (America/Los_Angeles, Australia/Sydney) — a simulated week must stay within the limit on every UTC day and still refresh every local day. Test in Task 3 (`week_within_limit_in_<zone>`).
3. **DST change and periods straddling local midnight** — day totals must put each period in the local day of its start and keep the 23 h / 25 h days right. Test in Task 2.
4. **One site of two failing** — no half-merged forecast; the previous forecast stays and the failure counts both attempted calls. Test in Task 8 (`one_failing_site_keeps_previous_forecast`).
5. **Long winter night** — 16 h between the last and first refresh must leave entities online and `stale == false`. Test in Task 6 (`winter_night_is_not_stale`).

---

## File map

**Create**
- `crates/oikos-solcast/Cargo.toml`, `src/lib.rs` (re-exports), `src/period.rs` (Period + parsing), `src/forecast.rs` (merge, trim, day figures), `src/budget.rs` (Ledger, slots, decide), `src/client.rs` (ApiKey, Client, SolcastError), `tests/fixtures/forecasts.json`, `tests/client.rs`.
- `crates/oikos-api/src/solar.rs` (SolarView and parts).
- `crates/oikos-os/src/solar/{mod.rs, view.rs, entities.rs, hub.rs, poller.rs}`.
- `crates/oikos-os/src/history/solar.rs`, `crates/oikos-os/migrations/0003_solar.sql`.
- `crates/oikos-os/tests/{solar_hub.rs, solar_poller.rs}`.
- `docs/users/solar-forecast.md`.

**Modify**
- `crates/oikos-api/src/{lib.rs, events.rs}`; `crates/oikos-core/src/registry/mod.rs:43-51`.
- `crates/oikos-os/{Cargo.toml, src/lib.rs, src/config.rs, src/hub.rs, src/main.rs, src/http/api.rs, src/mcp/tools.rs, src/mcp/mod.rs, src/history/mod.rs}`; `crates/oikos-os/tests/{config.rs, mcp_stdio.rs, history_db.rs}`.
- `crates/oikos-web/src/{house.rs:158, house_state.rs:79}` (ignore the new event).
- `oikos.example.toml`, `secrets/README.md`, `docs/reference/{configuration.md, http-api.md, mcp-tools.md, mqtt-topics.md}`.

---

### Task 1: Crate skeleton and response parsing

**Files:**
- Create: `crates/oikos-solcast/Cargo.toml`, `crates/oikos-solcast/src/lib.rs`, `crates/oikos-solcast/src/period.rs`, `crates/oikos-solcast/tests/fixtures/forecasts.json`

**Interfaces:**
- Produces:
  - `pub struct Period { pub end: DateTime<Utc>, pub minutes: u32, pub estimate: f64, pub low: f64, pub high: f64 }` — `Clone, Debug, PartialEq, Serialize, Deserialize`; `impl Period { pub fn start(&self) -> DateTime<Utc>; pub fn kwh(&self) -> f64 /* estimate × minutes / 60 */ }`.
  - `pub fn parse_forecasts(body: &str) -> Result<Vec<Period>, ParseError>`; `#[derive(Debug, thiserror::Error)] pub enum ParseError { #[error("invalid Solcast JSON: {0}")] Json(String), #[error("invalid period_end `{0}`")] Time(String) }`.

Dependencies: `serde`, `serde_json`, `thiserror`, `chrono` (workspace, add features `serde`, `clock` not needed), `chrono-tz = "0.10"`, `reqwest` (workspace, `json`). Dev: `tokio` (`rt`, `macros`), `axum`.

Fixture: hand-written from the documented shape, 2026-10-23 Europe/Paris, `period_end` with 7 fractional digits (`"2026-10-23T05:30:00.0000000Z"`), 48 consecutive 30-min periods from `2026-10-22T22:30Z` (local midnight + 30 min) to `2026-10-23T22:00Z`, zero at night, a bell between ~06:00Z and ~16:30Z peaking at `pv_estimate: 2.4` on the period ending `11:30Z`, `pv_estimate10 = 0.7 ×`, `pv_estimate90 = 1.15 ×` the estimate, plus one unknown field per entry (`"extra": 1`). Today's estimate sum must be **12.4 kWh** (scale the bell to hit it; tests below rely on it).

- [ ] **Step 1: Write the failing tests** (`period.rs` `#[cfg(test)]`)

```rust
#[test] fn parses_documented_shape() {
    let p = parse_forecasts(include_str!("../tests/fixtures/forecasts.json")).unwrap();
    assert_eq!(p.len(), 48);
    assert_eq!(p[0].end, "2026-10-22T22:30:00Z".parse::<DateTime<Utc>>().unwrap());
    assert_eq!(p[0].minutes, 30);
    let total: f64 = p.iter().map(Period::kwh).sum();
    assert!((total - 12.4).abs() < 1e-6);
}
#[test] fn period_duration_variants() { /* "PT30M"→30, "PT15M"→15, "PT1H"→60, missing or "garbage"→30 */ }
#[test] fn start_is_end_minus_duration() { /* end 12:00Z, 30 → start 11:30Z */ }
#[test] fn bad_json_and_bad_time_are_errors() { /* "{" → ParseError::Json, period_end "yesterday" → ParseError::Time */ }
#[test] fn empty_forecasts_is_ok_and_empty() { /* {"forecasts":[]} → Ok(vec![]) */ }
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-solcast` → compile error (`parse_forecasts` not found).
- [ ] **Step 3: Implement `period.rs`**; `lib.rs` declares the modules with `#![forbid(unsafe_code)]` and a crate doc line ("Solcast rooftop-site forecasts: parsing, day figures, the call budget and a small client"). Parse `period` as `PT<n>M` / `PT<n>H`, anything else 30.
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-solcast`.
- [ ] **Step 5: Commit** — `feat(solcast): crate skeleton and forecast parsing`.

---

### Task 2: Merging sites and day figures

**Files:**
- Create: `crates/oikos-solcast/src/forecast.rs`

**Interfaces:**
- Consumes: `Period` (Task 1).
- Produces:
  - `pub struct Forecast { pub fetched_at: i64 /* unix s */, pub periods: Vec<Period> /* summed */, pub sites: Vec<Vec<Period>> }` — `Clone, Debug, PartialEq, Serialize, Deserialize`.
  - `pub fn merge(sites: Vec<Vec<Period>>, fetched_at: i64) -> Forecast` — sums `estimate/low/high` of periods with the same `end`; a period missing from one site counts as 0 there; result sorted by `end`.
  - `impl Forecast { pub fn trim(&mut self, from: DateTime<Utc>, until: DateTime<Utc>) }` — keeps periods whose `start() >= from && end <= until`, in `periods` and every `sites[i]`.
  - `pub struct DayTotal { pub date: NaiveDate, pub estimate: f64, pub low: f64, pub high: f64 }` (`Clone, Debug, PartialEq, Serialize, Deserialize`).
  - `pub fn day_totals(periods: &[Period], zone: Tz) -> Vec<DayTotal>` — kWh per local date of each period's **start**, ascending.
  - `pub fn remaining(periods: &[Period], zone: Tz, now: DateTime<Utc>) -> f64` — kWh from `now` to the end of now's local day; the period containing `now` is prorated.
  - `pub fn power_at(periods: &[Period], now: DateTime<Utc>) -> Option<f64>` — estimate of the period with `start() <= now < end`.
  - `pub fn peak(periods: &[Period], date: NaiveDate, zone: Tz) -> Option<(f64, DateTime<Utc>)>` — max estimate of that local date and the **midpoint** of its period; earliest on ties; `None` when the max is 0 or the day has no period.
  - `pub fn best_hours(periods: &[Period], date: NaiveDate, zone: Tz) -> Option<(DateTime<Utc>, DateTime<Utc>)>` — longest contiguous run of that date's periods with `estimate >= 0.6 × peak`, as (start of first, end of last); earliest on ties; `None` without a peak.

- [ ] **Step 1: Write the failing tests** (in `forecast.rs`)

```rust
#[test] fn merge_sums_by_end_and_keeps_sites() { /* two 3-period sites, one missing a period → sums; sites.len()==2 */ }
#[test] fn fixture_day_total_is_12_4() { /* day_totals(fixture, Europe/Paris)[0] == 2026-10-23, estimate≈12.4, low≈0.7*12.4, high≈1.15*12.4 */ }
#[test] fn period_straddling_local_midnight_counts_in_its_start_day() { /* period 22:00Z→22:30Z on 2026-10-23 (Paris UTC+2 → 00:00–00:30 local on 24th) counts on 2026-10-24 */ }
#[test] fn dst_end_day_has_50_periods() { /* Europe/Paris 2026-10-25 (25 h): 50 half-hour periods of 1 kW → 25.0 kWh on that date */ }
#[test] fn remaining_prorates_current_period() { /* periods of 2 kW; now 15 min into one → remaining counts 0.25 kWh of it */ }
#[test] fn power_at_and_none_outside() { /* inside → Some; before first / after last → None */ }
#[test] fn peak_is_midpoint_of_max_period() { /* fixture → (2.4, 11:15Z) */ }
#[test] fn best_hours_longest_run_over_60_percent() { /* hand series with two runs ≥0.6×peak, the longer one wins; flat zero day → None */ }
#[test] fn trim_drops_outside_window_everywhere() { /* periods and sites[i] trimmed alike */ }
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-solcast forecast` → compile errors.
- [ ] **Step 3: Implement `forecast.rs`**; local date of an instant via `zone.from_utc_datetime(&t.naive_utc()).date_naive()`.
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-solcast`.
- [ ] **Step 5: Commit** — `feat(solcast): merge sites and compute day figures`.

---

### Task 3: The call budget planner

**Files:**
- Create: `crates/oikos-solcast/src/budget.rs`

**Interfaces:**
- Produces:
  - `pub const RESERVE: u32 = 2;`
  - `#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] pub struct Ledger { pub utc_day: Option<NaiveDate>, pub calls: u32, pub last_attempt: Option<i64>, pub blocked_until: Option<i64> }`
  - `impl Ledger { pub fn calls_on(&self, now: DateTime<Utc>) -> u32; pub fn record_attempt(&mut self, now: DateTime<Utc>); pub fn record_quota_exceeded(&mut self, now: DateTime<Utc>, daily_limit: u32) }` — `record_attempt` resets `calls` when the UTC day changed, then `calls += 1`, `last_attempt = now`; `record_quota_exceeded` sets `calls = daily_limit` for now's UTC day and `blocked_until = next UTC midnight + 5 min`.
  - `pub fn refreshes_per_day(daily_limit: u32, sites: u32) -> u32` = `max(1, daily_limit.saturating_sub(RESERVE) / sites.max(1))`.
  - `pub fn slots(sunrise: Option<DateTime<Utc>>, sunset: Option<DateTime<Utc>>, date: NaiveDate, zone: Tz, refreshes: u32) -> Vec<DateTime<Utc>>`
  - `#[derive(Debug, PartialEq, Eq)] pub enum Decision { CallNow, SleepUntil(DateTime<Utc>) }`
  - `pub fn decide(now: DateTime<Utc>, ledger: &Ledger, has_forecast: bool, slots: &[DateTime<Utc>], daily_limit: u32, sites: u32) -> Decision`

Slots (exact):
```
rise = sunrise, or date 07:00 local; set = sunset, or date 19:00 local
first = rise − 30 min
if refreshes == 1 → [first]
last = set − 60 min
others: for i in 1..refreshes: rise + (last − rise) × i / (refreshes − 1)
result = [first] + others, sorted, deduplicated, every slot > first kept only if ≤ last
```

Decide (exact order):
```
1. blocked_until > now                              → SleepUntil(blocked_until)
2. calls_on(now) + sites > daily_limit              → SleepUntil(next UTC midnight + 5 min)
3. !has_forecast                                    → CallNow
4. due = latest slot ≤ now with slot > last_attempt (any slot if None)
   due exists                                       → CallNow
5. next = earliest slot > now                       → SleepUntil(next), else SleepUntil(now + 1 h)
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn refreshes_per_day_values() { /* (10,1)→8, (10,2)→4, (8,1)→6, (3,2)→1, (1,1)→1 */ }
#[test] fn slots_paris_october_eight() { /* sunrise 06:20Z sunset 16:42Z, 8 → first 05:50Z, last 15:42Z, 8 slots strictly ascending */ }
#[test] fn slots_one_refresh_is_morning_only() {}
#[test] fn slots_polar_fallback_uses_7_and_19_local() {}
#[test] fn ledger_resets_on_new_utc_day() {}
#[test] fn quota_exceeded_blocks_until_next_utc_midnight_plus_5() {}
#[test] fn decide_calls_when_no_forecast_and_budget_left() {}
#[test] fn decide_never_calls_over_limit_even_without_forecast() {}
#[test] fn decide_missed_slots_coalesce_into_one_call() { /* last_attempt before 3 slots ≤ now → CallNow once; after record_attempt → SleepUntil(next slot) */ }
#[test] fn decide_sleeps_at_night_until_morning_slot() {}
#[test] fn restart_storm_never_exceeds_limit() {
    // a hub "restarting" every 10 minutes for a whole UTC day with has_forecast=false each time,
    // ledger persisted: calls_on never exceeds the limit (10, 1 site)
}
#[test] fn week_within_limit_in_paris() {}
#[test] fn week_within_limit_in_los_angeles() {}
#[test] fn week_within_limit_in_sydney() {}
```

The three `week_within_limit_in_<zone>` tests share one helper: step `now` by 5 minutes over 7 days; slots for the local dates yesterday/today/tomorrow (sunrise/sunset from `oikos_core`-free fixed times per zone, e.g. 06:30/18:30 local); on `CallNow` → `record_attempt` once per site (sites = 2); assert every UTC day `calls ≤ 10` and every local date got at least one call.

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-solcast budget`.
- [ ] **Step 3: Implement `budget.rs`.**
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-solcast`.
- [ ] **Step 5: Commit** — `feat(solcast): daily call budget planner`.

---

### Task 4: HTTP client

**Files:**
- Create: `crates/oikos-solcast/src/client.rs`, `crates/oikos-solcast/tests/client.rs`

**Interfaces:**
- Consumes: `parse_forecasts`, `Period` (Task 1).
- Produces:
  - `pub const BASE_URL: &str = "https://api.solcast.com.au";`
  - `pub struct ApiKey(String)` with `ApiKey::new(String) -> ApiKey` (trims) and a manual `Debug` printing `ApiKey(***)`; no `Display`.
  - `#[derive(Debug, thiserror::Error)] pub enum SolcastError { #[error("Solcast rejected the API key")] Unauthorized, #[error("Solcast daily request limit reached")] QuotaExceeded, #[error("Solcast answered HTTP {0}")] Status(u16), #[error("network error: {0}")] Network(String), #[error(transparent)] Parse(#[from] ParseError), #[error("cannot build the HTTP client: {0}")] Build(String) }`
  - `pub struct Client` with `pub fn new(base: impl Into<String>, key: ApiKey) -> Result<Client, SolcastError>` (connect timeout 5 s, total 20 s, `User-Agent: oikos-os/<CARGO_PKG_VERSION>`) and `pub async fn site_forecast(&self, resource_id: &str) -> Result<Vec<Period>, SolcastError>`.

Status mapping: 401/403 → `Unauthorized`, 429 → `QuotaExceeded`, other non-2xx → `Status`. `Network` text uses `reqwest::Error::without_url()` display.

- [ ] **Step 1: Write the failing tests** (`tests/client.rs`, axum stub on `127.0.0.1:0` recording the request)

```rust
#[tokio::test] async fn sends_bearer_and_path_and_parses() { /* path /rooftop_sites/abcd/forecasts, query has format=json and hours=192, header "Bearer k" → 48 periods */ }
#[tokio::test] async fn maps_401_403_429_500() {}
#[tokio::test] async fn key_never_in_debug_or_errors() { /* format!("{:?}", key) == "ApiKey(***)"; every error's Display/Debug lacks the key text "s3cr3t" */ }
#[tokio::test] async fn garbage_body_is_parse_error() {}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-solcast --test client`.
- [ ] **Step 3: Implement `client.rs`**; `lib.rs` re-exports `ApiKey, Client, SolcastError, BASE_URL, Period, parse_forecasts, Forecast, merge, DayTotal, day_totals, remaining, power_at, peak, best_hours, Ledger, Decision, decide, slots, refreshes_per_day, RESERVE`.
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-solcast`.
- [ ] **Step 5: Commit** — `feat(solcast): rooftop-site HTTP client`.

---

### Task 5: Config, API types, WS event, reserved sub-tree

**Files:**
- Create: `crates/oikos-api/src/solar.rs`
- Modify: `crates/oikos-api/src/lib.rs`, `crates/oikos-api/src/events.rs` (variant + serialization test next to the `WeatherChanged` one at line ~140), `crates/oikos-core/src/registry/mod.rs:43-51`, `crates/oikos-os/src/config.rs`, `crates/oikos-os/src/main.rs` (validate next to `cfg.weather.validate()`), `crates/oikos-os/Cargo.toml` (`oikos-solcast = { path = "../oikos-solcast" }`), `crates/oikos-web/src/house.rs:158`, `crates/oikos-web/src/house_state.rs:79`, `oikos.example.toml`, `secrets/README.md`
- Test: `crates/oikos-os/tests/config.rs`

**Interfaces:**
- Produces (`oikos-api`, all `Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema`, times Unix seconds, dates `YYYY-MM-DD` strings):
  - `SolarView { fetched_at: u64, stale: bool, next_refresh_at: Option<u64>, calls_today: u32, daily_limit: u32, now: SolarNow, today: SolarToday, days: Vec<SolarDay>, sites: Vec<SolarSite>, periods: Vec<SolarPeriod>, history: SolarHistory }`
  - `SolarNow { power: Option<f64>, remaining_today: Option<f64> }`
  - `SolarToday { date: String, estimate: Option<f64>, low: Option<f64>, high: Option<f64>, peak: Option<SolarPeak>, best_hours: Option<SolarSpan> }`
  - `SolarPeak { power: f64, at: u64 }`, `SolarSpan { from: u64, to: u64 }`
  - `SolarDay { date: String, estimate: f64, low: f64, high: f64 }`, `SolarSite { index: u32, today: Option<f64> }`
  - `SolarPeriod { end: u64, minutes: u32, estimate: f64, low: f64, high: f64 }`
  - `SolarHistory { forecasts: Vec<SolarForecastDay> }` (`#[serde(default)]`), `SolarForecastDay { date: String, estimate: f64, low: f64, high: f64, made_at: u64 }`
  - `WsEvent::SolarChanged` (unit variant, after `WeatherChanged`).
- Produces (`oikos-os::config`):
  - `pub const SOLCAST_KEY_FILE: &str = "secrets/solcast.key";`
  - `#[serde(default)] pub struct SolcastConfig { pub enabled: bool, pub sites: Vec<String>, pub daily_limit: u32 /* 10 */, pub api_key_file: Option<PathBuf> }` and `Config.solcast: SolcastConfig`.
  - `impl SolcastConfig { pub fn validate(&self) -> Result<(), String>; pub fn read_key(&self) -> Result<String, String> }`.
  - `Config::load`: with no `solcast.api_key_file`, set it to `SOLCAST_KEY_FILE` next to the TOML file (always, so the error names the path). `"solcast.api_key_file"` joins `STRING_ENV_KEYS`.

Validation messages (exact): `"[solcast] sites needs at least one rooftop resource id"`, `"[solcast] sites has an empty id"`, `"[solcast] daily_limit is {n}: it must be at least the number of sites ({m})"`, `"[solcast] cannot read the API key file {path}: {err}"`, `"[solcast] the API key file {path} is empty"`. Disabled → always `Ok`. `read_key` returns the trimmed content; its error never contains the content.

`oikos.example.toml` gains a commented block:
```toml
# [solcast]
# enabled = false
# sites = ["xxxx-xxxx-xxxx-xxxx"]   # rooftop resource ids from the Solcast Toolkit
# daily_limit = 10                   # this hub's share of the account's daily requests
# api_key_file = "secrets/solcast.key"
```
`secrets/README.md` gains `- \`solcast.key\`: the Solcast API key, one line. See \`docs/users/solar-forecast.md\`.`

- [ ] **Step 1: Write the failing tests** — in `tests/config.rs`: `solcast_defaults` (disabled, limit 10, key path ends with `secrets/solcast.key` next to the given TOML), `solcast_validate_messages` (each message above; a temp key file with `"  k \n"` → `read_key() == "k"`), `solcast_disabled_is_always_valid`; in `oikos-api/src/events.rs`: `solar_changed_serializes` next to the weather one; in `oikos-core` registry tests: `solar_is_reserved`.
- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-os --test config solcast; cargo test -p oikos-api; cargo test -p oikos-core registry`.
- [ ] **Step 3: Implement** the types, config, `default_reserved` += `"solar"`, `main.rs` validation, and `WsEvent::SolarChanged` added to the ignored arms in `house.rs` and `house_state.rs`.
- [ ] **Step 4: Run, expect PASS** — the three commands above, then `cargo check -p oikos-api --target wasm32-unknown-unknown && cargo check -p oikos-web --target wasm32-unknown-unknown` (as CI does) compiles.
- [ ] **Step 5: Commit** — `feat(hub): [solcast] config, SolarView and SolarChanged`.

---

### Task 6: View builder and entity payloads (pure)

**Files:**
- Create: `crates/oikos-os/src/solar/mod.rs`, `crates/oikos-os/src/solar/view.rs`, `crates/oikos-os/src/solar/entities.rs`
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod solar;`)

**Interfaces:**
- Consumes: crate `Forecast`, `Ledger`, day functions (Tasks 2–3); `SolarView` (Task 5).
- Produces (`solar/mod.rs`):
  - `pub fn is_stale(fetched_at: u64, failing_since: Option<u64>, now: u64) -> bool` — `failing_since` older than 3600 s, or `fetched_at` older than 18 h.
  - `pub fn is_expired(forecast: &Forecast, now: u64) -> bool` — no period with `start() <= now < end`, or `fetched_at` older than 24 h.
- Produces (`solar/view.rs`): `pub fn build_view(forecast: &Forecast, ledger: &Ledger, zone: Tz, now: DateTime<Utc>, next_refresh_at: Option<u64>, daily_limit: u32, stale: bool) -> SolarView` — `today` = now's local date; `days` = the next 7 local dates after today that have totals; `sites[i].today` from `day_totals(&forecast.sites[i], zone)`; `periods` from local midnight today to the end of tomorrow; kWh and kW rounded to 2 decimals; `history` empty.
- Produces (`solar/entities.rs`): `pub const KEYS: [&str; 10]` (spec §6.1 order: `forecast_today`, `forecast_today_low`, `forecast_today_high`, `forecast_remaining_today`, `forecast_tomorrow`, `forecast_tomorrow_low`, `forecast_tomorrow_high`, `forecast_power_now`, `forecast_peak_today`, `forecast_peak_time_today`); `pub fn configs(prefix: &str) -> Vec<(String, Value)>`; `pub fn states(view: &SolarView) -> Vec<(&'static str, String)>` (numbers as plain text, peak time RFC 3339 UTC, a missing value omitted); `pub fn state_topic(prefix: &str, key: &str) -> String`; `pub fn availability_topic(prefix: &str) -> String`; `pub fn cache_topic(prefix: &str) -> String`; `pub fn is_own_config(payload: &[u8]) -> bool`. Config JSON mirrors `weather/entities.rs` (`unique_id`, `object_id`, `name`, `state_topic`, `availability_topic`, `device {identifiers:["oikos_solar"], name:"Solar forecast", manufacturer:"Oikos"}` etc.), with `unit_of_measurement`, `icon: "mdi:solar-power"`, `state_class: "measurement"` only for `forecast_power_now`, `device_class: "timestamp"` only for the peak time.

- [ ] **Step 1: Write the failing tests** (module tests)

```rust
// view.rs
#[test] fn view_from_fixture_at_noon_paris() { /* today.estimate 12.4, peak (2.4, 11:15Z), remaining matches crate remaining(), sites.len()==1 */ }
#[test] fn view_days_start_tomorrow_max_seven() {}
#[test] fn view_periods_cover_today_and_tomorrow_only() {}
// mod.rs
#[test] fn winter_night_is_not_stale() { /* fetched 15:30Z, now next day 07:30Z (16 h), no failing → false; and not expired with periods covering now */ }
#[test] fn failing_over_an_hour_is_stale() {}
#[test] fn expired_after_24h_or_past_last_period() {}
// entities.rs
#[test] fn configs_have_no_energy_or_power_class() { /* no config has device_class energy/power or state_class total_increasing; peak time has timestamp */ }
#[test] fn configs_topics_and_ids() { /* "oikos/sensor/solar/forecast_today/config", unique_id "solar_forecast_today", state "oikos/solar/forecast_today" */ }
#[test] fn states_omit_missing_values() {}
#[test] fn own_config_recognised() {}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-os --lib solar`.
- [ ] **Step 3: Implement** the three files.
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-os --lib solar`.
- [ ] **Step 5: Commit** — `feat(hub): solar view and entity payloads`.

---

### Task 7: Hub slot, cache, REST and MCP

**Files:**
- Create: `crates/oikos-os/src/solar/hub.rs`, `crates/oikos-os/tests/solar_hub.rs`
- Modify: `crates/oikos-os/src/hub.rs` (fields next to the weather ones ~line 201, init ~line 326, `ingest_message` ~line 499), `crates/oikos-os/src/http/api.rs` (route next to `/weather`), `crates/oikos-os/src/mcp/tools.rs`, `crates/oikos-os/src/mcp/mod.rs` (instructions sentence), `crates/oikos-os/tests/mcp_stdio.rs` (tool list)

**Interfaces:**
- Consumes: Tasks 5–6.
- Produces (`Hub`):
  - fields `solar_enabled: bool`, `solar_sites: Vec<String>`, `solar_daily_limit: u32`, `solar_key_file: Option<PathBuf>`, `solar: Mutex<SolarSlot>`.
  - `pub(crate) struct SolarSlot { forecast: Option<Forecast>, ledger: Ledger, failing_since: Option<u64>, next_refresh_at: Option<u64>, published: BTreeMap<&'static str, String>, online: Option<bool>, configs_ok: bool }`
  - `pub fn solar_enabled(&self) -> bool`, `pub fn solar_sites(&self) -> Vec<String>`, `pub fn solar_daily_limit(&self) -> u32`, `pub fn solar_key_file(&self) -> Option<PathBuf>`, `pub fn solar_ledger(&self) -> Ledger`, `pub fn solar_has_forecast(&self) -> bool`.
  - `pub fn solar_view(&self) -> Result<SolarView, HubError>` — `HubError::Publish("solar unavailable")` (→ 503) while disabled or without a forecast; `stale` computed now.
  - `pub async fn solar_view_with_history(&self) -> Result<SolarView, HubError>` — Task 9 fills `history`; here it returns `solar_view()`.
  - `pub fn apply_solar_forecast(&self, forecast: Forecast)` — stores it, clears `failing_since`, republishes changed states (weather's self-healing configs logic, `None` for vanished keys), availability `online`, publishes the cache, broadcasts `SolarChanged` when the data changed.
  - `pub fn refresh_solar_states(&self)` — recomputes the view at now; republishes changed states; goes offline when `is_expired`; broadcasts `SolarChanged` when `now.power` or `now.remaining_today` changed.
  - `pub fn record_solar_attempt(&self)`, `pub fn record_solar_quota_exceeded(&self)`, `pub fn record_solar_failure(&self)` (sets `failing_since` if unset), `pub fn set_solar_next_refresh(&self, at: Option<u64>)` — each ledger change republishes the cache.
  - `pub fn publish_solar_configs(&self)`, `pub fn set_solar_offline(&self)`.
  - `pub(crate) fn ingest_solar_cache(&self, topic: &str, payload: &[u8]) -> bool` — on `cache_topic`: when enabled and the slot has no forecast yet, loads `{ forecast, ledger }`; malformed → `warn`, ignored; returns true for the cache topic either way.
  - `pub(crate) fn clear_disabled_solar(&self, topic: &str, payload: &[u8], retain: bool) -> bool` — the weather twin: while disabled, empties retained `P/solar/…` (cache included) and own configs.
  - Cache payload: `#[derive(Serialize, Deserialize)] struct SolarCache { forecast: Forecast, ledger: Ledger }` (no key, no site ids).
- REST: `.route("/solar", get(solar))` → `solar_view_with_history()`; MCP `#[tool(name = "get_solar", annotations(read_only_hint = true))]` doc: "The Solcast solar forecast: today, tomorrow, the next 7 days, 30-minute periods and the request budget; `solar unavailable` while `[solcast]` is off or before the first fetch."

- [ ] **Step 1: Write the failing tests** (`tests/solar_hub.rs`, `hub_with(enabled)` like `weather_hub.rs`, forecast built from the Task 1 fixture with `fetched_at` = now)

```rust
#[tokio::test] async fn apply_publishes_configs_states_availability_and_cache() {}
#[tokio::test] async fn solar_changed_only_when_data_changes() {}
#[tokio::test] async fn cache_restores_forecast_and_ledger_once() { /* ingest cache → solar_has_forecast, ledger equal; second cache with other data ignored */ }
#[tokio::test] async fn cache_never_contains_key_or_site_ids() { /* site id "site-secret", key file "s3cr3t": no published payload contains either */ }
#[tokio::test] async fn disabled_hub_clears_retained_solar_topics() {}
#[tokio::test] async fn view_is_503_when_disabled_or_empty() { /* HubError::Publish */ }
#[tokio::test] async fn expired_forecast_goes_offline_on_refresh() {}
```
Plus `mcp_stdio.rs`: `"get_solar"` in the expected tool list.

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-os --test solar_hub`.
- [ ] **Step 3: Implement** `solar/hub.rs`, wire `Hub::new` from `cfg.solcast`, add both checks at the top of `ingest_message` (after the weather one), the route and the tool.
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-os --test solar_hub --test mcp_stdio`.
- [ ] **Step 5: Commit** — `feat(hub): solar slot, retained cache, /api/v1/solar and get_solar`.

---

### Task 8: The poller

**Files:**
- Create: `crates/oikos-os/src/solar/poller.rs`, `crates/oikos-os/tests/solar_poller.rs`
- Modify: `crates/oikos-os/src/main.rs` (start after `start_weather`, abort on shutdown)

**Interfaces:**
- Consumes: crate `Client`, `ApiKey`, `slots`, `decide`, `refreshes_per_day`, `merge`; Task 7 hub methods; `oikos_core::flow::sun::sun_events(lat, lon, date)` with `hub.house_point()`, `hub.zone()`.
- Produces:
  - `impl Hub { pub fn start_solar(self: &Arc<Self>) -> Option<JoinHandle<()>> }` — `None` while disabled; otherwise `spawn_with(self, BASE_URL.to_owned(), CACHE_GRACE)`.
  - `pub const CACHE_GRACE: Duration = Duration::from_secs(3);`
  - `pub fn spawn_with(hub: &Arc<Hub>, base_url: String, cache_grace: Duration) -> JoinHandle<()>`

Loop (exact behaviour):
```
read key from hub.solar_key_file() with the same rules as SolcastConfig::read_key (extract a shared `pub(crate) fn read_key_file(path: &Path) -> Result<String, String>` in config.rs); error → error log, return
wait for ConnectionStatus::Online; publish_solar_configs(); sleep(cache_grace)
loop:
  now; slots for local yesterday, today, tomorrow (sun_events per date)
  decision = decide(now, ledger, has_forecast, slots, limit, sites.len())
  unauthorized flag set → treat as SleepUntil(far future) (no calls until restart)
  CallNow:
    for each site in order: record_solar_attempt(); site_forecast(site)
      Unauthorized → error log once, unauthorized = true, record_solar_failure(), stop this round
      QuotaExceeded → warn log, record_solar_quota_exceeded(), record_solar_failure(), stop
      other error   → warn log once per outage, record_solar_failure(), stop
    all ok → f = merge(results, now); f.trim(local midnight today, +8 days); apply_solar_forecast(f)
  set_solar_next_refresh(next slot > now)
  refresh_solar_states()
  sleep until min(SleepUntil target, next :00/:30 wall-clock boundary), at least 1 s
```

- [ ] **Step 1: Write the failing tests** (`tests/solar_poller.rs`, axum stub for `/rooftop_sites/{id}/forecasts` counting calls per site and serving the fixture, 401, 429 or 500 per test; `App` like `weather_poller.rs`; `cache_grace = Duration::ZERO`)

```rust
#[tokio::test] async fn first_start_fetches_once_per_site_and_serves_view() { /* 2 sites → 2 calls, GET /api/v1/solar 200, today.estimate ≈ 24.8 */ }
#[tokio::test] async fn restart_reads_cache_and_makes_no_call() { /* second hub fed the first one's retained cache before start → 0 new calls while no slot is due */ }
#[tokio::test] async fn one_failing_site_keeps_previous_forecast() { /* site B 500 after the first round; forced CallNow via ledger.last_attempt in the past → view unchanged, calls counted for A and B */ }
#[tokio::test] async fn unauthorized_stops_all_calls() { /* 401 → exactly 1 call over several loop turns */ }
#[tokio::test] async fn quota_exceeded_spends_the_day() { /* 429 → ledger.calls == limit, blocked_until set, no further call */ }
#[tokio::test] async fn disabled_makes_no_request() { /* start_solar() is None */ }
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-os --test solar_poller`.
- [ ] **Step 3: Implement `poller.rs`** and the `main.rs` wiring (validate already done in Task 5).
- [ ] **Step 4: Run, expect PASS** — `cargo test -p oikos-os --test solar_poller`.
- [ ] **Step 5: Commit** — `feat(hub): Solcast poller within the daily budget`.

---

### Task 9: Morning forecast rows in history

**Files:**
- Create: `crates/oikos-os/migrations/0003_solar.sql`, `crates/oikos-os/src/history/solar.rs`
- Modify: `crates/oikos-os/src/history/mod.rs` (`pub mod solar;`), `crates/oikos-os/src/solar/hub.rs` (write on apply, read in `solar_view_with_history`), `crates/oikos-os/tests/history_db.rs`

**Interfaces:**
- Produces:
  - Migration: `CREATE TABLE solar_forecast_days (date date PRIMARY KEY, estimate_kwh double precision NOT NULL, low_kwh double precision NOT NULL, high_kwh double precision NOT NULL, made_at timestamptz NOT NULL);` (plain table).
  - `pub struct SolarForecastRow { pub date: NaiveDate, pub estimate: f64, pub low: f64, pub high: f64, pub made_at: DateTime<Utc> }`
  - `impl Db { pub async fn insert_solar_forecast_day(&self, row: &SolarForecastRow) -> Result<bool, sqlx::Error> /* ON CONFLICT (date) DO NOTHING; true if inserted */; pub async fn solar_forecast_days(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<SolarForecastRow>, sqlx::Error> /* from ≤ date < to, ascending */ }`
  - `apply_solar_forecast`: when `hub.history()` is set, spawns the insert of today's totals (local date of now, `made_at = now`); an error is a `warn`.
  - `solar_view_with_history`: `history.forecasts` = rows for the 7 local dates before today; history off or a DB error → empty (DB error logged at `debug`).

- [ ] **Step 1: Write the failing tests** — in `history_db.rs` (skips without `TEST_DATABASE_URL`, like its neighbours): `solar_forecast_first_row_of_day_wins` (two inserts same date → first values kept, second returns false), `solar_forecast_days_range_is_half_open_and_sorted`.
- [ ] **Step 2: Run, expect FAIL** — `TEST_DATABASE_URL=… cargo test -p oikos-os --test history_db solar` (compile error first).
- [ ] **Step 3: Implement** migration, `history/solar.rs`, and the two hub hooks.
- [ ] **Step 4: Run, expect PASS** — the same command with the local Timescale from `deploy/timescaledb/` (if none is reachable, report the tests as skipped, not passed).
- [ ] **Step 5: Commit** — `feat(hub): keep each day's morning solar forecast`.

---

### Task 10: Docs

**Files:**
- Create: `docs/users/solar-forecast.md` (front matter like `weather.md`, `sidebar_position: 15`)
- Modify: `docs/reference/configuration.md`, `docs/reference/http-api.md`, `docs/reference/mcp-tools.md`, `docs/reference/mqtt-topics.md`

Content (from spec §11): what it does; the Solcast hobbyist account and rooftop site; `secrets/solcast.key`; `[solcast]` keys; **sharing an account** — set `daily_limit` to the account limit minus what other tools use (example: another tool makes 2 calls a day → `daily_limit = 8`, i.e. 6 refreshes); when refreshes happen (before sunrise, then spread over daylight, none at night); freshness and offline rules; the entity table (no energy class, why); one flow example ("if `solar_forecast_today` > 15, turn on the water heater at 11:00"); privacy (only the site ids and the key leave the LAN, only when enabled); credit "Forecasts by Solcast". Reference pages: `[solcast]` table, `GET /api/v1/solar` with the §6.2 JSON (plus `made_at` in `history.forecasts`), `get_solar`, `P/solar/…` topics and the `solar` reserved sub-tree. No real site id, address or machine name.

- [ ] **Step 1: Write the pages.**
- [ ] **Step 2: Verify** — `cd website && npm run build` passes (broken links fail the build); if the website toolchain is not installed, report it as not run.
- [ ] **Step 3: Commit** — `docs: Solcast solar forecast`.

---

### Task 11: Live check with the user's key (gated)

Run only with the user's explicit go-ahead in the session; it spends 1 call per site of their daily budget.

- [ ] **Step 1:** Ask the user to put the key in `secrets/solcast.key` and the site id(s) in a local, untracked `oikos.toml` `[solcast]` with `enabled = true`.
- [ ] **Step 2:** Run the hub against their broker (or `cargo run -p oikos-os -- run --config <local toml>`), wait for `solar: forecast applied` in the log, `curl -s localhost:<port>/api/v1/solar | jq '.today, .calls_today'`.
- [ ] **Step 3:** Compare the live response fields with the fixture shape; if they differ, save one real response as the fixture **with the site id replaced**, fix the parser, re-run Task 1–2 tests, commit `fix(solcast): match the live response`.
- [ ] **Step 4:** Report today's estimate and `calls_today` to the user.
