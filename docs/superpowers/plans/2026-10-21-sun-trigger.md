# Sun Trigger and `is_dark` Check Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `trigger.sun` and `if op: is_dark` to Oikos-os flows so a user can author "turn on the light at civil dusk" without hand-typing a sunrise table.

**Architecture:** A pure, wasm-clean `oikos-core::flow::sun` module built on one primitive (`sun_altitude`) so the trigger and the operand share a single -6° boundary. The hub reads `[flows] latitude`/`longitude` from `oikos.toml`, falling back to Paris (48.8566, 2.3522) when both are absent, and attaches a save-time warning on the two flow-write endpoints while any sun flow is saved. The engine arms one `tokio::time::sleep_until` per sun node, mirroring the schedule timer; the web canvas gets a per-node sheet, a collapsed `is_dark` body, and a persistent "Paris fallback" badge driven by a new `GET /api/v1/config/flows` endpoint.

**Tech Stack:** Rust workspace (`oikos-core` pure, `oikos-os` hub, `oikos-api` wire types, `oikos-web` Leptos SSR+WASM). Sun math via the `sunrise` crate (or wrapped NOAA formulae) under one wrapper function. `chrono` and `chrono-tz` already in use. `axum` REST, `rmcp` MCP.

**Spec:** `docs/superpowers/specs/2026-10-21-sun-trigger-design.md`

## Global Constraints

- `oikos-core` builds for `wasm32-unknown-unknown` with the same code. No `std::time`, no OS, no clock in `oikos-core::flow::sun`.
- Attribution is **Gekkotron only**. Every commit uses `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com` (do not edit `.git/config`).
- Workspace gates run at each task's final step: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and the three wasm checks whenever `oikos-core` or `oikos-web` was touched (`cargo check -p oikos-core --target wasm32-unknown-unknown`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, and a web bundle build equivalent to `trunk build`).
- Unknown JSON keys stay preserved via `extra` on every params struct. Nothing silently drops a key.
- The engine's known-to-known rule, retained-replay silence, and one-run-per-flow mode governance are unchanged.
- Civil dusk/dawn uses sun centre at **-6°**, refraction included; sunrise/sunset uses **-0.833°** apparent. The dusk/dawn threshold and the `is_dark` threshold come from a single `const CIVIL_ALTITUDE_DEG: f64 = -6.0;` in the sun module.
- Paris fallback coordinates: **48.8566, 2.3522** (city hall).
- `oikos-core::flow::sun` must expose exactly one evaluator (`sun_altitude`); `sun_events` and `is_dark_at` derive from it so `trigger.sun which=dusk` and `op: is_dark` cannot disagree.
- No live reload of `latitude`/`longitude`: a change requires a hub restart. `fallback_active` is latched at startup from the loaded config and stored on `Hub`.
- No catch-up on missed sun events, under any circumstance (boot, broker reconnect, flow enable, flow save). Only the next upcoming event + offset is armed.
- The hub's listening port is `0.0.0.0:8081` (per project memory). Tests that spin up the hub either take an ephemeral port or hit it on 8081.

## Review Focus

- A single-coordinate `oikos.toml` (one of `latitude`/`longitude` present, the other absent): startup error, not silent Paris fallback. Test owned by **Task 1**.
- `offset_minutes` at the boundary: `-720` and `720` accepted, `-721` and `721` rejected. Test owned by **Task 4**.
- An `if op: is_dark` immediately after `trigger.sun` with no `entity`: valid (the operand reads the sun angle itself). Test owned by **Task 4**.
- A sun flow whose next event is more than seven days out (polar edge): engine leaves it disarmed with a log line, and re-arms on an off→on `set_flow_enabled` toggle (no daily background recheck). Test owned by **Task 5**.
- Hub restart after today's sunset has already passed: engine arms tomorrow's sunset, not a replay of today's. Test owned by **Task 5**.

---

### Task 1: `[flows] latitude`/`longitude` in `oikos.toml`

**Files:**
- Modify: `crates/oikos-os/src/config.rs` (`FlowsConfig`)
- Modify: `crates/oikos-os/src/hub.rs` (`Hub::house_point`, `Hub::fallback_active`, store `fallback_active` latched at construction)
- Modify: `crates/oikos-os/src/main.rs` (call `cfg.flows.validate()` right after `Config::load`, bail on error)
- Test: `crates/oikos-os/src/config.rs` (unit tests at the bottom)

**Interfaces:**
- Consumes: nothing from this plan.
- Produces:
  - `pub struct FlowsConfig { pub timezone: Option<String>, pub latitude: Option<f64>, pub longitude: Option<f64> }` — the two coord fields are new; `timezone` stays as it is.
  - `impl FlowsConfig { pub fn validate(&self) -> Result<(), String>; pub fn house_point(&self) -> (f64, f64); pub fn fallback_active(&self) -> bool; }`.
  - On `Hub`: `pub fn house_point(&self) -> (f64, f64)` and `pub fn fallback_active(&self) -> bool`. `fallback_active` is a `bool` field on `Hub` populated once at construction from `cfg.flows.fallback_active()`.

- [ ] **Step 1: Write failing config tests**

```rust
#[test]
fn flows_config_both_coords_required_together() {
    let only_lat: FlowsConfig = toml::from_str("latitude = 45.75").unwrap();
    assert!(only_lat.validate().is_err());
    let only_lon: FlowsConfig = toml::from_str("longitude = 4.85").unwrap();
    assert!(only_lon.validate().is_err());
    let both: FlowsConfig = toml::from_str("latitude = 45.75\nlongitude = 4.85").unwrap();
    assert_eq!(both.validate(), Ok(()));
    let neither = FlowsConfig::default();
    assert_eq!(neither.validate(), Ok(()));
}

#[test]
fn flows_config_coord_ranges_enforced() {
    let bad_lat: FlowsConfig = toml::from_str("latitude = 91.0\nlongitude = 4.85").unwrap();
    assert!(bad_lat.validate().is_err());
    let bad_lon: FlowsConfig = toml::from_str("latitude = 45.75\nlongitude = 181.0").unwrap();
    assert!(bad_lon.validate().is_err());
}

#[test]
fn flows_config_house_point_falls_back_to_paris() {
    let cfg = FlowsConfig::default();
    assert_eq!(cfg.house_point(), (48.8566, 2.3522));
    assert!(cfg.fallback_active());
    let configured: FlowsConfig =
        toml::from_str("latitude = 45.75\nlongitude = 4.85").unwrap();
    assert_eq!(configured.house_point(), (45.75, 4.85));
    assert!(!configured.fallback_active());
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p oikos-os --lib config::`
Expected: FAIL (fields and methods absent).

- [ ] **Step 3: Add `latitude: Option<f64>` and `longitude: Option<f64>` to `FlowsConfig` (both `#[serde(default)]`), implement `validate`, `house_point`, `fallback_active`**

`validate` enforces: `self.latitude.is_some() == self.longitude.is_some()`; `latitude` in `[-90.0, 90.0]` when present; `longitude` in `[-180.0, 180.0]` when present. On `Hub`, store a `fallback_active: bool` populated from `cfg.flows.fallback_active()` at construction and return it from `Hub::fallback_active`. `Hub::house_point` delegates to `cfg.flows.house_point()`.

- [ ] **Step 4: Wire `main.rs` to bail on `cfg.flows.validate()` and run tests to confirm they pass**

Run: `cargo test -p oikos-os --lib config::`
Expected: PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-core --target wasm32-unknown-unknown`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(config): [flows] latitude/longitude with Paris fallback"
```

---

### Task 2: `oikos-core::flow::sun` — pure sun computation

**Files:**
- Create: `crates/oikos-core/src/flow/sun.rs`
- Modify: `crates/oikos-core/src/flow/mod.rs` (`pub mod sun;` + re-exports used elsewhere)
- Modify: `crates/oikos-core/Cargo.toml` (add the chosen sun crate; constraints below)
- Test: `crates/oikos-core/src/flow/sun.rs` (unit tests at the bottom)

**Interfaces:**
- Consumes: nothing from this plan.
- Produces:
  - `pub const CIVIL_ALTITUDE_DEG: f64 = -6.0;`
  - `pub const APPARENT_ALTITUDE_DEG: f64 = -0.833;`
  - `pub fn sun_altitude(lat: f64, lon: f64, at: DateTime<Utc>) -> f64` — apparent solar altitude in degrees, refraction included.
  - `pub struct SunEvents { pub sunrise: Option<DateTime<Utc>>, pub sunset: Option<DateTime<Utc>>, pub dawn: Option<DateTime<Utc>>, pub dusk: Option<DateTime<Utc>> }`.
  - `pub fn sun_events(lat: f64, lon: f64, date: NaiveDate) -> SunEvents` — root-found from `sun_altitude`. `None` on days where the event does not exist (polar night / midnight sun).
  - `pub fn is_dark_at(lat: f64, lon: f64, at: DateTime<Utc>) -> bool` = `sun_altitude(lat, lon, at) <= CIVIL_ALTITUDE_DEG`.

- [ ] **Step 1: Write failing tests pinning the primitive, the derived events, the shared boundary, and polar safety**

```rust
#[test]
fn paris_sunrise_sunset_within_sixty_seconds_of_published_values() {
    // Paris, 2026-01-01: NOAA sunrise ~08:44 CET, sunset ~17:03 CET.
    let paris_tz = chrono_tz::Europe::Paris;
    let e = sun_events(48.8566, 2.3522, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let sunrise = e.sunrise.unwrap().with_timezone(&paris_tz);
    let sunset = e.sunset.unwrap().with_timezone(&paris_tz);
    let want_sunrise = paris_tz.with_ymd_and_hms(2026, 1, 1, 8, 44, 0).unwrap();
    let want_sunset  = paris_tz.with_ymd_and_hms(2026, 1, 1, 17, 3, 0).unwrap();
    assert!((sunrise - want_sunrise).num_seconds().abs() <= 60);
    assert!((sunset  - want_sunset ).num_seconds().abs() <= 60);
}

#[test]
fn lyon_summer_solstice_has_four_events_in_order() {
    let e = sun_events(45.75, 4.85, NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
    assert!(e.dawn.unwrap() < e.sunrise.unwrap());
    assert!(e.sunset.unwrap() < e.dusk.unwrap());
}

#[test]
fn is_dark_boundary_matches_dusk_and_dawn() {
    let (lat, lon) = (48.8566, 2.3522);
    let e = sun_events(lat, lon, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let dusk = e.dusk.unwrap();
    assert!(is_dark_at(lat, lon, dusk));
    assert!(!is_dark_at(lat, lon, dusk - Duration::seconds(60)));
    let dawn = e.dawn.unwrap();
    assert!(is_dark_at(lat, lon, dawn - Duration::seconds(60)));
    assert!(!is_dark_at(lat, lon, dawn + Duration::seconds(60)));
}

#[test]
fn polar_night_leaves_events_none_without_panicking() {
    let e = sun_events(78.22, 15.65, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    assert_eq!(e.sunrise, None);
    assert_eq!(e.sunset, None);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p oikos-core flow::sun::`
Expected: FAIL (module absent).

- [ ] **Step 3: Pick the sun crate and implement the primitive and derived events**

Pick `sunrise` (pure, wasm-friendly, no OS time) or hand-write the NOAA solar position formula. Constraints: `sun_altitude` is the ONLY altitude evaluator, `sun_events` finds crossings by sampling `sun_altitude` minute-wise over the local day and bisecting on the two straddling samples, `is_dark_at` is `altitude <= CIVIL_ALTITUDE_DEG`. Verify the chosen crate builds for `wasm32-unknown-unknown` without `getrandom` or any OS-time dependency.

- [ ] **Step 4: Run tests to confirm they pass, including the wasm smoke build**

Run: `cargo test -p oikos-core flow::sun:: && cargo check -p oikos-core --target wasm32-unknown-unknown`
Expected: PASS and PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(core): pure sun altitude, events and is_dark primitive"
```

---

### Task 3: `trigger.sun` and `op: is_dark` in the flow model

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs`
- Test: `crates/oikos-core/src/flow/model.rs` (tests module at the bottom)

**Interfaces:**
- Consumes: nothing from this plan.
- Produces:
  - `#[serde(rename_all = "snake_case")] pub enum SunWhich { Sunrise, Sunset, Dusk, Dawn }`.
  - `pub struct TriggerSunParams { pub which: SunWhich, #[serde(default, skip_serializing_if = "Option::is_none")] pub offset_minutes: Option<i32>, #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")] pub extra: BTreeMap<String, Value> }` with `impl TriggerSunParams { pub fn offset(&self) -> i32 { self.offset_minutes.unwrap_or(0) } }`.
  - `NodeKind::TriggerSun(TriggerSunParams)` with `#[serde(rename = "trigger.sun")]`. `NodeKind::type_name()` returns `"trigger.sun"` and `NodeKind::is_trigger()` returns `true` for it. `ports()` stays empty for triggers.
  - `pub enum Op { Eq, Ne, Gt, Lt, Gte, Lte, IsDark }` — adds `IsDark`, renamed on the wire as `is_dark`.
  - `pub struct IfParams { pub op: Op, #[serde(default, skip_serializing_if = "Option::is_none")] pub value: Option<Value>, pub entity: Option<String>, pub missing: Option<Missing>, pub extra: BTreeMap<String, Value> }` — `value` becomes optional. The existing `missing` and `entity` fields are unchanged in shape.

- [ ] **Step 1: Write failing round-trip tests**

```rust
#[test]
fn trigger_sun_round_trips_with_offset_and_extra() {
    let node: Node = serde_json::from_str(
        r#"{"id":"ev","type":"trigger.sun","params":{"which":"sunset","offset_minutes":-30,"note":"n"}}"#,
    ).unwrap();
    assert!(node.kind.is_trigger());
    assert_eq!(node.kind.type_name(), "trigger.sun");
    match &node.kind {
        NodeKind::TriggerSun(p) => {
            assert_eq!(p.which, SunWhich::Sunset);
            assert_eq!(p.offset(), -30);
            assert_eq!(p.extra.get("note"), Some(&json!("n")));
        }
        other => panic!("expected trigger.sun, got {other:?}"),
    }
    let again: Node = serde_json::from_value(serde_json::to_value(&node).unwrap()).unwrap();
    assert_eq!(again, node);
}

#[test]
fn trigger_sun_offset_defaults_to_zero_and_is_omitted_from_wire() {
    let node: Node = serde_json::from_str(
        r#"{"id":"dawn","type":"trigger.sun","params":{"which":"dawn"}}"#,
    ).unwrap();
    match &node.kind {
        NodeKind::TriggerSun(p) => assert_eq!(p.offset(), 0),
        other => panic!("{other:?}"),
    }
    let wire = serde_json::to_value(&node).unwrap();
    assert!(wire["params"].get("offset_minutes").is_none());
}

#[test]
fn if_is_dark_round_trips_without_value_entity_or_missing() {
    let node: Node = serde_json::from_str(
        r#"{"id":"dark","type":"if","params":{"op":"is_dark"}}"#,
    ).unwrap();
    match &node.kind {
        NodeKind::If(p) => {
            assert_eq!(p.op, Op::IsDark);
            assert!(p.value.is_none());
            assert!(p.entity.is_none());
            assert!(p.missing.is_none());
        }
        other => panic!("{other:?}"),
    }
    let wire = serde_json::to_value(&node).unwrap();
    assert_eq!(wire["params"]["op"], json!("is_dark"));
    assert!(wire["params"].get("value").is_none());
}

#[test]
fn existing_if_round_trips_with_value_present() {
    let p: IfParams = serde_json::from_str(r#"{"op":"lt","value":65,"entity":"t"}"#).unwrap();
    assert_eq!(p.value, Some(json!(65)));
    assert_eq!(p.op, Op::Lt);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p oikos-core flow::model::`
Expected: FAIL.

- [ ] **Step 3: Edit `model.rs` to add `SunWhich`, `TriggerSunParams`, `NodeKind::TriggerSun`, `Op::IsDark`, change `IfParams.value` to `Option<Value>`**

Update `NodeKind::type_name()` and `NodeKind::is_trigger()` for the new variant. Keep `#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]` on both new structs. The compiler surfaces every existing `IfParams` struct-literal (e.g. in engine `if` evaluation) — update them to wrap in `Some(...)`.

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p oikos-core flow::model::`
Expected: PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-core --target wasm32-unknown-unknown`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(core): trigger.sun node and op: is_dark on if"
```

---

### Task 4: Validation of `trigger.sun`, `op: is_dark` and the sun-triggered reach rule

**Files:**
- Modify: `crates/oikos-core/src/flow/validate.rs`
- Test: `crates/oikos-core/src/flow/validate.rs` (tests module)

**Interfaces:**
- Consumes: Task 3's `NodeKind::TriggerSun`, `TriggerSunParams`, `Op::IsDark`, `IfParams.value: Option<Value>`.
- Produces: two new `FlowError` variants and one rename:
  - `#[error("node `{node}`: offset_minutes {offset_minutes} is outside [-720, 720]")] BadSunOffset { node: String, offset_minutes: i32 }`.
  - `#[error("node `{node}`: {reason}")] BadOperand { node: String, reason: String }`.
  - `check_ifs_after_schedules` → `check_ifs_after_valueless_triggers`; it walks from both `TriggerSchedule` and `TriggerSun` roots, and when it sees an `if` without `entity`, it treats `op == Op::IsDark` as self-sufficient (does not raise `IfNeedsEntity`).

- [ ] **Step 1: Write failing tests for offset range, operand rules, and reach**

```rust
#[test]
fn trigger_sun_offset_boundary() {
    assert_eq!(validate(&sun_flow("ev", SunWhich::Sunset, Some(720))), Ok(()));
    assert_eq!(validate(&sun_flow("ev", SunWhich::Sunset, Some(-720))), Ok(()));
    assert!(matches!(
        validate(&sun_flow("ev", SunWhich::Sunset, Some(721))),
        Err(FlowError::BadSunOffset { offset_minutes: 721, .. })
    ));
    assert!(matches!(
        validate(&sun_flow("ev", SunWhich::Sunset, Some(-721))),
        Err(FlowError::BadSunOffset { offset_minutes: -721, .. })
    ));
}

#[test]
fn op_is_dark_rejects_entity_value_and_missing() {
    for (field, patch) in [
        ("value",   r#"{"op":"is_dark","value":1}"#),
        ("entity",  r#"{"op":"is_dark","entity":"e"}"#),
        ("missing", r#"{"op":"is_dark","missing":"true"}"#),
    ] {
        let err = validate(&flow_with_if_params_json(patch)).unwrap_err();
        assert!(
            matches!(err, FlowError::BadOperand { ref reason, .. } if reason.contains(field)),
            "{field}: {err:?}"
        );
    }
}

#[test]
fn non_is_dark_op_requires_value() {
    let err = validate(&flow_with_if_params_json(r#"{"op":"lt","entity":"t"}"#)).unwrap_err();
    assert!(matches!(err, FlowError::BadOperand { ref reason, .. } if reason.contains("value")));
}

#[test]
fn if_is_dark_is_valid_right_after_trigger_sun() {
    // The exemption: op: is_dark needs no entity even on a path with no trigger value.
    assert_eq!(validate(&sun_then_is_dark_flow()), Ok(()));
}

#[test]
fn plain_if_without_entity_after_trigger_sun_is_still_rejected() {
    assert_eq!(
        validate(&sun_then_plain_if_flow()),
        Err(FlowError::IfNeedsEntity { node: "chk".into() })
    );
}
```

Test fixtures (`sun_flow`, `flow_with_if_params_json`, `sun_then_is_dark_flow`, `sun_then_plain_if_flow`) are tiny helpers built on `Flow::decode` with the matching JSON literal.

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p oikos-core flow::validate::`
Expected: FAIL.

- [ ] **Step 3: Extend `validate` and rename the walker**

In the per-node loop, after the existing arms add `NodeKind::TriggerSun(p) => if !(-720..=720).contains(&p.offset()) { return Err(FlowError::BadSunOffset { ... }); }` and `NodeKind::If(p) if p.op == Op::IsDark => check that value/entity/missing are all None, else BadOperand(reason naming the field)` and `NodeKind::If(p) if p.op != Op::IsDark && p.value.is_none() => BadOperand(reason "value is required for op …")`. Rename `check_ifs_after_schedules` to `check_ifs_after_valueless_triggers` and seed it with `TriggerSchedule` **and** `TriggerSun` nodes. In its `if` branch, skip the `IfNeedsEntity` check when `p.op == Op::IsDark`.

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p oikos-core flow::validate::`
Expected: PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-core --target wasm32-unknown-unknown`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(core): validate trigger.sun offset range and op: is_dark siblings"
```

---

### Task 5: Engine arms sun timers and evaluates `op: is_dark`

**Files:**
- Create: `crates/oikos-os/src/flows/sun.rs`
- Modify: `crates/oikos-os/src/flows/mod.rs` (`mod sun;`)
- Modify: `crates/oikos-os/src/flows/engine.rs` (arm sun timers alongside schedules; evaluate `op: is_dark` in the `if` step)
- Test: `crates/oikos-os/src/flows/sun.rs` (unit tests)
- Test: `crates/oikos-os/tests/engine_sun.rs` (new integration file)

**Interfaces:**
- Consumes: Task 1's `Hub::house_point` / `Hub::fallback_active`; Task 2's `oikos_core::flow::sun::{sun_events, is_dark_at, SunEvents}`; Task 3's `NodeKind::TriggerSun`, `SunWhich`, `TriggerSunParams::offset`, `Op::IsDark`; the existing engine `Now` type and `house_zone` helper from `flows::schedule`.
- Produces:
  - `pub(super) struct ArmedSun { pub node: String, pub at: DateTime<Tz>, pub which: SunWhich, pub offset_minutes: i32 }` in `flows/sun.rs`.
  - `pub(super) fn next_sun_fire(lat: f64, lon: f64, zone: Tz, which: SunWhich, offset_minutes: i32, now: DateTime<Tz>) -> Option<DateTime<Tz>>` — the first fire strictly after `now` within seven days, else `None`.
  - `Engine` grows `sun_timers: BTreeMap<String, ArmedSun>` keyed `"{flow}/{node}"`. `arm_schedules` gains a sibling `arm_suns`; `next_deadline` considers both maps; the fire loop handles both. Reload paths (`EngineRequest::Reload`, `set_flow`, `set_flow_enabled`, `delete_flow`) call `arm_suns`.
  - The engine's `if` step execution learns one new case: `Op::IsDark` ⇒ take the `true` port iff `is_dark_at(hub.house_point().0, hub.house_point().1, now())`, else `false`. No entity read, no `missing` handling. Step detail logged as `"is_dark → true"` / `"is_dark → false"`.

- [ ] **Step 1: Write failing unit tests for `next_sun_fire`**

```rust
#[test]
fn next_sun_fire_skips_past_events_and_arms_tomorrow() {
    let zone = chrono_tz::Europe::Paris;
    let (lat, lon) = (48.8566, 2.3522);
    // 2026-01-01 20:00 Paris: today's sunset at ~17:03 has already passed.
    let now = zone.with_ymd_and_hms(2026, 1, 1, 20, 0, 0).single().unwrap();
    let at = next_sun_fire(lat, lon, zone, SunWhich::Sunset, 0, now).unwrap();
    assert_eq!(at.date_naive(), chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
}

#[test]
fn next_sun_fire_applies_signed_offset() {
    let zone = chrono_tz::Europe::Paris;
    let (lat, lon) = (48.8566, 2.3522);
    let now = zone.with_ymd_and_hms(2026, 1, 1, 6, 0, 0).single().unwrap();
    let base    = next_sun_fire(lat, lon, zone, SunWhich::Sunset,   0, now).unwrap();
    let minus30 = next_sun_fire(lat, lon, zone, SunWhich::Sunset, -30, now).unwrap();
    assert_eq!((base - minus30).num_minutes(), 30);
}

#[test]
fn next_sun_fire_returns_none_in_polar_night() {
    let zone = chrono_tz::Europe::Oslo;
    let now = zone.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).single().unwrap();
    assert_eq!(next_sun_fire(78.22, 15.65, zone, SunWhich::Sunrise, 0, now), None);
}
```

- [ ] **Step 2: Write failing integration tests for the engine**

```rust
// crates/oikos-os/tests/engine_sun.rs
#[tokio::test(start_paused = true)]
async fn sun_trigger_fires_at_mocked_dusk_and_rearms() {
    let hub = test_hub_with_point(45.75, 4.85, chrono_tz::Europe::Paris).await;
    hub.set_flow(sun_flow("evening", SunWhich::Dusk, 0)).await.unwrap();
    hub.set_flow_enabled("evening", true).await.unwrap();
    tokio::time::advance(Duration::from_secs(24 * 3600)).await;
    let runs = hub.get_flow("evening").await.unwrap().runs;
    assert!(!runs.is_empty(), "expected a run at mocked dusk");
    hub.set_flow_enabled("evening", false).await.unwrap();
    tokio::time::advance(Duration::from_secs(24 * 3600)).await;
    assert_eq!(hub.get_flow("evening").await.unwrap().runs.len(), runs.len());
}

#[tokio::test(start_paused = true)]
async fn seven_day_cap_leaves_polar_flow_disarmed_until_toggle() {
    let hub = test_hub_with_point(78.22, 15.65, chrono_tz::Europe::Oslo).await;
    hub.set_flow(sun_flow("polar", SunWhich::Sunrise, 0)).await.unwrap();
    hub.set_flow_enabled("polar", true).await.unwrap();
    tokio::time::advance(Duration::from_secs(7 * 24 * 3600)).await;
    assert!(hub.get_flow("polar").await.unwrap().runs.is_empty());
    hub.set_flow_enabled("polar", false).await.unwrap();
    hub.set_flow_enabled("polar", true).await.unwrap();
    assert!(hub.get_flow("polar").await.unwrap().runs.is_empty()); // still polar, still no fire
}

#[tokio::test(start_paused = true)]
async fn if_is_dark_takes_true_branch_at_night_regardless_of_trigger() {
    let hub = test_hub_with_point(48.8566, 2.3522, chrono_tz::Europe::Paris).await;
    hub.set_flow(state_then_is_dark_flow("night_switch")).await.unwrap();
    hub.set_flow_enabled("night_switch", true).await.unwrap();
    // Advance to 23:00 Paris; sun is well below -6°.
    advance_clock_to(&hub, chrono_tz::Europe::Paris
        .with_ymd_and_hms(2026, 1, 1, 23, 0, 0).unwrap()).await;
    hub.publish_state("some_entity", json!("on")).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let step = hub.get_flow("night_switch").await.unwrap().runs[0]
        .steps.iter().find(|s| s.node == "dark").unwrap();
    assert!(step.detail.as_deref().unwrap_or("").contains("is_dark → true"));
}
```

- [ ] **Step 3: Run both failing**

Run: `cargo test -p oikos-os flows::sun && cargo test -p oikos-os --test engine_sun`
Expected: FAIL.

- [ ] **Step 4: Implement `next_sun_fire`, arm sun timers in the engine, and evaluate `Op::IsDark`**

- `next_sun_fire`: iterate `date = now.date_naive() .. now.date_naive() + 8` (inclusive end gives a 7-day window), compute `sun_events(lat, lon, date)`, select the field for `which`, add `offset_minutes` as `Duration::minutes`, convert to `zone`, keep the first result strictly greater than `now`. Return the first such instant, else `None`.
- `Engine::arm_suns(&mut self)`: for each enabled flow, for each `trigger.sun` node, compute the next fire; insert an `ArmedSun` under `"{flow_id}/{node_id}"`; if `None`, log `tracing::warn!(flow=…, node=…, "sun flow disarmed: next event more than 7 days out or does not exist")` and skip the insert. The engine never background-rechecks a disarmed sun node; the next chance is a reload path.
- `Engine::next_deadline`: `self.schedules.values().map(|a| a.at).chain(self.sun_timers.values().map(|a| a.at)).min()`.
- Fire path: when a sun timer's instant is reached, start a run (respecting `mode` exactly like a schedule tick), then call `next_sun_fire` for the same node and replace the `ArmedSun` entry (or remove it if `None`).
- `if` evaluator: in the step that handles `NodeKind::If(p)`, add a branch for `p.op == Op::IsDark` that returns `is_dark_at(hub.house_point().0, hub.house_point().1, (self.now)())`, mapped to the `true`/`false` port and logged as `is_dark → true` / `is_dark → false`.

- [ ] **Step 5: Run tests to confirm they pass**

Run: `cargo test -p oikos-os`
Expected: PASS.

- [ ] **Step 6: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(flows): arm trigger.sun timers and evaluate op: is_dark"
```

---

### Task 6: `FlowWriteOutcome` wrapper on REST + `GET /api/v1/config/flows`

**Files:**
- Modify: `crates/oikos-api/src/flows.rs` (`FlowWriteOutcome`, `FlowsConfigView`)
- Modify: `crates/oikos-api/src/lib.rs` (re-exports)
- Modify: `crates/oikos-os/src/flows/hub.rs` (`set_flow` returns `FlowWriteOutcome`)
- Modify: `crates/oikos-os/src/http/api.rs` (`put_flow` returns the wrapper; new `GET /api/v1/config/flows` handler)
- Modify: `crates/oikos-core/src/flow/mod.rs` or `flow/sun.rs` (tiny helper `pub fn uses_sun_or_is_dark(flow: &Flow) -> bool`)
- Test: `crates/oikos-os/tests/http_flows_config.rs` (new)

**Interfaces:**
- Consumes: Task 1's `Hub::fallback_active`; Task 3's `NodeKind::TriggerSun` and `Op::IsDark`.
- Produces:
  - `pub struct FlowWriteOutcome { pub flow: FlowView, pub warning: Option<String> }` — note **no** `skip_serializing_if`: the key is always present (`null` when none), so the wire shape is fixed.
  - `pub struct FlowsConfigView { pub latitude: Option<f64>, pub longitude: Option<f64>, pub timezone: String, pub fallback_active: bool }`.
  - `pub fn uses_sun_or_is_dark(flow: &Flow) -> bool` — true iff any node is `NodeKind::TriggerSun` or an `If` whose `op == Op::IsDark`.
  - `Hub::set_flow(&self, flow: Flow) -> Result<FlowWriteOutcome, HubError>` — warning string, exact wording from spec §3: `"Running sun flows against the Paris fallback. Set \`[flows] latitude\` and \`[flows] longitude\` in \`oikos.toml\` to use your house's point."`, present iff `self.fallback_active() && uses_sun_or_is_dark(&flow)`.
  - REST: `PUT /api/v1/flows/{id}` returns `Json<FlowWriteOutcome>`; `GET /api/v1/config/flows` returns `Json<FlowsConfigView>`.
  - REST shapes unchanged on `GET /flows`, `GET /flows/{id}`, `DELETE /flows/{id}`, `POST /flows/{id}/enabled`, `POST /flows/{id}/run`.

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test]
async fn put_flow_wraps_with_warning_when_coords_absent_and_flow_uses_sun() {
    let app = test_app_without_coords().await;
    let sun = json!({ "id":"s","name":"S","nodes":[{
        "id":"ev","type":"trigger.sun","params":{"which":"sunset"}
    }] });
    let out: FlowWriteOutcome = put_json(&app, "/api/v1/flows/s", &sun).await;
    assert!(out.warning.as_deref().map_or(false, |w| w.contains("Paris fallback")));
    assert_eq!(out.flow.definition.id, "s");

    // Non-sun flow with coords absent: no warning.
    let plain = json!({ "id":"p","name":"P","nodes":[{
        "id":"t","type":"trigger.state","params":{"entity":"e"}
    }] });
    let out: FlowWriteOutcome = put_json(&app, "/api/v1/flows/p", &plain).await;
    assert!(out.warning.is_none());
}

#[tokio::test]
async fn put_flow_no_warning_when_coords_set() {
    let app = test_app_with_point(45.75, 4.85).await;
    let sun = json!({ "id":"s","name":"S","nodes":[{
        "id":"ev","type":"trigger.sun","params":{"which":"sunset"}
    }] });
    let out: FlowWriteOutcome = put_json(&app, "/api/v1/flows/s", &sun).await;
    assert!(out.warning.is_none());
}

#[tokio::test]
async fn warning_key_is_always_present_on_the_wire() {
    let app = test_app_with_point(45.75, 4.85).await;
    let raw: serde_json::Value = put_raw(&app, "/api/v1/flows/s", &sun_flow_json("s")).await;
    assert!(raw.get("warning").is_some());
    assert_eq!(raw["warning"], json!(null));
}

#[tokio::test]
async fn get_flows_config_reports_fallback_active() {
    let app = test_app_without_coords().await;
    let v: FlowsConfigView = get_json(&app, "/api/v1/config/flows").await;
    assert_eq!(v.latitude, None);
    assert_eq!(v.longitude, None);
    assert!(v.fallback_active);
    let app = test_app_with_point(45.75, 4.85).await;
    let v: FlowsConfigView = get_json(&app, "/api/v1/config/flows").await;
    assert_eq!(v.latitude, Some(45.75));
    assert!(!v.fallback_active);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test -p oikos-os --test http_flows_config`
Expected: FAIL.

- [ ] **Step 3: Add the wire types, the helper, change `Hub::set_flow`, add the route handlers**

Register `GET /api/v1/config/flows` next to the other flow routes. The handler reads `cfg.flows` from the `Hub` and reports `fallback_active = hub.fallback_active()` (latched at startup per Task 1). Update `put_flow` to return `Json<FlowWriteOutcome>`. Update any other in-crate callers of `Hub::set_flow` surfaced by the compiler to read `.flow` on the result.

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p oikos-os --test http_flows_config`
Expected: PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-core --target wasm32-unknown-unknown`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(http): wrap flow writes with Paris-fallback warning; GET /api/v1/config/flows"
```

---

### Task 7: MCP `set_flow` returns the wrapper; server instructions grow the sun paragraph

**Files:**
- Modify: `crates/oikos-os/src/mcp/flow_tools.rs` (`set_flow` return type)
- Modify: `crates/oikos-os/src/mcp/mod.rs` (append the sun paragraph to the instructions string)
- Test: `crates/oikos-os/src/mcp/flow_tools.rs` (tests at the bottom, or alongside existing MCP tests)

**Interfaces:**
- Consumes: Task 6's `FlowWriteOutcome`.
- Produces: `set_flow` MCP tool returns `ToolResult<FlowWriteOutcome>`; `get_flow`, `delete_flow`, `set_flow_enabled`, `run_flow`, `list_flows` unchanged.

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn mcp_set_flow_returns_wrapper_and_warning() {
    let server = test_mcp_without_coords().await;
    let out: FlowWriteOutcome = server
        .call_tool("set_flow", json!({ "flow": sun_flow_json("s") }))
        .await
        .unwrap();
    assert!(out.warning.as_deref().map_or(false, |w| w.contains("Paris fallback")));
    assert_eq!(out.flow.definition.id, "s");
}

#[tokio::test]
async fn mcp_instructions_mention_trigger_sun_and_is_dark() {
    let instr = oikos_server_instructions();
    assert!(instr.contains("trigger.sun"));
    assert!(instr.contains("is_dark"));
    assert!(instr.contains("Paris"));
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p oikos-os mcp::`
Expected: FAIL.

- [ ] **Step 3: Change `set_flow` to return `ToolResult<FlowWriteOutcome>`; append the sun paragraph**

Instructions paragraph (append after the current flows block, before the `get_history` block): `"Sun events use \`trigger.sun\` with \`which\` one of \`sunrise\`, \`sunset\`, \`dusk\` (civil), \`dawn\` (civil), and an optional \`offset_minutes\` in [-720, 720]. \`if\` with \`op: is_dark\` is true between civil dusk and civil dawn at the configured point. Set \`[flows] latitude\` and \`[flows] longitude\` in \`oikos.toml\`; the hub falls back to Paris otherwise and reports a warning on save."`

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test -p oikos-os mcp::`
Expected: PASS.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(mcp): set_flow returns FlowWriteOutcome; sun paragraph in instructions"
```

---

### Task 8: Web canvas — sun sheet, collapsed `is_dark` body, Paris-fallback badge and toast

**Files:**
- Create: `crates/oikos-web/src/screens/flow_canvas/sun.rs`
- Modify: `crates/oikos-web/src/screens/flow_canvas/settings.rs` (route `NodeKind::TriggerSun` to the new sheet; route `If { op: IsDark }` to the collapsed body)
- Modify: `crates/oikos-web/src/screens/flow_canvas/graph.rs` (sun glyph on `trigger.sun`; moon glyph on `if is_dark`; "Paris fallback" badge rendering)
- Modify: `crates/oikos-web/src/screens/flow_canvas/toolbar.rs` (two new entries in the Add-node menu: "Sun event" and "When it's dark")
- Modify: `crates/oikos-web/src/icons.rs` (sun, moon glyphs — flat, one colour, matching existing set)
- Modify: `crates/oikos-web/src/api.rs` (`pub async fn get_flows_config() -> Result<FlowsConfigView, _>`; the save path reads `FlowWriteOutcome`)
- Modify: `crates/oikos-web/src/house.rs` (new `pub flows_config: RwSignal<Option<FlowsConfigView>>` loaded on mount, refreshed on each save)
- Test: `crates/oikos-web/src/screens/flow_canvas/sun.rs` tests

**Interfaces:**
- Consumes: Task 2's `sun_events` (called directly in the browser via wasm-clean `oikos-core::flow::sun`); Task 3's `NodeKind::TriggerSun`, `SunWhich`, `TriggerSunParams`, `Op::IsDark`; Task 6's `FlowsConfigView`, `FlowWriteOutcome`, `GET /api/v1/config/flows`.
- Produces:
  - `pub fn sun_sheet(node: RwSignal<Node>, cfg: Signal<Option<FlowsConfigView>>) -> impl IntoView` — the per-node editor for `trigger.sun`.
  - `pub fn sun_summary(which: SunWhich, offset_minutes: i32) -> String` — e.g. `"At sunset"`, `"15 min after sunrise"`, `"15 min before civil dusk"`.
  - `pub fn next_three_fires(lat: f64, lon: f64, zone: Tz, which: SunWhich, offset_minutes: i32, now: DateTime<Utc>) -> Vec<DateTime<Tz>>` — three successive calls to `next_sun_fire` with each result as the following `now`.
  - `pub fn fallback_badge_visible(cfg: &FlowsConfigView) -> bool` — true iff `cfg.fallback_active`.
  - `House::flows_config: RwSignal<Option<FlowsConfigView>>`, populated once on app mount by `get_flows_config` and refreshed after every save that returns a `FlowWriteOutcome`.

- [ ] **Step 1: Write failing unit tests**

```rust
#[test]
fn preset_sunset_with_minus_30_builds_the_expected_params() {
    let p = sun_params_from_preset(SunWhich::Sunset, Some(-30));
    assert_eq!(p.which, SunWhich::Sunset);
    assert_eq!(p.offset(), -30);
}

#[test]
fn summary_line_renders_expected_string() {
    assert_eq!(sun_summary(SunWhich::Sunset, 0),      "At sunset");
    assert_eq!(sun_summary(SunWhich::Sunrise, 15),    "15 min after sunrise");
    assert_eq!(sun_summary(SunWhich::Dusk, -15),      "15 min before civil dusk");
    assert_eq!(sun_summary(SunWhich::Dawn, 0),        "At civil dawn");
}

#[test]
fn next_three_fire_times_derive_from_shared_sun_events() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 6, 0, 0).unwrap();
    let times = next_three_fires(45.75, 4.85, chrono_tz::Europe::Paris, SunWhich::Sunset, 0, now);
    assert_eq!(times.len(), 3);
    assert!(times[0] < times[1] && times[1] < times[2]);
}

#[test]
fn fallback_badge_tracks_flows_config() {
    let cfg = FlowsConfigView { latitude: None, longitude: None,
        timezone: "Europe/Paris".into(), fallback_active: true };
    assert!(fallback_badge_visible(&cfg));
    let cfg = FlowsConfigView { latitude: Some(45.75), longitude: Some(4.85),
        timezone: "Europe/Paris".into(), fallback_active: false };
    assert!(!fallback_badge_visible(&cfg));
}
```

- [ ] **Step 2: Run them to confirm they fail**

Run: `cargo test -p oikos-web`
Expected: FAIL.

- [ ] **Step 3: Build the sun sheet, the collapsed `is_dark` body, the add-node entries, the glyphs, and the fallback plumbing**

- Sun sheet (`sun.rs`): segmented control for `which` (Sunrise/Sunset/Dawn/Dusk), chips "At the event" / "Before" / "After" that reveal a numeric minutes input (clamped to 0..=720 on the chip side; a free numeric field accepts the signed range), the live `sun_summary` line, a next-three-fire-times list computed on the client with `next_three_fires` using `cfg.get().house_point()` (or Paris when `cfg` is `None`/`fallback_active`), and the "Paris fallback" badge gated on `fallback_badge_visible(&cfg)`.
- `if` with `op: is_dark` body: entity picker and value field hidden; show the single line `"Is it dark now? (sun below -6°)"`; Yes/No port labels unchanged; the "Paris fallback" badge mirrors the sun badge.
- Add-node menu: two new entries, "Sun event" inserts `TriggerSunParams { which: SunWhich::Sunset, offset_minutes: None, extra: BTreeMap::new() }`; "When it's dark" inserts an `If` node with `IfParams { op: Op::IsDark, value: None, entity: None, missing: None, extra: BTreeMap::new() }`.
- `api.rs`: `get_flows_config()` hits `GET /api/v1/config/flows`; the save path reads `FlowWriteOutcome { flow, warning }`, replaces the local flow with `flow`, and when `warning.is_some()` raises a one-shot toast with the exact warning string.
- `house.rs`: `flows_config` signal, loaded once at `House::new()` boot, refreshed after each successful save.

- [ ] **Step 4: Run tests to confirm they pass; then build the browser bundle**

Run: `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS and PASS. Also run the project's web bundle build (equivalent to `trunk build`) to confirm the browser build still assembles.

- [ ] **Step 5: Workspace gates and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-core --target wasm32-unknown-unknown && cargo check -p oikos-web --target wasm32-unknown-unknown`

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com \
    commit -am "feat(web): sun node sheet, is_dark body, Paris-fallback badge and toast"
```
