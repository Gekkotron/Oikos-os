# Climate Room-Thermometer Correction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nudge a zone's climate devices in steps, from the room thermometer, so the room settles
near its target instead of overshooting (feature A of the spec; presence is a later plan).

**Architecture:** A pure `feedback` module in `oikos-climate` computes the offset from the previous
per-zone state, the room thermometer and the settings; `decide` applies it in the Schedule row. The
hub keeps the state in `ClimateRuntime`, passes the room thermometer's own reading, and stores the
offset with each decision so the learner learns the target, not the corrected value. The web room
panel gets a settings card.

**Tech Stack:** Rust (workspace), serde, sqlx + TimescaleDB migrations, Leptos (web, wasm).

**Spec:** `docs/superpowers/specs/2026-10-29-climate-room-feedback-and-presence-design.md`
(§1–§6, §8, §9, §11; §7 presence is out of this plan).

## Global Constraints

- Defaults: `enabled: false`, `band: 0.5`, `step: 0.5`, `interval_min: 15`, `warmup_min: 60`,
  `max_below: 3.0`, `max_above: 0.5`, `quiet_limit_min: 360`.
- Feature acts only in the **Schedule** row of an **active** zone with a temperature setpoint;
  never under a whole-zone override, safety, holiday, eco or away.
- Only the room thermometer (`settings.sensor`, else the area thermometer); never device readings.
- The safety staleness (`stale_after_min`) is unchanged.
- Reason strings are English, built from inputs (engine spec §5.3); UI text in English and French.
- The repo is public and generic: no real room, device, host or person names in code, tests or docs.
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`),
  ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- CI must pass: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test --workspace`.

## Review Focus

1. **Corrected commands after a hub restart.** The ledger rebuilt from the retained decision must
   treat corrected setpoints as the engine's own, not hand changes → test in Task 4.
2. **Oscillation at the band edge.** A reading sitting exactly at `target + band` must not step
   (strict `>`), and readings arriving every few seconds must not step faster than `interval` →
   tests in Task 2.
3. **A device with a coarser step** (1 °C) next to a 0.5 °C one: the zone steps by 1 °C →
   test in Task 3.
4. **The thermometer goes silent mid-correction** (> `quiet_limit`): offset back to 0, plain
   schedule sent, one reason line → test in Task 2 and Task 3.
5. **Learning after weeks of corrections:** the learner sees the target, not the corrected value →
   test in Task 5.

---

### Task 1: Settings and decision fields

**Files:**
- Modify: `crates/oikos-climate/src/doc.rs` (`ZoneSettings`, `Decision`, validation, tests)
- Modify: every `Decision { … }` literal in the workspace (15) and `ZoneSettings` literals, to add the new fields

**Interfaces:**
- Produces:
  - `pub struct FeedbackSettings { pub enabled: bool, pub band: f64, pub step: f64, pub interval_min: u32, pub warmup_min: u32, pub max_below: f64, pub max_above: f64, pub quiet_limit_min: u32 }`
    with `Default` (values above), `#[serde(default)]`, `schemars` like its neighbours, and
    `pub fn validate(&self) -> Result<(), String>`.
  - `ZoneSettings.feedback: FeedbackSettings` (`#[serde(default)]` already on the struct).
  - `Decision.adjust: Option<f64>`: `#[serde(default, skip_serializing_if = "Option::is_none")]`;
    the offset applied to the zone's unheld devices, `None` when 0 or inactive. Included in
    `same_outcome`.

- [ ] **Step 1: Write the failing tests** in `doc.rs` tests:
  - `feedback_defaults_and_old_documents`: `serde_json::from_str::<ZoneSettings>("{}")` gives
    `feedback == FeedbackSettings::default()` and `!feedback.enabled`, `band == 0.5`,
    `quiet_limit_min == 360`.
  - `feedback_validation`: `step: 0.0`, `band: 0.0`, `interval_min: 0`, `quiet_limit_min: 0`,
    `max_below: -1.0`, `max_above: -0.5` each make `ZoneSettings::validate()` return `Err`;
    the default is `Ok`.
  - `decision_adjust_is_optional`: a decision JSON without `adjust` parses with `adjust == None`;
    serializing a decision with `adjust: None` has no `adjust` key.
- [ ] **Step 2:** `cargo test -p oikos-climate doc::` → FAIL (no field).
- [ ] **Step 3:** Add the types and fields; `ZoneSettings::validate` calls `self.feedback.validate()`
  before the rules. Fix the struct literals (`adjust: None`).
- [ ] **Step 4:** `cargo test -p oikos-climate` and `cargo check --workspace --all-targets` → PASS.
- [ ] **Step 5: Commit** `feat(climate): thermometer correction settings and decision adjust`.

### Task 2: Pure correction

**Files:**
- Create: `crates/oikos-climate/src/feedback.rs` (+ `pub mod feedback;` in `lib.rs`), tests inline.

**Interfaces:**
- Consumes: `FeedbackSettings`, `Decision.adjust` (Task 1); `ReadingSource` (`schedule.rs`); `Mode`, `Source`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct FeedbackState { pub target: f64, pub offset: f64, pub since: u64, pub last_step_at: Option<u64> }`
    (`since`: when this target began; `last_step_at`: the last offset change).
  - `pub struct CorrectionInput<'a> { pub settings: &'a FeedbackSettings, pub mode: Mode, pub target: f64, pub base: Option<f64>, pub frost: f64, pub thermometer: Option<ReadingSource>, pub step: f64, pub frozen: bool, pub previous: Option<&'a FeedbackState>, pub now: u64 }`
  - `pub struct Correction { pub offset: f64, pub note: Option<String> }`
  - `pub fn correct(input: &CorrectionInput) -> Correction`
  - `impl FeedbackState { pub fn after(previous: Option<&FeedbackState>, decision: &Decision, enabled: bool, now: u64) -> Option<FeedbackState> }`

`correct` rules (spec §6.1–§6.3):
- `previous` counts only if `previous.target` equals `target` (within 0.01); otherwise start fresh:
  offset 0, `since = now`, no last step.
- No thermometer → offset 0, note `"no room thermometer"`. Thermometer older than
  `quiet_limit_min` → offset 0, note `"room thermometer silent for {h} h: no correction"`.
- `ready` = no last step, or `now − last_step_at ≥ interval_min × 60`.
  `warm` = `now − since ≥ warmup_min × 60`.
- Heat: `r > target + band` and ready → −step; `r < target − band` and ready and warm → +step.
  Cool: `r > target + band` and ready and warm → −step; `r < target − band` and ready → +step.
  `frozen` → no step. Comparisons are strict.
- Clamp the offset: heat lowest `max(target − max_below, base, frost) − target` (never above 0),
  highest `max_above`; cool lowest `−max_above`, highest `min(max_below, base − target)` when
  `base` is set (never below 0), else `max_below`.
- Note when the room is outside the band or the offset is not 0:
  `"room {r:.1} °C{age} {above|below|near} {target ± band:.1} °C: AC {target+offset:.1} °C ({offset:+.1})"`,
  where `{age}` is `" ({n} h ago)"` / `" ({n} min ago)"` when the reading is at least 15 min old.

`after`: `Some` only when `enabled`, `decision.source == Source::Schedule` and `decision.setpoint`
is a temperature `t`. Same target as `previous` → keep `since`; `last_step_at = Some(now)` when the
offset (`decision.adjust.unwrap_or(0.0)`) differs from `previous.offset`, else keep it. New
target → `since = now`, `last_step_at = None`.

- [ ] **Step 1: Write the failing tests** (heat target 19.0, base 15.0, frost 7.0, step 0.5, defaults):
  - `overshoot_steps_down_once_per_interval`: r 21.6, no previous → offset −0.5; previous
    `{offset −0.5, last_step_at: now−600}` → still −0.5; `now−900` → −1.0.
  - `stops_at_max_below_and_base`: previous offset −3.0, r 22 → −3.0; with base 17.0 →
    clamped to −2.0.
  - `band_edge_does_not_step`: r 19.5 and r 18.5 → offset stays 0 and note is `None`.
  - `undershoot_waits_for_warmup_and_max_above`: r 18.0, `since` now−1800 → 0; now−3600 → +0.5;
    previous +0.5 ready → stays +0.5.
  - `new_target_resets`: previous `{target 19.0, offset −1.0, last_step_at: now−60}`, target
    17.0, r 21.6 → offset −0.5 (fresh start, one step at once; not −1.5, not held by the interval).
  - `quiet_thermometer`: age 3 h → steps; age 6 h + 1 s → offset 0 and note contains `"silent for 6 h"`.
  - `no_thermometer_note`: `thermometer: None` → note `Some("no room thermometer")`.
  - `frozen_keeps_offset`: frozen, previous −0.5 ready, r 22 → −0.5.
  - `cool_mirrors`: cool target 26, base 28, r 27 ready+warm → −0.5; r 24.5 → +0.5; previous
    +2.0 → clamped to +2.0 (base − target); previous −0.5 → stays −0.5 (−max_above).
  - `note_text`: r 20.6, age 7200 s, offset −0.5 →
    `"room 20.6 °C (2 h ago) above 19.5 °C: AC 18.5 °C (-0.5)"`.
  - `after_tracks_since_and_steps`: same target, adjust changes → `last_step_at == Some(now)`,
    `since` kept; source `Override` → `None`; `enabled false` → `None`.
- [ ] **Step 2:** `cargo test -p oikos-climate feedback::` → FAIL.
- [ ] **Step 3:** Implement `correct` and `FeedbackState::after` in `feedback.rs`.
- [ ] **Step 4:** `cargo test -p oikos-climate` → PASS.
- [ ] **Step 5: Commit** `feat(climate): pure room-thermometer correction`.

### Task 3: Apply it in the decision

**Files:**
- Modify: `crates/oikos-climate/src/decide.rs` (`ZoneInputs`, `rules`, tests)
- Modify: the 5 `ZoneInputs { … }` literals (hub `inputs.rs`, tests)

**Interfaces:**
- Consumes: `correct`, `CorrectionInput`, `FeedbackState` (Task 2).
- Produces: `ZoneInputs.thermometer: Option<ReadingSource>` (the room thermometer's own value and
  age, whatever `reading` says) and `ZoneInputs.feedback: Option<&'a FeedbackState>` (previous
  state). `Decision.adjust` set by `rules`.

In `rules`, when `inp.settings.feedback.enabled`, the choice is `Source::Schedule`, the mode is set
and the setpoint is `Temp(target)`:
- unheld devices = devices without a live per-device override; `frozen` = none unheld;
- `step` = max of `settings.feedback.step` and the unheld devices' `temp_step` (0.5 when absent);
- `base` = the mode's rules base when it is a temperature; `frost` = `house.frost`;
- `d.entities = targets(devices, Temp(target + offset), mode)`; `d.setpoint` stays `Temp(target)`;
  `d.adjust = (offset != 0).then_some(offset)`; append `"; " + note` to the reason when there is one;
- `hold_devices` runs after, unchanged, so held devices keep their values.

- [ ] **Step 1: Write the failing tests** in `decide.rs` tests (zone active, heat, block 19 °C):
  - `correction_lowers_unheld_devices`: thermometer 21.6 fresh, enabled → source `Schedule`,
    `setpoint == Temp(19.0)`, entity setpoint 18.5, `adjust == Some(-0.5)`, reason contains
    `"above 19.5 °C: AC 18.5 °C"`.
  - `correction_off_by_default`: same inputs, disabled → entity 19.0, `adjust == None`.
  - `no_correction_under_zone_override`: live whole-zone override 21 → entity 21.0, `adjust None`.
  - `held_device_keeps_its_value`: two devices, one held at 20 → held 20.0, other 18.5.
  - `all_held_freezes`: both held, previous offset −0.5 ready, r 22 → `adjust == Some(-0.5)`,
    devices at their held values.
  - `coarse_device_sets_the_step`: devices with `temp_step` 0.5 and 1.0 → offset −1.0, both at 18.0.
  - `silent_thermometer_sends_the_schedule`: thermometer age 7 h → entity 19.0, reason contains
    `"silent for"`.
  - `stale_reading_but_quiet_thermometer_still_corrects`: `reading` Fresh from devices with
    `sensor_stale: true`, thermometer age 2 h r 21 → entity 18.5.
- [ ] **Step 2:** `cargo test -p oikos-climate decide::` → FAIL.
- [ ] **Step 3:** Add the fields and the step in `rules` (a small helper beside `hold_devices`).
- [ ] **Step 4:** `cargo test -p oikos-climate` → PASS.
- [ ] **Step 5: Commit** `feat(climate): schedule row applies the thermometer correction`.

### Task 4: Hub state and inputs

**Files:**
- Modify: `crates/oikos-os/src/climate/mod.rs` (`ClimateRuntime.feedback: HashMap<String, FeedbackState>`)
- Modify: `crates/oikos-os/src/climate/inputs.rs` (`OwnedZoneInputs.thermometer`, `.feedback`;
  fill `thermometer` from the `sensor` that `reading_sources` returns)
- Modify: `crates/oikos-os/src/climate/engine.rs` (`evaluate_zone`: after `decide`, store
  `FeedbackState::after(previous, &decision, settings.feedback.enabled, now)`, removing the entry
  on `None`)
- Modify: `crates/oikos-os/src/climate/hub.rs` (`set_climate_zone`: drop the zone's feedback state)

**Interfaces:**
- Consumes: `ZoneInputs.thermometer`, `ZoneInputs.feedback` (Task 3), `FeedbackState::after` (Task 2).
- Produces: nothing new for later tasks beyond `ClimateRuntime.feedback`.

- [ ] **Step 1: Write the failing tests** in the engine's existing hub tests (same harness as the
  hand-change tests):
  - `correction_steps_down_and_is_not_a_hand_change`: active zone, enabled, room thermometer 21.6,
    block 19 → device commanded 18.5; echo it; next pass makes no override and keeps 18.5.
  - `correction_survives_restart_without_hand_change`: rebuild the runtime from the retained
    decision (as on boot) with the device at 18.5 → no hand-change override is recorded.
  - `settings_change_resets_correction`: after a step, `set_climate_zone` with a new `band` →
    `climate_runtime().feedback` has no entry for the zone.
  - `real_hand_change_still_holds`: during a correction the device goes to 21 by hand → override
    recorded, device left at 21.
- [ ] **Step 2:** `cargo test -p oikos-os climate::` → FAIL.
- [ ] **Step 3:** Implement the runtime map, inputs and the store after `decide`.
- [ ] **Step 4:** `cargo test -p oikos-os climate::` → PASS.
- [ ] **Step 5: Commit** `feat(climate): hub keeps the correction state per zone`.

### Task 5: History and learner

**Files:**
- Create: `crates/oikos-os/migrations/0005_climate_adjust.sql` —
  `ALTER TABLE climate_decision ADD COLUMN adjust double precision;`
- Modify: `crates/oikos-os/src/history/climate.rs` (`ClimateDecisionRow.adjust: Option<f64>`,
  insert/select it)
- Modify: `crates/oikos-os/src/climate/engine.rs` (`decision_rows`: per device, `setpoint` = the
  decision's target and `adjust` = commanded − target for unheld devices with a non-zero
  difference; held devices keep their own value and `None`)
- Modify: `crates/oikos-climate/src/learn.rs` (`EngineCommand.adjust: Option<f64>`; in
  `device_slot`, an engine command matches the device when `setpoint + adjust` is within
  `ENGINE_TOL` of the sample, and the slot's value is then the command's `setpoint`)
- Modify: `crates/oikos-os/src/climate/learning.rs` (`commands_from_rows` copies `adjust`)
- Modify: `crates/oikos-climate/src/view.rs` (`DecisionRow.adjust: Option<f64>`, serde default)
  and where `DecisionRow` is built from rows

**Interfaces:**
- Consumes: `Decision.adjust` (Task 1).
- Produces: `ClimateDecisionRow.adjust`, `EngineCommand.adjust`, `DecisionRow.adjust`.

- [ ] **Step 1: Write the failing tests:**
  - `learn_scenarios.rs` `corrected_slots_count_as_the_target`: 14 days, device target 19 from
    09:00 to 20:30 but 18.0 from 12:00 to 16:00, engine commands `Schedule` 19.0 at 09:00 and
    `Schedule` setpoint 19.0 `adjust Some(-1.0)` at 12:00 and 19.0 at 16:00 → learned block
    09:00–20:30 at 19.0 (no 18.0 block).
  - `engine.rs` `decision_rows_store_target_and_adjust`: decision setpoint 19, entities
    `a` 18.5 unheld, `b` 21 held → rows `(a, 19.0, Some(-0.5))`, `(b, 21.0, None)`.
- [ ] **Step 2:** `cargo test -p oikos-climate --test learn_scenarios` and
  `cargo test -p oikos-os decision_rows` → FAIL.
- [ ] **Step 3:** Implement the column, the row mapping and the learner match.
- [ ] **Step 4:** `cargo test --workspace` → PASS (with a history database when the harness
  provides one; the migration is checked by the existing migration test if present).
- [ ] **Step 5: Commit** `feat(climate): decisions keep the correction; learner learns the target`.

### Task 6: Read view, web card, docs

**Files:**
- Modify: `crates/oikos-climate/src/view.rs` (`ZoneView.thermometer: Option<ThermometerReading>`,
  `pub struct ThermometerReading { pub entity: String, pub value: f64, pub age_s: u64 }`, and
  `ZoneView.correction_step_at: Option<u64>` from `ClimateRuntime.feedback`'s `last_step_at`;
  both serde default) and its builder in the hub (wherever `ZoneView` is filled). The offset itself
  is `decision.adjust`.
- Modify: `crates/oikos-web/src/climate/settings_sheet.rs` (make `NumField` and `Unit` `pub(crate)`)
- Create: `crates/oikos-web/src/climate/correction_card.rs` (`#[component] pub fn CorrectionCard(zone: Signal<ZoneView>, on_changed: Callback<()>)`)
- Modify: `crates/oikos-web/src/climate/room_panel.rs` (render the card in `climate-panel__lower`
  after the learn card; show `"{value} · {age}"` under the reading when `thermometer` is older
  than 15 min, using `age_text`)
- Modify: `crates/oikos-web/src/i18n/climate.rs` (new `ClimateText` keys, en + fr)
- Modify: `crates/oikos-web/styles/main.css` (only if the card needs layout beyond `climate-field`)
- Modify: `docs/users/climate.md` (a "Thermometer correction" section)

**Interfaces:**
- Consumes: `FeedbackSettings` (Task 1), `api::put_zone` (existing), `NumField`.
- Produces: user-facing settings.

The card: a switch **Thermometer correction / Correction par le thermomètre**, then (when on)
`NumField`s for step (°C), band (°C), interval (min), warm-up (min), max below (°C), max above (°C),
quiet limit (h, stored as minutes). Each change sends the zone settings with `api::put_zone`, as
`set_sensor` does. Without a room thermometer (`zone.room_sensor` and `settings.sensor` both none)
the card shows **No room thermometer: pick one above / Pas de thermomètre : choisissez-en un
ci-dessus**.

- [ ] **Step 1: Write the failing tests:** `view.rs` `zone_view_thermometer_is_optional`
  (old JSON without `thermometer` parses); in the web crate, a pure helper
  `fn quiet_hours(min: u32) -> f64` / `fn quiet_minutes(h: f64) -> u32` with
  `quiet_minutes(6.0) == 360` and `quiet_hours(90) == 1.5`.
- [ ] **Step 2:** `cargo test -p oikos-climate view::` and `cargo test -p oikos-web quiet_` → FAIL.
- [ ] **Step 3:** Implement the view field, the card, i18n keys and the docs section.
- [ ] **Step 4:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test --workspace` → PASS.
  Then run the web locally (`trunk serve`, port 3000) against the hub and check the card in both
  themes and languages at 1440 px and 390 px.
- [ ] **Step 5: Commit** `feat(web): thermometer correction card and reading age`.

### Finish

- [ ] Open one PR against `main` with all six commits (design doc commit included), body listing
  the defaults and the after-merge steps: rebuild hub and web on the live hub; switch the
  correction on for the overshooting room and watch **Recent decisions** and the Energy view.
