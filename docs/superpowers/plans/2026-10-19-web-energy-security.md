# Web Energy and Security Tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the web UI the app's Energy and Security tabs, and make Home follow the app's placement rule.

**Architecture:** The rules that decide what is shown and how it is computed are ported from the app's pure Kotlin into `oikos-core` (`model::placement`, `energy::{units,meters,flow}`) and tested natively; `oikos-web` adds two pure presentation modules (`security.rs`, `energy.rs`), two screens, one hero component and CSS. No hub endpoint or config changes.

**Tech Stack:** Rust 2024, Leptos 0.7 (CSR wasm), leptos_router, inline SVG, Trunk, wasm-pack tests in headless Chrome.

**Spec:** `docs/superpowers/specs/2026-10-19-web-energy-security-design.md`

## Global Constraints

- `oikos-core` has no I/O and compiles to wasm32; `js_sys` only in `oikos-web`.
- Words are the app's: "Open/Closed", "Motion/Clear", "Detected/Clear", "Leak/Dry", "SOS/OK", "Vibration/Still", "Tampered/OK", "All clear", "Unavailable", "Unassigned", "Exit delay…", "Entry delay…", "Triggered!", "Disarming…".
- `IDLE_W = 10.0`, `SELF_CONSUMPTION_MIN_W = 50.0`, `REORDER_MS = 5000`, `ARC_SLOTS = 3`, `CONSUMER_PREFIX = "consumer:"`.
- Layout-B fractions: solar (0.5, 0.12), house (0.5, 0.52), grid (0.12, 0.52), bottom-left (0.15, 0.86); arcs `1: [(0.86,0.52)]`, `2: [(0.86,0.34),(0.86,0.72)]`, `3: [(0.82,0.2),(0.88,0.52),(0.82,0.84)]`; viewBox `0 0 260 250`.
- No `window.confirm`/`alert`; confirmation is in-page.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` stay green; `wasm-pack test --headless --chrome` in `crates/oikos-web` stays green.
- Commits are authored as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`.

## Review Focus

1. An entity with `energyRole` **and** `homePinned` stays on Home and is on Energy too (Task 1 test `pinned_stays_home`).
2. A `grid` sensor whose unit is `kWh` by mistake makes the grid node unknown ("—"), not a 1000× value (Task 4 test `bad_unit_is_unknown`).
3. An opted-in **button** action has no state word and sends `Press`, asking first by default (Task 6 tests `button_tap_asks_by_default`, `button_has_no_state_word`).
4. A triggered panel shows Disarm only, full width, and its live region is assertive (Task 7 wasm test `triggered_hero_shows_disarm_only`).
5. A derived house with an unavailable solar inverter shows "—" and is **not** greyed as unavailable (Task 4 test `unavailable_input_leaves_house_unknown_not_unavailable`).

---

### Task 1: Placement and sensor groups (`oikos-core`)

**Files:**
- Create: `crates/oikos-core/src/model/placement.rs`
- Modify: `crates/oikos-core/src/model/mod.rs` (add `mod placement;` and `pub use placement::{Placement, SensorGroup, SensorSection};`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum SensorGroup { Smoke, Co, Gas, Leak, Sos, Opening, Motion, Vibration, Tamper }
  impl SensorGroup {
      pub const ALL: [SensorGroup; 9]; // in the order above (danger first)
      pub fn of(device_class: Option<&str>) -> Option<SensorGroup>;
      pub fn danger(self) -> bool;
      /// The summary word: "smoke", "CO", "gas", "leak", "SOS", "open", "motion", "vibration", "tamper".
      pub fn word(self) -> &'static str;
  }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum SensorSection { Openings, Motion, Fire, Water, Other }
  impl SensorSection {
      pub const ALL: [SensorSection; 5];
      pub fn of(group: SensorGroup) -> SensorSection;
      pub fn title(self) -> &'static str; // "Openings", "Motion", "Fire", "Water", "Other"
  }
  pub struct Placement;
  impl Placement {
      pub fn security_kind(kind: Kind, device_class: Option<&str>) -> bool;
      pub fn action_kind(kind: Kind) -> bool;
      pub fn on_security(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool;
      pub fn elsewhere(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool;
      pub fn on_home(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool;
  }
  ```

- [ ] **Step 1: Write the failing tests** in `placement.rs` `#[cfg(test)]`: `groups_by_device_class` (door/window/garage_door/opening → Opening; motion/occupancy/presence → Motion; smoke, carbon_monoxide → Co, gas, moisture → Leak, safety → Sos, vibration, tamper; `None` and `"battery"` → None), `danger_subset` (Smoke, Co, Gas, Leak, Sos true; others false), `sections` (Fire = Smoke|Co|Gas, Water = Leak, Other = Sos|Vibration|Tamper), `security_kind_and_actions` (AlarmPanel true; BinarySensor door true; BinarySensor battery false; Switch/Button `action_kind` true, Light false), `on_security_rules` (door + `security_hidden` → false; switch + `security_action` → true; light + `security_action` → false), `pinned_stays_home` (sensor with `energy_role: Some("grid")` → `elsewhere` true, `on_home` false; with `home_pinned` → `on_home` true; `hidden` → false even when pinned).
- [ ] **Step 2: Run** `cargo test -p oikos-core placement` → FAIL (module missing).
- [ ] **Step 3: Implement** the enums and `Placement` as the app's `Placement.kt` / `SensorGroup.kt`.
- [ ] **Step 4: Run** `cargo test -p oikos-core placement` → PASS; `cargo clippy -p oikos-core --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(core): placement rule and sensor groups`.

### Task 2: Energy units, roles and icons (`oikos-core`)

**Files:**
- Create: `crates/oikos-core/src/energy/mod.rs`, `crates/oikos-core/src/energy/units.rs`
- Modify: `crates/oikos-core/src/lib.rs` (`pub mod energy;`)

**Interfaces:**
- Produces (`energy/mod.rs` re-exports everything public from `units`, `meters`, `flow`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum EnergyRole { Solar, Grid, GridExport, Battery, House, Consumer, Water }
  impl EnergyRole {
      pub fn parse(s: &str) -> Option<EnergyRole>; // "water_heater" → Consumer
      pub fn signed(self) -> bool;  // Grid | Battery
      pub fn power(self) -> bool;   // != Water
      pub fn name(self) -> &'static str; // "Solar", "Grid", "Grid export", "Battery", "House", "Consumer", "Water"
  }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum ConsumerIcon { WaterHeater, HeatPump, Oven, Fridge, Washer, Dryer, Dishwasher }
  impl ConsumerIcon { pub fn parse(s: &str) -> Option<ConsumerIcon>; pub fn key(self) -> &'static str; }
  pub struct PowerUnits;  impl PowerUnits  { pub fn factor(unit: Option<&str>) -> Option<f64>; }
  pub struct EnergyUnits; impl EnergyUnits { pub fn factor(unit: Option<&str>) -> Option<f64>; } // → kWh
  pub struct VolumeUnits; impl VolumeUnits { pub fn is_volume(unit: Option<&str>) -> bool; pub fn is_meter_scale(unit: Option<&str>) -> bool; }
  ```
  Units (copy the app's `PowerUnits.kt`, `EnergyUnits.kt`, `VolumeUnits.kt` in `SourcesPerso/Oikos/core/model/src/commonMain/kotlin/com/gekkotron/oikos/model/`): power `W`×1, `kW`×1000 (case-insensitive), `mW`×0.001, `MW`×1e6 (exact case); energy `Wh`×0.001, `kWh`×1, `MWh`×1000 (same case rules); volume: meter-scale `m³`, `m3`, `ft³`, `ft3`, `CCF`; volume also `L`, `l`, `mL`, `gal`, `hL`.

- [ ] **Step 1: Write failing tests** `parses_roles_with_alias`, `power_factors` (`"W"`→1, `"kw"`→1000, `"mW"`→0.001, `"MW"`→1e6, `"mw"`→None, `"kWh"`→None, None→None), `energy_factors`, `volume_units`.
- [ ] **Step 2: Run** `cargo test -p oikos-core energy::units` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(core): energy roles and units`.

### Task 3: Meter pairing (`oikos-core`)

**Files:**
- Create: `crates/oikos-core/src/energy/meters.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces:
  ```rust
  /// The slice of an entity the energy maths need; built by the caller (the web from `EntityView`).
  #[derive(Clone, Debug, PartialEq)]
  pub struct EnergyEntity {
      pub id: String,
      pub device_id: Option<String>,
      pub name: String,          // effective name
      pub label: Option<String>, // discovery label, for `auto` pairing
      pub unit: Option<String>,
      pub value: Option<f64>,    // StateValue::Number, else None
      pub available: bool,       // availability != Offline
      pub role: Option<EnergyRole>,
      pub invert: bool,
      pub order: Option<i32>,
      pub icon: Option<ConsumerIcon>,
      pub meter: Option<String>, // ui.energy_meter
  }
  pub struct EnergyMeters;
  impl EnergyMeters {
      pub fn counters<'a>(all: &'a [EnergyEntity]) -> Vec<&'a EnergyEntity>;
      pub fn meter_for<'a>(power: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>;
      pub fn auto<'a>(power: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>;
      pub fn volumes<'a>(all: &'a [EnergyEntity]) -> Vec<&'a EnergyEntity>;
      pub fn water_meter_for<'a>(water: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>;
      pub fn water_auto<'a>(water: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>;
      pub fn reverse_for<'a>(power: &EnergyEntity, all: &'a [EnergyEntity]) -> Option<&'a EnergyEntity>;
      pub fn node_meters<'a>(sensors: &[&EnergyEntity], all: &'a [EnergyEntity]) -> Vec<&'a EnergyEntity>;
      pub fn house_meters<'a>(by_role: &BTreeMap<EnergyRole, Vec<&'a EnergyEntity>>, all: &'a [EnergyEntity]) -> Option<HouseMeters<'a>>;
  }
  pub struct HouseMeters<'a> { pub plus: Vec<&'a EnergyEntity>, pub minus: Vec<&'a EnergyEntity> }
  ```
  Port `EnergyMeters.kt` (`feature/energy/src/main/kotlin/com/gekkotron/oikos/feature/energy/presentation/EnergyMeters.kt`) rule for rule, including `labelOf` (label, else name), `normalize`, `twin` (`…_power_a_…` ↔ `…_energy_a_…`, `PRODUCED = "energy_produced"`) and `houseMeters`.

- [ ] **Step 1: Write failing tests** with the live house's ids: grid `0xa4c13898428fae3a_power_a_zigbee2mqtt` (label `power_a`) pairs with `…_energy_a_…`; solar `…_power_b_…` pairs with `…_energy_produced_b_…` (production twin) and `reverse_for` gives `…_energy_b_…`; `reverse_for` of the grid gives `…_energy_produced_a_…`; an explicit `meter` wins; a dangling explicit `meter` pairs nothing; a device with one power and one counter pairs them; water `L` sensor auto-pairs with the device's `m³` sensor, and a `m³` water sensor pairs nothing; `house_meters` for {solar, grid} without a house = plus [grid energy_a, solar energy_produced_b], minus [grid energy_produced_a, solar energy_b].
- [ ] **Step 2: Run** `cargo test -p oikos-core energy::meters` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(core): energy meter pairing`.

### Task 4: Energy flow builder (`oikos-core`)

**Files:**
- Create: `crates/oikos-core/src/energy/flow.rs`

**Interfaces:**
- Consumes: Tasks 2–3.
- Produces:
  ```rust
  pub const REORDER_MS: u64 = 5_000; pub const IDLE_W: f64 = 10.0; pub const SELF_CONSUMPTION_MIN_W: f64 = 50.0;
  pub const ARC_SLOTS: usize = 3; pub const CONSUMER_PREFIX: &str = "consumer:";
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum NodeKind { Solar, Grid, Battery, House, Consumer, More, Water }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Flow { In, Out, Idle }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)] pub enum NodeStatus { Live, Unavailable }
  #[derive(Clone, Debug, PartialEq)]
  pub struct EnergyNode {
      pub id: String, pub kind: NodeKind, pub name: Option<String>,
      pub value: Option<f64>, pub unit: Option<String>, pub flow: Flow, pub status: NodeStatus,
      pub count: usize, pub sensor_ids: Vec<String>, pub icon: Option<ConsumerIcon>,
      pub total_kwh: Option<f64>, pub total_status: NodeStatus, pub has_meter: bool,
      pub water_total: Option<f64>, pub water_total_unit: Option<String>,
  }
  #[derive(Clone, Debug, PartialEq)] pub struct ConsumerOrder { pub ids: Vec<String>, pub at_ms: u64 }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum EnergyHint { CheckSign }
  #[derive(Clone, Debug, PartialEq, Default)]
  pub struct EnergyUi {
      pub loading: bool, pub empty: bool,
      pub diagram: Vec<EnergyNode>, pub cards: Vec<EnergyNode>, pub consumers: Vec<EnergyNode>,
      pub self_consumption: Option<u8>, pub hint: Option<EnergyHint>, pub order: Option<ConsumerOrder>,
  }
  impl EnergyUi { pub fn node(&self, id: &str) -> Option<&EnergyNode>; }
  pub struct EnergyFlow;
  impl EnergyFlow { pub fn build(entities: &[EnergyEntity], now_ms: u64, previous: Option<&ConsumerOrder>) -> EnergyUi; }
  ```
  Port `EnergyFlowBuilder.kt` minus `today`, `sensors`, `meters`, `waterMeters`, `link`, `stale`. `loading` when `entities` is empty; `empty` when none has a role.

- [ ] **Step 1: Write failing tests** (the app's `EnergyFlowBuilderTest` cases): `inverts_signed_roles_only`, `grid_minus_export`, `battery_directions` (+300 → In, −300 → Out, 5 → Idle), `measured_house_wins`, `derived_house_sums_inputs` (S 3200, G −400 → H 2800, self-consumption 87, grid node value 400 flow Out), `unknown_input_makes_house_unknown`, `unavailable_input_leaves_house_unknown_not_unavailable`, `negative_house_is_zero_with_hint`, `bad_unit_is_unknown` (grid in `kWh` → grid value None, flow Idle), `summed_roles_add_up`, `self_consumption_hidden_under_50w`, `folds_four_consumers_into_more` (2 largest + More count 2 with summed value; `cards` still lists all 4), `order_is_throttled` (a previous order 2 s old with the same ids is kept even if powers changed; 6 s old is recomputed; `order` override first), `water_takes_bottom_left_only_without_battery`, `lifetime_total_from_paired_meters` (grid node `total_kwh` = energy_a reading; derived house total = plus − minus, clamped ≥ 0), `empty_and_loading`.
- [ ] **Step 2: Run** `cargo test -p oikos-core energy::flow` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p oikos-core` → PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(core): energy flow builder`.

### Task 5: Navigation, routes, icons and Home placement (`oikos-web`)

**Files:**
- Modify: `crates/oikos-web/src/app.rs`, `crates/oikos-web/src/screens/home.rs`, `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/src/icons.rs`, `crates/oikos-web/src/lib.rs`, `crates/oikos-web/styles/main.css`
- Create: placeholders `crates/oikos-web/src/screens/energy.rs`, `crates/oikos-web/src/screens/security.rs` (a `#[component] pub fn EnergyScreen()` / `SecurityScreen()` rendering the title only; Tasks 7 and 9 replace them), `crates/oikos-web/src/security.rs` and `crates/oikos-web/src/energy.rs` as empty modules.

**Interfaces:**
- Consumes: Task 1 `Placement`.
- Produces in `icons.rs`: `pub const SUN`, `PYLON`, `HOUSE`, `PLUG`, `PLAY`, `DROP`, `WAVES`, `LOCK_OPEN`, `SHIELD`, `BOLT: &str` (24×24 line SVGs in the existing `svg!` style; `SHIELD` = `CHANNEL_SECURITY`, `BOLT` = `CHANNEL_ENERGY`, `DROP` = `HUMIDITY`), `pub fn consumer_icon(icon: Option<ConsumerIcon>) -> &'static str` (seven glyphs, default `PLUG`), `pub fn sensor_icon(device_class: Option<&str>) -> &'static str` (door, window, garage_door→DOOR, opening→DOOR, motion, occupancy, presence→OCCUPANCY, smoke, carbon_monoxide→SMOKE, gas→SMOKE, moisture→DROP, safety→SHIELD, vibration→WAVES, tamper→LOCK_OPEN, else BINARY_SENSOR), `pub fn role_icon(kind: NodeKind) -> &'static str`.
- Produces in `home.rs`: `pub(crate) fn shows_on_home(e: &EntityView) -> bool` = `e.is_primary() && Placement::on_home(e.kind, e.device_class.as_deref(), &e.ui)`; chips use a `home_counts: BTreeMap<String, usize>` computed from `shows_on_home` instead of `primary_count`.
- Produces in `app.rs`: nav order Home · Energy · Security · Alerts · Flows · Devices · Settings; `<Route path=path!("/energy") view=EnergyScreen />`, `/security` likewise; the Energy and Security links contain `<span class="nav__glyph" class:nav__glyph--solar=… inner_html=BOLT>` / `class:nav__glyph--armed=… inner_html=SHIELD>` driven by two memos: `solar_active` (any entity with `energy_role == Some("solar")` whose numeric value × `PowerUnits::factor` ≥ 10) and `alarm_armed` (any `Kind::AlarmPanel` whose `StateValue::Alarm(s)` is not `disarmed`/unknown/`disarming`).

- [ ] **Step 1: Write failing tests** in `home.rs`: `energy_and_security_entities_leave_home` (a `grid` sensor and a door sensor are not in `entity_ids`; a pinned grid sensor is; a hidden one is not), `chip_counts_follow_home_rule`; in `icons.rs`: `sensor_icons_cover_every_group` (every `SensorGroup`-mapped class gives an svg; `consumer_icon(None) == PLUG`).
- [ ] **Step 2: Run** `cargo test -p oikos-web home icons` → FAIL.
- [ ] **Step 3: Implement** the above. CSS: `.nav__glyph` 16 px inline svg before the text, `--accent-amber` when `--solar`/`--armed`; add `--solar: var(--accent-amber)` to both themes in `tokens.css` if not present; keep the nav scrollable on narrow widths (`overflow-x: auto`, no wrap) since it now has seven links.
- [ ] **Step 4: Run** `cargo test -p oikos-web` → PASS; `cd crates/oikos-web && trunk build` succeeds.
- [ ] **Step 5: Commit** `feat(web): energy and security routes, home placement rule`.

### Task 6: Security presentation (`oikos-web/src/security.rs`)

**Files:**
- Modify: `crates/oikos-web/src/security.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  ```rust
  pub struct PanelUi { pub id: String, pub name: String, pub state: Option<String>, pub available: bool, pub capabilities: Capabilities, pub order: Option<i32> }
  pub struct SensorRow { pub id: String, pub name: String, pub area_name: Option<String>, pub device_class: Option<String>, pub group: SensorGroup, pub active: Option<bool>, pub available: bool }
  pub struct SectionUi { pub section: SensorSection, pub rows: Vec<SensorRow> }
  pub struct ActionRow { pub id: String, pub name: String, pub area_name: Option<String>, pub kind: Kind, pub on: Option<bool>, pub available: bool, pub ask: bool }
  pub struct ActionTap { pub command: Command, pub confirm: bool }
  impl ActionRow { pub fn tap(&self) -> ActionTap; pub fn state_word(&self) -> Option<&'static str>; }
  pub struct GroupCount { pub group: SensorGroup, pub count: usize }
  #[derive(Default)]
  pub struct SecurityUi { pub panels: Vec<PanelUi>, pub summary: Vec<GroupCount>, pub sensors: Vec<SensorRow>, pub sections: Vec<SectionUi>, pub actions: Vec<ActionRow> }
  impl SecurityUi {
      pub fn build(entities: &BTreeMap<String, EntityView>, areas: &[AreaView]) -> SecurityUi;
      pub fn is_empty(&self) -> bool;
      pub fn summary_text(&self) -> String; // "1 smoke · 2 open · 1 motion" or "All clear"
  }
  impl SensorRow { pub fn state_word(&self) -> &'static str; pub fn danger_active(&self) -> bool; pub fn intrusion_active(&self) -> bool; pub fn aria_label(&self) -> String; }
  /// "Disarmed", "Armed · Away", "Exit delay…", "Entry delay…", "Triggered!", "Disarming…", "Unavailable", "—"
  pub fn panel_state_text(state: Option<&str>, available: bool) -> String;
  /// "disarmed" | "armed" | "arming" | "pending" | "triggered" | "disarming" | "off": the hero's modifier class.
  pub fn panel_tone(state: Option<&str>, available: bool) -> &'static str;
  ```
  Port `SecurityUiBuilder.kt` / `SecurityModels.kt`: panels sorted by `order`, lowercase name, id; sensors and actions sorted by (area_name is None, area_name lowercase, name lowercase, id); `summary` counts `available && active == Some(true)` in `SensorGroup::ALL` order; sections in `SensorSection::ALL` order, empty left out. `area_name` is None for `UNASSIGNED_AREA`, else the area's name, else the raw id.

- [ ] **Step 1: Write failing tests** `filters_by_placement` (hidden door stays; `securityHidden` door leaves; `partOf` leaves; opted-in switch is an action), `stable_order_area_then_name`, `summary_words` ("1 smoke · 2 open · 1 motion", "All clear"; unavailable active sensors do not count), `sections_in_order_without_empties`, `state_words_per_group`, `button_tap_asks_by_default`, `button_has_no_state_word`, `switch_on_turns_off_without_asking`, `panel_text_and_tone` (`armed_away` → "Armed · Away"/"armed"; `triggered` → "Triggered!"/"triggered"; `None` available → "—"/"off"; unavailable → "Unavailable"/"off").
- [ ] **Step 2: Run** `cargo test -p oikos-web security` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(web): security presentation`.

### Task 7: Alarm hero and the Security screen

**Files:**
- Create: `crates/oikos-web/src/components/alarm_hero.rs`, `crates/oikos-web/tests/security.rs`
- Modify: `crates/oikos-web/src/screens/security.rs`, `crates/oikos-web/src/components/mod.rs`, `crates/oikos-web/styles/main.css`

**Interfaces:**
- Consumes: Task 6, Task 5 icons, `cards::Sender`, `cards::alarm::mode_label`, `components::alert_row::RecentAlerts`, `components::card_frame::open_entity`.
- Produces: `#[component] pub fn AlarmHero(panel: PanelUi) -> impl IntoView` (`<section class="hero hero--{tone}" aria-live=…>` with `.hero__state`, code `<input class="field">` when required, `.hero__modes` buttons `btn btn--compact` with `btn--active` on the current mode, `.hero__disarm`); `#[component] pub fn SecurityScreen() -> impl IntoView` with `.security`, `.section-title`, `.tiles` (2-col grid), `.tile`, `.tile--danger`, `.tile--intrusion`, `.tile--off`, `.tile--confirm`, `.summary`.

- [ ] **Step 1: Write the failing wasm test** `tests/security.rs` (`run_in_browser`): mount `SecurityScreen` inside `<Router>` with a `House` fed `WsEvent::Snapshot` holding one panel (`armed_away`, modes home/away/night), a door (open, area "garage"), a smoke sensor (clear), a siren switch (`securityAction`, off); assert the DOM text contains "Armed · Away", "1 open", "Garage"… and the tile words "Open", "Clear", "Off"; `triggered_hero_shows_disarm_only`: with state `triggered` exactly one button in the hero, text "Disarm", and `aria-live="assertive"`; `ask_action_confirms_before_sending`: clicking the siren tile shows "Turn on" and "Cancel" and the house's command path is not called (no `.card--pending`), clicking "Cancel" returns to the tile.
- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome --test security` → FAIL.
- [ ] **Step 3: Implement** the hero and the screen per spec §6; the confirm state is a `RwSignal<Option<String>>` of the action id, cleared by an `Effect` when that entity is missing or unavailable; `Memo`s on `house.entities`/`house.areas` for `SecurityUi::build`.
- [ ] **Step 4: Run** the wasm test and `cargo test -p oikos-web` → PASS; `trunk build` ok.
- [ ] **Step 5: Commit** `feat(web): security screen`.

### Task 8: Energy presentation (`oikos-web/src/energy.rs`)

**Files:**
- Modify: `crates/oikos-web/src/energy.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces:
  ```rust
  pub fn energy_entity(e: &EntityView) -> EnergyEntity; // role/icon parsed leniently; available = availability != Offline
  pub fn energy_entities(entities: &BTreeMap<String, EntityView>) -> Vec<EnergyEntity>;
  pub const VIEW_W: f64 = 260.0; pub const VIEW_H: f64 = 250.0; pub const NODE_R: f64 = 22.0; pub const CLEARANCE: f64 = 26.0;
  pub fn positions(diagram: &[EnergyNode]) -> BTreeMap<String, (f64, f64)>; // in viewBox px
  pub struct Edge { pub start: (f64, f64), pub c1: (f64, f64), pub c2: (f64, f64), pub end: (f64, f64) }
  impl Edge { pub fn reversed(&self) -> Edge; pub fn path(&self) -> String; /* "M x y C x1 y1, x2 y2, x y" */ }
  pub fn edge(node: (f64, f64), hub: (f64, f64), clearance: f64, below: Option<f64>) -> Edge;
  pub fn level(watts: Option<f64>) -> f64; pub fn stroke(watts: Option<f64>, flow: Flow) -> f64; pub fn speed(watts: Option<f64>) -> f64;
  pub fn power_text(watts: Option<f64>) -> String;   // "3.2 kW", "253 W", "—"
  pub fn energy_text(kwh: Option<f64>) -> String;    // "6 584 kWh", "32.7 kWh", "—"
  pub fn grid_text(node: &EnergyNode) -> String;     // power_text + " ↙" (In) / " ↗" (Out)
  pub fn node_label(node: &EnergyNode) -> String;    // role name, consumer name, "+N"
  pub fn aria_sentence(ui: &EnergyUi) -> String;
  pub fn now_ms() -> u64; // js_sys::Date::now() on wasm, SystemTime elsewhere
  ```

- [ ] **Step 1: Write failing tests** `positions_follow_layout_b` (solar at (130, 30); 3 arc nodes at the three slots; water at bottom-left only without battery), `edge_keeps_clearance` (grid→house starts at grid.x + 26, ends at house.x − 26; solar uses `below`), `stroke_and_speed` (10 W → 1 and 0.5; 10 kW → 6 and 2; None → 1 / 0.5 with Idle), `texts` ("3.2 kW", "253 W", "—", "6 584 kWh", "32.7 kWh"), `aria_sentence_example` (S 3200, G −400, consumers Water heater 1700, AC 600 → the spec's sentence).
- [ ] **Step 2: Run** `cargo test -p oikos-web energy` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(web): energy presentation and geometry`.

### Task 9: The Energy screen

**Files:**
- Modify: `crates/oikos-web/src/screens/energy.rs`, `crates/oikos-web/styles/main.css`
- Create: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: Tasks 4, 5 (icons `role_icon`, `consumer_icon`), 8.
- Produces: `#[component] pub fn EnergyScreen() -> impl IntoView` with `.energy`, `.energy__subtitle`, `.energy__hint`, `.energy-diagram` (glass card holding the svg), `.energy-edge`, `.energy-edge--idle`, `.energy-edge--out`, `.energy-node`, `.energy-node--water`, `.energy-node--off`, `.energy-cards` (the existing `.grid`), `.energy-card__total`, `.energy-more` (the expanded consumer list), `.energy__empty`.

- [ ] **Step 1: Write the failing wasm test** `tests/energy.rs`: snapshot with grid (W, 253.3), solar (W, 3.0), two consumers, water (L) and their counters (the live house ids from Task 3 tests) → the DOM has four `.energy-node` role/consumer circles plus water, the subtitle "Live", a card "Grid" with "253 W" and "6 584 kWh"; `four_consumers_fold_into_more`: a "+2" node and, after clicking it, a list of four names; `no_roles_shows_empty_state`.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome --test energy` → FAIL.
- [ ] **Step 3: Implement** per spec §7: `Memo<EnergyUi>` rebuilt from `house.entities` with the previous `ConsumerOrder` held in a `StoredValue`; edges as `<path class="energy-edge" style="stroke-width: …; animation-duration: …s">` with `stroke-dasharray: 6 8` and a `@keyframes energy-dash` on `stroke-dashoffset` (reversed with `animation-direction: reverse` for `--out`), static alpha 0.25 for idle; `@media (prefers-reduced-motion: reduce) { .energy-edge { animation: none } }`; the card click opens the entity when `sensor_ids.len() == 1`, else toggles a sensor list.
- [ ] **Step 4: Run** the wasm tests and `cargo test -p oikos-web` → PASS; `trunk build` ok.
- [ ] **Step 5: Commit** `feat(web): energy screen`.

### Task 10: Docs, build, live verification

**Files:**
- Modify: `README.md` (Web UI screens: Energy, Security; Home note: "Home follows the app's placement rule: entities with an energy role or shown on Security leave Home unless pinned"), `docs/superpowers/plans/2026-10-19-web-energy-security-deferred.md` (create; the spec's §9 plus anything found).

- [ ] **Step 1:** `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`; `cd crates/oikos-web && wasm-pack test --headless --chrome` → all green.
- [ ] **Step 2:** `tools/restart.sh` → "hub up"; `curl -s http://127.0.0.1:8081/health`.
- [ ] **Step 3:** Headless Chrome over CDP (see memory `reference_headless_chrome_cdp_screenshots`): screenshots of `/`, `/energy`, `/security` at 390×844 and 1100×800 in the scratchpad; check the Energy diagram shows Grid ↙, Solar, Climatisation, Chauffe eau, water; Security shows the Jeedom panel "Disarmed", the siren action, the sections; Home no longer shows them.
- [ ] **Step 4:** Update README and the deferred doc; commit `docs(web): energy and security tabs`; push `main`.
