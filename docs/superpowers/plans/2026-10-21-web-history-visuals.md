# Web history visuals Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a full-screen `/history/:id` route to the web UI that charts every recorded entity over 24h / 7d / 30d / 1y, plus a kWh-today tile under each Energy node and a "since X" row under each Security sensor — all served by the hub's existing `/api/v1/history/*` routes.

**Architecture:** A new typed `api::history` module wraps the four hub routes with `thiserror` variants for 400 / 404 / 503 / other. A new `components::history_chart` module draws the chart as hand-written SVG — step line for the series flavor, daily bars for the counter flavor — reusing the Oikos glass aesthetic. A new `screens::history_detail` route composes range chips + chart; the entity sheet, Energy screen and Security screen gain small affordances that read the same routes. Pure chart math lives in native-testable helpers; Leptos components are the thin shell.

**Tech Stack:** Rust 2024, Leptos 0.7, `leptos_router`, `gloo-net`, `gloo-timers`, `time` crate, hand-written SVG. No new workspace dependencies.

**Spec:** `docs/superpowers/specs/2026-10-21-web-history-design.md`

## Global Constraints

- Attribution: never write the user's real name. Commit as `Gekkotron` using `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, with every commit message ending `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`.
- Do not push without the user's say-so.
- Do not stage `oikos.toml` or edit it; it is git-ignored and holds secrets.
- Range values on the wire are exactly `24h`, `7d`, `30d`, `1y` (`HistoryRange` in `oikos-api`). Daily windows are exactly `7`, `30`, `365`. Counter kind on the wire is `power` or `counter`.
- The chart renders as a **step line** (held-until-next-row), never linear interpolation — this is a deliberate choice tied to how the hub stores values.
- Error codes to handle: `400 bad_request`, `404 not_recorded`, `503 history_unavailable`. All three are drawn, not thrown.
- Local time rendering uses the house timezone (`Europe/Paris` for this user). No raw unix timestamps in UI copy.
- `oikos-web` builds as a wasm32 binary; code may be `#[cfg(target_arch = "wasm32")]`-gated when it touches the browser. Pure math/formatting lives in natively-testable modules.
- Narrow viewports (≥ 320 px) must not horizontally scroll.

## Review Focus

Five input classes the spec implies but no single task tests on its own. Each one has a test added to the task that owns the code (noted inline below).

1. **Entity with no numeric data** (text, alarm, vacuum): opening `/history/entity.x` returns 404 `not_recorded` from the hub — the page must draw "No data recorded for this entity yet", not crash or spin. Owned by **Task 5** (`HistoryDetailScreen` state tree).
2. **Counter with no paired power sensor on the 24h view**: `counter_power` returns `None` — the page must fall back to a single-day bar from `/daily?days=7&kind=counter` sliced to today, not render an empty chart. Owned by **Task 5**.
3. **Energy screen tab visibility toggle**: hide the tab, 10 minutes pass, show it again — the 5-min tick must not have fired while hidden, and must refresh once on `visibilitychange`. Owned by **Task 7**.
4. **Security screen with more than 50 sensors**: the `/last_change` route refuses more than 50 ids per call; the screen must chunk into batches of 50 and merge the results in input order. Owned by **Task 8**.
5. **Range chip rapidly switched while a previous fetch is in flight**: the previous response must not clobber the newer range's result when it arrives late. Owned by **Task 4** (`HistoryChart` fetch effect).

---

## Task 1: `counter_power` reverse lookup in `oikos_core::energy::meters`

**Files:**
- Modify: `crates/oikos-core/src/energy/meters.rs` — add one associated function on `EnergyMeters`.
- Test: same file (existing `#[cfg(test)] mod tests`).

**Interfaces:**
- Consumes: existing `EnergyEntity`, `EnergyMeters::meter_for`, `is_counter`, `PowerUnits::factor`.
- Produces: `pub fn EnergyMeters::counter_power<'a>(counter: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>` — the power sensor paired with `counter`. Scans every `EnergyEntity` where `PowerUnits::factor(unit).is_some()` and `EnergyMeters::meter_for(power, all).map(|c| &c.id) == Some(&counter.id)`. First-registered wins on ties (iteration order of `all`).

- [ ] **Step 1: Write the failing test**

Add to the existing test module in `meters.rs`:

```rust
#[test]
fn counter_power_pairs_back() {
    let all = house();
    // energy_a on the grid clamp
    let counter = by_id(&all, "0xa4c13898428fae3a_energy_a_zigbee2mqtt");
    assert_eq!(
        id(EnergyMeters::counter_power(counter, &all)),
        Some("0xa4c13898428fae3a_power_a_zigbee2mqtt")
    );
}

#[test]
fn counter_power_is_none_when_no_power_pairs() {
    let all = vec![
        EnergyEntity { unit: Some("kWh".into()), ..e("c") },
    ];
    assert_eq!(EnergyMeters::counter_power(&all[0], &all), None);
}

#[test]
fn counter_power_first_registered_wins_on_ties() {
    // Two power sensors on the same device, each auto-pairing with the one counter.
    let mk = |i: &str, unit: &str, dev: &str| EnergyEntity {
        device_id: Some(dev.into()),
        unit: Some(unit.into()),
        ..e(i)
    };
    let all = vec![
        mk("p1", "W", "d"),
        mk("p2", "W", "d"),
        mk("c",  "kWh", "d"),
    ];
    // Neither power pairs to the counter under current auto rules (two powers, one counter), so
    // the paired-sum must be None here — this test guards us against accidentally pairing twice.
    assert_eq!(EnergyMeters::counter_power(&all[2], &all), None);

    // Now a labelled match for p1 only
    let mut all = all;
    all[0].label = Some("Power".into());
    all[2].label = Some("Energy".into());
    assert_eq!(id(EnergyMeters::counter_power(&all[2], &all)), Some("p1"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p oikos-core --lib energy::meters::tests::counter_power -- --nocapture`
Expected: FAIL with "no method named `counter_power` found".

- [ ] **Step 3: Implement `EnergyMeters::counter_power`**

Add to the `impl EnergyMeters` block:

```rust
pub fn counter_power<'a>(
    counter: &EnergyEntity,
    all: &'a [EnergyEntity],
) -> Option<&'a EnergyEntity> {
    all.iter().find(|p| {
        PowerUnits::factor(p.unit.as_deref()).is_some()
            && Self::meter_for(p, all).map(|c| c.id.as_str()) == Some(counter.id.as_str())
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p oikos-core --lib energy::meters::tests -- --nocapture`
Expected: PASS, no regressions on existing tests.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-core/src/energy/meters.rs
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(energy): counter_power reverse lookup for history pairing

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `api::history` module — typed wrappers over the four hub routes

**Files:**
- Create: `crates/oikos-web/src/api/mod.rs` — new module root that re-exports the current `api.rs` surface and declares `pub mod history`.
- Rename: `crates/oikos-web/src/api.rs` → `crates/oikos-web/src/api/core.rs` (keep contents intact, re-exported from `mod.rs`).
- Create: `crates/oikos-web/src/api/history.rs` — the four typed wrappers.
- Modify: `crates/oikos-web/src/lib.rs` — `mod api;` already covers it, no change if the module structure is kept flat; otherwise adjust.

**Interfaces:**
- Consumes: `encode_id` (now `crate::api::encode_id`), `oikos_api::{HistoryRange, HistorySeries, HistoryDaily, HistoryToday, LastChange, EnergyKind}`.
- Produces:
  - `#[derive(Debug, thiserror::Error, Clone)] pub enum HistoryApiError { #[error("bad request: {0}")] BadRequest(String), #[error("not recorded")] NotRecorded, #[error("history unavailable")] Unavailable, #[error("{0}")] Other(String) }`
  - `pub async fn fetch_series(entity: &str, range: HistoryRange) -> Result<HistorySeries, HistoryApiError>`
  - `pub async fn fetch_daily(entity: &str, days: u16, kind: EnergyKind) -> Result<HistoryDaily, HistoryApiError>`
  - `pub async fn fetch_today(entity: &str, kind: EnergyKind) -> Result<HistoryToday, HistoryApiError>`
  - `pub async fn fetch_last_change(ids: &[String]) -> Result<LastChange, HistoryApiError>` — chunks internally into 50-id requests and concatenates the `items` in input order.
- URL builders are pure functions, exposed `pub(crate)` for testing:
  - `fn series_url(entity: &str, range: HistoryRange) -> String`
  - `fn daily_url(entity: &str, days: u16, kind: EnergyKind) -> String`
  - `fn today_url(entity: &str, kind: EnergyKind) -> String`
  - `fn last_change_url(ids: &[&str]) -> String`

- [ ] **Step 1: Write failing tests for URL builders and error classification**

Add to `crates/oikos-web/src/api/history.rs` (file does not exist yet; this is the test module that will live at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::{EnergyKind, HistoryRange};

    #[test]
    fn series_url_uses_wire_range_names_and_encodes_id() {
        assert_eq!(
            series_url("sensor.living/room temp", HistoryRange::Day),
            "/api/v1/history/series?entity=sensor.living%2Froom%20temp&range=24h"
        );
        assert_eq!(
            series_url("x", HistoryRange::Year).ends_with("&range=1y"),
            true
        );
    }

    #[test]
    fn daily_url_uses_days_and_kind() {
        assert_eq!(
            daily_url("counter.a", 30, EnergyKind::Counter),
            "/api/v1/history/daily?entity=counter.a&days=30&kind=counter"
        );
    }

    #[test]
    fn today_url_uses_kind() {
        assert_eq!(
            today_url("p", EnergyKind::Power),
            "/api/v1/history/today?entity=p&kind=power"
        );
    }

    #[test]
    fn last_change_url_repeats_entity_param_in_order() {
        let ids = ["a", "b/c", "d"];
        assert_eq!(
            last_change_url(&ids),
            "/api/v1/history/last_change?entity=a&entity=b%2Fc&entity=d"
        );
    }

    #[test]
    fn classify_maps_statuses() {
        assert!(matches!(classify_status(400, "not a 24h"), HistoryApiError::BadRequest(_)));
        assert!(matches!(classify_status(404, ""), HistoryApiError::NotRecorded));
        assert!(matches!(classify_status(503, ""), HistoryApiError::Unavailable));
        assert!(matches!(classify_status(500, "oops"), HistoryApiError::Other(_)));
    }
}
```

Also add a native test in the same module for the chunking of `fetch_last_change`'s URL — refactor the chunking out so it is pure:

```rust
#[test]
fn chunk_ids_batches_at_50() {
    let ids: Vec<String> = (0..103).map(|i| format!("e{i}")).collect();
    let chunks = chunk_ids(&ids, 50);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 50);
    assert_eq!(chunks[1].len(), 50);
    assert_eq!(chunks[2].len(), 3);
    assert_eq!(chunks[2][0], "e100");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p oikos-web --lib api::history::tests -- --nocapture`
Expected: FAIL with "cannot find module `history` in `api`" (or "no such function").

- [ ] **Step 3: Create the module structure and implement the wrappers**

  1. Move the current `crates/oikos-web/src/api.rs` to `crates/oikos-web/src/api/core.rs` (contents unchanged).
  2. Create `crates/oikos-web/src/api/mod.rs`:
     ```rust
     mod core;
     pub mod history;
     pub use core::*;
     ```
  3. Create `crates/oikos-web/src/api/history.rs` with:
     - `HistoryApiError` enum (fields as above).
     - `pub(crate) fn classify_status(status: u16, body: &str) -> HistoryApiError` — 400 → `BadRequest(body-or-"bad_request")`, 404 → `NotRecorded`, 503 → `Unavailable`, else → `Other(format!("HTTP {status}"))`.
     - `fn series_url(entity: &str, range: HistoryRange) -> String` — uses `super::encode_id` and the exact serde names of `HistoryRange` (match on the enum, output `24h`/`7d`/`30d`/`1y`).
     - `fn daily_url(entity: &str, days: u16, kind: EnergyKind) -> String` — `kind` serialises as `power` or `counter`.
     - `fn today_url(entity: &str, kind: EnergyKind) -> String`.
     - `fn last_change_url(ids: &[&str]) -> String` — joins encoded `entity=<id>` with `&`.
     - `fn chunk_ids(ids: &[String], max: usize) -> Vec<Vec<&str>>` — groups input in order.
     - `pub async fn fetch_series/fetch_daily/fetch_today/fetch_last_change` — build URL, `Request::get(&url).send().await`, read status; on 2xx, parse JSON (`gloo-net`); on error, map through `classify_status`. `fetch_last_change` loops over `chunk_ids(50)` and concatenates `items` in order.

- [ ] **Step 4: Run tests to verify they pass, plus `cargo check -p oikos-web --target wasm32-unknown-unknown`**

Run: `cargo test -p oikos-web --lib api::history::tests -- --nocapture`
Expected: PASS.
Then: `cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/api/ crates/oikos-web/src/lib.rs
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): typed history API wrappers over /api/v1/history/*

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Chart math and formatting — pure helpers

**Files:**
- Create: `crates/oikos-web/src/components/history_chart/geom.rs` — SVG viewBox math (auto-scale, step-line path builder, daily-bar layout).
- Create: `crates/oikos-web/src/components/history_chart/format.rs` — axis tick labels, tooltip time formatting (per range), legend copy, state-axis labels (`off / on`, `0 / 100`).
- Create: `crates/oikos-web/src/components/history_chart/mod.rs` — module root re-exporting the above (component itself lands in Task 4).
- Modify: `crates/oikos-web/src/components/mod.rs` — `pub mod history_chart;`

**Interfaces:**
- Consumes: `oikos_api::{HistoryRange, HistorySeries, HistoryDaily}`, `time::OffsetDateTime`, `time_tz::Tz`.
- Produces (all pure, native-testable):
  - `#[derive(Clone, Copy)] pub struct YScale { pub min: f64, pub max: f64 }`
  - `pub fn y_scale(points: &[(i64, f64, f64, f64)]) -> YScale` — min/max of mean columns with 10% headroom; falls back to `0..1` on empty.
  - `pub struct StepPath { pub d: String, pub band_d: String }`
  - `pub fn step_path(points: &[(i64, f64, f64, f64)], y_scale: YScale, view_w: f64, view_h: f64) -> StepPath` — the `d` attribute for both the mean step line and the min/max shaded band. Empty input yields empty paths.
  - `pub struct Bar { pub x: f64, pub y: f64, pub w: f64, pub h: f64, pub t: i64, pub kwh: f64 }`
  - `pub fn daily_bars(days: &[(i64, f64)], view_w: f64, view_h: f64) -> Vec<Bar>` — one bar per day, equal widths, auto-scaled to the max.
  - `pub fn tick_labels(range: HistoryRange, now: i64) -> Vec<(f64, String)>` — x positions (0..1) and the labels for the range, in the house zone.
  - `pub fn tooltip_time(range: HistoryRange, t_secs: i64, now_secs: i64) -> String` — relative on 24h ("14:20"), absolute from 7d up ("Tue 06 Oct 14:20").
  - `pub fn state_axis_labels(unit: Option<&str>, kind: &str) -> Option<(String, String)>` — `Some(("off", "on"))` for OnOff/binary sensors and lights (0/1), `Some(("0", "100"))` for covers, `None` for numeric sensors (fall back to numeric Y labels).

- [ ] **Step 1: Write failing tests**

Create `crates/oikos-web/src/components/history_chart/geom.rs` with just the test module and the signatures (no bodies yet), and `format.rs` likewise. Tests:

```rust
// geom.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_scale_adds_ten_percent_headroom() {
        let p = vec![(0, 20.0, 20.0, 20.0), (1, 30.0, 30.0, 30.0)];
        let s = y_scale(&p);
        assert!((s.min - 19.0).abs() < 1e-6);
        assert!((s.max - 31.0).abs() < 1e-6);
    }

    #[test]
    fn y_scale_falls_back_to_unit_range_on_empty() {
        let s = y_scale(&[]);
        assert_eq!((s.min, s.max), (0.0, 1.0));
    }

    #[test]
    fn step_path_produces_horizontal_then_vertical_segments() {
        let p = vec![(0, 0.0, 0.0, 0.0), (10, 1.0, 1.0, 1.0)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 100.0, 50.0);
        // y=50 at min, y=0 at max; horizontal to x=100 at y=50, then V 0, then H 100
        assert!(out.d.contains("M0,50"));
        assert!(out.d.contains("H") && out.d.contains("V"));
    }

    #[test]
    fn step_path_empty_input_is_empty() {
        let out = step_path(&[], YScale { min: 0.0, max: 1.0 }, 100.0, 50.0);
        assert_eq!(out.d, "");
        assert_eq!(out.band_d, "");
    }

    #[test]
    fn daily_bars_lay_out_evenly_and_scale_to_max() {
        let days = vec![(0, 2.0), (86_400, 4.0), (172_800, 1.0)];
        let bars = daily_bars(&days, 300.0, 100.0);
        assert_eq!(bars.len(), 3);
        let w_each = 100.0; // 300 / 3
        assert!((bars[0].x - 0.0).abs() < 1e-6);
        assert!((bars[1].x - 100.0).abs() < 1e-6);
        // max value is 4.0 → full height (bar height = 100 * kwh / 4)
        assert!((bars[1].h - 100.0).abs() < 1e-6);
        assert!((bars[0].h - 50.0).abs() < 1e-6);
        assert!((bars[0].w - w_each * 0.8).abs() < w_each * 0.21); // some gap allowance
    }
}
```

```rust
// format.rs
#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::HistoryRange;

    #[test]
    fn tick_labels_for_24h_uses_6h_steps() {
        // noon on 2026-10-21 Europe/Paris
        let labels: Vec<String> = tick_labels(HistoryRange::Day, 1_792_497_600)
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert!(labels.iter().any(|l| l == "00:00"));
        assert!(labels.iter().any(|l| l == "06:00"));
        assert!(labels.iter().any(|l| l == "12:00"));
        assert!(labels.iter().any(|l| l == "18:00"));
        assert_eq!(labels.last().unwrap(), "now");
    }

    #[test]
    fn tooltip_time_is_relative_on_24h_absolute_from_7d() {
        let now = 1_792_497_600;
        let t = now - 3_600;
        assert_eq!(tooltip_time(HistoryRange::Day, t, now), "11:00");
        assert!(tooltip_time(HistoryRange::Week, t, now).contains("11:00"));
        assert!(tooltip_time(HistoryRange::Week, t, now).len() > 5);
    }

    #[test]
    fn state_axis_labels_pair_each_kind() {
        assert_eq!(state_axis_labels(None, "OnOff"), Some(("off".into(), "on".into())));
        assert_eq!(state_axis_labels(None, "Light"), Some(("off".into(), "on".into())));
        assert_eq!(state_axis_labels(None, "BinarySensor"), Some(("off".into(), "on".into())));
        assert_eq!(state_axis_labels(None, "Cover"), Some(("0".into(), "100".into())));
        assert_eq!(state_axis_labels(Some("°C"), "Sensor"), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p oikos-web --lib components::history_chart -- --nocapture`
Expected: FAIL (functions not implemented).

- [ ] **Step 3: Implement the pure helpers**

- `y_scale`: linear scan for min/max of `mean` (column index 1); on empty, `(0.0, 1.0)`; else expand by 10% of range each side.
- `step_path`: emit `M {x0},{y0}` then alternating `H {xi}` / `V {yj}` as the step line; `band_d` is the min/max polygon per point (two polylines merged).
- `daily_bars`: slot width = `view_w / days.len()`, bar width ≈ 80% of slot, max for height from max kWh; `t`/`kwh` preserved for tooltip.
- `tick_labels`: use `time` + `time_tz` to format in `Europe/Paris` from `now - span`; produce 5 positions for 24h (`00:00 06:00 12:00 18:00 now`), 7 (day names) for 7d, 4 for 30d (`01 08 15 22`), 12 (months) for 1y.
- `tooltip_time`: 24h → `HH:MM`; 7d/30d → `Day DD Mon HH:MM`; 1y → `Mon YYYY`.
- `state_axis_labels`: match on the `kind` string value — `"OnOff" | "Light" | "BinarySensor"` → `("off", "on")`, `"Cover"` → `("0", "100")`, else `None`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p oikos-web --lib components::history_chart -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/components/history_chart/ crates/oikos-web/src/components/mod.rs
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): pure chart geometry and tick formatters for history

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `HistoryChart` Leptos component

**Files:**
- Create: `crates/oikos-web/src/components/history_chart/view.rs` — the component.
- Modify: `crates/oikos-web/src/components/history_chart/mod.rs` — `pub use view::HistoryChart;`

**Interfaces:**
- Consumes: `api::history::{fetch_series, fetch_daily, HistoryApiError}`, `geom::*`, `format::*`, `oikos_api::{HistoryRange, EnergyKind}`.
- Produces:
  - `#[derive(Clone, Copy, PartialEq)] pub enum Flavor { Series, Counter }`
  - `#[component] pub fn HistoryChart(entity_id: Signal<String>, range: Signal<HistoryRange>, flavor: Signal<Flavor>, kind: Signal<String>, unit: Signal<Option<String>>, power_pair: Signal<Option<String>>) -> impl IntoView` — `power_pair` is the entity id of a counter's paired power sensor (used only on `Flavor::Counter` + `HistoryRange::Day`); `None` means "fall back to a single-day bar".
  - `pub trait HistoryClient` test seam with default impl that calls the module in Task 2; tests inject their own via `provide_context`.
- Behavior:
  - On any input signal change, kicks off the right fetch (`fetch_series` for Series flavor; `fetch_daily` for Counter flavor on 7d/30d/1y; `fetch_series` on paired power sensor for Counter 24h, else `fetch_daily(entity, 1, Counter)` single-bar fallback).
  - Guards against stale responses with a monotonic request counter (`u32`); results drop silently when the counter does not match the request's.
  - Renders four states: `Loading` → shimmer rect; `Err(NotRecorded)` → empty copy; `Err(Unavailable)` → unavailable copy; `Err(Other)` → "Could not load history" + `Retry` button that bumps the request counter; `Ok(data)` → SVG via `geom` + axis labels via `format` + hover tooltip.
  - Hover/tap: a Leptos `Signal<Option<(f64, f64)>>` for the cursor position; the component finds the nearest point in native coords and shows the tooltip pill.

- [ ] **Step 1: Write the failing test**

Add a wasm-bindgen test for the stale-response guard (uses `wasm_bindgen_test`):

```rust
// tests/history_chart_stale.rs or an inline #[wasm_bindgen_test] in view.rs
#[wasm_bindgen_test::wasm_bindgen_test]
async fn stale_fetch_does_not_overwrite_latest() {
    // Set up a mock history client that resolves two requests out of order.
    // Assert the second-issued request's result is the one in the signal.
}
```

Full test body is written to run against a `history_client` trait seam injected into the chart via context — introduce `pub trait HistoryClient { fn series(...); fn daily(...); }` in `view.rs` with a `provide_context`-based default (production path) and a test override. The test path uses two channels; stale resolution ordering is deterministic.

- [ ] **Step 2: Run tests to verify they fail**

Run: `wasm-pack test --headless --chrome crates/oikos-web -- components::history_chart::view::stale_fetch_does_not_overwrite_latest`
(or the project's existing `tools/wasm-test.sh` equivalent; check `Cargo.toml` dev-dependencies and the project's existing wasm test invocation — Taskfile or `tools/*.sh`.)
Expected: FAIL (trait seam not present).

- [ ] **Step 3: Implement `HistoryChart`**

- Define `Flavor` enum and the trait seam.
- `view.rs`: compose the four states as `view!` blocks. For `Ok(series)`, draw `<svg viewBox="0 0 W H">` with `step_path.d`, `step_path.band_d`, grid lines, Y labels (numeric when `state_axis_labels` returns `None`, else the pair), X ticks via `tick_labels`. For `Ok(daily)`, draw bars. For `Flavor::Counter` on `HistoryRange::Day`, read `power_pair`: `Some(pid)` → fetch the series for `pid`; `None` → fetch `daily(entity, 1, Counter)` and render as a single bar.

- [ ] **Step 4: Run tests to verify they pass**

Run the wasm test and `cargo check -p oikos-web --target wasm32-unknown-unknown`.
Expected: PASS and compiles.

Also add CSS classes used by this component (`.history-chart`, `.history-chart__shimmer`, `.history-chart__tooltip`, `.history-chart__empty`, `.history-chart__err`) to `crates/oikos-web/styles/main.css`, reusing the existing glass tokens; include them in this commit.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/components/history_chart/view.rs \
        crates/oikos-web/src/components/history_chart/mod.rs \
        crates/oikos-web/styles/main.css
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): HistoryChart component with stale-fetch guard

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `/history/:id` route and `HistoryDetailScreen`

**Files:**
- Create: `crates/oikos-web/src/screens/history_detail.rs` — the page.
- Modify: `crates/oikos-web/src/screens/mod.rs` — `pub mod history_detail;`
- Modify: `crates/oikos-web/src/app.rs` — add `<Route path=path!("/history/:id") view=HistoryDetailScreen />` to the `<Routes>` block.
- Modify: `crates/oikos-web/src/nav.rs` — update `return_target` so `/history/...` is a return target of the entity sheet (the sheet goes back to the history page, which goes back to its own `ReturnPath`).

**Interfaces:**
- Consumes: `HistoryChart`, `House` context (to look up the entity), `oikos_core::energy::meters::{EnergyMeters, EnergyEntity}` for `counter_power`.
- Produces: `#[component] pub fn HistoryDetailScreen() -> impl IntoView`.
- Behavior:
  - Reads `use_params::<HistoryParams>()` for `:id` and `use_query_map` for `range`; default `range` is `24h`. Writing a chip updates the URL via `navigate(replace: true)` so Back works.
  - Classifies the entity's flavor: `Flavor::Counter` if `EnergyUnits::factor(entity.unit).is_some()`, else `Flavor::Series`.
  - For `Counter`, resolves `power_pair = EnergyMeters::counter_power(&entity_as_EnergyEntity, &all_entities).map(|p| p.id.clone())`. On 24h, `HistoryChart` uses it to pick the step-line endpoint; if `None`, chart falls back to a single-day daily bar for today.
  - Chrome row: back arrow (uses the existing `ReturnPath`), name + area/type/availability, range chips `24h · 7d · 30d · 1y`.
  - Legend row under the chart: series flavor → `"Last {range_name} · held-until-next-row"` left, `"min X · max Y"` right; counter flavor → `"Last {range_name} · daily total"` left, `"{range_name} total: {sum} kWh"` right.

- [ ] **Step 1: Write the failing tests (native where possible, wasm for routing)**

Two tests:

```rust
// native test for flavor classification (pure function extracted from the screen)
#[test]
fn flavor_for_entity_is_counter_for_energy_units() {
    assert_eq!(flavor_for_unit(Some("kWh")), Flavor::Counter);
    assert_eq!(flavor_for_unit(Some("Wh")), Flavor::Counter);
    assert_eq!(flavor_for_unit(Some("W")), Flavor::Series);
    assert_eq!(flavor_for_unit(Some("°C")), Flavor::Series);
    assert_eq!(flavor_for_unit(None), Flavor::Series);
}

// wasm test for route params
#[wasm_bindgen_test::wasm_bindgen_test]
fn opens_from_route_with_range_default() {
    // navigate to /history/sensor.living_room_temperature
    // assert the screen reads id="sensor.living_room_temperature" and range=Day
    // navigate to /history/sensor.x?range=7d
    // assert range=Week
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run both the native and wasm tests; both should fail.

- [ ] **Step 3: Implement `HistoryDetailScreen` and wire the route**

- Add the `path!("/history/:id")` route to `app.rs`.
- Extend `nav.rs::return_target` to accept `/history/...` as a return target (so sheets opened from the history page go back to the history page, which in turn goes back to the original screen). Add a test next to the existing ones.
- Implement the screen: chrome row (reuse the existing sheet classes `sheet__panel`, `.chip`), chip row (reuse `.chips` / `.chip` / `.chip--selected`), `<HistoryChart ... />` card, legend row.
- Add Review-Focus test for Task 5 Review #1 (text entity renders empty copy, not panic) and #2 (counter without paired power on 24h renders the single-day bar fallback):

```rust
#[wasm_bindgen_test::wasm_bindgen_test]
async fn text_entity_shows_empty_copy_not_crash() { /* mock api returns NotRecorded */ }

#[wasm_bindgen_test::wasm_bindgen_test]
async fn counter_without_paired_power_falls_back_to_daily_bar_on_24h() { /* mock api */ }
```

- [ ] **Step 4: Run tests + browser smoke**

Run native + wasm tests. Expected: PASS.
Then launch dev server (`tools/web-dev.sh` or `trunk serve`), open the browser, navigate manually to a known entity's history page, confirm the chart draws for 24h/7d/30d/1y.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/screens/history_detail.rs \
        crates/oikos-web/src/screens/mod.rs \
        crates/oikos-web/src/app.rs \
        crates/oikos-web/src/nav.rs
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): HistoryDetailScreen on /history/:id with range chips

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Entity sheet — "View history →" chip

**Files:**
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs` — add one link row.
- Modify: `crates/oikos-web/styles/main.css` — nothing new; the row uses existing `.chip`, `.sheet__panel`.

**Interfaces:**
- Consumes: `encode_id`, the sheet's `entity_id` signal.
- Produces: a `<A href={format!("/history/{}", encode_id(id))} class="chip">"View history →"</A>` row inside the Controls panel.

- [ ] **Step 1: Write failing wasm test**

```rust
#[wasm_bindgen_test::wasm_bindgen_test]
fn sheet_has_view_history_link_pointing_to_history_route() {
    // mount EntitySheetScreen for an entity id
    // assert there is an anchor whose href is "/history/<encoded_id>"
}
```

- [ ] **Step 2: Run test to verify it fails**

Run the wasm test. Expected: FAIL (no such element).

- [ ] **Step 3: Add the link to the Controls panel**

Place the link as the last row of the `Controls` panel. No kind predicate: the chip is universal. Visual spacing uses the existing `.sheet__panel` row rhythm.

- [ ] **Step 4: Run test + browser smoke**

Expected: PASS. Open any entity sheet in the dev server, confirm the link is present and the click navigates to the history page.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/screens/entity_sheet.rs
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): View history → chip on the entity sheet Controls panel

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Energy screen — kWh-today tiles + derived-house fan-out + 5-min tick

**Files:**
- Modify: `crates/oikos-web/src/screens/energy.rs` — add a per-node kWh-today signal map and a `visibilitychange`-aware 5-min refresh effect.
- Modify: `crates/oikos-web/src/energy.rs` (module helpers) — expose a `plus_minus_ids(by_role, all)` helper that returns the two id-vecs from `house_meters(...)` (so the Energy screen can fan out `/today` without reaching into the full `HouseMeters` type).
- Modify: `crates/oikos-web/styles/main.css` — new class `.energy-tile-today` (small secondary line under each node name).

**Interfaces:**
- Consumes: `api::history::fetch_today`, `oikos_core::energy::meters::{EnergyMeters, HouseMeters}`, existing node-building code.
- Produces:
  - `pub fn plus_minus_ids(by_role: &BTreeMap<EnergyRole, Vec<&EnergyEntity>>, all: &[EnergyEntity]) -> Option<(Vec<String>, Vec<String>)>` in `crates/oikos-web/src/energy.rs` — wraps `EnergyMeters::house_meters` and returns `(plus_ids, minus_ids)` as owned strings; `None` when no derived house.
  - `pub fn next_fire_delay(hidden: bool, interval_ms: u32) -> Option<u32>` — pure helper for the visibility-aware tick (`None` when hidden).
  - `pub struct HouseTodayParts { pub plus: Vec<Option<f64>>, pub minus: Vec<Option<f64>> }` with `pub fn derived_house_today(parts: &HouseTodayParts) -> Option<f64>` — sums plus − minus; returns `None` if any part is `None`.
  - For each named counter node, a signal `kwh_today: RwSignal<Option<f64>>` and a derived display string "x.x kWh today". For the derived-house node, a `Memo` composing from the per-id signals via `derived_house_today`.
- Behavior:
  - On screen mount, fire one `fetch_today` per counter id (and one per plus/minus id for the derived house). Compose the derived value in a `Memo` once the per-id signals populate.
  - Create a `gloo-timers` 5-minute interval. On tick, re-fire if `document.hidden == false`. Pause by not scheduling the next fire when hidden; the `visibilitychange` listener re-arms it.
  - Add Review-Focus test #3: visibility toggling does not fire the tick, and `visibilitychange` to visible refreshes once.

- [ ] **Step 1: Write failing tests**

```rust
// native test — pull the paused-tick logic out of the view into a pure helper:
#[test]
fn next_fire_delay_pauses_when_hidden() {
    assert_eq!(next_fire_delay(false, 300_000), Some(300_000));
    assert_eq!(next_fire_delay(true, 300_000), None);
}

// native test for the derived sum
#[test]
fn derived_house_today_sums_plus_minus_minus() {
    let m = HouseTodayParts {
        plus: vec![Some(10.0), Some(4.0)],
        minus: vec![Some(1.5)],
    };
    assert_eq!(derived_house_today(&m), Some(12.5));
    let partial = HouseTodayParts {
        plus: vec![Some(10.0), None],
        minus: vec![Some(1.0)],
    };
    assert_eq!(derived_house_today(&partial), None); // any missing -> None
}

// wasm test for the tile presence (Task 7 Review #3 coverage):
#[wasm_bindgen_test::wasm_bindgen_test]
async fn each_node_shows_kwh_today_after_fetch() { /* mock api */ }
```

- [ ] **Step 2: Run tests to verify they fail**

Expected: FAIL (functions and tiles not present).

- [ ] **Step 3: Implement the tiles and the tick**

- Pure helpers go in `energy.rs` (module), tested natively.
- The screen reads them. The derived-house node is **not** wrapped in a link; it is a static block showing the sum.
- Named counter nodes keep their existing link; the tile sits under the name as a secondary line (CSS class `.energy-tile-today`).

- [ ] **Step 4: Run tests + browser smoke**

Expected: PASS. In the dev server, open Energy, confirm each node shows its kWh-today under the name, confirm the derived house tile has a value, and that hiding the tab for 6 min and returning does not fire during the hidden window.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/screens/energy.rs \
        crates/oikos-web/src/energy.rs \
        crates/oikos-web/styles/main.css
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): kWh-today tiles under energy nodes with visibility-aware 5-min tick

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Security screen — batched `last_change` + "since X"

**Files:**
- Modify: `crates/oikos-web/src/screens/security.rs` — one batched call on mount, a map `id -> Option<i64>`, a per-row "since X" secondary line.
- Modify: `crates/oikos-web/styles/main.css` — new class `.security-since` (small secondary line under the sensor name).

**Interfaces:**
- Consumes: `api::history::fetch_last_change` (already chunks internally), `components::now::Now` context for the current wall clock, `oikos-api::LastChangeItem`.
- Produces: `pub fn since_label(now_secs: i64, changed_at: Option<i64>) -> String` — pure helper returning `"just now"`, `"3 min ago"`, `"14:20"` same day, `"yesterday 14:20"`, `"3 days ago"` beyond; `""` when `changed_at` is `None`.
- Behavior:
  - On mount, build the id list from the currently visible sensors; call `fetch_last_change(&ids)`; store the resulting map.
  - On WebSocket events that update a sensor state, the row recomputes `since` from the new event's timestamp (no refetch).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn since_label_covers_the_natural_language_scale() {
    let now = 1_792_497_600; // noon on 2026-10-21 Europe/Paris
    assert_eq!(since_label(now, None), "");
    assert_eq!(since_label(now, Some(now - 30)), "just now");
    assert_eq!(since_label(now, Some(now - 180)), "3 min ago");
    assert_eq!(since_label(now, Some(now - 3_600)), "11:00");
    // yesterday same clock time
    let yesterday = now - 86_400;
    assert!(since_label(now, Some(yesterday)).starts_with("yesterday"));
    // older
    assert_eq!(since_label(now, Some(now - 3 * 86_400)), "3 days ago");
}

// wasm test:
#[wasm_bindgen_test::wasm_bindgen_test]
async fn security_screen_batches_last_change_and_shows_since() { /* mock api */ }
```

- [ ] **Step 2: Run tests to verify they fail**

Expected: FAIL.

- [ ] **Step 3: Implement**

- Pure `since_label` in `security.rs` (natively tested).
- Chunking in `api::history::fetch_last_change` is already done in Task 2; this task just calls it with the full visible list and renders the per-row line.
- Review Focus #4 test (51+ sensors chunk cleanly) uses the existing `chunk_ids` test from Task 2 as its proof; add one wasm test asserting a 60-id screen does not error.

- [ ] **Step 4: Run tests + browser smoke**

Expected: PASS. In the dev server, open Security, confirm each sensor shows "since X" under its name with the right scale labels.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    add crates/oikos-web/src/screens/security.rs \
        crates/oikos-web/styles/main.css
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -m "$(cat <<'EOF'
feat(web): since X lines on Security via batched last_change

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Finish

Run the whole workspace once more and confirm green:

```bash
cargo test --workspace
cargo check -p oikos-web --target wasm32-unknown-unknown
```

Then rebuild the web asset (per the project's existing script, e.g. `tools/web-build.sh`) and restart the hub so the new UI is served. Do not push without the user's say-so.
