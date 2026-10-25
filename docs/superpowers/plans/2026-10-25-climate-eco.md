# Climate eco Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One house-wide eco setting (setpoints + fan action) applied to a room until a date, or to every room while nobody is home (alarm armed away), never using more energy than the schedule.

**Architecture:** The pure crate `oikos-climate` gains the documents (`EcoSettings`, `RoomEco`, `FanRestore`), an eco row in `decide`, and a pure `eco::fan_plan` that turns (previous memory, new decision, devices, actions) into fan commands and the new memory. The hub stores the room eco document, feeds it into the inputs, reads alarm panels as presence, and sends the fan commands. The web adds the settings group, the room control and a marker.

**Tech Stack:** Rust (workspace crates `oikos-climate`, `oikos-os`, `oikos-web` with Leptos), MQTT retained JSON, axum HTTP, rmcp MCP.

**Spec:** `docs/superpowers/specs/2026-10-25-climate-eco-design.md` (builds on `docs/superpowers/specs/2026-10-22-climate-engine-design.md`).

## Global Constraints

- Generic code: no room names, family names, vendor fan values (a device's own mode names) or entity ids in code, tests, docs or commit messages. Test zones keep the existing neutral ids (`living`, `z`); new tests use `bedroom` when a second zone is needed.
- New defaults, the only new numbers in code: `eco.heat` 16.0, `eco.cool` 28.0, `eco.actions` empty.
- Validation: `eco.heat >= frost`; `eco.cool <= overheat`; at most one `fan_mode` action; a `fan_mode` value is not empty.
- No climate topic ends in `/config`; the new document is `<prefix>/zone/<zone>/eco`, QoS 1, retained.
- Safety rows stay above everything; eco sits below overrides and holiday.
- Eco never raises use: heat → lower of eco and schedule, cool → higher, schedule `off` stays `off`, no rules for the mode → idle.
- All new web text in English and French through `crates/oikos-web/src/i18n/climate.rs`.
- Before each build: check `df -h ~` and `du -sh ~/dev-scratch/cargo-target`; over 5 GB, `cargo clean`. Run per-crate tests (`-p`), not the whole workspace, when disk is below 5 GB free.
- Commits as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.

## Review Focus

1. **Alarm triggered or entry delay while away** → the house stays in eco (last definite answer); pinned in Task 5 (`alarm_transient_states_keep_last_answer`).
2. **A failed fan command** → retried at the next pass, not "remembered" as done, so the fan is not left at the eco value with nothing to restore; pinned in Task 6 (`failed_fan_command_is_retried`).
3. **Room eco set, then the hub restarts mid-eco** → eco and fan memory come back from the retained documents and the fan is restored at the end; pinned in Task 6 (`fan_restored_after_restart`).
4. **Room eco expires between passes** → the next decision is the schedule even before the document is deleted, and the document is deleted; pinned in Task 2 (`expired_room_eco_is_ignored`) and Task 4 (`expired_room_eco_is_deleted`).
5. **Eco while a hand change holds one unit** → that unit keeps its hold and gets no fan command; pinned in Task 3 (`held_or_off_units_get_no_fan_command`).

---

### Task 1: Eco documents in the pure crate

**Files:**
- Modify: `crates/oikos-climate/src/doc.rs` (HouseSettings, ZoneSettings, Source, Decision; remove `AwaySetpoints`)
- Modify: `crates/oikos-climate/src/decide.rs` (`DeviceCaps`, `Device`; the away row reads `house.eco` for now; test helpers)
- Modify: `crates/oikos-climate/src/topics.rs` (`ZoneDoc::Eco`)
- Modify: `crates/oikos-climate/src/view.rs` (`ZoneView.eco`, `RoomEcoRequest`)
- Modify: `crates/oikos-climate/src/learn.rs` (`Source::Eco` is no data)
- Modify: `crates/oikos-os/src/climate/inputs.rs`, `crates/oikos-os/src/climate/hub.rs` (compile fallout only: fan fields on `Device`, `eco: None` in `zone_view`)

**Interfaces:**
- Produces:
  - `pub struct EcoSettings { pub heat: f64, pub cool: f64, pub actions: Vec<EcoAction> }`, `#[serde(default)]`, `Default` = 16.0 / 28.0 / empty; field `HouseSettings.eco: EcoSettings`.
  - `#[serde(rename_all = "snake_case")] pub enum EcoAction { FanMode(String) }` (externally tagged: `{ "fan_mode": "low" }`; an unknown key fails to parse).
  - `impl EcoSettings { pub fn fan_mode(&self) -> Option<&str>; pub fn setpoint(&self, mode: Mode) -> f64 }`.
  - `pub struct RoomEco { pub until: u64, pub set_at: u64 }` (the `…/eco` document).
  - `pub struct FanRestore { pub was: String, pub set: String }`; field `Decision.eco_restore: BTreeMap<String, FanRestore>` with `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`, included in `same_outcome`. (A top-level map instead of the spec's per-entity field: a unit can be remembered while safety or an override leaves it out of `entities`.)
  - `Source::Eco` (serde `"eco"`).
  - `DeviceCaps.fan_modes: Vec<String>`, `Device.fan_mode: Option<String>`.
  - `ZoneDoc::Eco` with segment `"eco"`.
  - `ZoneView.eco: Option<RoomEco>` (`#[serde(default)]`).
  - `view::RoomEcoRequest { pub until: u64 }` (`deny_unknown_fields`) with `fn into_room_eco(self, now: u64) -> Result<RoomEco, String>` refusing `until <= now` with `"until is already past"`.

- [ ] **Step 1: Write the failing tests** in `doc.rs` and `topics.rs` tests:

```rust
#[test]
fn eco_defaults_and_shape() {
    let h: HouseSettings = serde_json::from_str("{}").unwrap();
    assert_eq!((h.eco.heat, h.eco.cool), (16.0, 28.0));
    assert!(h.eco.actions.is_empty());
    let e: EcoSettings = serde_json::from_str(r#"{"heat":15,"actions":[{"fan_mode":"low"}]}"#).unwrap();
    assert_eq!(e.fan_mode(), Some("low"));
    assert_eq!(e.setpoint(Mode::Cool), 28.0);
    assert!(serde_json::from_str::<EcoSettings>(r#"{"actions":[{"swing_mode":"off"}]}"#).is_err());
}

#[test]
fn eco_validation() {
    let mut h = HouseSettings::default();
    h.eco.heat = 6.0; // below frost 7
    assert!(h.validate().unwrap_err().contains("eco"));
    let mut h = HouseSettings::default();
    h.eco.cool = 31.0; // above overheat 30
    assert!(h.validate().is_err());
    let mut h = HouseSettings::default();
    h.eco.actions = vec![EcoAction::FanMode("low".into()), EcoAction::FanMode("high".into())];
    assert!(h.validate().is_err());
    let mut h = HouseSettings::default();
    h.eco.actions = vec![EcoAction::FanMode(String::new())];
    assert!(h.validate().is_err());
}

#[test]
fn old_zone_settings_with_away_setpoint_still_load() {
    let z: ZoneSettings = serde_json::from_str(r#"{"state":"active","away_setpoint":{"heat":16,"cool":28}}"#).unwrap();
    assert_eq!(z.state, ZoneState::Active);
}

#[test]
fn decision_without_eco_restore_round_trips() { /* serialize a Decision with an empty map: no "eco_restore" key; deserialize JSON lacking it: empty map */ }

// topics.rs
#[test]
fn eco_topic() {
    assert_eq!(zone_topic("oikos/climate", "living", ZoneDoc::Eco), "oikos/climate/zone/living/eco");
    assert_eq!(parse("oikos/climate", "oikos/climate/zone/living/eco"),
        Some(ClimateTopic::Zone { zone: "living".into(), doc: ZoneDoc::Eco }));
}
```

Also extend `learn.rs::engine_commands_other_than_schedule_and_idle_are_no_data` with `Source::Eco` in the no-data list, and `view.rs` with `room_eco_request_refuses_the_past`.

- [ ] **Step 2: Run** `cargo test -p oikos-climate` — expected: compile errors / FAIL on the new names.

- [ ] **Step 3: Implement** the Interfaces above. Delete `AwaySetpoints` and `ZoneSettings.away_setpoint` (the struct has no `deny_unknown_fields`, so old documents still load). In `choose`, the away row reads `inp.house.eco.setpoint(mode)` (Task 2 replaces the row). Rename the decide test `away_uses_away_setpoint` → `away_uses_eco_setpoint`. Add `Source::Eco` next to `Source::Away` in `learn.rs`'s no-data `matches!`. Fix every `DeviceCaps`/`Device` constructor the compiler names (hub `inputs.rs` fills `fan_modes` from `entity.capabilities.fan_modes` and `fan_mode` from `climate.fan_mode`), and `zone_view` sets `eco: docs.eco.clone()` once Task 4 adds `ZoneDocs.eco` (here: `eco: None`). Update `doc.rs`'s existing spec-defaults test that reads `z.away_setpoint`.

- [ ] **Step 4: Run** `cargo test -p oikos-climate && cargo build -p oikos-os -p oikos-web` — expected: PASS / builds.

- [ ] **Step 5: Commit** `feat(climate): eco settings, room eco document and fan memory types`.

---

### Task 2: The eco row in `decide`

**Files:**
- Modify: `crates/oikos-climate/src/decide.rs` (`ZoneInputs`, `choose`, tests)
- Modify: `crates/oikos-os/src/climate/inputs.rs` (`OwnedZoneInputs.room_eco: Option<RoomEco>`, `borrowed()`; filled with `None` until Task 4)

**Interfaces:**
- Consumes: `EcoSettings::setpoint`, `RoomEco`, `Source::Eco` (Task 1).
- Produces: `ZoneInputs.room_eco: Option<&'a RoomEco>`; `pub fn eco_setpoint(mode: Mode, eco: f64, schedule: Setpoint) -> Setpoint` (heat: `Off`→`Off`, `Temp(s)`→`Temp(s.min(eco))`; cool: `Temp(s.max(eco))`).

Row order inside `choose`: holiday → eco → schedule. Eco applies when the room eco is live (`until > now`) or `!home`. It first computes the schedule setpoint for the mode; with no rules for the mode the result is the existing idle choice (no eco). Source: `Eco` when the room eco is live, else `Away`. Reasons (exact copy):
- room eco: `Eco until {Sun 00:00}: {15.0 °C}` — the time formatted `%a %H:%M` from `local_now + (until − now)`;
- both: `Eco until {…} (nobody home): {…}`;
- away only: `Nobody home: eco {…}`;
- when `eco_setpoint` returned the schedule's value: append `; schedule already lower` (heat) or `; schedule already higher` (cool).

- [ ] **Step 1: Write the failing tests** (in the `Case` harness; `Case` gains `room_eco: Option<RoomEco>`, the house default eco is 16.0, the `Case` schedule at `local()` is the 20.5 block over base 17.0):

```rust
#[test] fn room_eco_lowers_to_eco() { /* room_eco until NOW+3600 → source Eco, trv setpoint 16.0, reason starts "Eco until " */ }
#[test] fn away_uses_eco_setpoint() { /* home=false → source Away, 16.0, reason "Nobody home: eco 16.0 °C" */ }
#[test] fn room_eco_and_away_together() { /* both → source Eco, reason contains "(nobody home)" */ }
#[test] fn eco_never_raises_heat() { /* clear blocks, base 14.0, room eco → setpoint 14.0, source Eco, reason ends "; schedule already lower" */ }
#[test] fn eco_never_raises_cool() { /* mode Cool, cool rules base 30.0 (eco cool 28) → setpoint 30.0 */ }
#[test] fn eco_keeps_schedule_off() { /* block setpoint Off, room eco → entities["trv"] mode "off" */ }
#[test] fn eco_without_rules_for_mode_stays_idle() { /* rules.heat = None, home=false → source Idle */ }
#[test] fn expired_room_eco_is_ignored() { /* room_eco until == NOW → source Schedule */ }
#[test] fn override_and_holiday_win_over_eco() { /* user zone override live → Override; holiday=true → Holiday */ }
#[test] fn hand_hold_survives_eco() { /* per-device override on "trv" → entities["trv"].held == true under source Eco */ }
```

- [ ] **Step 2: Run** `cargo test -p oikos-climate decide` — expected: FAIL.
- [ ] **Step 3: Implement** the row and `eco_setpoint` as above; `zone_override`'s fallback `choose(...)` keeps calling the same function.
- [ ] **Step 4: Run** `cargo test -p oikos-climate && cargo test -p oikos-os --test climate_hub` — expected: PASS (the existing `away_after_grace` still sees 16.0 under a 17.0 schedule).
- [ ] **Step 5: Commit** `feat(climate): eco row in the decision tree, never above the schedule`.

---

### Task 3: Pure fan plan

**Files:**
- Create: `crates/oikos-climate/src/eco.rs` (and `pub mod eco;` in `lib.rs`)

**Interfaces:**
- Consumes: `Decision`, `Device` (with `fan_mode`, `caps.fan_modes`), `EcoAction`, `FanRestore`, `Source`.
- Produces:
  - `pub struct FanCommand { pub entity: String, pub fan_mode: String }`
  - `pub fn fan_plan(previous: &BTreeMap<String, FanRestore>, decision: &Decision, devices: &[Device], actions: &[EcoAction], zone_off: bool) -> (BTreeMap<String, FanRestore>, Vec<FanCommand>)`

Rules (the algorithm; `want` = the `fan_mode` action, if any; "driven" = in `decision.entities`, not `held`, target mode not `"off"`, present in `devices`; `restore(id, r)` = command `r.was` if the device is present and its `fan_mode == Some(r.set)`, then drop the entry):
1. `zone_off`, or source not in {Eco, Away, Safety, Override} → `restore` every entry; new memory empty.
2. Source Safety or Override → memory unchanged, no commands.
3. Source Eco or Away, per device in `devices`:
   - entry present, `want` is `None` or not in the device's `fan_modes` → `restore`.
   - driven, `want = v` listed, no entry, `fan_mode = Some(f)` with `f != v` → entry `{was: f, set: v}`, command `v`. Same value already, or no fan state → nothing.
   - driven, entry `{set: s}` with `s != v` (setting changed): device at `s` → command `v`, entry `{was, set: v}`; else drop the entry (the user moved it).
   - driven, entry with `set == v`, device not at `v` → drop the entry (the user moved it).
   - not driven → keep any entry, no command.
   - Entries whose device is absent from `devices` (offline or left the zone) are kept untouched; the hub handles departed units (Task 6).

- [ ] **Step 1: Write the failing tests** in `eco.rs` (helpers build a `Decision` with a given `source` and `entities`, devices with `fan_modes: ["auto","low","high"]`):

```rust
#[test] fn entering_eco_sets_the_fan_and_remembers() { /* Eco, device at "auto", want "low" → cmd low, memory {was:auto,set:low} */ }
#[test] fn already_at_eco_value_sends_nothing() {}
#[test] fn unit_without_the_value_is_left_alone() { /* fan_modes ["auto"] → no cmd, no memory */ }
#[test] fn held_or_off_units_get_no_fan_command() { /* held=true and mode "off" targets → no cmd */ }
#[test] fn leaving_eco_restores_when_unchanged() { /* Schedule, memory {auto,low}, device at low → cmd auto, memory empty */ }
#[test] fn leaving_eco_leaves_a_hand_changed_fan() { /* device at "high" → no cmd, memory empty */ }
#[test] fn safety_or_override_keeps_memory() { /* Safety → same memory, no cmd */ }
#[test] fn eco_to_away_sends_nothing_again() { /* memory {auto,low}, Away, device at low → no cmd, memory kept */ }
#[test] fn changing_the_eco_value_moves_units_still_at_the_old_one() { /* memory set low, want high, device at low → cmd high, memory {auto,high} */ }
#[test] fn removing_the_action_restores() { /* want None under Eco → cmd auto */ }
#[test] fn zone_off_restores() { /* zone_off=true → cmd auto */ }
#[test] fn absent_device_keeps_its_memory() {}
```

- [ ] **Step 2: Run** `cargo test -p oikos-climate eco` — expected: FAIL.
- [ ] **Step 3: Implement** `fan_plan` per the rules.
- [ ] **Step 4: Run** `cargo test -p oikos-climate` — expected: PASS.
- [ ] **Step 5: Commit** `feat(climate): pure fan plan for eco extra actions`.

---

### Task 4: Room eco in the hub, HTTP and MCP

**Files:**
- Modify: `crates/oikos-os/src/climate/store.rs` (`ZoneDocs.eco: Option<RoomEco>`, `apply_zone` arm, `is_empty`)
- Modify: `crates/oikos-os/src/climate/proposals.rs` (next to the override methods)
- Modify: `crates/oikos-os/src/climate/inputs.rs` (read `docs.eco` into `OwnedZoneInputs.room_eco`)
- Modify: `crates/oikos-os/src/climate/engine.rs` (`tidy_zone` deletes an expired eco)
- Modify: `crates/oikos-os/src/climate/hub.rs` (`zone_view` → `eco: docs.eco.clone()`)
- Modify: `crates/oikos-os/src/http/api.rs` (route + handlers)
- Modify: `crates/oikos-os/src/mcp/climate_tools.rs`, `crates/oikos-os/src/mcp/params.rs`, `crates/oikos-os/src/mcp/mod.rs` (instructions)
- Test: `crates/oikos-os/tests/climate_hub.rs`, `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: `RoomEco`, `RoomEcoRequest`, `ZoneDoc::Eco`, `ZoneInputs.room_eco` (Tasks 1–2).
- Produces:
  - `Hub::set_climate_eco(self: &Arc<Self>, zone: &str, eco: RoomEco) -> Result<(), HubError>` — `require_climate`, `require_zone`, refuses `ZoneState::Off` with `"zone is off"` and `until <= now` with `"eco already ended"`; writes `ZoneDoc::Eco` via `write_zone_doc`.
  - `Hub::clear_climate_eco(self: &Arc<Self>, zone: &str) -> Result<(), HubError>` — writes an empty document.
  - HTTP `PUT /api/v1/climate/zones/{id}/eco` (body `RoomEcoRequest`, 204) and `DELETE` (204), same error mapping as the override routes.
  - MCP `set_climate_eco` (`SetClimateEcoParams { zone: String, until: u64 }` — Unix seconds, like overrides) and `clear_climate_eco` (`ClimateZoneParams`), both returning `ClimateAck`.
  - MCP instructions, climate paragraph, one sentence: "Eco: `set_climate_settings` holds `eco` (`heat`, `cool`, `actions` such as `{\"fan_mode\":\"low\"}`); `set_climate_eco` puts one zone in eco until a Unix time, `clear_climate_eco` ends it; a presence entity that is an alarm panel armed away also means nobody home."

- [ ] **Step 1: Write the failing tests** in `climate_hub.rs` (`mod engine`, using `Rig`):

```rust
#[tokio::test(start_paused = true)]
async fn room_eco_lowers_then_ends() {
    // heat_rules(19.0); set_climate_eco("living", RoomEco{until: t0+3600, set_at: t0}) → pass(0) temps [16.0], source Eco,
    // zone().eco is Some; pass(3700) → temps [19.0], source Schedule, zone().eco is None (expired_room_eco_is_deleted)
}
#[tokio::test] async fn room_eco_refused_on_off_zone_and_in_the_past() {}
#[tokio::test] async fn clear_room_eco_returns_to_schedule() {}
```

and in `tests/mcp.rs` a `set_climate_eco` / `clear_climate_eco` round trip asserting the retained `oikos/climate/zone/living/eco` publish (follow the existing `set_climate_override` test there).

- [ ] **Step 2: Run** `cargo test -p oikos-os --test climate_hub room_eco` — expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces; `tidy_zone` checks `docs.eco.until <= now` and writes `None::<&RoomEco>`, then clears `inp.room_eco`.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test climate_hub && cargo test -p oikos-os --test mcp` — expected: PASS.
- [ ] **Step 5: Commit** `feat(climate): room eco until a date over HTTP and MCP`.

---

### Task 5: Alarm panels as presence

**Files:**
- Modify: `crates/oikos-os/src/climate/inputs.rs` (`home_of`, `presence_home_now`)
- Modify: `crates/oikos-os/src/climate/mod.rs` (`ClimateRuntime.presence_last: HashMap<String, bool>`)
- Test: `crates/oikos-os/tests/climate_hub.rs`

**Interfaces:**
- Produces: `fn alarm_home(state: &str) -> Option<bool>`: `armed_away`, `armed_vacation` → `Some(false)`; `disarmed`, `armed_home`, `armed_night`, `armed_custom_bypass` → `Some(true)`; everything else (`arming`, `disarming`, `pending`, `triggered`, `unknown`, …) → `None`. `home_of` maps `StateValue::Alarm(s)` through it. `presence_home_now` records each definite answer in `presence_last` and uses the stored answer when an entity's current answer is `None`; nothing stored at boot.

- [ ] **Step 1: Write the failing tests** — ingest an `alarm_control_panel` discovery (`oikos/alarm_control_panel/test/panel/config`, `state_topic` `t/panel`), put it in `house.presence`, `heat_rules(19.0)`:

```rust
#[tokio::test(start_paused = true)]
async fn armed_away_puts_the_house_in_eco() { /* "disarmed" → 19; "armed_away", pass(960) → 16, source Away; "disarmed" → 19 */ }
#[tokio::test(start_paused = true)]
async fn armed_home_and_night_are_home() {}
#[tokio::test(start_paused = true)]
async fn alarm_transient_states_keep_last_answer() { /* armed_away → Away; "triggered" then "pending" → still Away; "disarmed" → Schedule */ }
```

plus a unit test of `alarm_home` covering each listed state.

- [ ] **Step 2: Run** `cargo test -p oikos-os --test climate_hub armed` — expected: FAIL.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test climate_hub` — expected: PASS.
- [ ] **Step 5: Commit** `feat(climate): an alarm panel armed away means nobody home`.

---

### Task 6: Fan commands in the engine

**Files:**
- Modify: `crates/oikos-os/src/climate/engine.rs` (`evaluate_zone`, `tidy_zone`, new `command_fan`)
- Test: `crates/oikos-os/tests/climate_hub.rs` (`Sim` learns fans)

**Interfaces:**
- Consumes: `eco::fan_plan`, `FanCommand`, `Decision.eco_restore` (Tasks 1, 3).
- Produces: `async fn command_fan(&self, id: &str, fan_mode: &str, now: u64) -> bool` — sends `Command::SetFanMode { mode }` through `send_command`; shares `rt.failures` and `alerts::command_id(id)` with `command_device` (same `MAX_FAILURES`, pause and alert), but not `last_sent`, so the fan can follow a temperature command in the same pass.

Wiring in `evaluate_zone`, after `decide` and the target commands:
- `previous = last_decision(zone).map(|d| d.eco_restore).unwrap_or_default()`;
- `(memory, commands) = fan_plan(&previous, &decision, &inp.devices, &inp.house.eco.actions, state == ZoneState::Off)`;
- commands are sent when `state == Active`, or when `state == Off` and they are restores (the one last restore of the spec); a `pending` zone sends none and keeps the memory empty;
- a command not accepted removes its entity from `memory` (an entering command) or keeps the entry (a restore), so the next pass retries;
- `decision.eco_restore = memory` before `publish_decision`.

`tidy_zone`: for each `eco_restore` entry whose entity is no longer in `inp.entity_ids` but still in the registry with `fan_mode == set`, send `was` via `command_fan`; drop every such entry (and those of entities gone from the registry).

- [ ] **Step 1: Write the failing tests** — `Sim` gains `fans: BTreeMap<u8, String>`, reported as `"fan_mode"` in the state payload, and handles `/ha/fan_mode/set`; a helper `fan_commands(out) -> Vec<String>`. The eco value is read at runtime from the fixture entity's `capabilities.fan_modes` (its first value other than `auto`), so no vendor value is written in the test; the unit starts at `"auto"`:

```rust
#[tokio::test(start_paused = true)] async fn eco_sets_the_fan_and_restores_it() { /* fan "auto"; room eco → fan_commands [v]; eco cleared → ["auto"] */ }
#[tokio::test(start_paused = true)] async fn a_fan_changed_during_eco_is_not_restored() {}
#[tokio::test(start_paused = true)] async fn failed_fan_command_is_retried() { /* sim.echo=false on the first pass → memory empty in decision; echo=true next pass → sent again */ }
#[tokio::test(start_paused = true)] async fn fan_restored_after_restart() { /* eco on, fan set; build a fresh Hub fed the retained decision + eco docs; clear eco → "auto" sent */ }
#[tokio::test(start_paused = true)] async fn switching_the_zone_off_restores_the_fan_once() {}
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test climate_hub fan` — expected: FAIL.
- [ ] **Step 3: Implement** as above.
- [ ] **Step 4: Run** `cargo test -p oikos-os && cargo clippy -p oikos-climate -p oikos-os -- -D warnings && cargo fmt --check` — expected: PASS.
- [ ] **Step 5: Commit** `feat(climate): eco fan action with restore`.

---

### Task 7: Web — settings, room control, marker

**Files:**
- Modify: `crates/oikos-web/src/api/climate.rs` (`set_eco(zone, until) / clear_eco(zone)`)
- Modify: `crates/oikos-web/src/climate/settings_sheet.rs` (Eco group)
- Modify: `crates/oikos-web/src/climate/room_panel.rs` (Eco until… / Cancel eco)
- Modify: `crates/oikos-web/src/climate/room_list.rs` (marker)
- Modify: `crates/oikos-web/src/climate/mod.rs` (`source_word`, `history_source`, helpers)
- Modify: `crates/oikos-web/src/i18n/climate.rs`
- Modify: the climate stylesheet the screen already uses (find with `grep -rn "climate-overrides" crates/oikos-web`)

**Interfaces:**
- Consumes: `ZoneView.eco`, `HouseSettings.eco`, HTTP routes of Task 4.
- Produces (in `climate/mod.rs`, unit-tested natively):
  - `pub fn fan_mode_choices(entities: &[EntityView]) -> (Vec<(String, usize)>, usize)` — the union of `capabilities.fan_modes` of `Kind::Climate` entities not starting with `group_`, in first-seen order, each with how many units list it, and the unit count.
  - `pub fn eco_until(date: &str, time: &str, tz: Tz) -> Option<u64>` — `"YYYY-MM-DD"`, `"HH:MM"` local; a nonexistent DST time takes the earliest valid instant.
  - `pub fn in_eco(z: &ZoneView, now: u64) -> bool` — room eco live, or the decision source is `Eco`/`Away`.

UI: settings sheet group **Eco** with `NumField`s for heat and cool and **Extra actions**: rows `Fan speed: <select>` from `fan_mode_choices` + a "Don't change" empty choice that removes the action, each choice labelled `"{value} (on {n} of {m} units)"`; the presence field gets a hint line "An alarm panel can be added: armed away means nobody home". Room panel, shown when `can_hold`-style rule allows it (state not `Off`): **Eco until…** opens a date input + time input (default `00:00`) and a confirm button; while eco is live, the line `Eco until {until_text}` and **Cancel eco**. Room list: a small eco marker on rows where `in_eco`. `source_word(Source::Eco)` → ("Eco", "Éco"); `history_source("eco")`. French strings: Eco → "Éco", "Eco until…" → "Éco jusqu'au…", "Cancel eco" → "Annuler l'éco", "Extra actions" → "Actions en plus", "Fan speed" → "Vitesse du ventilateur", "Don't change" → "Ne pas changer", "on {n} of {m} units" → "sur {n} des {m} appareils".

- [ ] **Step 1: Write the failing tests** in `climate/mod.rs` tests: `fan_mode_choices_unions_and_counts`, `fan_mode_choices_skips_groups_and_non_climate`, `eco_until_local_and_dst_gap`, `in_eco_for_room_and_away`, `history_source_knows_eco`; and extend the i18n completeness test (the existing `ALL` list in `i18n/climate.rs`) with the new keys.
- [ ] **Step 2: Run** `cargo test -p oikos-web climate` — expected: FAIL.
- [ ] **Step 3: Implement** the helpers, then the components.
- [ ] **Step 4: Run** `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown && cargo clippy -p oikos-web --target wasm32-unknown-unknown -- -D warnings` (the CI checks) — expected: PASS. Then check the screen in a browser (memory: drive headless Chrome over CDP, window ≥ 500 px) in both themes.
- [ ] **Step 5: Commit** `feat(web): eco settings, room eco until a date and eco marker`.

---

### Task 8: Docs

**Files:**
- Modify: `docs/users/climate.md` (new "Eco" section; remove any per-zone away setpoint mention)
- Modify: `docs/reference/mqtt-topics.md` (`<prefix>/zone/<zone>/eco`), `docs/reference/http-api.md` (`PUT`/`DELETE …/eco`, `eco` in house and zone), `docs/reference/mcp-tools.md` (`set_climate_eco`, `clear_climate_eco`)

- [ ] **Step 1: Write** the user section: what eco does (one temperature per season, never above the schedule), room eco until a date (web and MCP), house eco (add the alarm panel to Presence; armed away only), extra actions (fan speed, units that don't offer it are left alone, restored after). Neutral examples only ("a bedroom").
- [ ] **Step 2: Check** `grep -rniE "away_setpoint" docs crates` → no hits outside `docs/superpowers`; if the website builds docs (`website/`), run its build command from its `package.json` → PASS.
- [ ] **Step 3: Commit** `docs(climate): eco`.
