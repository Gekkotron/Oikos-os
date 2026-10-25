# Native Alarm Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An optional, hub-owned alarm. It publishes one `alarm_control_panel`, watches openings and motion per mode, rings sirens for a bounded time, sends a critical push with Disarm, and arms and disarms from presence, remotes, web, app, MCP and flows.

**Architecture:** A new pure crate `oikos-alarm` holds the settings document, its shape validation, the topics and the state machine: `step(runtime, settings, world, event, now) -> effects`. It does no I/O and builds for wasm, as `oikos-climate` does. The hub side in `crates/oikos-os/src/alarm/` does the rest:
- it ingests the retained `<p>/settings` and `<p>/state`
- it publishes or removes the discovery entity
- it turns registry events into machine events
- it runs effects (sirens, chirp, push, panel state, runtime persistence)

HTTP, MCP and the web settings page sit on top.

**Tech Stack:** Rust 2024 workspace, tokio, rumqttc, axum, rmcp, Leptos 0.8 (CSR), serde/schemars.

**Spec:** `docs/superpowers/specs/2026-10-24-native-alarm-design.md`. Read it before any task. Section numbers below (§n) refer to it.

## Global Constraints

- **Optional.** The engine always runs but stays idle while `settings.enabled == false` (the default). Disabled means no entity, no watching, no alerts, and no change for houses that don't use it (§7.1).
- **Defaults** (§7.2):
  - `entry_delay_s` 30, `exit_delay_s` 60, `siren_s` 180
  - `motion.count` 2, `motion.window_s` 120
  - `presence.arm` true, `presence.disarm` true, `presence.arm_after_s` 120
- **Ranges:** entry 0–300, exit 0–300, siren 10–900, motion count 1–5, window 10–900, arm_after 0–3600.
- **Modes and states:**
  - Arm modes: `away`, `home` and `night` only.
  - States published: `disarmed`, `arming`, `armed_away`, `armed_home`, `armed_night`, `pending`, `triggered`.
- **Codes:** none. Discovery says `code_arm_required: false` and `code_disarm_required: false`.
- **Entity:**
  - Unique id `oikos_alarm_house`, name "Alarm".
  - Config `<discovery P>/alarm_control_panel/alarm/house/config`.
  - State `<p>/panel`, command `<p>/panel/set`, attributes `<p>/panel/attributes`, where `p` is `[alarm] prefix`, default `oikos/alarm`.
- **Push** uses source `"alarm"`, channel `alarm`. Alert ids start with `alarm-`.
- **No user house data in tracked files.** No device ids, IPs or names from the author's house in code, tests or docs: use invented ids in fixtures ([feedback: project must be generic]).
- **Labels:** every new web label has `en` and `fr` through `crates/oikos-web/src/i18n.rs`.
- **Builds:** before building, check `df -h ~` and `du -sh ~/dev-scratch/cargo-target`. If the target is over 5 GB, run `cargo clean` first. When free disk is under 5 GB, run per-crate tests only.
- **Commits:**
  - Identity: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`
  - The message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  - Stage explicit paths only (another session may share the worktree).
- **Pass gate** before every commit: `cargo fmt --all --check`, `cargo clippy -p <crate> --all-targets -- -D warnings`, then the task's tests.

## Review Focus

1. **A sensor's retained state replayed at boot or on reconnect** must never count as an opening or a motion. The sensor-change map is cleared on disconnect, like the flows engine's `known` map. The test is `replayed_states_never_trigger` (Task 5).
2. **Several trackers, one unknown or offline:** no auto-arm, and a later definite "everyone away" still arms once (`unknown_tracker_blocks_auto_arm`, Task 3).
3. **A settings write that removes a sensor while it is in `open_at_arm` or counted in `motion_hits`:** the next `step` must drop it, never index into the settings with a stale id (`removed_sensor_is_forgotten`, Task 2).
4. **Disarm arriving twice** (push action and presence together), or **Disarm while already disarmed with an open alert:** resolve once, no error, no siren command when nothing is ringing (`double_disarm_is_harmless`, Task 2).
5. **Broker offline while triggered:** a siren publish is refused with `HubError::Publish`. The other sirens still get commanded, and the failure is listed in the alert text (`failing_siren_does_not_block_others`, Task 6).

---

### Task 1: `oikos-alarm` crate — settings document, validation, topics

**Files:**
- Create: `crates/oikos-alarm/Cargo.toml` (copy `crates/oikos-climate/Cargo.toml`: serde, serde_json, optional schemars feature; dev-dep none needed)
- Create: `crates/oikos-alarm/src/lib.rs` (`pub mod doc; pub mod machine; pub mod topics; pub mod view;` — `machine` and `view` start as empty modules filled by Tasks 2 and 4)
- Create: `crates/oikos-alarm/src/doc.rs`
- Create: `crates/oikos-alarm/src/topics.rs`

**Interfaces:**
- Produces (`doc.rs`). Every type derives `Serialize, Deserialize, Clone, Debug, PartialEq`, plus `JsonSchema` under `cfg_attr(feature = "schemars")`. Every enum uses `#[serde(rename_all = "snake_case")]`.
  - `pub enum ArmMode { Away, Home, Night }`. Also `Copy, Eq, Ord, Hash`.
  - `pub enum SensorKind { Opening, Motion }`
  - `pub enum Entry { Instant, Delayed }`. The default is `Instant`.
  - `pub struct SensorRule { pub entity: String, pub kind: SensorKind, pub modes: BTreeSet<ArmMode>, #[serde(default)] pub entry: Entry }`
  - `pub struct MqttSiren { pub topic: String, pub on: String, pub off: String, #[serde(default)] pub chirp: Option<String> }`
  - `#[serde(untagged)] pub enum Siren { Entity { entity: String, #[serde(default)] chirp_ms: Option<u32> }, Mqtt { mqtt: MqttSiren } }`
  - `pub enum RemoteAction { Away, Home, Night, Disarm, Sos }`
  - `pub struct Remote { pub entity: String, pub signals: BTreeMap<String, RemoteAction> }`
  - `pub struct MotionRule { pub count: u8, pub window_s: u32 }`. The default is `{2, 120}`.
  - `pub struct PresenceRule { pub trackers: Vec<String>, pub arm: bool, pub disarm: bool, pub arm_after_s: u32 }`. The default is `{[], true, true, 120}`.
  - `pub struct AlarmSettings { pub enabled: bool, pub entry_delay_s: u32, pub exit_delay_s: u32, pub siren_s: u32, pub motion: MotionRule, pub sensors: Vec<SensorRule>, pub sirens: Vec<Siren>, pub remotes: Vec<Remote>, pub presence: PresenceRule }`, with `#[serde(default)]` and a `Default` that uses the Global Constraints values.
  - `impl AlarmSettings { pub fn validate(&self) -> Result<(), String>; pub fn sensor(&self, entity: &str) -> Option<&SensorRule>; pub fn watches(&self, entity: &str, mode: ArmMode) -> bool }`
  - `pub fn default_signals() -> BTreeMap<String, RemoteAction>` maps:
    - `arm_all_zones` → Away
    - `arm_day_zones` → Home
    - `arm_night_zones` → Night
    - `disarm` → Disarm
    - `emergency` → Sos
    - `panic` → Sos
  - `pub fn default_sensor(entity: &str, kind: SensorKind) -> SensorRule`: an opening defaults to {Away, Night}, Instant; a motion to {Away}, Instant (§4.1).
- Produces (`topics.rs`):
  - `pub fn settings_topic(p: &str) -> String` → `<p>/settings`
  - `pub fn runtime_topic(p: &str) -> String` → `<p>/state`
  - `pub fn panel_state_topic(p: &str) -> String` → `<p>/panel`
  - `pub fn panel_command_topic(p: &str) -> String` → `<p>/panel/set`
  - `pub fn panel_attributes_topic(p: &str) -> String` → `<p>/panel/attributes`
  - `pub fn filters(p: &str) -> [String; 3]` → settings, runtime, panel command (never `<p>/#`)
  - `pub enum AlarmTopic { Settings, Runtime, PanelCommand }; pub fn parse(p: &str, topic: &str) -> Option<AlarmTopic>`
  - All of them trim a trailing `/` from `p`.

- [ ] **Step 1: Write the failing tests** in `doc.rs` and `topics.rs` (`#[cfg(test)] mod tests`):
  - `defaults_match_spec`: `AlarmSettings::default()` has `enabled == false`, delays 30/60/180, motion `{2,120}`, and presence `{[], true, true, 120}`. `serde_json::from_str::<AlarmSettings>("{}")` equals the default.
  - `validate_rejects_out_of_range` covers `entry_delay_s = 301`, `siren_s = 9`, `motion.count = 0`, `motion.count = 6`, `motion.window_s = 901` and `presence.arm_after_s = 3601`. Each is `Err` naming the field.
  - `validate_rejects_duplicates`: the same entity twice in `sensors`, in `remotes` or in `presence.trackers` is `Err`.
  - `validate_rejects_bad_mqtt_siren`: an empty topic, or a topic containing `+` or `#`, is `Err`. Empty `on` or `off` is `Err`.
  - `siren_untagged_roundtrip`: `{"entity":"switch.x","chirp_ms":null}` parses as `Siren::Entity`; `{"mqtt":{"topic":"a/b/set","on":"1","off":"0","chirp":"c"}}` parses as `Siren::Mqtt`.
  - `opening_defaults_to_away_night_instant` and `motion_defaults_to_away`.
  - `topics_layout`: with `p = "oikos/alarm/"`, `settings_topic` is `"oikos/alarm/settings"` and `parse` maps the three filter topics back. `parse` returns `None` for `"oikos/alarm/panel"` and for `"oikos/alarm/panel/attributes"`, because the hub publishes those and the registry ingests them.
- [ ] **Step 2: Add the crate.** Run `cargo test -p oikos-alarm`. Expected: compile errors or FAIL.
- [ ] **Step 3: Implement** the types, `validate`, `default_signals`, `default_sensor` and the topic helpers above.
- [ ] **Step 4: Run** `cargo test -p oikos-alarm` and `cargo build -p oikos-alarm --target wasm32-unknown-unknown`. Expected: PASS, and the wasm build succeeds.
- [ ] **Step 5: Commit** `feat(alarm): settings document, validation and topics`.

### Task 2: State machine — arming, rules, sirens, disarm

**Files:**
- Modify: `crates/oikos-alarm/src/machine.rs`

**Interfaces:**
- Consumes: Task 1 `AlarmSettings`, `ArmMode`, `SensorKind`, `Entry`.
- Produces. Every type is `Serialize/Deserialize/Clone/Debug/PartialEq`, because `Runtime` is persisted retained (§9):
  - `pub enum Phase { Disarmed, Arming { mode: ArmMode, until: u64 }, Armed { mode: ArmMode }, Pending { mode: ArmMode, until: u64 }, Triggered { back_to: Option<ArmMode>, until: u64 } }`, with `#[serde(tag = "phase", rename_all = "snake_case")]`
  - `pub enum Source { Presence, Remote, Sos, Command }`
  - `pub enum Presence { Home, Away, Unknown }`
  - `pub struct Runtime { pub phase: Phase, pub source: Option<Source>, pub since: u64, pub motion_hits: Vec<(u64, String)>, pub open_at_arm: BTreeSet<String>, pub cause: Vec<String>, pub alert_ids: Vec<String>, pub trackers: BTreeMap<String, Presence>, pub auto_arm_ready: bool, pub everyone_away_since: Option<u64> }`, with `Default` = Disarmed and everything empty
  - `pub struct World<'a> { pub on: &'a BTreeSet<String> }`: the watched entities whose current state is on
  - `pub enum Event { Arm { mode: ArmMode, source: Source }, Disarm { source: Source }, Sensor { entity: String, on: bool }, Signal { entity: String, signal: String }, Tracker { entity: String, seen: Presence }, Tick, Resume }`
  - `pub enum Effect { SirensOn, SirensOff, Chirp, Alert { cause: Vec<String> }, ResolveAlerts, OpenAtArm(Vec<String>) }`
  - `pub fn step(rt: &mut Runtime, s: &AlarmSettings, world: &World, ev: Event, now: u64) -> Vec<Effect>`
  - `pub fn next_deadline(rt: &Runtime, s: &AlarmSettings) -> Option<u64>`: the earliest of the phase `until`, the `everyone_away_since + arm_after_s` auto-arm time, and the motion-hit expiry
  - `impl Runtime { pub fn state_name(&self) -> &'static str; pub fn mode(&self) -> Option<ArmMode>; pub fn deadline(&self) -> Option<u64> }`

  This task covers `Arm`, `Disarm`, `Sensor`, `Tick` and the `Signal` → Sos path. Task 3 adds the presence and remote mapping and `Resume`. For now, `Tracker` and `Resume` return `vec![]`.

Rules to encode (§3, §4, §5):
- **Arm** from `Disarmed`:
  - `source == Presence`: go straight to `Armed`.
  - otherwise: `Arming{until: now + exit_delay_s}`. With `exit_delay_s == 0`, go straight to `Armed`.
  - Arm from `Armed` in another mode follows the same path.
  - Arm from `Arming`, `Pending` or `Triggered` is a no-op returning `vec![]`.
- **Entering `Armed`** (from Arming at its deadline, or directly) does three things:
  - sets `open_at_arm` to the openings watched in the mode that are in `world.on`;
  - emits `Chirp`;
  - emits `OpenAtArm(list)` when the list is non-empty.
- **While `Armed{mode}`**, for sensors watched in `mode`:
  - **opening `on`:** Instant → trigger; Delayed → `Pending{until: now + entry_delay_s}`.
  - **opening `off` for an entity in `open_at_arm`:** remove it from the set, then handle it as an opening event per its `entry` (§4.3).
  - **motion `on`:** push `(now, entity)`, drop hits older than `window_s`, and trigger when `hits.len() >= count`.
- **While `Pending`:** an instant opening triggers; delayed openings and motion are ignored. A `Tick` at or after `until` triggers.
- **Trigger:**
  - `Triggered{back_to: Some(mode), until: now + siren_s}`
  - `cause` is set: the entity ids, and for motion every entity in the window
  - effects `[SirensOn, Alert{cause}]`
- **SOS** (a `Signal` whose remote maps to Sos; Task 3 adds the lookup — here, test with a settings remote mapping `emergency` to Sos) works from any phase: `Triggered{back_to: current armed mode or None}`, `cause = ["sos:<remote entity>"]`.
- **`Tick` with `Triggered` past `until`:**
  - emit `SirensOff`
  - phase = `Armed{back_to}` (or `Disarmed` when `None`)
  - clear `motion_hits`
  - `open_at_arm` = the watched openings in `world.on`
  - no chirp
  - `alert_ids` are kept
- **While `Triggered`,** further detections are ignored (no re-ring until it has returned to armed). There is no ring cap (§3, decided with the user).
- **Disarm** from any phase:
  - `SirensOff` only if `Triggered`; `ResolveAlerts` only if `alert_ids` is non-empty (the hub clears `alert_ids` after resolving);
  - phase `Disarmed`; clear `motion_hits`, `open_at_arm` and `cause`;
  - `auto_arm_ready` = any tracker `Home` (Task 3 uses it).
- Every phase change sets `since = now` and `source`.
- **Settings drift:** an event for an entity that is no longer in `s.sensors` is ignored. Each `step` first prunes `open_at_arm` and `motion_hits` of entities not in `s.sensors`.

- [ ] **Step 1: Write the failing tests** (`#[cfg(test)] mod tests` in `machine.rs`, with a `settings()` fixture holding:
  - `door` (opening, all modes, Delayed)
  - `window` (opening, {Away, Night}, Instant)
  - `hall` and `kitchen` (motion, {Away})
  - a remote `remote.x` mapping `emergency` → Sos).
  - `manual_arm_goes_through_exit_delay`: Arm{Away, Command} at t=0 → `Arming{until: 60}`, no effects. Tick at 60 → `Armed{Away}`, effects contain `Chirp`.
  - `presence_arm_skips_exit_delay`: Arm{Away, Presence} → `Armed` at once, plus `Chirp`.
  - `instant_opening_triggers`: armed away, `window` on → `Triggered{Some(Away), until: now+180}`, effects `[SirensOn, Alert{cause:["window"]}]`.
  - `delayed_opening_waits_entry_delay`: `door` on at t → `Pending{until: t+30}`. Disarm at t+10 → `Disarmed`, no `SirensOn` anywhere. A second run with no disarm: Tick at t+30 → `Triggered`.
  - `motion_ignored_while_pending`: Pending, then `hall` on → still Pending.
  - `two_motions_within_window_trigger`: `hall` at t and `kitchen` at t+119 → Triggered, cause `["hall","kitchen"]`. A separate run with t and t+121 → still Armed with one hit.
  - `same_motion_sensor_twice_counts`: `hall` on, off, on within 120 s → Triggered.
  - `mode_filters_sensors`: armed night, `hall` on twice → still `Armed{Night}`.
  - `open_window_at_arm_is_watched_for_closing`: `window` in `world.on` when Armed is entered → `open_at_arm == {"window"}` and `OpenAtArm(["window"])` is emitted. `window` off → Triggered with cause `["window"]`.
  - `closing_during_exit_delay_is_not_an_event`: `window` on at the arm command and off during Arming. At the Tick at the deadline, `world.on` is empty → Armed, empty `open_at_arm`, no trigger.
  - `ring_ends_and_rearms`: Triggered; Tick at `until` → `[SirensOff]`, `Armed{Away}`, `alert_ids` kept, `motion_hits` empty. A new `window` on → Triggered again, effects `[SirensOn, Alert{..}]`.
  - `detections_while_ringing_are_ignored`: Triggered, `hall` on → no effects.
  - `sos_from_disarmed_returns_to_disarmed`: Disarmed; Signal{remote.x, emergency} → `Triggered{back_to: None}`. Tick at until → `Disarmed`, `[SirensOff]`.
  - `double_disarm_is_harmless`: Triggered with `alert_ids = ["alarm-1"]`. Disarm → `[SirensOff, ResolveAlerts]`; set `alert_ids` empty (as the hub would); Disarm again → `[]`.
  - `removed_sensor_is_forgotten`: Armed with `open_at_arm = {"window"}` and a `hall` hit. Then step with settings lacking both, and Event `Tick` → `open_at_arm` and `motion_hits` are empty, no panic.
  - `state_names`: each phase maps to its published string (Armed{Home} → `"armed_home"`, Pending → `"pending"`).
  - `runtime_roundtrips_json`: serialize a `Triggered` runtime and parse it back equal.
- [ ] **Step 2: Run** `cargo test -p oikos-alarm machine`. Expected: FAIL.
- [ ] **Step 3: Implement** `step`, `next_deadline` and the `Runtime` helpers per the rules above.
- [ ] **Step 4: Run** `cargo test -p oikos-alarm`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(alarm): state machine for arming, rules and sirens`.

### Task 3: State machine — presence, remotes, resume

**Files:**
- Modify: `crates/oikos-alarm/src/machine.rs`

**Interfaces:**
- Consumes: Task 2 types.
- Produces: the `Tracker`, `Signal` (all actions) and `Resume` handling inside `step`, plus `pub fn initial_ready(trackers: &BTreeMap<String, Presence>) -> bool`. The hub calls it when the alarm is turned on: true when any tracker is `Home`.

Rules (§6.1, §6.2, §9):
- **`Tracker{entity, seen}`:**
  - record `rt.trackers[entity] = seen`;
  - if it changed to `Home`: set `auto_arm_ready = true`, and if `presence.disarm` and the phase is not `Disarmed`, disarm with source `Presence` (same effects as Disarm);
  - recompute `everyone_away_since`: `Some(now)` when every tracker in `s.presence.trackers` is `Away` and it was `None`; `None` otherwise. An `Unknown` or missing tracker counts as not away.
- **`Tick`** additionally auto-arms (`Arm{Away, Presence}`, then `auto_arm_ready = false`) when all of these hold:
  - `presence.arm`
  - non-empty trackers
  - `auto_arm_ready`
  - phase `Disarmed`
  - `everyone_away_since + arm_after_s <= now`
- **`Signal{entity, signal}`:** look up `s.remotes` for the entity, then `signals[signal]`.
  - Away/Home/Night → `Arm{mode, source: Remote}`
  - Disarm → `Disarm{source: Remote}`
  - Sos → the SOS rule from Task 2
  - unknown remote or signal → `vec![]`
- **`Resume`** (once at boot, after the retained runtime is loaded):
  - `Arming` past `until` → enter Armed, as Task 2 does;
  - `Pending` past `until`:
    - if `now < until + siren_s`: Triggered with `until: until + siren_s`, effects `[SirensOn, Alert{cause}]`;
    - otherwise: Armed, effects `[Alert{cause}]` (the ring was missed, the alert is still raised);
  - `Triggered` with `until > now` → `[SirensOn]`;
  - `Triggered` with `until <= now` → the Task 2 Tick path (`SirensOff`, back to armed);
  - otherwise `vec![]`.

  Tracker reconciliation is the hub's job (Task 6): it sends `Tracker` events for trackers whose live state differs from `rt.trackers`.

- [ ] **Step 1: Write the failing tests:**
  - `everyone_away_arms_after_grace`: trackers `[a, b]`, ready. a Away at t, b Away at t+10 → `everyone_away_since == Some(t+10)`. Tick at t+129 → Disarmed. Tick at t+130 → `Armed{Away}`, source Presence, plus `Chirp`.
  - `unknown_tracker_blocks_auto_arm`: a Away, b Unknown, Tick at +1000 → Disarmed. b Away at T → it arms at T+120.
  - `disarm_while_away_stays_disarmed`: auto-armed, Disarm{Command} while both are Away → `auto_arm_ready == false`. Ticks for 1 h → still Disarmed. a Home then a Away → it arms again after 120 s.
  - `enable_with_everyone_away_does_not_arm`: `initial_ready` with all Away is false.
  - `tracker_home_disarms`: Triggered; a changes to Home → `[SirensOff, ResolveAlerts]` (with alert ids), Disarmed, source Presence.
  - `tracker_home_noop_when_already_home`: armed night by Command while a is already Home; `Tracker{a, Home}` (no change) → still `Armed{Night}`.
  - `presence_switches_off`: with `presence.arm = false`, nothing auto-arms; with `presence.disarm = false`, Home doesn't disarm.
  - `remote_signals_map_to_actions`: `arm_all_zones` → Arming Away (source Remote), `arm_day_zones` → Arming Home, `disarm` → Disarmed, unknown signal → no change.
  - `resume_cases`: one assertion block per §9 case above, including `pending_deadline_missed_long_ago_rearms_with_alert`.
- [ ] **Step 2: Run** `cargo test -p oikos-alarm`. Expected: FAIL on the new tests.
- [ ] **Step 3: Implement** the rules above.
- [ ] **Step 4: Run** `cargo test -p oikos-alarm`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(alarm): presence, remotes and resume`.

### Task 4: Hub — config, store, entity publication, settings API, view

**Files:**
- Modify: `crates/oikos-os/Cargo.toml` (add `oikos-alarm = { path = "../oikos-alarm", features = ["schemars"] }`)
- Modify: `crates/oikos-os/src/config.rs` (add `AlarmConfig { prefix: String }`, default `"oikos/alarm"`, `#[serde(default)]`. `validate()` always applies the climate prefix rules. Add `"alarm.prefix"` to the env/path key list next to `"climate.prefix"`. Add unit tests modelled on the climate ones.)
- Modify: `crates/oikos-os/src/main.rs` (call `cfg.alarm.validate()`)
- Create: `crates/oikos-os/src/alarm/mod.rs`, `crates/oikos-os/src/alarm/hub.rs`, `crates/oikos-os/src/alarm/entity.rs`
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod alarm;`)
- Modify: `crates/oikos-os/src/hub.rs`:
  - field `pub(crate) alarm: AlarmHub`, built in `Hub::new` from `cfg.alarm.prefix`
  - in `ingest_message`, a call `if self.ingest_alarm(topic, payload, retain) { return Vec::new(); }` next to `ingest_climate`
  - `extra_filters` chains `oikos_alarm::topics::filters(prefix)`
- Create: `crates/oikos-alarm/src/view.rs`
- Test: `crates/oikos-os/tests/alarm_hub.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `oikos_alarm::view::AlarmView { pub enabled: bool, pub entity_id: String, pub state: String, pub mode: Option<ArmMode>, pub source: Option<Source>, pub since: u64, pub deadline: Option<u64>, pub cause: Vec<String>, pub open_at_arm: Vec<String>, pub settings: AlarmSettings, pub settings_invalid: Option<String>, pub warnings: Vec<String> }`, with Serialize/Deserialize and schemars under the feature
  - `pub const ALARM_ENTITY_ID: &str = "oikos_alarm_house";` in `alarm/mod.rs`
  - `pub struct AlarmHub { pub(crate) prefix: String, pub(crate) settings: Mutex<AlarmSettings>, pub(crate) settings_invalid: Mutex<Option<String>>, pub(crate) runtime: Mutex<Runtime>, pub(crate) runtime_loaded: AtomicBool, pub(crate) pass: tokio::sync::Mutex<()> }`
  - `alarm/entity.rs`: `pub fn config_topic(discovery_prefix: &str) -> String` and `pub fn build_config(discovery_prefix: &str, alarm_prefix: &str) -> serde_json::Value`. The config holds:
    - `name` "Alarm", `unique_id` `oikos_alarm_house`, `state_topic`, `command_topic`, `json_attributes_topic`
    - `supported_features: ["arm_away","arm_home","arm_night"]`
    - `code_arm_required: false`, `code_disarm_required: false`
    - `device {identifiers:["oikos_alarm"], name:"Alarm"}`
  - `Hub` methods in `alarm/hub.rs`:
    - `pub fn alarm_view(&self) -> AlarmView`. `warnings` lists the configured entity ids absent from the registry (§7.2, run-time).
    - `pub fn alarm_settings(&self) -> AlarmSettings`
    - `pub async fn set_alarm_settings(self: &Arc<Self>, settings: AlarmSettings) -> Result<(), HubError>`. In order:
      1. `validate()` → `HubError::Rejected`
      2. `check_alarm_entities` → `HubError::Rejected`
      3. publish retained `<p>/settings`, then ingest locally
      4. `sync_alarm_entity()`
      5. if it went from enabled to disabled: `alarm_disable()`
      6. if it went from disabled to enabled: reset the runtime with `auto_arm_ready = initial_ready(live trackers)` and persist it
      7. broadcast `WsEvent::AlarmChanged`
    - `pub(crate) fn check_alarm_entities(&self, s: &AlarmSettings) -> Result<(), String>`. Each entity must exist and have the right kind:
      - sensors: `Kind::BinarySensor`
      - entity sirens: `Kind::Switch`, or a generic siren with a `power` control
      - remotes: an entity `signals_of` can emit for (a `device_automation.*` or `event.*` id)
      - trackers: an entity whose generic state options include `home` and `not_home`
    - `pub(crate) fn sync_alarm_entity(&self) -> Result<(), HubError>`. When enabled: publish and ingest the retained config, then publish the current panel state and attributes. When disabled: publish an empty retained config, so the registry drops the entity.
    - `pub(crate) fn ingest_alarm(&self, topic: &str, payload: &[u8], retain: bool) -> bool`. It handles:
      - Settings: `apply_validated` semantics, as in `climate/store.rs`: an invalid document keeps the previous one and sets `settings_invalid`.
      - Runtime: parse into `Runtime`, set `runtime_loaded`, ignore it when invalid.
      - PanelCommand: non-retained only; Task 5 fills it in, here it returns true and does nothing.
  - `WsEvent::AlarmChanged` (unit variant) in `crates/oikos-api/src/events.rs`, with a doc comment.

- [ ] **Step 1: Write the failing tests** in `crates/oikos-os/tests/alarm_hub.rs`. Follow the harness in `tests/climate_hub.rs`: `Hub::new(&cfg, tx)`, `set_connection(Online)`, `ingest`, `drain(rx)`. Load invented discovery fixtures for two door contacts, one motion sensor, one siren switch and one tracker. Write a small fixture helper or reuse `oikos-core/tests/fixtures/discovery/` files.
  - `disabled_by_default_publishes_nothing`: a fresh hub. `alarm_view().enabled == false`, and no publish to any `alarm_control_panel` config topic.
  - `enable_publishes_entity`: `set_alarm_settings` with `enabled: true` and one sensor publishes the retained config at `oikos/alarm_control_panel/alarm/house/config`, with `supported_features` and `code_arm_required: false`. The registry then has `oikos_alarm_house` with `Kind::AlarmPanel` and state `disarmed`.
  - `disable_removes_entity_keeps_settings`: enable, then disable → an empty retained config is published, the registry no longer has the entity, and `alarm_settings().sensors` still has its sensor.
  - `rejects_unknown_or_wrong_kind_entities`: a sensor id that doesn't exist → `Rejected`; a sensor that is a `switch` → `Rejected`; a remote that is a `sensor` → `Rejected`.
  - `invalid_retained_settings_keep_previous`: ingest a valid doc, then `{"siren_s": 5}` → `settings_invalid` is `Some`, and the settings are unchanged.
  - `view_warns_about_missing_entities`: ingest a retained settings doc that names an entity unknown to the registry → `alarm_view().warnings` contains it.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test alarm_hub`. Expected: FAIL.
- [ ] **Step 3: Implement** the config, `AlarmHub`, `entity.rs`, `alarm/hub.rs`, the `hub.rs` hooks, `view.rs` and `WsEvent::AlarmChanged`. Fix every exhaustive `match` on `WsEvent` that the new variant breaks: the web `house.rs` ignores it in this task.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test alarm_hub`, `cargo test -p oikos-os config` and `cargo build -p oikos-web --target wasm32-unknown-unknown`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(alarm): hub config, settings and published entity`.

### Task 5: Hub engine — events in, panel state and runtime out, commands

**Files:**
- Create: `crates/oikos-os/src/alarm/engine.rs`
- Modify: `crates/oikos-os/src/alarm/hub.rs` (PanelCommand handling)
- Modify: `crates/oikos-os/src/hub.rs` (`send_command`: before the `group_` check, `if id == ALARM_ENTITY_ID { return Ok(self.alarm_command(cmd).await); }`)
- Modify: `crates/oikos-os/src/main.rs` (`let alarm = hub.start_alarm();`, aborted on shutdown like `climate`)
- Test: `crates/oikos-os/tests/alarm_hub.rs`

**Interfaces:**
- Consumes: Task 4 `AlarmHub`, `ingest_alarm`, `sync_alarm_entity`; Task 2/3 `step`, `next_deadline`, `Event`, `Effect`, `World`.
- Produces:
  - `pub fn start_alarm(self: &Arc<Self>) -> JoinHandle<()>`. Always spawned. It subscribes to `subscribe()` before spawning.
  - `pub async fn alarm_event(self: &Arc<Self>, ev: Event, now: u64)`. It takes `pass`, then:
    1. builds `World` from the registry (watched sensors whose `Scalar::of_state` is on)
    2. locks the runtime, calls `step`, and releases the lock
    3. calls `run_alarm_effects`
    4. publishes the panel state and attributes, then persists the runtime (both retained, then ingested)
    5. broadcasts `AlarmChanged` when the state name or deadline changed

    It is a no-op while disabled.
  - `pub(crate) async fn run_alarm_effects(self: &Arc<Self>, effects: Vec<Effect>)`. Task 6 fills in `SirensOn/Off/Chirp/Alert/ResolveAlerts/OpenAtArm`; here it handles none of them and only logs.
  - `pub(crate) async fn alarm_command(self: &Arc<Self>, cmd: Command) -> CommandResponse`:
    - `Command::Arm{mode: Away|Home|Night, ..}` → `Event::Arm{source: Command}`
    - `Command::Disarm{..}` → `Event::Disarm{source: Command}`
    - another mode or command → `accepted: false` with a reason
    - disabled → `accepted: false, reason: "alarm disabled"`
    - it returns `accepted: true` with the new entity state
  - Raw command on `<p>/panel/set` (non-retained): payload `ARM_AWAY` / `ARM_HOME` / `ARM_NIGHT` / `DISARM` → the same path. Spawn it the way `on_group_command` is spawned in `ingest_message`.
  - Panel attributes JSON: `{"source", "since", "deadline", "cause", "open_at_arm"}`.

Loop (mirror `climate_loop` in `climate/engine.rs`):
1. Wait for `ConnectionStatus::Online`, then `BOOT_SETTLE` (reuse the climate constant or define `ALARM_BOOT_SETTLE = 5s`).
2. Send `Event::Resume`. Tracker reconciliation is Task 6.
3. `select!` over:
   - **`events.recv()`:**
     - `StateChanged{id}` for a watched sensor → compute on/off with `Scalar::of_state`. Keep a `known: HashMap<String, bool>`, and send `Event::Sensor` only when both the previous and the new values are known and different. Clear `known` on `ConnectionChanged(!Online)`, as `flows/engine.rs` `handle_event` does.
     - `StateChanged{id}` for a tracker → `Event::Tracker` (only on a real change, the same rule).
     - `SignalReceived{id, signal}` for a remote → `Event::Signal`.
     - `AlarmChanged` → recompute the deadline.
     - `Lagged` → recompute the deadline.
   - **`sleep_until(next_deadline)`** → `Event::Tick`.

- [ ] **Step 1: Write the failing tests** (`#[tokio::test(start_paused = true)]`, a driver that pumps `rx` and ingests the hub's own retained publishes back, as the climate `Rig` does):
  - `replayed_states_never_trigger`: enable, arm away by `send_command`, advance 61 s → `armed_away`. Simulate a reconnect (`ConnectionChanged(Offline)`, then `Online`) and ingest the door contact retained as `ON` (a replay with no known previous value) → still `armed_away`.
  - `door_open_triggers_via_registry`: armed away, door contact goes `OFF` → `ON` → the entity state is `triggered`, and the attributes `cause` is `["<door id>"]`.
  - `send_command_arms_and_disarms`: `send_command(ALARM_ENTITY_ID, Command::Arm{mode: Away, code: None})` → accepted, state `arming`. `Disarm` → `disarmed`.
  - `raw_mqtt_command_arms`: ingest a non-retained `ARM_NIGHT` on `oikos/alarm/panel/set` → `arming`. A retained `ARM_NIGHT` → ignored.
  - `vacation_mode_refused`: `Arm{mode: Vacation}` → `accepted: false`.
  - `runtime_persisted_retained`: after arming, the last retained publish on `oikos/alarm/state` parses as a `Runtime` in `Arming`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test alarm_hub`. Expected: FAIL on the new tests.
- [ ] **Step 3: Implement** the engine, the command paths and the panel and runtime publication.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test alarm_hub` and `cargo test -p oikos-os --lib`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(alarm): engine loop, commands and panel state`.

### Task 6: Hub effects — sirens, chirp, push and replies, availability, boot reconcile

**Files:**
- Create: `crates/oikos-os/src/alarm/effects.rs`
- Modify: `crates/oikos-os/src/alarm/engine.rs` (tracker reconciliation after `Resume`; availability watch)
- Modify: `crates/oikos-os/src/hub.rs`:
  - the responses branch of `ingest_message` subscribes when `self.climate.is_some() || alarm enabled`. A reply whose `id` starts with `alarm-` goes to `on_alarm_reply`; everything else goes to `on_climate_reply`.
  - `extra_filters` adds `push.responses_topic()` when the alarm is enabled.
- Test: `crates/oikos-os/tests/alarm_hub.rs`

**Interfaces:**
- Consumes: Task 5 `run_alarm_effects` (filled in here), `Hub::notify`, `Hub::send_command`, `Hub::publish`.
- Produces:
  - `run_alarm_effects` handling:
    - **SirensOn / SirensOff:** for each siren, an entity siren gets `send_command`: a switch gets `Command::TurnOn { .. }` with every option `None`, or `Command::TurnOff`; a generic siren gets the generic control command with control `"power"` (check the exact variant in `oikos-core/src/model/command.rs`; it is what MCP `send_command` builds from `{"type":"control",…}`). An mqtt siren gets `publish{topic, on|off, retain: false}`. Commands run concurrently (`join_all`). A failure is collected, never returned early.
    - **Chirp:** for each siren with a chirp: mqtt → publish `chirp`; entity with `chirp_ms` → on, `sleep(chirp_ms)`, off, spawned so the step doesn't wait.
    - **Alert{cause}:** `notify("alarm", {v:1, id:"alarm-<now>-<n>", title, body, channel:"alarm", severity:"critical", actions:[{id:"disarm", label:"Disarm"}], entity_id: ALARM_ENTITY_ID})`. Push the id onto `runtime.alert_ids`.
      - The title is "Alarm".
      - The body is "<names> opened" for openings, "<n> motions (<names>)" for motion, "SOS (<remote name>)" for SOS. Names are the registry display names.
      - Siren failures from the same effect batch are appended as "Siren failed: <name or topic> (<reason>)".
    - **ResolveAlerts:** `notify("alarm", {v:1, id, cancel:true})` for each id, then clear `alert_ids`.
    - **OpenAtArm(list):** `notify` with severity `info`, `silent: true`, body "Armed <mode>. <names> open, watched for closing".
  - `pub(crate) fn on_alarm_reply(self: &Arc<Self>, payload: &[u8])`: action `disarm` on an id in `alert_ids` (or any `alarm-` id while not disarmed) → `alarm_event(Disarm{source: Command})`. Any other reply is ignored.
  - **Availability:** a watched sensor going `Availability::Offline` while armed → one warning alert, id `alarm-offline-<entity>`, body "Alarm sensor offline: <name>". It is cancelled when the sensor is back online, and on disarm.
  - **Boot reconcile:** after `Resume`, for each configured tracker whose live presence (`home` / `not_home` / anything else = Unknown) differs from `runtime.trackers`, send `Event::Tracker`.
  - **Disable** (`alarm_disable`, called by Task 4's `set_alarm_settings`): `SirensOff` if triggered, `ResolveAlerts`, cancel the offline alerts, reset the runtime to the default, persist it.

- [ ] **Step 1: Write the failing tests:**
  - `trigger_rings_both_siren_kinds_and_pushes`: settings with one switch siren and one mqtt siren `{topic:"dev/siren/set", on:"ON", off:"OFF", chirp:"BEEP"}`. Armed (drain), then the door opens →
    - a `turn_on` publish on the switch's command topic
    - a publish `ON` on `dev/siren/set`
    - an alert in `hub.alerts()` with channel `alarm`, severity `critical` and one action `disarm`
  - `ring_stops_after_siren_s`: advance 180 s → the switch is commanded off and `dev/siren/set` gets `OFF`. The state is `armed_away` again, and the alert is still listed and unresolved.
  - `arming_chirps`: arming publishes `BEEP` on `dev/siren/set` once, when the armed state is entered (not at the arm command).
  - `push_disarm_reply_disarms`: ingest a non-retained reply `{id:<alert id>, action:"disarm"}` on `oikos/push/responses` → state `disarmed`, sirens off, alert resolved.
  - `failing_siren_does_not_block_others`: the switch siren's entity is offline → the mqtt siren still gets `ON`, and the alert body contains "Siren failed".
  - `open_window_at_arm_sends_silent_note`: the window is `ON` when the exit delay ends → an alert with severity `info` and `silent: true` naming the window.
  - `offline_sensor_warns_once_and_resolves`: armed, the sensor goes offline twice → one `alarm-offline-<id>` alert; back online → resolved.
  - `tracker_change_while_down_disarms_on_boot`: a retained runtime `armed_away` with `trackers {t: Away}` and the live tracker `home` → after boot settle the state is `disarmed`.
  - `restart_resumes_ringing`: a retained runtime `Triggered{until: now+100}` → after boot the sirens are commanded on, and they are commanded off ~100 s later.
  - `disable_while_ringing_silences`: triggered, then `set_alarm_settings(enabled:false)` → sirens off, alert resolved, the entity config is emptied.
  - `climate_replies_still_routed`: with climate on, a reply whose id isn't `alarm-` still reaches `on_climate_reply` (assert through an existing climate proposal test helper, or by checking that the alarm state is unchanged and no panic happens).
- [ ] **Step 2: Run** `cargo test -p oikos-os --test alarm_hub`. Expected: FAIL on the new tests.
- [ ] **Step 3: Implement** the effects, the reply routing, the availability watch and the boot reconcile.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test alarm_hub` and `cargo test -p oikos-os --test climate_hub`. Expected: PASS (no climate regressions).
- [ ] **Step 5: Commit** `feat(alarm): sirens, chirp, push with Disarm, boot reconcile`.

### Task 7: HTTP and MCP

**Files:**
- Modify: `crates/oikos-os/src/http/api.rs` (routes `GET /alarm` → `AlarmView`, `PUT /alarm/settings` → 204, `422` on `HubError::Rejected`, like `put_climate_house`)
- Create: `crates/oikos-os/src/mcp/alarm_tools.rs` (router `alarm_tool_router`, same shape as `climate_tools.rs`)
- Modify: `crates/oikos-os/src/mcp/mod.rs` (combine the router; add one paragraph to the server instructions)
- Modify: `crates/oikos-os/src/mcp/params.rs` (`SetAlarmSettingsParams { pub settings: AlarmSettings }`)
- Test: the existing HTTP and MCP test files (follow the climate tests there)

**Interfaces:**
- Consumes: Task 4 `alarm_view`, `set_alarm_settings`.
- Produces:
  - MCP `alarm_status` (`read_only_hint`) → `AlarmView`
  - MCP `set_alarm_settings` (`idempotent_hint`, `log_write` first) → the fresh `AlarmView`
  - The instructions paragraph says:
    - the alarm is optional (`enabled` in its settings)
    - arming and disarming go through `send_command` on `oikos_alarm_house` with `{"type":"arm","mode":"away"}` / `{"type":"disarm"}`
    - no codes
    - read `alarm_status` before replacing the settings

- [ ] **Step 1: Write the failing tests:** `get_alarm_returns_view` (200, `enabled:false` on a fresh hub); `put_alarm_settings_validates` (422 on `siren_s: 5`, then 204 on a valid doc); `mcp_lists_alarm_tools` (the tool list contains both names); the server-instructions test asserts the text mentions `oikos_alarm_house`.
- [ ] **Step 2: Run** `cargo test -p oikos-os`. Expected: FAIL on the new tests.
- [ ] **Step 3: Implement** the routes, the tools and the params.
- [ ] **Step 4: Run** `cargo test -p oikos-os`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(alarm): HTTP routes and MCP tools`.

### Task 8: Web — Settings → Alarm page, AlarmHero countdown and cause, device-sheet line

**Files:**
- Modify: `crates/oikos-web/Cargo.toml` (add `oikos-alarm`)
- Create: `crates/oikos-web/src/api/alarm.rs` (`pub async fn get_alarm() -> Result<AlarmView, ApiError>`, `pub async fn put_alarm_settings(s: &AlarmSettings) -> Result<(), ApiError>`, following `api/climate.rs`)
- Modify: `crates/oikos-web/src/api/mod.rs` (export it)
- Create: `crates/oikos-web/src/screens/alarm_settings.rs` (`#[component] pub fn AlarmSettingsPage() -> impl IntoView`)
- Modify: `crates/oikos-web/src/screens/settings.rs`:
  - `SettingsPage::Alarm`, after `Home` in `ALL` (the array grows to 7)
  - slug `"alarm"`, key `Key::Alarm`, hint "Sensors, sirens, remotes and presence"
  - status text: "Off" when disabled, otherwise the translated state
  - match arm in `SettingsPageScreen`
- Modify: `crates/oikos-web/src/shell/route.rs` (breadcrumb, if it lists pages explicitly)
- Modify: `crates/oikos-web/src/house.rs` (`alarm_rev: RwSignal<u64>`, bumped on `WsEvent::AlarmChanged`)
- Modify: `crates/oikos-web/src/security.rs` (`PanelUi` gains `deadline: Option<u64>`, `cause: Vec<String>` and `source: Option<String>`, read from the entity state's `attributes`)
- Modify: `crates/oikos-web/src/components/alarm_hero.rs` (a countdown line while `arming` or `pending`, from `deadline`; a cause line while `triggered`, with display names)
- Modify: `crates/oikos-web/src/override_editor.rs` (a read-only "In alarm: …" line linking to `/settings/alarm` for an entity listed in the alarm settings)
- Modify: `crates/oikos-web/src/i18n.rs` (every new `Key` with en/fr)

**Interfaces:**
- Consumes: Task 4 `AlarmView`, `AlarmSettings`, `default_sensor`, `default_signals`; Task 7 routes.
- Produces: pure helpers with unit tests (the web crate has no DOM tests; keep the logic in pure functions):
  - `pub fn sensor_candidates(entities: &[EntityView]) -> Vec<(String, SensorKind)>`: binary sensors whose `device_class` is in {door, window, opening, garage_door} → Opening; in {motion, occupancy, presence} → Motion. Entities already listed are excluded.
  - `pub fn alarm_line(settings: &AlarmSettings, entity: &str, lang: Lang) -> Option<String>`, for example "In alarm: delayed opening · away, home, night" / "Dans l'alarme : ouverture temporisée · absent, présent, nuit"
  - `pub fn countdown_text(deadline: u64, now: u64) -> String` → "0:27"
  - `pub fn settings_status(view: &AlarmView, lang: Lang) -> String`

Page sections (§8):
- an Enable switch
- **Sensors:** a table with kind, three mode checkboxes and an instant/delayed toggle (openings only), with an "Add" picker from `sensor_candidates`. Above the table, a hint saying to tick "home" on the outside doors and to set entry doors to delayed.
- **Sirens:** an entity picker (switches, generic sirens), or an "MQTT topic" form (topic, on, off, chirp) with a "Test chirp" button that publishes the chirp through a new `POST /api/v1/alarm/test-chirp`. *(Add that route in this task, `Hub::alarm_test_chirp(index)`.)*
- **Remotes:** a picker of `device_automation.*` / `event.*` entities, pre-filled with `default_signals()`, with an editable signal → action list. When no signal entity exists, the empty state reads "A remote shows up here after its first button press."
- **Presence:** a tracker picker plus the two switches and `arm_after_s`.
- **Timing:** entry, exit, siren, motion count and window.
- **Save:** PUT; show `settings_invalid` and HTTP 422 reasons inline. Copy the draft/merge/error handling from `screens/home_settings_panel.rs`.

- [ ] **Step 1: Write the failing tests** for the pure helpers:
  - `sensor_candidates_classifies_by_device_class`
  - `alarm_line_en_fr`
  - `countdown_formats_minutes_seconds` (27 → "0:27", 75 → "1:15", a past deadline → "0:00")
  - `settings_status_off_when_disabled`
  - `settings_page_alarm_slug_roundtrip` (`SettingsPage::from_slug("alarm") == Some(SettingsPage::Alarm)`)
- [ ] **Step 2: Run** `cargo test -p oikos-web`. Expected: FAIL.
- [ ] **Step 3: Implement** the helpers, the API client, the page, the hero additions, the device-sheet line, the i18n keys and the test-chirp route (hub test `test_chirp_publishes_chirp` in `alarm_hub.rs`).
- [ ] **Step 4: Run** `cargo test -p oikos-web`, `cargo test -p oikos-os --test alarm_hub` and `cargo build -p oikos-web --target wasm32-unknown-unknown`. Expected: PASS. Then run the hub locally against a broker and screenshot `/settings/alarm` and `/security`, in both themes and both languages, through headless Chrome over CDP ([reference: headless Chrome CDP screenshots]). Check that nothing overflows at a 500 px width.
- [ ] **Step 5: Commit** `feat(web): alarm settings page, hero countdown and device-sheet line`.

### Task 9: Docs

**Files:**
- Create: `docs/users/alarm.md`, modelled on `docs/users/climate.md`. It covers:
  - what the alarm does, and that it's optional
  - enabling it
  - the modes table: away = everything; night = openings; home = the openings you tick
  - instant and delayed openings, and the motion rule
  - open windows: watched for closing, tilt-and-turn windows, and the ajar-casement limit (§4.3)
  - sirens: entity, or MQTT with payloads (generic example only)
  - remotes, with the troubleshooting from §6.2: first press, battery, re-pair, `0x0000000000000000` addresses, IAS enrolment
  - presence: edge-triggered, and a disarm while away stays disarmed
  - the push with Disarm
  - restarts
  - moving off another controller: enable here, test, then remove the other controller's alarm and its retained discovery topic
- Modify:
  - `docs/reference/configuration.md` (`## [alarm]`)
  - `oikos.example.toml` (a commented `[alarm]` block)
  - `docs/reference/mqtt-topics.md` (`<p>/settings`, `<p>/state`, `<p>/panel`, `<p>/panel/set`, `<p>/panel/attributes` and the discovery config topic)
  - `docs/reference/mcp-tools.md` (`## Alarm`)
  - `docs/reference/http-api.md` (`## Alarm`, the test-chirp route, `AlarmChanged` in the WebSocket section)
  - `docs/users/security.md` (the native alarm, no codes, the LAN caveat)
  - `docs/users/push-notifications.md` (the `alarm` channel and the Disarm action)
  - `docs/users/web-ui.md` (Settings → Alarm)

- [ ] **Step 1: Write the pages.** No house-specific ids, names or IPs.
- [ ] **Step 2: Run** `cd website && npm ci && npm run build`. Expected: build OK, no broken links.
- [ ] **Step 3: Commit** `docs(alarm): user guide and reference`.

---

## Spec coverage check (self-review)

| Spec | Task |
|---|---|
| §3 states, modes, return after ring, SOS from disarmed | 2, 3 |
| §4.1 sensors, defaults per kind | 1, 8 |
| §4.2 rules: instant, delayed, motion window, pending | 2 |
| §4.3 open at arm, watched for closing, checked at exit end | 2, 6 |
| §4.4 unavailable sensors warn | 6 |
| §5.1 sirens (entity, mqtt), bounded ring, failure isolation | 2, 6 |
| §5.2 push, Disarm action, silent open note, reply routing | 6 |
| §5.3 chirp | 2, 6 |
| §6.1 presence (edge, grace, unknown blocks) | 3, 5, 6 |
| §6.2 remotes, SOS, empty-state hint | 3, 8, 9 |
| §6.3 entity, send_command, raw command, flows, attributes, sources | 4, 5, 8 |
| §7 on/off in doc, shape, validation, run-time warnings | 1, 4 |
| §8 web page, hero, device sheet, i18n | 8 |
| §9 restarts, presence reconcile | 3, 6 |
| §10 MCP, HTTP, `AlarmChanged` | 4, 7 |
| §11 docs | 9 |
| §12 tests | every task |
