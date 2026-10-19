# Oikos-os — Web UI: Energy and Security tabs

Status: design, 2026-10-19. Parent: `2026-10-18-oikos-os-design.md` §Web UI, which said: "entities the app moves to its Energy and Security tabs stay on the web Home, because v1 has no other screen for them. When those screens arrive, the web Home adopts the app's placement rule." This is that wave. Reference: the Android app's `2026-10-02-oikos-energy-design.md`, `2026-10-02-oikos-security-design.md` and `2026-10-03-oikos-security-actions-design.md` (in `SourcesPerso/Oikos/docs/superpowers/specs/`).

## 1. Intent

The web UI served by the hub gets the two tabs the Android app has next to Home: **Energy** (the live power flow of the house) and **Security** (the alarm panel, the safety and intrusion sensors, and the opted-in actions such as the siren). Same entities, same rules, same words as the app, so a browser on the LAN shows what the phone shows. Home then stops showing what those tabs own, exactly like the app.

The request was "add Security and Energy tabs, the same as in the app". Everything below that is not in the app is marked as the builder's call.

## 2. Goals, non-goals, anti-goals

Goals:
1. `/energy` and `/security` routes, linked from the top bar right after Home.
2. Every rule that decides *what* is shown and *how it is computed* is a port of the app's pure Kotlin (`Placement`, `SensorGroup`, `SecurityUiBuilder`, `EnergyFlowBuilder`, `EnergyMeters`, `PowerUnits`/`EnergyUnits`/`VolumeUnits`), lives in `oikos-core` or `oikos-web` as pure Rust, and is unit-tested natively.
3. Web Home adopts `Placement::on_home`: an entity with an energy role or shown on Security leaves Home unless `homePinned`.
4. Live data only, from the snapshot and the WebSocket; no new hub endpoint, no new config.

Non-goals (deferred, listed in §9):
- "kWh today", sensor "since 5 min" and water "today": they need the OikosHistory service, which the hub does not know and which is not reachable from the dev machine.
- The app's history detail screens, widgets, voice.
- A bottom tab bar: the web keeps its top bar.

Anti-goals:
- Diverging from the app's maths or words. If the app says "Leak / Dry", the web says "Leak / Dry".
- A different placement rule between Home, Energy and Security, so an entity is never on two tabs or on none.

## 3. Constraints

- Leptos 0.7 wasm, Trunk, the existing `styles/tokens.css` themes (Night, Warm, Auto). Phone width first (min ~360 px), no horizontal scroll.
- `oikos-core` compiles to native and wasm; no I/O there.
- `UiOverride.energy_role`, `energy_icon`, `energy_meter` are strings in the hub (unknown values round-trip). The port parses them leniently: an unknown role or icon is ignored, as in the app's `coerceInputValues`.
- Commands go through the existing `Sender::send` / `fire_command` path (pending dot, toast on failure, `CommandFailed` from the hub).
- No browser dialogs (`window.confirm`): confirmation is in-page.

## 4. Shared rules (`oikos-core`)

### 4.1 `model/placement.rs`

```rust
pub enum SensorGroup { Smoke, Co, Gas, Leak, Sos, Opening, Motion, Vibration, Tamper }
impl SensorGroup {
    pub fn of(device_class: Option<&str>) -> Option<Self>; // app's SensorGroup.of
    pub fn danger(self) -> bool;                            // Smoke, Co, Gas, Leak, Sos
    pub const ALL: [SensorGroup; 9];                        // danger first, the app's enum order
}
pub enum SensorSection { Openings, Motion, Fire, Water, Other }
impl SensorSection { pub fn of(group: SensorGroup) -> Self; pub const ALL: [SensorSection; 5]; pub fn title(self) -> &'static str; }
pub struct Placement;
impl Placement {
    pub fn security_kind(kind: Kind, device_class: Option<&str>) -> bool;
    pub fn action_kind(kind: Kind) -> bool;                 // Switch | Button
    pub fn on_security(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool;
    pub fn elsewhere(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool; // energy_role.is_some() || on_security
    pub fn on_home(kind: Kind, device_class: Option<&str>, ui: &UiOverride) -> bool;   // !hidden && (home_pinned || !elsewhere)
}
```

Re-exported from `oikos_core::model`. `EntityView::is_primary` and `AreaView::primary_count` are unchanged (they still mean "shown by default" for the API and MCP); the web Home applies `on_home` on top and computes its own chip counts.

### 4.2 `energy/` module

- `units.rs`: `PowerUnits::factor(unit) -> Option<f64>` (`W`/`w`=1, `kW`/`kw`=1000 case-insensitive; `mW`=0.001, `MW`=1e6 exact case), `EnergyUnits::factor` (`Wh`=0.001, `kWh`=1, `MWh`=1000, case-insensitive on the kilo/mega prefix the way the app does), `VolumeUnits::is_volume` / `is_meter_scale` (`m³`, `m3`, `ft³`, `CCF` are meter-scale; `L`, `l`, `gal`, `mL`… are volume). `EnergyRole::parse(&str) -> Option<EnergyRole>` with `water_heater` → `Consumer`; `EnergyRole::{signed, power}`. `ConsumerIcon::parse` for the seven app keys.
- `meters.rs`: a port of `EnergyMeters`: `counters`, `meter_for`, `auto`, `volumes`, `water_meter_for`, `water_auto`, `reverse_for`, `node_meters`, `house_meters` (plus/minus). Label normalisation and the `…_power_a_…` ↔ `…_energy_a_…` twin rule are the app's.
- `flow.rs`: `EnergyFlow::build(entities: &[EnergyEntity], now_ms: u64, previous: Option<&ConsumerOrder>) -> EnergyUi`. `EnergyEntity` is a small input struct (`id, device_id, name, label, unit, state value, availability, ui`) built from `EntityView` by the web, so the core stays free of API types. The output mirrors the app's `EnergyUiState` minus history fields: `loading, empty, diagram, cards, consumers, self_consumption, hint, order, sensors`. `EnergyNode { id, kind, name, value, unit, flow, status, count, sensor_ids, icon, total_kwh, total_status, has_meter, water_total, water_total_unit }`. Constants: `REORDER_MS = 5000`, `IDLE_W = 10`, `SELF_CONSUMPTION_MIN_W = 50`, `ARC_SLOTS = 3`, `CONSUMER_PREFIX = "consumer:"`.
- The web has no "stale" (cached) values, so `NodeStatus` is `Live | Unavailable`; `Availability::Unknown` counts as live, as the app treats "no availability topic".

## 5. Navigation and Home

- Top bar links, in order: **Home · Energy · Security · Alerts · Flows · Devices · Settings**. Routes `/energy` and `/security`.
- Builder's call: the Energy and Security links carry a small glyph (bolt, shield) that takes the solar colour when solar ≥ 10 W, and the warning colour when any panel is armed/arming/pending/triggered, as the app tints its tab icons. Text stays, so the bar works without colour.
- Home: `entity_ids` filters with `e.is_primary() && Placement::on_home(e.kind, e.device_class, &e.ui)` (is_primary already covers hidden/folded/diagnostic). Chip counts are computed from the same filter; "Favorites" keeps `ui.favorite` on top of it. Empty-room text unchanged.

## 6. Security screen (`/security`)

Order, top to bottom (app §4.3 and actions §3.2): title row, alarm hero(s), Actions, summary line, sensor sections, Recent alerts.

- **Visible set:** `part_of.is_none() && Placement::on_security(...)`. `hidden` does not remove an entity from Security; only `securityHidden` does (and `securityAction` adds).
- **Alarm hero** (`components/alarm_hero.rs`): one per panel, in `order` then name. Colour class by state: `disarmed` mint, `armed_*` violet, `arming` violet pulsing, `pending` amber pulsing, `triggered` rose filled, `disarming` mint pulsing, unavailable/unknown greyed. State text: "Disarmed", "Armed · Away" (Home/Night/Vacation/Custom bypass), "Exit delay…", "Entry delay…", "Triggered!", "Disarming…", "Unavailable", "—". Buttons: arm modes from `capabilities.alarm_modes` in `AlarmMode` order (current one marked), Disarm when armed/arming/pending/triggered; `triggered` shows Disarm only, full width; `disarming` shows none. Code: when `code_arm_required`/`code_disarm_required`, an inline code field (numeric or text per `code_format`) above the buttons; a button that needs a code is disabled until something is typed; the field clears after sending. (Builder's call: inline field instead of the app's keypad sheet; same as the Home alarm card today.) The header is one `aria-live="polite"` region (`assertive` when triggered). Pulses obey `prefers-reduced-motion`.
- **Actions** (only when non-empty): title "Actions", two-column tiles. Line 1: plug glyph (switch, amber when on in dark themes) or play glyph (button) and the name. Line 2: area ("Unassigned" when none) and "On" / "Off" / "Unavailable" (nothing for an available button). Tap: switch on → `TurnOff` at once; switch off/unknown or button → when `securityAsk` the tile turns into a confirm state ("Turn on Siren?" / "Press X?", buttons "Turn on"/"Press" and "Cancel"); without ask it sends at once. The confirm state closes on its own if the entity disappears or becomes unavailable. Unavailable tiles are disabled. Long-press is not a web gesture: the tile's name is a link to the entity sheet, where the override editor already has "Show on Security" (`securityAction`), "Ask before turning on" (`securityAsk`) and "Hidden from security" (`securityHidden`).
- **Summary line:** counts of active, available sensors by group, danger groups first in `SensorGroup::ALL` order: "1 smoke · 2 open · 1 motion" with the words smoke, CO, gas, leak, SOS, open, motion, vibration, tamper (plural never changes the word, as in the app's strings), or "All clear". `aria-live="polite"`.
- **Sections:** Openings, Motion, Fire, Water, Other; empty ones left out. Within a section, tiles in stable order: area name (Unassigned last), then name, then id. Tiles do not move when a state flips.
- **Sensor tiles:** two per row, same height. Line 1: device-class glyph (door, window, motion, occupancy, smoke, drop for moisture, shield for safety, waves for vibration, lock-open for tamper) and name. Line 2: area and the state word: Open/Closed, Motion/Clear, Detected/Clear (smoke, CO, gas), Leak/Dry, SOS/OK, Vibration/Still, Tampered/OK; "Unavailable" when offline; "—" when no state yet. Active danger: rose border and word; active intrusion: amber. Unavailable: greyed. Names and areas are cut with an ellipsis, never the state word. The tile is a link to the entity sheet. `aria-label` "Garage door, garage, open".
- **Recent alerts:** the existing `RecentAlerts` component (alarm, security, camera channels).
- **Empty cases:** no panel → a slim one-line card "No alarm panel. Publish an MQTT `alarm_control_panel` to see it here."; no sensors → "No security sensors."; nothing at all (no panel, no sensors, no actions) → one combined empty state "Nothing on Security yet." The skeleton shows until the first snapshot, as on Home.

## 7. Energy screen (`/energy`)

- **Header:** title "Energy", subtitle "Live · self-consumption 87 %" or "Live". The hint "Check the grid or battery sign" appears under it when the derived house went negative.
- **Diagram:** an inline `<svg viewBox="0 0 260 250">` in a glass card (`.energy-diagram`), node centres at the app's layout-B fractions: solar (0.5, 0.12), house (0.5, 0.52), grid (0.12, 0.52), bottom-left (0.15, 0.86) for battery else water; consumers on the arc slots `1: [(0.86,0.52)]`, `2: [(0.86,0.34),(0.86,0.72)]`, `3: [(0.82,0.2),(0.88,0.52),(0.82,0.84)]`. Each node: a circle (r 22) with the role glyph (sun, pylon, battery, house, plug or consumer icon, drop; water dashed outline), the label under it (role name or consumer name, ellipsised) and the value label ("3.2 kW" / "253 W" / "—"; grid adds ↙ import / ↗ export; water shows its own unit). Edges are the app's cubic paths node ↔ house with a 26 px clearance (solar leaves below its labels). Stroke width 1 + 5·level where level = clamp((log10 W − 1)/3, 0, 1); idle or unknown edges are static at alpha 0.25 with no dash. Live edges animate `stroke-dashoffset` with a CSS animation whose duration is 1/speed s (speed 0.5 + 1.5·level cycles per second), reversed for outward flow; `prefers-reduced-motion` keeps the width and stops the dash. One `<title>`/`aria-label` sentence on the svg: "Solar 3.2 kilowatts to the house. House uses 2.8 kilowatts. Exporting 0.4 kilowatts to the grid. Water heater 1.7 kilowatts."
- **"+N" node:** with 4+ consumers the arc shows the 2 largest and a "+N" node whose value is the sum of the rest; tapping it (or the "+N" card) expands a list of every consumer with its value below the cards (`<details>`-style, no sheet). Order: `order` override first, then power high → low, then name, recomputed at most every 5 s (`ConsumerOrder` kept in a signal; `js_sys::Date::now()` in wasm).
- **Cards:** under the diagram, one `.card` per node in `cards` order (Solar, Grid, Battery, House, each consumer, Water): glyph and name, the live value big, and a second line "6 584 kWh" (lifetime, from the paired meters, `has_meter`), or for water "123.4 m³" from its meter index. Unknown → "—"; unavailable → greyed "Unavailable". A card whose node has exactly one sensor opens that entity's sheet; with several (summed role, derived house) it expands to list its sensors as links.
- **Value format:** kW with one decimal at ≥ 1000 W, else whole W; kWh with no decimal above 100, one below; thin-space thousands grouping (`format::power`, `format::energy`).
- **Empty state:** when no entity has an energy role: "No energy roles yet. Open a power sensor and give it an energy role (solar, grid, consumer…)." with a link to Home. Skeleton until the first snapshot.
- **Tab glyph:** bolt tinted when solar ≥ 10 W (§5).

## 8. Architecture

| Crate / file | Change |
|---|---|
| `oikos-core/src/model/placement.rs` | `SensorGroup`, `SensorSection`, `Placement` (+ tests) |
| `oikos-core/src/energy/{mod,units,meters,flow}.rs` | the energy maths (+ tests; the app's `EnergyFlowBuilderTest` cases ported) |
| `oikos-web/src/security.rs` | pure presentation: `SecurityUi::build(&[EntityView], &[AreaView])`, `PanelUi`, `SensorRow`, `ActionRow`, `ActionTap`, summary text, state words (+ tests) |
| `oikos-web/src/energy.rs` | `EnergyEntity::from(&EntityView)`, diagram geometry (`positions`, `edge`, `stroke`, `speed`), value formatting, the aria sentence (+ tests) |
| `oikos-web/src/components/alarm_hero.rs` | the hero |
| `oikos-web/src/screens/{security,energy}.rs` | the screens |
| `oikos-web/src/icons.rs` | sun, pylon, house, plug, play, drop, waves, lock-open, shield (reuse channel glyphs where identical), the seven consumer icons |
| `oikos-web/src/app.rs`, `screens/home.rs` | routes, nav, placement |
| `oikos-web/styles/main.css` | `.hero`, `.tile`, `.tiles`, `.summary`, `.energy-*`, nav glyphs |
| `oikos-web/tests/{security,energy}.rs` | wasm smoke tests: the screens mount with a fixture house and show the expected words |
| `README.md` | the two screens, the Home placement note |

## 9. Deferred (next waves)

- **History:** "x.x kWh today" on Energy cards, water "today", and the sensors' "since 5 min" / "over 30 days". Needs `[history] url` in the hub, a `/api/v1/history/*` proxy (or CORS on OikosHistory) and the app's `EnergyToday` request logic.
- The app's keypad sheet for alarm codes (the web uses an inline field).
- Consumer icon picker: the override editor edits `energyIcon` as text; a chip picker like the app's would be nicer.
- A stale ("last known") state: the web has no cache, so there is nothing to show.

## 10. Testing

- Native: `cargo test -p oikos-core` (placement, units, meters, flow: inversion, grid − export, battery directions, measured vs derived house, unknown propagation, negative house hint, unit conversion and bad unit, summed roles, self-consumption 87 % example, +N fold, order throttle, unavailable, empty), `cargo test -p oikos-web` (security builder: filter, hidden vs securityHidden, actions, order, summary, sections, empty cases; energy geometry and formatting).
- wasm: `wasm-pack test --headless --chrome` with `tests/security.rs` and `tests/energy.rs` mounting the screens with a fixture house.
- Manual: `tools/restart.sh`, then headless Chrome CDP screenshots of `/energy`, `/security` and `/` on the live house at 390 px and 1100 px, both themes.
