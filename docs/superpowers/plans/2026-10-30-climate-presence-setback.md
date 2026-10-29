# Climate Room Presence Setback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lower a room's scheduled temperature by a set amount after it has been empty for a while,
and bring it back once someone has stayed (feature B of the spec).

**Architecture:** A pure `presence` module in `oikos-climate` tracks whether a room is empty from
the time its sensors last detected someone; `decide` turns an empty room's block into a new
`Source::Empty` row with a lower target, and the thermometer correction (A) runs on top of it. The
hub records when each presence sensor was last `on` (from state-change events), keeps the
per-zone presence state in `ClimateRuntime`, and shows it in the read view. The learner skips
`empty` slots like `away` and `eco`. The web room panel gets a Presence card.

**Tech Stack:** Rust (workspace), serde, Leptos (web, wasm).

**Spec:** `docs/superpowers/specs/2026-10-29-climate-room-feedback-and-presence-design.md`
(§2, §3, §5, §7, §8, §9, §11; feature A is already merged, PR #105).

## Global Constraints

- Defaults: `presence: { enabled: false, sensors: [], empty_after_min: 30, back_after_min: 5, setback: 2.0 }`.
- Validation: `empty_after_min > 0`, `back_after_min > 0`, `setback` finite and `> 0`.
- Setback acts only in the Schedule row of an `active` zone, during a **block** (`ModeRules::at`
  returns `Some(index)`) whose setpoint is a temperature. Safety, overrides, holiday, room eco and
  house away win as today.
- Heat: target = `max(scheduled − setback, base, frost)` (`base` only when it is a temperature).
  Cool: target = `min(scheduled + setback, base)` (`base` only when it is a temperature). When the
  clamped target equals `scheduled`, there is no setback (source stays `schedule`).
- Any one configured sensor `on` means detected. Unavailable or unknown sensors are ignored; no
  usable sensor at all means occupied.
- After a hub restart (no previous state) the room counts as occupied until `empty_after` passes
  with no detection.
- Reason strings are English, built from inputs (engine spec §5.3), and must not change from tick
  to tick while nothing changes (no ages or clocks that move). UI text in English and French.
- The repo is public and generic: no real room, device, host or person names in code, tests or docs.
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`),
  ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- CI must pass: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test --workspace`.

## Review Focus

1. **A motion pulse between two ticks.** A sensor `on` for 40 s between two 60 s passes must still
   count as a detection, both for staying occupied and for opening the "back" window → the hub
   records detections from state-change events, test in Task 4.
2. **Periodic reports of an idle sensor.** A Zigbee sensor re-reporting `occupancy: false` (battery,
   link quality) must not count as a detection; only `on`, and the `on` → `off` edge, do → test in
   Task 4.
3. **Someone passing through, then nobody.** One detection, then nothing for `back_after`: the room
   stays empty, and a later detection opens a new window → test in Task 2.
4. **Setback start and end reset the correction.** The offset must start from 0 against the new
   target in both directions → test in Task 3.
5. **The reason line must not churn.** An empty room's decision republishes only when its outcome
   changes (no "since" clock or minutes counter in the reason) → test in Task 4.

---

### Task 1: Settings and the new source

**Files:**
- Modify: `crates/oikos-climate/src/doc.rs` (`PresenceSettings`, `ZoneSettings.presence`,
  `Source::Empty`, validation, tests)
- Modify: every exhaustive `match` on `Source` the compiler flags (at least
  `crates/oikos-web/src/climate/mod.rs` `source_word` / `history_source`, and
  `crates/oikos-web/src/i18n/climate.rs` for the new `SrcEmpty` key)

**Interfaces:**
- Produces:
  - `pub struct PresenceSettings { pub enabled: bool, pub sensors: Vec<String>, pub empty_after_min: u32, pub back_after_min: u32, pub setback: f64 }`
    with `Default` (values above), `#[serde(default)]`, `schemars` like `FeedbackSettings`, and
    `pub fn validate(&self) -> Result<(), String>` (messages `"presence.empty_after_min must be above 0"`,
    `"presence.back_after_min must be above 0"`, `"presence.setback must be above 0"`).
  - `ZoneSettings.presence: PresenceSettings`; `ZoneSettings::validate` calls
    `self.presence.validate()?` after the feedback check.
  - `Source::Empty` (serialized `"empty"`), placed after `Away`.
  - Web: `ClimateText::SrcEmpty` = `("Empty room", "Pièce vide")`; `source_word` maps
    `Source::Empty` to it and `history_source` maps `"empty"`.

- [ ] **Step 1: Write the failing tests** in `doc.rs` tests:
  - `presence_defaults_and_old_documents`: `serde_json::from_str::<ZoneSettings>("{}")` gives
    `presence == PresenceSettings::default()`, `!enabled`, `sensors.is_empty()`,
    `empty_after_min == 30`, `back_after_min == 5`, `setback == 2.0`.
  - `presence_validation`: `empty_after_min: 0`, `back_after_min: 0`, `setback: 0.0`,
    `setback: f64::NAN` each make `ZoneSettings::validate()` `Err`; the default is `Ok`.
  - `empty_source_round_trips`: `serde_json::to_string(&Source::Empty) == "\"empty\""` and back.
  - In `oikos-web/src/climate/mod.rs` tests: `history_source(Lang::En, "empty") == "Empty room"`
    and `history_source(Lang::Fr, "empty") == "Pièce vide"`.
- [ ] **Step 2:** `cargo test -p oikos-climate doc::` → FAIL (no field, no variant).
- [ ] **Step 3:** Add the type, field, variant and web mappings. Fix `ZoneSettings { … }` literals
  that list every field, if any.
- [ ] **Step 4:** `cargo test -p oikos-climate doc::`, `cargo test -p oikos-web history_source` and
  `cargo check --workspace --all-targets` → PASS.
- [ ] **Step 5: Commit** `feat(climate): room presence settings and the empty source`.

### Task 2: Pure presence tracking

**Files:**
- Create: `crates/oikos-climate/src/presence.rs` (+ `pub mod presence;` in `lib.rs`), tests inline.

**Interfaces:**
- Consumes: `PresenceSettings` (Task 1).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct PresenceState { pub last_detected: u64, pub empty_since: Option<u64>, pub arrival: Option<u64> }`
    (`last_detected`: the latest detection known, or boot / the last time no sensor was usable;
    `empty_since`: when the room became empty, `None` while occupied; `arrival`: the first detection
    while empty, opening the "back" window).
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Sensed { NoSensor, Seen(Option<u64>) }`
    (`NoSensor`: none configured or none usable; `Seen(latest)`: the latest time any usable sensor
    was detecting, `Some(now)` when one is `on` now, `None` when none has been seen `on` since boot).
  - `pub const CONFIRM_S: u64 = 120;`
  - `pub fn track(previous: Option<&PresenceState>, settings: &PresenceSettings, sensed: Sensed, now: u64) -> PresenceState`
  - `impl PresenceState { pub fn is_empty(&self) -> bool }`

`track` rules (spec §7.1):
- `previous == None` or `sensed == NoSensor` → `{ last_detected: now, empty_since: None, arrival: None }`
  (comfort first).
- `last = max(previous.last_detected, seen.unwrap_or(0))`; `fresh` = `seen > previous.last_detected`
  (a detection this state has not counted yet).
- Occupied (`empty_since == None`): empty when `now − last ≥ empty_after_min × 60` →
  `empty_since = Some(now)`, `arrival = None`.
- Empty, no `arrival`: a `fresh` detection sets `arrival = Some(seen)`.
- Empty with `arrival = a`, `end = a + back_after_min × 60`, `confirm = min(CONFIRM_S, back_after_min × 60)`:
  when `now ≥ end`, occupied (`empty_since = None`, `arrival = None`) if `last ≥ end − confirm`,
  else the window lapses (`arrival = None`). Before `end`, nothing changes.
  A radar `on` the whole time ends it at `end`; motion pulses that keep coming end it at `end`;
  a single pass at `a` does not.

- [ ] **Step 1: Write the failing tests** (defaults: 30 min empty, 5 min back; `T` = 1_000_000):
  - `boot_counts_as_occupied`: no previous, `Seen(None)` → not empty, `last_detected == T`;
    then `Seen(None)` at `T + 1799` → not empty; at `T + 1800` → empty, `empty_since == Some(T + 1800)`.
  - `detection_keeps_it_occupied`: occupied, `Seen(Some(T + 1000))` at `T + 2700` → not empty
    (only 1700 s since); at `T + 2800` → empty.
  - `no_sensor_is_occupied`: empty previous, `NoSensor` → not empty, `last_detected == now`.
  - `a_pass_does_not_end_it`: empty since `E`; `Seen(Some(E + 10))` at `E + 10` → `arrival == Some(E + 10)`;
    same `Seen` at `E + 309` → still empty; at `E + 310` (window end) → still empty, `arrival == None`.
  - `a_later_detection_opens_a_new_window`: continuing the above, `Seen(Some(E + 900))` at
    `E + 900` → `arrival == Some(E + 900)`.
  - `staying_ends_it_after_back_after`: empty since `E`, arrival `E + 10`; `Seen(Some(now))` at
    every 60 s up to `E + 300` → still empty; at `E + 310` with `Seen(Some(E + 310))` → occupied.
  - `pulses_near_the_end_count`: arrival `E + 10`, last pulse `Seen(Some(E + 200))`, at `E + 310`
    → occupied (`E + 200 ≥ E + 310 − 120`); last pulse `E + 150` → window lapses.
  - `back_after_shorter_than_confirm`: `back_after_min: 1`, arrival `A`, `Seen(Some(A))` at
    `A + 60` → occupied (confirm capped to 60 s).
- [ ] **Step 2:** `cargo test -p oikos-climate presence::` → FAIL.
- [ ] **Step 3:** Implement `track` and `is_empty` in `presence.rs`.
- [ ] **Step 4:** `cargo test -p oikos-climate presence::` → PASS.
- [ ] **Step 5: Commit** `feat(climate): pure room presence tracking`.

### Task 3: Apply the setback in the decision

**Files:**
- Modify: `crates/oikos-climate/src/decide.rs` (`ZoneInputs`, `choose`, `rules`, tests)
- Modify: `crates/oikos-climate/src/feedback.rs` (`FeedbackState::after` keeps state for
  `Source::Empty` too; test)
- Modify: the `ZoneInputs { … }` literals (hub `inputs.rs` `borrowed`, decide tests)

**Interfaces:**
- Consumes: `PresenceSettings`, `Source::Empty` (Task 1).
- Produces: `ZoneInputs.room_empty: bool` — the room counts as empty now (the hub sets it from
  `PresenceState::is_empty`, only when `settings.presence.enabled`).

In `choose`, after the eco/away branch, when `inp.settings.presence.enabled && inp.room_empty`,
`block.is_some()` and `scheduled` is `Temp(s)`: compute the clamped target per Global Constraints;
when it differs from `s` by more than 0.01, return
`Choice { source: Source::Empty, setpoint: Some(Temp(target)), reason }` with
- heat, not clamped: `"No one in the room for {empty_after_min} min: {target:.1} °C ({s:.1} − {setback:.1})"`;
- cool, not clamped: `"… : {target:.1} °C ({s:.1} + {setback:.1})"`;
- clamped by base or frost: `"No one in the room for {n} min: {target:.1} °C (base)"` /
  `"(frost)"`.

In `rules`, run `apply_correction` for `Source::Schedule | Source::Empty` with a temperature
setpoint (the correction works against the setback target, spec §7.2).

- [ ] **Step 1: Write the failing tests** in `decide.rs` tests (zone active, heat, base 15 °C,
  block 19 °C at `local_now`, presence enabled):
  - `empty_room_lowers_a_block`: `room_empty: true` → source `Empty`, `setpoint == Temp(17.0)`,
    entity setpoint 17.0, reason `"No one in the room for 30 min: 17.0 °C (19.0 − 2.0)"`.
  - `empty_room_keeps_the_base`: same inputs at a time outside the block → source `Schedule`,
    setpoint 15.0.
  - `setback_stops_at_the_base`: base 18.0 → setpoint 18.0, reason ends `"18.0 °C (base)"`.
  - `setback_is_off_by_default`: `room_empty: true`, presence disabled → source `Schedule`, 19.0.
  - `away_wins_over_setback`: `home: false`, `room_empty: true` → source `Away`.
  - `cool_setback_raises`: cool rules base 28, block 25, setback 2 → setpoint 27.0, reason
    `"… 27.0 °C (25.0 + 2.0)"`; base 26 → 26.0 `"(base)"`.
  - `correction_runs_against_the_setback_target`: feedback enabled, thermometer 19.0 fresh,
    `room_empty: true` → source `Empty`, setpoint 17.0, entity 16.5, `adjust == Some(-0.5)`.
  - In `feedback.rs`: `after_keeps_state_for_empty`: a `Source::Empty` decision at 17.0 → `Some`
    with `target == 17.0`; a previous state at target 19.0 → fresh (`since == now`), so the
    setback starting and ending both reset the offset.
- [ ] **Step 2:** `cargo test -p oikos-climate decide:: feedback::` → FAIL.
- [ ] **Step 3:** Add `room_empty`, the setback branch in `choose`, the source match in `rules`
  and in `FeedbackState::after`.
- [ ] **Step 4:** `cargo test -p oikos-climate` → PASS.
- [ ] **Step 5: Commit** `feat(climate): an empty room's block is set back`.

### Task 4: Hub detection, state and inputs

**Files:**
- Modify: `crates/oikos-os/src/climate/mod.rs` (`ClimateRuntime.presence: HashMap<String, PresenceState>`,
  `ClimateRuntime.detected_at: HashMap<String, u64>`, `ClimateRuntime.sensor_on: HashMap<String, bool>`,
  `ClimateRuntime.presence_sensors: HashSet<String>` — the enabled zones' sensors, rebuilt in
  `refresh_house`)
- Modify: `crates/oikos-os/src/climate/inputs.rs` (`OwnedZoneInputs.presence: Option<PresenceState>`
  — the state *after* this tick, computed with `track`; `borrowed` sets `room_empty`)
- Modify: `crates/oikos-os/src/climate/engine.rs` (`refresh_house` adds the enabled zones' presence
  sensors to `watched` and `presence_sensors`; `observe_climate`, right after `note_availability`,
  calls `note_detection` when `id` is in `presence_sensors` — the loop already calls it for every
  watched `StateChanged`; after `decide`, store `inp.presence` in `rt.presence` or remove the entry
  when `None`)
- Modify: `crates/oikos-os/src/climate/hub.rs` (`set_climate_zone`: drop `rt.presence[zone]` when
  `settings.presence != current.presence`)
- Test: `crates/oikos-os/tests/climate_hub.rs`

**Interfaces:**
- Consumes: `track`, `Sensed`, `PresenceState` (Task 2); `ZoneInputs.room_empty` (Task 3).
- Produces:
  - `pub(crate) fn note_detection(rt: &mut ClimateRuntime, id: &str, on: Option<bool>, now: u64)`
    in `inputs.rs`: `on == Some(true)`, or the sensor was `on` before and now is not (the pulse
    ended now), sets `detected_at[id] = now`; `Some(_)` updates `sensor_on`; `None` (unusable)
    changes nothing.
  - `fn detecting(state: &EntityState) -> Option<bool>` in `inputs.rs`: `None` when offline or not
    an on/off value, else the value.
  - `ClimateRuntime.presence` for Task 5's view.

`zone_inputs_owned` (only when `settings.presence.enabled`): for each configured sensor read
`detecting`; call `note_detection` for each; `Sensed::NoSensor` when none is usable, else
`Sensed::Seen(max detected_at over the usable sensors)`; `presence = Some(track(rt.presence.get(zone), …, now))`.
Disabled → `presence = None`.

- [ ] **Step 1: Write the failing tests** in `climate_hub.rs` (a heat zone with a 19 °C block over a
  15 °C base all day, presence enabled with one occupancy sensor fed from the
  `binary_sensor__0x00158d00044c4d4c__occupancy.json` fixture, placed in the zone's area; a helper
  `feed_occupancy(hub, area, on) -> String` publishes `{"occupancy": on}` on its state topic):
  - `an_empty_room_is_set_back_after_30_min`: sensor off from the start; pass at 0 → device 19.0;
    pass at 1800 → device 17.0, decision source `Empty`.
  - `a_pulse_between_passes_counts`: sensor off; at 1700 the sensor goes on then, at 1740, off
    (each ingest followed by `rig.hub.observe_climate(&sensor, t0 + t)`, as the loop does on
    `StateChanged`); pass at 1800 → still 19.0; pass at 3539 → 19.0; pass at 3540 → 17.0 (the
    pulse ended at 1740).
  - `idle_reports_are_not_detections`: sensor off; re-ingest `{"occupancy": false}` every 600 s;
    pass at 1800 → 17.0.
  - `staying_brings_it_back`: empty at 1800 (17.0); sensor on at 1900 and stays on; pass at 2140
    → 17.0; pass at 2200 → 19.0, source `Schedule`.
  - `empty_decision_is_stable`: passes at 1800 and 1860 with the room still empty → only the first
    publishes a decision (`decisions_published`).
  - `presence_settings_change_resets_state`: after the room is empty, `set_climate_zone` with
    `setback: 3.0` → `climate_runtime().presence` has no entry for the zone.
  - `unavailable_sensor_keeps_the_room_occupied`: the sensor goes unavailable (bridge offline
    payload on its availability topic) at 0 → pass at 3600 → 19.0.
- [ ] **Step 2:** `cargo test -p oikos-os --test climate_hub presence` → FAIL.
- [ ] **Step 3:** Implement the runtime maps, `detecting`, `note_detection`, the inputs, the
  watch list, the event hook and the store after `decide`.
- [ ] **Step 4:** `cargo test -p oikos-os --test climate_hub` and `cargo test -p oikos-os climate::` → PASS.
- [ ] **Step 5: Commit** `feat(climate): hub tracks room presence and sets empty rooms back`.

### Task 5: Learner, read view

**Files:**
- Modify: `crates/oikos-climate/src/learn.rs` (`Source::Empty` joins the excluded sources in
  `device_slot`)
- Modify: `crates/oikos-climate/tests/learn_scenarios.rs` (test)
- Modify: `crates/oikos-climate/src/view.rs` (`ZoneView.presence: Option<PresenceView>`,
  `pub struct PresenceView { pub empty_since: Option<u64> }`, serde default, schemars like
  `ThermometerReading`)
- Modify: `crates/oikos-os/src/climate/hub.rs` (fill `zone.presence` from `rt.presence` when
  `settings.presence.enabled`: `Some(PresenceView { empty_since })`, else `None`)

**Interfaces:**
- Consumes: `ClimateRuntime.presence` (Task 4).
- Produces: `ZoneView.presence` for the web (Task 6).

- [ ] **Step 1: Write the failing tests:**
  - `learn_scenarios.rs` `empty_slots_do_not_change_what_is_learned`: 14 days, device target
    19 from 09:00 to 20:30 except 17.0 from 13:00 to 15:00, engine commands `Schedule` 19.0 at
    09:00, `Empty` 17.0 at 13:00, `Schedule` 19.0 at 15:00 → learned block 09:00–20:30 at 19.0
    (no 17.0 block).
  - `view.rs` `zone_view_presence_is_optional`: a `ZoneView` JSON without `presence` parses with
    `presence == None`.
  - `climate_hub.rs` `view_shows_room_presence`: after the room is empty at 1800,
    `rig.zone().presence == Some(PresenceView { empty_since: Some(t0 + 1800) })`; presence
    disabled → `None`.
- [ ] **Step 2:** run the three → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p oikos-climate` and `cargo test -p oikos-os --test climate_hub` → PASS.
- [ ] **Step 5: Commit** `feat(climate): learner skips empty slots; view shows room presence`.

### Task 6: Web Presence card, docs

**Files:**
- Create: `crates/oikos-web/src/climate/presence_card.rs`
  (`#[component] pub fn PresenceCard(zone: Signal<ZoneView>, on_changed: Callback<()>)`), and
  `mod presence_card;` in `climate/mod.rs`
- Modify: `crates/oikos-web/src/climate/room_panel.rs` (render it after `CorrectionCard`)
- Modify: `crates/oikos-web/src/i18n/climate.rs` (new keys, en + fr, added to the key list test)
- Modify: `crates/oikos-web/src/entity_picker/needs.rs` (`pub fn is_room_presence(e) -> bool`:
  binary sensor with class `presence`, `occupancy` or `motion`; test)
- Modify: `docs/users/climate.md` ("Room presence" section after "Thermometer correction")

**Interfaces:**
- Consumes: `PresenceSettings` (Task 1), `ZoneView.presence` (Task 5), `NumField`, `Unit`,
  `EntityPickListField`, `api::put_zone` (existing, as `CorrectionCard` uses them).

The card mirrors `CorrectionCard` (same `save` closure shape): a switch **Room presence / Présence
dans la pièce**; when on, a status line, an `EntityPickListField` **Presence sensors / Détecteurs de
présence** with `pool` = `is_room_presence` and the zone's area (`e.area == zone.zone`), then
`NumField`s **Empty after / Vide après** (min), **Back after / Occupée de nouveau après** (min),
**Lower by / Baisser de** (°C). The status line: `presence.empty_since` `Some(t)` →
**Empty since {HH:MM} / Vide depuis {HH:MM}** in the house time zone (as the panel's other clocks);
`None` → **Occupied / Occupée**. No sensors picked → hint **Pick a presence sensor: without one the
room counts as occupied. / Choisissez un détecteur : sans lui, la pièce compte comme occupée.**

- [ ] **Step 1: Write the failing tests:** `needs.rs` `room_presence_takes_binary_sensors_only`
  (occupancy/motion/presence binary sensors yes; a tracker, an alarm panel, a `door` binary sensor
  no); `i18n/climate.rs`: the existing every-key-has-text test covers the new keys once they are in
  its list.
- [ ] **Step 2:** `cargo test -p oikos-web room_presence` → FAIL.
- [ ] **Step 3:** Implement the predicate, the card, the i18n keys and the docs section (defaults,
  what "empty" and "back" mean, that safety/eco/away/overrides win, that it shows as **Empty room**
  in Recent decisions and that the correction runs on top).
- [ ] **Step 4:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test -p oikos-web` → PASS.
- [ ] **Step 5: Commit** `feat(web): room presence card`.

### Finish

- [ ] `cargo test --workspace` (DB tests run in CI), push, open one PR against `main` with the
  plan commit and the six commits; body: defaults, what the user sees, after-merge steps (rebuild
  hub and web on the live hub; pick the room's presence sensors and switch it on; watch **Recent
  decisions** and the Energy view).
