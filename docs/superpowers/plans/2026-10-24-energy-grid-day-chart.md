# Energy Grid Day Chart Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Hellowatts-style day chart on the web `/energy` page: half-hour bars of what crossed the grid meter, bought above zero split off-peak/peak, sold below, with off-peak windows as a dated hub setting.

**Architecture:** Pure maths in `oikos-core` (tariff windows and their history, slot attribution of counter increases, grid counter resolution). The hub resolves the grid counters from its snapshot, reads raw counter rows for one house-local day and returns `GridDay` on `GET /api/v1/history/grid_day` and the MCP tool `get_grid_day`. The web renders it as an SVG card under the live diagram and edits the windows in Settings › Home.

**Tech Stack:** Rust, Axum, sqlx/TimescaleDB, chrono / chrono-tz, rmcp, Leptos (wasm).

**Spec:** `docs/superpowers/specs/2026-10-24-energy-grid-day-chart-design.md`

## Global Constraints

- Generic: no device names, host names, IPs or this house's off-peak hours in code, tests' prose, docs or defaults. Tests may use any made-up windows (01:34–07:04 style odd minutes are fine as values).
- Only what crosses the grid meter. Bought = `meter_for(s)` of every `grid` sensor; sold = `reverse_for(s)` of every `grid` sensor plus `meter_for(x)` of every `grid_export` sensor; each counter counted once.
- Counters only (never power). Increase rule: `max(0, Δ)` between consecutive readings, carry = last reading at or before the cut; an increase lands in the slot and tariff of the reading that shows it.
- Slots: 30 min of real time from house-local midnight; 46 / 48 / 50 per day; today stops at the current half hour.
- kWh rounded to 3 decimals (`round3`). Totals = sum of slots, `off_peak + peak` = bought.
- Off-peak windows: `HH:MM` 24 h, local wall clock, `end < start` crosses midnight, `start == end` and overlaps refused; same windows every weekday.
- Tariff history: hub-written only; a change applies from the start of the house-local day it is saved; same-day saves keep the last; the oldest entry also covers earlier days; clearing is an entry with `windows: []`; a client's `off_peak_history` is ignored.
- "Today" comes from the hub reply (`today`), never the browser clock.
- Web copy: the Energy screen is English literals like the rest of `screens/energy.rs`; the Settings › Home additions are FR/EN through `HomeText` like the rest of that panel.
- Live diagram and meter cards unchanged.
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Before building, check `du -sh ~/dev-scratch/cargo-target`; over 5 GB run `cargo clean`.

## Review Focus

1. A meter that reports rarely or goes quiet for hours: its catch-up increase lands in the slot of the next reading, never negative, never lost (Task 2 test `quiet_meter_catch_up_lands_on_next_reading`).
2. A counter reset or replaced meter mid-day (value drops): no negative bar, totals still non-negative (Task 2 test `reset_never_counts_negative`).
3. The autumn DST day (25 h): 50 slots, the repeated 02:00–03:00 hour present twice, totals unchanged (Task 4 test `fall_back_day_has_50_slots`).
4. A house with a grid sensor but no export counter, or with two grid sensors (three-phase): no bars below zero / both summed once (Task 3 tests `grid_counters_sum_phases_once`, `grid_counters_without_export`).
5. Saving Settings › Home without touching the off-peak windows must not append a history entry (Task 3 test `unchanged_windows_keep_history`).

---

### Task 1: Off-peak windows and their history (core)

**Files:**
- Create: `crates/oikos-core/src/energy/tariff.rs`
- Modify: `crates/oikos-core/src/energy/mod.rs` (add `mod tariff; pub use tariff::*;`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))] pub struct OffPeakWindow { pub start: String, pub end: String }`
  - `#[derive(... same ...)] pub struct OffPeakEntry { pub since: String, pub windows: Vec<OffPeakWindow> }` — `since` is `"YYYY-MM-DD"` (a `String`: the workspace `chrono` has no `serde` feature, keep it that way); entries whose `since` does not parse are ignored by `windows_for`
  - `pub fn parse_hhmm(s: &str) -> Option<u16>` — minute of day, `"00:00"`..`"23:59"`, exactly two digits each side.
  - `pub fn validate_windows(ws: &[OffPeakWindow]) -> Result<(), String>` — messages start with `off_peak` (the web maps them to the field): `off_peak window N: bad time`, `off_peak window N: start equals end`, `off_peak windows N and M overlap` (N, M 1-based).
  - `pub fn is_off_peak(ws: &[OffPeakWindow], minute: u16) -> bool` — `[start, end)`, wrapping when `end < start`.
  - `pub fn windows_for(history: &[OffPeakEntry], date: NaiveDate) -> &[OffPeakWindow]` — latest entry with `since <= date`, else the oldest entry, else `&[]`. History is kept sorted by `since`.
  - `pub fn record_change(history: &[OffPeakEntry], next: &[OffPeakWindow], today: NaiveDate) -> Vec<OffPeakEntry>` — returns `history` unchanged when `next` equals the windows in force today; otherwise replaces an entry with `since == today` or appends `{since: today.format("%Y-%m-%d"), windows: next}`. An empty history with empty `next` stays empty.

- [ ] **Step 1: Write the failing tests** (in `tariff.rs` `#[cfg(test)]`)

```rust
#[test] fn parses_hhmm() {
    assert_eq!(parse_hhmm("01:34"), Some(94));
    assert_eq!(parse_hhmm("23:59"), Some(1439));
    for bad in ["24:00", "1:34", "01:60", "0134", ""] { assert_eq!(parse_hhmm(bad), None, "{bad}"); }
}
#[test] fn off_peak_wraps_midnight() {
    let ws = [w("22:00", "06:00")];
    assert!(is_off_peak(&ws, 23 * 60)); assert!(is_off_peak(&ws, 0)); assert!(is_off_peak(&ws, 359));
    assert!(!is_off_peak(&ws, 360)); assert!(!is_off_peak(&ws, 12 * 60));
}
#[test] fn off_peak_at_odd_minutes() {
    let ws = [w("01:34", "07:04"), w("12:05", "14:34")];
    assert!(!is_off_peak(&ws, 93)); assert!(is_off_peak(&ws, 94)); assert!(!is_off_peak(&ws, 424));
    assert!(is_off_peak(&ws, 12 * 60 + 5)); assert!(!is_off_peak(&ws, 14 * 60 + 34));
}
#[test] fn validation_messages() {
    assert_eq!(validate_windows(&[w("25:00", "06:00")]).unwrap_err(), "off_peak window 1: bad time");
    assert_eq!(validate_windows(&[w("06:00", "06:00")]).unwrap_err(), "off_peak window 1: start equals end");
    assert_eq!(validate_windows(&[w("22:00", "06:00"), w("05:00", "07:00")]).unwrap_err(), "off_peak windows 1 and 2 overlap");
    assert!(validate_windows(&[w("01:34", "07:04"), w("12:05", "14:34")]).is_ok());
    assert!(validate_windows(&[]).is_ok());
}
#[test] fn windows_for_picks_entry_in_force() {
    let h = vec![entry("2026-03-01", &[w("01:00", "07:00")]), entry("2026-10-24", &[w("02:00", "08:00")])];
    assert_eq!(windows_for(&h, d("2026-01-10")), &h[0].windows[..]); // oldest covers earlier days
    assert_eq!(windows_for(&h, d("2026-10-23")), &h[0].windows[..]);
    assert_eq!(windows_for(&h, d("2026-10-24")), &h[1].windows[..]);
    assert!(windows_for(&[], d("2026-10-24")).is_empty());
}
#[test] fn record_change_rules() {
    let a = vec![w("01:00", "07:00")]; let b = vec![w("02:00", "08:00")];
    let h1 = record_change(&[], &a, d("2026-10-20"));
    assert_eq!(h1, vec![entry("2026-10-20", &a)]);
    assert_eq!(record_change(&h1, &a, d("2026-10-24")), h1);           // unchanged: no entry
    let h2 = record_change(&h1, &b, d("2026-10-24"));
    assert_eq!(h2.len(), 2);
    let h3 = record_change(&h2, &a, d("2026-10-24"));                  // same day: last wins
    assert_eq!(h3, vec![entry("2026-10-20", &a), entry("2026-10-24", &a)]);
    let h4 = record_change(&h1, &[], d("2026-10-25"));                 // clearing is an entry
    assert_eq!(h4.last().unwrap().windows, vec![]);
    assert!(record_change(&[], &[], d("2026-10-25")).is_empty());
}
```
(`w`, `entry`, `d` are small test helpers in the test module.)

- [ ] **Step 2:** Run `cargo test -p oikos-core energy::tariff` — expect FAIL (unresolved items).
- [ ] **Step 3:** Implement the five functions and two types in `tariff.rs`. Overlap check: expand each window into minute ranges on a 1440-minute circle (a wrapping window is two ranges) and compare pairwise.
- [ ] **Step 4:** Run `cargo test -p oikos-core energy::tariff` — expect PASS.
- [ ] **Step 5:** Commit `feat(core): off-peak windows and their dated history`.

---

### Task 2: Slot attribution of counter increases (core)

**Files:**
- Create: `crates/oikos-core/src/history/grid_day.rs`
- Modify: `crates/oikos-core/src/history/mod.rs` (`pub mod grid_day;`)

**Interfaces:**
- Consumes: `Sample`, `Micros`, `round3` from `history::buckets`.
- Produces:
  - `pub const HALF_HOUR: Micros = 30 * 60 * 1_000_000;`
  - `#[derive(Clone, Debug, PartialEq)] pub struct SlotKwh { pub start: Micros, pub off_peak: f64, pub peak: f64, pub export: f64 }`
  - `pub fn slot_starts(day_start: Micros, day_end: Micros, until: Micros) -> Vec<Micros>` — every `day_start + k·HALF_HOUR` below `min(day_end, until)`; when `until <= day_start`, empty.
  - `pub fn increases(carry: Option<f64>, samples: &[Sample]) -> Vec<(Micros, f64)>` — one `(sample.at, max(0, v − prev))` per sample with a previous value (carry for the first), zero deltas dropped.
  - `pub fn fill_slots(slots: &[Micros], bought: &[(Micros, f64)], sold: &[(Micros, f64)], off_peak_at: impl Fn(Micros) -> bool) -> Vec<SlotKwh>` — each increase goes to the slot `s` with `s <= at < s + HALF_HOUR`; bought splits by `off_peak_at(at)`; increases outside every slot are dropped; values `round3`ed at the end. `bought`/`sold` may hold several counters' increases concatenated, in any order.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn slots_stop_at_until() {
    let s = slot_starts(0, 48 * HALF_HOUR, 3 * HALF_HOUR + 1);
    assert_eq!(s, vec![0, HALF_HOUR, 2 * HALF_HOUR, 3 * HALF_HOUR]);
    assert_eq!(slot_starts(0, 48 * HALF_HOUR, i64::MAX).len(), 48);
    assert_eq!(slot_starts(0, 50 * HALF_HOUR, i64::MAX).len(), 50);
}
#[test] fn reset_never_counts_negative() {
    let inc = increases(Some(100.0), &[s(1, 100.5), s(2, 3.0), s(3, 3.2)]);
    assert_eq!(inc.len(), 2);
    assert!((inc[0].1 - 0.5).abs() < 1e-9 && (inc[1].1 - 0.2).abs() < 1e-9);
}
#[test] fn quiet_meter_catch_up_lands_on_next_reading() {
    // last reading 00:10 at 10.0, silent until 05:40 at 12.0
    let inc = increases(Some(10.0), &[s(min(340), 12.0)]);
    let slots = slot_starts(0, 48 * HALF_HOUR, i64::MAX);
    let out = fill_slots(&slots, &inc, &[], |_| false);
    assert_eq!(out[11].peak, 2.0);                     // 05:30–06:00
    assert!(out[..11].iter().all(|x| x.peak == 0.0));
}
#[test] fn split_by_tariff_at_reading_time() {
    let slots = slot_starts(0, 48 * HALF_HOUR, i64::MAX);
    let bought = vec![(min(92), 0.1), (min(95), 0.2)]; // 01:32 peak, 01:35 off-peak
    let out = fill_slots(&slots, &bought, &[(min(93), 0.05)], |at| at >= min(94));
    assert_eq!((out[3].peak, out[3].off_peak, out[3].export), (0.1, 0.2, 0.05));
}
#[test] fn sums_several_counters() {
    let slots = slot_starts(0, 48 * HALF_HOUR, i64::MAX);
    let out = fill_slots(&slots, &[(min(1), 0.1), (min(2), 0.3)], &[], |_| true);
    assert_eq!(out[0].off_peak, 0.4);
}
```
(`s(at, num)` builds a `Sample`; `min(m)` is `m * 60_000_000`.)

- [ ] **Step 2:** Run `cargo test -p oikos-core history::grid_day` — expect FAIL.
- [ ] **Step 3:** Implement the three functions; slot lookup by `(at − slots[0]) / HALF_HOUR` with a bounds check.
- [ ] **Step 4:** Run `cargo test -p oikos-core history::grid_day` — expect PASS.
- [ ] **Step 5:** Commit `feat(core): half-hour slot attribution of counter increases`.

---

### Task 3: Settings document, grid counters and wire types

**Files:**
- Modify: `crates/oikos-core/src/energy/meters.rs` (add `GridCounters`, `EnergyMeters::grid_counters`)
- Create: `crates/oikos-api/src/energy.rs`; Modify `crates/oikos-api/src/lib.rs` (export it)
- Modify: `crates/oikos-web/src/energy.rs:37-55` (`energy_entity` re-exports / calls the moved function)
- Modify: `crates/oikos-api/src/home.rs` (two fields), `crates/oikos-api/src/history.rs` (`GridDay` types)
- Modify: `crates/oikos-os/src/home.rs` (`validate`, `set_home_settings`)
- Modify: `crates/oikos-os/src/mcp/tools.rs:46-49` (describe `off_peak` in `set_home_settings`)
- Fix every `HomeSettings { .. }` literal the compiler flags (add `..HomeSettings::default()` or the new fields).

**Interfaces:**
- Consumes: Task 1 `OffPeakWindow`, `OffPeakEntry`, `validate_windows`, `record_change`.
- Produces:
  - `#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct GridCounters { pub bought: Vec<String>, pub sold: Vec<String> }` and `impl EnergyMeters { pub fn grid_counters(all: &[EnergyEntity]) -> GridCounters }` — ids sorted and deduplicated.
  - `oikos_api::energy_entity(e: &EntityView) -> EnergyEntity` and `oikos_api::energy_entities<'a>(views: impl IntoIterator<Item = &'a EntityView>) -> Vec<EnergyEntity>` (skips ids starting `group_`), moved from the web; the web's `energy_entities(&BTreeMap)` becomes a one-line call.
  - `HomeSettings` gains `#[serde(default)] pub off_peak: Vec<OffPeakWindow>` and `#[serde(default)] pub off_peak_history: Vec<OffPeakEntry>`.
  - Wire types in `oikos-api/src/history.rs` (all `Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq`):
    - `pub struct GridSlot { pub start: i64, pub local: String, pub off_peak: f64, pub peak: f64, pub export: f64 }` — `local` is the house-local start `"HH:MM"` (the web labels slots with it, never with the browser zone)
    - `pub struct GridTotals { pub off_peak: f64, pub peak: f64, pub export: f64 }`
    - `pub struct GridDay { pub date: String, pub today: String, pub first_date: Option<String>, pub day_slots: usize, pub slots: Vec<GridSlot>, pub bands: Vec<(i64, i64)>, pub totals: GridTotals, pub has_export: bool, pub tariffs: bool }` — `day_slots` is the full day's slot count (46/48/50, also for a partial today) so the chart keeps a fixed x axis; `bands` are the off-peak intervals of that day as `[from, to)` unix seconds clipped to the day (empty without tariffs)
  - Hub: `validate` also runs `validate_windows(&settings.off_peak)`; `set_home_settings` sets `settings.off_peak_history = record_change(&current.off_peak_history, &settings.off_peak, today)` (client value discarded), `today` = house-local date of `clock::now_micros()` in `self.zone()`.

- [ ] **Step 1: Write the failing tests**
  - `meters.rs`: `grid_counters_sum_phases_once` — two `grid` power sensors on two devices, each with `energy_a`/`energy_produced_a` twins, plus a `grid_export` sensor whose meter is one of those produced counters → `bought` has 2 ids, `sold` has 2 ids (no duplicate). `grid_counters_without_export` — one grid sensor with only `energy_a` → `sold` empty.
  - `oikos-api/src/home.rs`: `off_peak_defaults_to_empty` — `{"eco_setpoint": 15.0}` decodes with empty `off_peak` and `off_peak_history`; update the existing default-serialisation assertion to include `"off_peak": [], "off_peak_history": []`.
  - `oikos-os/src/home.rs` tests: `validate_refuses_overlapping_windows` (message starts `off_peak windows 1 and 2 overlap`); `set_records_window_change` (history gets one entry dated today); `unchanged_windows_keep_history` (a second save with the same windows and a different `eco_setpoint` leaves history equal); `client_history_is_ignored` (a document carrying a forged `off_peak_history` is stored with the hub's history).
- [ ] **Step 2:** Run `cargo test -p oikos-core energy::meters && cargo test -p oikos-api && cargo test -p oikos-os home::` — expect FAIL.
- [ ] **Step 3:** Implement as specified; move `energy_entity` into `oikos-api/src/energy.rs` unchanged in behaviour.
- [ ] **Step 4:** Run the same commands plus `cargo test -p oikos-web` — expect PASS (web energy tests still green after the move).
- [ ] **Step 5:** Commit `feat: off-peak setting with hub-kept history, grid counter resolution`.

---

### Task 4: Hub day query, HTTP route and MCP tool

**Files:**
- Modify: `crates/oikos-os/src/history/db.rs` (two queries)
- Modify: `crates/oikos-os/src/history/mod.rs` (`HistoryError::NoGridMeter`, code `no_grid_meter`)
- Modify: `crates/oikos-os/src/history/query.rs` (`day_bounds`, `grid_day`)
- Modify: `crates/oikos-os/src/http/api.rs` (route + handler; `NoGridMeter` → 404)
- Modify: `crates/oikos-os/src/mcp/tools.rs`, `crates/oikos-os/src/mcp/params.rs` (tool `get_grid_day`)
- Modify: a hub method in `crates/oikos-os/src/hub.rs` or next to `home.rs`: `pub fn grid_counters(&self) -> GridCounters` (from `self.snapshot().entities` via `oikos_api::energy_entities`)
- Test: `crates/oikos-os/tests/history_db.rs`, `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `Db::last_before(&self, entity: &str, at: Micros) -> Result<Option<f64>, sqlx::Error>` — latest raw `num` with `time < at`.
  - `Db::first_time_of(&self, entities: &[String]) -> Result<Option<Micros>, sqlx::Error>` — `MIN(time)` over those ids.
  - `pub fn day_bounds(zone: Tz, date: NaiveDate) -> (Micros, Micros)` — local midnight of `date` and of `date + 1`, same DST handling as `day_starts`.
  - `pub async fn grid_day(h: &History, counters: &GridCounters, history: &[OffPeakEntry], date: Option<NaiveDate>, now: Micros) -> Result<GridDay, HistoryError>` — `NoGridMeter` when `counters.bought` is empty; `BadRequest` when `date` is after today; `today`/`date`/`first_date` as `YYYY-MM-DD` in the house zone; carry = `last_before(day_start)` (raw, not hourly) per counter, samples `[day_start, min(day_end, now))`; `off_peak_at(at)` = `is_off_peak(windows_for(history, date), local minute of at)`; `tariffs = !windows.is_empty()`; `has_export = !counters.sold.is_empty()`; `local` from the slot start in the zone; `bands` by walking the day minute by minute and joining runs where `off_peak_at` holds (≤ 1 500 steps).
  - `GET /api/v1/history/grid_day?date=YYYY-MM-DD` (date optional; unparsable → 400 `bad_request`).
  - MCP `get_grid_day` with `GetGridDayParams { date: Option<String> }`, read-only, description: "What crossed the grid meter on one house-local day, per half hour: kWh bought off-peak and peak, kWh sold. `date` is YYYY-MM-DD, today when absent."

- [ ] **Step 1: Write the failing tests**
  - `query.rs` unit: `fall_back_day_has_50_slots` — `day_bounds(Paris, 2026-10-25)` spans 25 h and `slot_starts` over it gives 50; `day_bounds(Paris, 2026-03-29)` gives 46.
  - `history_db.rs` (skips without `TEST_DATABASE_URL`):
    - `grid_day_totals_match_counter_increase` — bought counter rows 100.0 (yesterday 23:50), then 100.4 at 01:40, 101.0 at 08:00, 101.5 at 13:00 local; sold counter 5.0 (yesterday 23:50) → 5.6 at 13:10; windows `01:34–07:04`, `12:05–14:34` from an entry dated before → `totals.off_peak == 0.9`, `totals.peak == 0.6`, `totals.export == 0.6`, 48 slots, slot for 01:30 has `off_peak 0.4`.
    - `grid_day_today_is_partial` — `now` at 10:15 local → 21 slots, `day_slots == 48`, `date == today`.
    - `grid_day_bands_follow_windows` — windows `01:34–07:04`, `12:05–14:34` → two bands, the first starting at local 01:34 and ending at 07:04.
    - `grid_day_uses_windows_of_that_day` — history `[since D-1: A, since D: B]`, query D-1 splits with A, D with B.
    - `grid_day_future_date_is_bad_request` and `grid_day_without_counters_is_no_grid_meter`.
  - `mcp.rs`: `get_grid_day_is_listed_read_only` following the file's existing tool-listing test pattern.
- [ ] **Step 2:** Run `cargo test -p oikos-os grid_day && cargo test -p oikos-os --test mcp` — expect FAIL. Run `TEST_DATABASE_URL=… cargo test -p oikos-os --test history_db grid_day` against the local Timescale if available; report if it is not.
- [ ] **Step 3:** Implement; reuse `round3`; one `samples` call per counter for the day.
- [ ] **Step 4:** Re-run the Step 2 commands — expect PASS (DB tests PASS with the database, "skipping" without).
- [ ] **Step 5:** Commit `feat(hub): grid day query on /history/grid_day and MCP get_grid_day`.

---

### Task 5: Web day-chart presentation (pure)

**Files:**
- Create: `crates/oikos-web/src/energy_day.rs`; Modify `crates/oikos-web/src/lib.rs` (`pub mod energy_day;`)
- Modify: `crates/oikos-web/src/api/history.rs` (`grid_day_url`, `fetch_grid_day`; map a 404 body `no_grid_meter` to a new `HistoryApiError::NoGridMeter`)
- Test: `crates/oikos-web/tests/energy_day.rs`

**Interfaces:**
- Consumes: `GridDay`, `GridSlot` from Task 3.
- Produces:
  - `pub(crate) fn grid_day_url(date: Option<&str>) -> String` → `/api/v1/history/grid_day` or `…?date=2026-10-23`; `pub async fn fetch_grid_day(date: Option<&str>) -> Result<GridDay, HistoryApiError>`.
  - `pub fn day_label(date: &str, today: &str) -> String` — `"Today"`, `"Yesterday"`, else `"Fri 23 Oct"` style (`%a %-d %b`).
  - `pub fn shift_day(date: &str, by: i64, first: Option<&str>, today: &str) -> Option<String>` — `None` past today or before `first`.
  - `pub struct Scale { pub up: f64, pub down: f64 }` and `pub fn scale(day: &GridDay) -> Scale` — largest bought and largest export slot, each at least 0.5 kWh, rounded up to the next 0.5.
  - `pub fn tick_hours(slots: usize, narrow: bool) -> Vec<usize>` — slot indexes of hour ticks every 3 h (6 h when `narrow`).
  - `pub fn slot_range(local: &str) -> String` — `"14:30"` → `"14:30–15:00"`, `"23:30"` → `"23:30–00:00"`.
  - `pub fn x_of(t: i64, day: &GridDay) -> f64` — fraction 0..1 of the day axis for a unix second: `(t − slots[0].start) / (day_slots · 1800)`, clamped; used for bands. Returns 0 when `slots` is empty.
  - `pub fn tooltip(slot: &GridSlot, tariffs: bool) -> String` — uses `slot_range(&slot.local)`, e.g. `"14:30–15:00 · off-peak 0.21 · peak 0.10 · sold 0.05 kWh"`, omitting zero parts, `"bought"` instead of off-peak/peak when `!tariffs`.
  - `pub fn totals_text(day: &GridDay) -> (String, Option<String>, Option<String>)` — `("Bought 17.1 kWh", Some("off-peak 7.2 · peak 9.9"), Some("Sold 3.1 kWh"))`, split `None` without tariffs, sold `None` without export.
  - `pub fn aria_summary(day: &GridDay) -> String` — one sentence with bought, split and sold.

- [ ] **Step 1: Write the failing tests** in `tests/energy_day.rs`: `day_labels` (today / yesterday / `Fri 23 Oct` for 2026-10-23), `shift_day_bounds` (› from today is `None`, ‹ from `first` is `None`), `scale_rounds_up` (max bought 1.73 → up 2.0, no export → down 0.5), `ticks_every_3h_or_6h` (48 slots → `[0,6,12,…,42]`; narrow → `[0,12,24,36]`), `tooltip_omits_zero_parts`, `slot_range_wraps_midnight`, `x_of_spans_the_full_day`, `totals_without_tariffs_or_export`, `url_with_and_without_date`.
- [ ] **Step 2:** Run `cargo test -p oikos-web --test energy_day` — expect FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p oikos-web --test energy_day` — expect PASS.
- [ ] **Step 5:** Commit `feat(web): grid day chart presentation and client`.

---

### Task 6: Web day-chart card on /energy

**Files:**
- Create: `crates/oikos-web/src/screens/energy_day.rs` (`GridDayCard`); Modify `crates/oikos-web/src/screens/mod.rs`
- Modify: `crates/oikos-web/src/screens/energy.rs` (place `<GridDayCard />` between the diagram block and `energy-cards`)
- Modify: `crates/oikos-web/styles/main.css` (`.energy-day*` rules, theme tokens for the three bar colours and the off-peak band in both themes)

**Interfaces:**
- Consumes: Task 5 functions; `next_fire_delay` / visibility pattern from `crates/oikos-web/src/energy.rs`.
- Produces: `#[component] pub fn GridDayCard() -> impl IntoView`.

Behaviour to build (all from the spec): header `‹ label ›` with › disabled on today and ‹ disabled at `first_date`; totals row; SVG bars (bought stacked off-peak under peak, export below zero), dashed gridlines with kWh labels, zero line, off-peak bands shaded from `GridDay.bands` via `x_of`; the x axis always spans `day_slots` (a partial today leaves the rest empty); hour tick labels from the slots' `local`; legend entries that apply; tooltip on hover and on tap, bars focusable (`tabindex=0`, `aria-label` = tooltip text); `aria_summary` on the SVG; empty states (`Unavailable` → "History is off" with a link to the history docs page; `NoGridMeter` → "Give your grid meter the Grid role on its device page" linking `/devices`; no slots with data → "No data for this day"). Today refetches every 60 000 ms while visible and on `visibilitychange`; a past day fetches once per change of day. The day is component state, reset to today on mount.

- [ ] **Step 1:** `cargo build -p oikos-web --target wasm32-unknown-unknown` (or `trunk build` per `crates/oikos-web/Trunk.toml`) — expect success after implementing.
- [ ] **Step 2:** Run the hub locally against the test database or the live hub's API, open `/energy`, check: today shows partial bars and refreshes; ‹ › move days; phone width (375 px) has no horizontal scroll; Night Glass and Ivoire both readable. Screenshot via headless Chrome over CDP (window ≥ 500 px; see memory note on wasm screenshots).
- [ ] **Step 3:** `cargo test -p oikos-web` — expect PASS.
- [ ] **Step 4:** Commit `feat(web): grid day chart card on the Energy page`.

---

### Task 7: Settings › Home off-peak editor and docs

**Files:**
- Modify: `crates/oikos-web/src/screens/home_settings_panel.rs` (`Draft.off_peak: Vec<(String, String)>`, `Field::OffPeak`, editor rows with add / remove, `draft_to_settings`, `merge`, `field_of_message` maps `"off_peak"`, `message_key` maps the three messages)
- Modify: `crates/oikos-web/src/i18n.rs` (`HomeText::OffPeakHours`, `OffPeakHint`, `AddWindow`, `RemoveWindow`, `BadTime`, `StartEqualsEnd`, `WindowsOverlap`, FR/EN)
- Modify: `crates/oikos-web/src/screens/settings.rs:204` (description `"Eco setpoint, house mode, indoor temperature, off-peak hours"`)
- Modify: `docs/users/web-ui.md` (Energy page section: the day chart), `docs/users/history.md` (grid day), `docs/reference/http-api.md` (`/history/grid_day`, `home/settings` new fields), `docs/reference/mcp-tools.md` (`get_grid_day`, `set_home_settings` off_peak)

**Interfaces:**
- Consumes: Task 3 `HomeSettings.off_peak`; hub validation messages from Task 1.

- [ ] **Step 1: Write the failing tests** (panel's test module): `draft_round_trips_off_peak` (`[("01:34","07:04")]` → `HomeSettings.off_peak == [OffPeakWindow{start:"01:34",end:"07:04"}]` and back via `Draft::from`); `merge_keeps_edited_windows`; `off_peak_messages_map_to_field` (`"off_peak windows 1 and 2 overlap"` → `Field::OffPeak`, `HomeText::WindowsOverlap`). The draft never carries `off_peak_history`.
- [ ] **Step 2:** Run `cargo test -p oikos-web home_settings` — expect FAIL.
- [ ] **Step 3:** Implement the editor (two `<input type="time">` per row, Add / Remove buttons, same field styles as the panel) and the docs.
- [ ] **Step 4:** Run `cargo test -p oikos-web` and `cargo clippy --workspace --all-targets -- -D warnings` — expect PASS.
- [ ] **Step 5:** Commit `feat(web): off-peak hours in Settings › Home; docs`.
