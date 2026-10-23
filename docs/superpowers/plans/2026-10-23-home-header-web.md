# Home header (web) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the greeting, mood sentence, chips and weather card on top of the house overview, add a `/weather` page and a "Home" Settings panel, plus two hub threshold settings.

**Architecture:** Pure, natively tested functions (`home_header::summary`, `home_header::mood`, `home_header::weather_fmt`, FR/EN text in `i18n`) compute everything from `House` signals; thin Leptos components wire them. `House` gains the weather and the home settings, refreshed from REST on WS events and on reconnect. The hub only gains `cold_below`/`hot_above` in `HomeSettings`.

**Tech Stack:** Rust workspace, Leptos (wasm, `trunk`), `oikos-api` types, `oikos_core::flow::sun`, `chrono`/`chrono-tz`, `wasm-pack test` for browser tests.

**Spec:** `docs/superpowers/specs/2026-10-23-home-header-web-design.md` (read it first; section numbers below refer to it).

## Global Constraints

- Header only on the house overview (`HomeScreen`), never on authored dashboards.
- Every user-visible string in FR and EN through `crates/oikos-web/src/i18n.rs` (a submodule `i18n/` is fine if the file grows); no literal UI text in components.
- Styling only with theme tokens from `styles/tokens.css` (works in Ivoire and Night Glass); icons as inline SVG in `icons.rs`, same stroke style as the existing ones.
- Defaults: cold 15 °C, hot 26 °C; weather card hidden after 6 h (21 600 s) since `fetched_at`.
- Generic project: no machine names, addresses or the user's rooms in code, tests or docs.
- Commits as `Gekkotron`: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Before building, check `du -sh ~/dev-scratch/cargo-target`; over 5 GB run `cargo clean`.
- Each task ends with `cargo test -p <crate>`, `cargo clippy --all-targets -- -D warnings` on the touched crates and `cargo fmt --check` green.

## Review Focus

- No indoor temperature at all (no `indoor_entity`, no room thermometer): sentence keeps only the activity half, indoor chip hidden — test in Task 3 (`indoor_none_without_sensors`) and Task 4 (`sentence_without_temperature`).
- `indoor_entity` set but offline or non-numeric: fall back to the room mean, never show a stale or blank value — Task 3 `indoor_falls_back_when_setting_offline`.
- Weather off (503) or older than 6 h: card hidden and the sunset moves into the detail line at any hour — Task 4 `sunset_always_without_card`, Task 5 `card_hidden_when_old_or_missing`.
- `mode_entity` pointing at an entity that no longer exists: pill hidden, nothing panics — Task 3 `mode_chip_missing_entity`.
- A thermostat whose state is unknown (`StateValue::Unknown` or `mode: None`): ignored by the heating chip, never turns the chip into "Heating off" by itself — Task 3 `heating_ignores_unknown_state`.

---

### Task 1: Hub — `cold_below` / `hot_above` home settings

**Files:**
- Modify: `crates/oikos-api/src/home.rs` (struct + shape test)
- Modify: `crates/oikos-os/src/home.rs` (`validate` + tests)
- Modify: `crates/oikos-os/src/mcp/params.rs:533`, `crates/oikos-os/src/mcp/tools.rs:46`, `crates/oikos-os/src/mcp/mod.rs:169` (doc strings)
- Modify: `docs/reference/http-api.md:45-46`, `docs/reference/mcp-tools.md`

**Interfaces:**
- Produces: `HomeSettings { eco_setpoint, mode_entity, indoor_entity, cold_below: Option<f64>, hot_above: Option<f64> }` (serialize as `null` when unset; struct keeps `#[serde(default)]`).

- [ ] **Step 1: Failing tests.** In `oikos-os/src/home.rs` tests add `thresholds_bounds_and_order`: `cold_below` 5.0 and 35.0 ok, 4.9 / 35.1 / NaN rejected with the message containing `cold_below`; same for `hot_above`; `cold_below 20, hot_above 20` rejected with a message containing `cold_below`; `cold_below 15, hot_above 26` ok; either alone ok. In `oikos-api/src/home.rs` update the shape test to expect `{"eco_setpoint":null,"mode_entity":null,"indoor_entity":null,"cold_below":null,"hot_above":null}` and add that `{"eco_setpoint":15.0}` (old document) still decodes with both new fields `None`.
- [ ] **Step 2:** `cargo test -p oikos-api -p oikos-os home` → FAIL (unknown fields).
- [ ] **Step 3:** Add the fields (doc comments: "°C, below this the house reads cold; unset = 15" / "above this it reads warm; unset = 26"), constants `THRESHOLD_MIN = 5.0`, `THRESHOLD_MAX = 35.0` and the checks in `validate` (bounds first, then order). Update the three MCP doc strings and the two reference docs (fields, range, order rule, defaults applied by the web).
- [ ] **Step 4:** `cargo test -p oikos-api -p oikos-os` → PASS; clippy, fmt.
- [ ] **Step 5:** Commit `feat(hub): cold_below and hot_above home settings`.

### Task 2: Web — weather and home settings in `House`

**Files:**
- Create: `crates/oikos-web/src/api/home.rs` (`pub mod home;` in `api/mod.rs`)
- Modify: `crates/oikos-web/src/house.rs` (fields, refresh fns, `WeatherChanged`/`HomeSettingsChanged` at :158)
- Modify: `crates/oikos-web/src/ws.rs:33` (refetch both after a `Snapshot`, i.e. on every (re)connect)
- Modify: `crates/oikos-web/src/app.rs:150` (initial refresh next to `refresh_flows_config`)

**Interfaces:**
- Produces:
  - `api::home::get_weather() -> Result<Option<WeatherView>, ApiError>` — `Ok(None)` on HTTP 503, `Err` on other failures.
  - `api::home::get_home_settings() -> Result<HomeSettings, ApiError>`
  - `api::home::put_home_settings(&HomeSettings) -> Result<HomeSettings, ApiError>` (the hub's 422 message comes back in `ApiError.error`).
  - `House.weather: RwSignal<Option<WeatherView>>` (`None` = unavailable; a fetch error sets `None`).
  - `House.home_settings: RwSignal<HomeSettings>` (default until fetched).
  - `House::refresh_weather(&self)`, `House::refresh_home_settings(&self)`.

- [ ] **Step 1: Failing test** in `house.rs` tests: `home_settings_event_updates_signal` — apply `WsEvent::HomeSettingsChanged { settings }` with `eco_setpoint: Some(16.0)` via the existing `apply` path and assert `house.home_settings.get_untracked().eco_setpoint == Some(16.0)`. (`WeatherChanged` only triggers a fetch; covered by Task 5's browser test.)
- [ ] **Step 2:** `cargo test -p oikos-web --lib house` → FAIL.
- [ ] **Step 3:** Implement following `refresh_flows_config` and `api/core.rs` `check`/`read_json` (503 is detected before `check` turns it into an error). `WeatherChanged` → `self.refresh_weather()`. Leave `house_state.rs:79` as a no-op but update its comment.
- [ ] **Step 4:** `cargo test -p oikos-web --lib` → PASS; clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): fetch weather and home settings into House`.

### Task 3: Web — pure house summary

**Files:**
- Create: `crates/oikos-web/src/home_header/mod.rs`, `crates/oikos-web/src/home_header/summary.rs` (`pub mod home_header;` in `lib.rs`)

**Interfaces:**
- Consumes: `EntityView`, `AreaView`, `HomeSettings`, `format::is_on`, `format::entity_title`, `format::room_name`, `UNASSIGNED_AREA`.
- Produces:
```rust
pub struct Opening { pub title: String, pub room: Option<String> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeatingChip { Off, Eco, Cooling }
pub struct ModeChip { pub entity_id: String, pub current: String, pub options: Vec<String> }
pub struct HouseSummary {
    pub indoor: Option<f64>, pub lights_on: usize, pub openings: Vec<Opening>,
    pub unseen_alerts: usize, pub heating: Option<HeatingChip>, pub mode: Option<ModeChip>,
}
pub fn indoor_temperature(entities: &BTreeMap<String, EntityView>, settings: &HomeSettings) -> Option<f64>;
pub fn lights_on(entities: &BTreeMap<String, EntityView>) -> usize;
pub fn openings(entities: &BTreeMap<String, EntityView>, areas: &[AreaView]) -> Vec<Opening>;
pub fn heating_chip(entities: &BTreeMap<String, EntityView>, eco_setpoint: Option<f64>) -> Option<HeatingChip>;
pub fn mode_chip(entities: &BTreeMap<String, EntityView>, mode_entity: Option<&str>) -> Option<ModeChip>;
pub fn summarize(entities: &BTreeMap<String, EntityView>, areas: &[AreaView], settings: &HomeSettings, unseen_alerts: usize) -> HouseSummary;
```

Rules are spec §4.2 and §5, exactly. Reminders: indoor = `indoor_entity` if present, not `Offline` and `StateValue::Number`, else mean of `Kind::Sensor` + `device_class == "temperature"` + area ≠ `UNASSIGNED_AREA` + not `Offline` + numeric + `device_id != Some("oikos_weather")`; lights = `kind == Kind::Light`, id not `group_*`, `!ui.hidden`, category not diagnostic/config, `is_on`; openings = `Kind::BinarySensor`, device_class in `door|window|garage_door|opening`, primary, `!ui.hidden`, `StateValue::OnOff(true)`, sorted by title; heating per spec §5 bullet list (units: primary, `!ui.hidden`, `Kind::Climate`, `capabilities.hvac_modes` contains `"heat"`, state is `StateValue::Climate` with `mode: Some(_)`); mode options = `capabilities.options`, current = the state text.

- [ ] **Step 1: Failing tests** (helper `ent(id, kind)` like `flow_picker::tests::ent`):
  - `indoor_prefers_setting`: setting → 21.0 sensor, two room sensors 18/20 → `Some(21.0)`.
  - `indoor_falls_back_when_setting_offline`: same with the setting sensor `Offline` → `Some(19.0)`.
  - `indoor_mean_skips_unassigned_offline_and_weather`: room 18 + room 20 + unassigned 5 + offline 30 + `oikos_weather` device 9 → `Some(19.0)`.
  - `indoor_none_without_sensors` → `None`.
  - `lights_count_skips_groups_hidden_and_off`: on light, off light, `group_x` on, hidden on light, switch whose `kind` is `Light` and on → `2`.
  - `openings_lists_open_doors_and_windows_with_room`: open window in "Cuisine", closed door, open motion sensor → one `Opening { title, room: Some("Cuisine") }`.
  - `heating_off_when_all_off` (modes `off`, `dry`) → `Some(Off)`; `heating_eco_at_or_below_setpoint` (heat 15 with eco 15, auto 14) → `Some(Eco)`; `heating_hidden_above_setpoint` (heat 20) → `None`; `heating_hidden_without_target` (heat, target None) → `None`; `heating_eco_needs_setpoint` (heat 14, eco None) → `None`; `cooling_wins` (one cool, one heat 20) → `Some(Cooling)`; `cool_only_unit_ignored` (hvac_modes `["off","cool"]`, mode off, alone) → `None`; `heating_ignores_unknown_state` (one heat-capable unit `StateValue::Unknown`, one off) → `Some(Off)` and with only the unknown one → `None`.
  - `mode_chip_reads_select` (options `["Home","Away","Night"]`, state `Away`) and `mode_chip_missing_entity` → `None`; `mode_chip_none_when_unset`.
- [ ] **Step 2:** `cargo test -p oikos-web --lib home_header` → FAIL.
- [ ] **Step 3:** Implement the signatures above.
- [ ] **Step 4:** → PASS; clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): house summary for the home header`.

### Task 4: Web — mood sentence, greeting, details and their FR/EN text

**Files:**
- Create: `crates/oikos-web/src/home_header/mood.rs`
- Modify: `crates/oikos-web/src/i18n.rs` (home header text functions; new `Key`s go in `Key::ALL` too)
- Modify: `crates/oikos-web/src/screens/flow_canvas/sun.rs:87` (`house_place` becomes `pub(crate)`)

**Interfaces:**
- Consumes: Task 3 types; `WeatherView`; `FlowsConfigView`; `oikos_core::flow::sun::{sun_events, is_dark_at}`.
- Produces:
```rust
pub const COLD_DEFAULT: f64 = 15.0;
pub const HOT_DEFAULT: f64 = 26.0;
pub enum Greeting { Morning, Afternoon, Evening }
pub enum Activity { Attention, Open(Opening), OpenMany(usize), Asleep, Quiet, Awake }
pub enum TempBand { Cold, Comfortable, Warm }
pub enum Detail { RainSoon, FrostTonight, Sunset(NaiveTime), SunsetFallback }
pub fn greeting(hour: u32) -> Greeting;                 // 5..=11, 12..=17, else
pub fn activity(s: &HouseSummary, dark: bool, hour: u32) -> Activity;  // table §4.2, first match
pub fn temp_band(t: f64, settings: &HomeSettings) -> TempBand;        // strict < cold, strict > hot
pub fn details(weather: Option<&WeatherView>, sunset: Option<NaiveTime>, fallback: bool, now: NaiveTime, card_visible: bool) -> Vec<Detail>; // order + max 2, §4.3
pub fn sun_today(cfg: Option<&FlowsConfigView>, now: DateTime<Utc>) -> (Option<NaiveTime> /*sunrise*/, Option<NaiveTime> /*sunset*/, bool /*dark*/, bool /*fallback*/); // local times via house_place
// i18n.rs
pub fn home_date(lang: Lang, date: NaiveDate) -> String;      // "Vendredi 23 octobre" / "Friday 23 October"
pub fn home_time(lang: Lang, t: NaiveTime) -> String;         // "18 h 42" / "6:42 pm"
pub fn home_number(lang: Lang, v: f64, decimals: usize) -> String; // "19,6" / "19.6"
pub fn greeting_text(lang: Lang, g: Greeting) -> &'static str;
pub fn mood_sentence(lang: Lang, a: &Activity, band: Option<TempBand>) -> String; // with final "."
pub fn detail_text(lang: Lang, d: &Detail) -> String;
```

- [ ] **Step 1: Failing tests.** In `mood.rs`: `greeting_bands` (4→Evening, 5→Morning, 11→Morning, 12→Afternoon, 17→Afternoon, 18→Evening); `activity_order` (unseen alert beats an open window; one opening → `Open`; two → `OpenMany(2)`; dark at 23 h with no light → `Asleep`; dark at 21 h no light → `Quiet`; not dark at 23 h no light → `Quiet`; one light → `Awake`); `temp_band_edges` (14.9 Cold, 15.0 Comfortable, 26.0 Comfortable, 26.1 Warm; settings 18/24 move the edges); `details_order_and_cap` (rain + frost at 16:00 + sunset within 2 h → `[RainSoon, FrostTonight]`); `frost_only_from_15h`; `sunset_two_hours_before`; `sunset_always_without_card` (10:00, `card_visible == false` → contains `Sunset`); `fallback_sunset_hint` (fallback → `SunsetFallback`, never `Sunset`). In `i18n.rs`: `mood_sentence_fr_en` asserting exact copy: `"La maison est calme, il fait bon."`, `"La maison dort, il fait froid."`, `"Ouverture : Fenêtre (Cuisine), il fait froid."` (Opening title "Fenêtre", room "Cuisine"; no room → `"Ouverture : Fenêtre, il fait froid."`), EN `"Open: Window (Kitchen), it's cold."`, `"2 ouvertures, il fait chaud."`, `"Quelque chose demande ton attention."`; EN `"The house is quiet, it's comfortable."`, `"The house is asleep, it's cold."`, `"The house is awake, it's warm."`, `"2 doors or windows open."`; `sentence_without_temperature` (`band == None` → `"La maison est calme."`); `home_date_fr_en` (2026-10-23 → `"Vendredi 23 octobre"`, `"Friday 23 October"`); `home_time_fr_en` (18:42 → `"18 h 42"`, `"6:42 pm"`); `detail_text_fr` (`"Pluie attendue dans l'heure"`, `"Gel cette nuit"`, `"Coucher du soleil à 18 h 42"`).
- [ ] **Step 2:** `cargo test -p oikos-web --lib home_header i18n` → FAIL.
- [ ] **Step 3:** Implement. The opening copy is gender-neutral on purpose (French titles and rooms cannot be inflected generically): FR « Ouverture : {title} ({room}) », EN "Open: {title} ({room})", the parenthesis dropped without a room. `detail_text` for `SunsetFallback` reads « Coucher du soleil : position de la maison non réglée » / "Sunset: house location not set" (`[flows]` latitude/longitude unset).
- [ ] **Step 4:** → PASS; clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): mood sentence, greeting and detail line`.

### Task 5: Web — weather formatting and the header component

**Files:**
- Create: `crates/oikos-web/src/home_header/weather_fmt.rs`, `crates/oikos-web/src/home_header/view.rs`
- Modify: `crates/oikos-web/src/icons.rs` (weather set), `crates/oikos-web/src/i18n.rs` (chip labels, condition labels, compass, UV words, "inside", "feels like", "data from")
- Modify: `crates/oikos-web/src/screens/home.rs:102` (render `<HomeHeader />` before the rooms, inside the loaded branch)
- Modify: `crates/oikos-web/styles/main.css` (`.home-header…` block, desktop two columns, ≤ 640 px stacked)
- Test: `crates/oikos-web/tests/home_header.rs` (wasm)

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces:
```rust
pub const WEATHER_MAX_AGE_S: u64 = 21_600;
pub fn weather_card_visible(w: Option<&WeatherView>, now_s: u64) -> bool;
pub fn condition_icon(c: WeatherCondition, daylight: bool) -> &'static str; // icons.rs consts
pub fn compass_index(deg: f64) -> usize;           // 0..8, N = 0, clockwise, 22.5° sectors
pub fn uv_level(uv: f64) -> UvLevel;               // Low <3, Moderate <6, High <8, VeryHigh <11, Extreme
// i18n.rs: condition_label(lang, WeatherCondition), compass_word(lang, usize), uv_word(lang, UvLevel)
#[component] pub fn HomeHeader() -> impl IntoView;
```
Component behaviour is spec §3 and §5: chips hidden when their input is missing; the lights chip hidden at 0; the mode pill opens a menu of `options` and sends `Command::SelectOption { option }` via `crate::commands` like other cards; the weather card is an `<A href="/weather">`; when stale (< 6 h) a muted "data from HH:MM" line. Phone width uses short chip labels (CSS swaps two spans; both strings come from i18n). A one-minute timer (`set_interval` as other screens do) feeds `now`.

- [ ] **Step 1: Failing tests.** Native in `weather_fmt.rs`: `card_hidden_when_old_or_missing` (`None` → false; `fetched_at = now - 21_600` → true; `now - 21_601` → false); `compass_sectors` (0→0, 22.4→0, 22.5→1, 246→5 (SW), 350→0); `uv_bands` (2.9 Low, 3 Moderate, 7.9 High, 8 VeryHigh, 11 Extreme); `every_condition_has_icon_and_labels` (loop `WeatherCondition::ALL`, both langs non-empty). Wasm `tests/home_header.rs` (pattern of `tests/settings_updates.rs`): `header_shows_sentence_and_chips` — house with one light on in a room, a 20.0 °C room sensor, `weather = None` → text contains "The house is awake, it's comfortable." (lang En default), a lights chip "1", no `.home-header__weather` element; `header_shows_weather_card` — `weather = Some(view with temperature 9.4, fetched_at now)` → `.home-header__weather` exists and links to `/weather`.
- [ ] **Step 2:** `cargo test -p oikos-web --lib weather_fmt` → FAIL; `cd crates/oikos-web && wasm-pack test --headless --chrome --test home_header` → FAIL.
- [ ] **Step 3:** Implement; icons: clear (day = existing `SUN`, night = existing `MOON`), partly cloudy day/night, cloudy, fog, drizzle, rain, freezing rain, snow, showers, thunderstorm.
- [ ] **Step 4:** Both commands → PASS; clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): home header on the overview`.

### Task 6: Web — `/weather` page

**Files:**
- Create: `crates/oikos-web/src/screens/weather.rs` (`pub mod weather;` in `screens/mod.rs`)
- Modify: `crates/oikos-web/src/app.rs:157-175` (route `/weather`), `crates/oikos-web/src/shell/route.rs` (`section_for` → `Section::Overview`; crumb `Key::Weather`), `crates/oikos-web/src/i18n.rs` (`Key::Weather` « Météo »/"Weather", tile labels, "Now"/« Maintenant », "Next 5 days"/« 5 prochains jours », "No rain within the hour"/« Pas de pluie dans l'heure », "Updated at"/« Mis à jour à », unavailable text, credit)
- Modify: `crates/oikos-web/styles/main.css` (`.weather-page…`: tiles 4 columns, 2 under 640 px; days 5 columns, horizontal scroll under 640 px)
- Test: `crates/oikos-web/tests/weather_page.rs` (wasm)

**Interfaces:**
- Consumes: `House.weather`, `House.flows_config`, Task 4 `sun_today`, Task 5 helpers.
- Produces: `#[component] pub fn WeatherScreen() -> impl IntoView`.

Content is spec §6. The weekday of a forecast day comes from parsing `WeatherDay.date` (`%Y-%m-%d`); short weekday FR "Sam. 24", EN "Sat 24".

- [ ] **Step 1: Failing tests.** In `shell/route.rs` tests: `weather_route_crumb_and_section` (`/weather` → one crumb "Weather", `Section::Overview`). Wasm `weather_page_shows_tiles_and_days`: house with a full `WeatherView` (5 days) → 8 `.weather-tile`, 5 `.weather-day`, text contains "Open-Meteo"; `weather_page_unavailable` with `weather = None` → no tiles, the unavailable text present.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run → PASS; clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): weather page`.

### Task 7: Web — Settings "Home" panel

**Files:**
- Create: `crates/oikos-web/src/screens/home_settings_panel.rs` (`pub mod home_settings_panel;`)
- Modify: `crates/oikos-web/src/screens/settings.rs:273` (panel before "Theme"), `crates/oikos-web/src/climate/entity_field.rs:187` (`ThermometerField` takes `label` and `auto_text` props so the panel can say "Indoor sensor" / "Automatic (mean of rooms)"; the climate screen passes its current strings)
- Modify: `crates/oikos-web/src/i18n.rs` (panel labels)
- Test: `crates/oikos-web/tests/home_settings_panel.rs` (wasm)

**Interfaces:**
- Consumes: Task 2 `put_home_settings`, `House.home_settings`; `ThermometerField`.
- Produces: `#[component] pub fn HomeSettingsPanel() -> impl IntoView`; pure `fn draft_to_settings(draft: &Draft) -> Result<HomeSettings, (Field, &'static str)>` where `Draft` holds the five fields as `String`/`Option<String>` and `Field` names the field (number parsing accepts `,` or `.`; empty = `None`).

Spec §7: one Save sends the whole document built from the latest `House.home_settings` plus the edits; a 422 shows its message under the field it names (match the field name in the hub message); the panel re-reads on `HomeSettingsChanged`. Mode picker lists `Kind::Select` entities, ids starting with `virtual_` first, then by title, with "None".

- [ ] **Step 1: Failing tests.** Native `draft_parses_numbers` (`"15,5"` → 15.5, `""` → None, `"abc"` → Err naming the field). Wasm `panel_lists_selects_and_saves_draft` — house with `virtual_mode` (select) and a sensor → the mode `<select>` has options None, the virtual select; the cold field placeholder is "15".
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run → PASS (also the existing climate wasm tests that use `ThermometerField`); clippy; fmt.
- [ ] **Step 5:** Commit `feat(web): home settings panel`.

### Task 8: Docs and visual check

**Files:**
- Modify: `docs/users/web-ui.md` (the overview header: sentence rules table, chips, weather card, `/weather`), `docs/users/weather.md` (header card and page), the home settings paragraph in `docs/users/web-ui.md` (five fields, defaults 15/26)

- [ ] **Step 1:** Write the docs from spec §4–§7 (FR examples with EN equivalents).
- [ ] **Step 2:** `cd crates/oikos-web && trunk build --release`, run the hub locally, then screenshot the overview and `/weather` at 1280 px and 390 px in Ivoire and Night Glass with headless Chrome over CDP (the extension times out on the wasm page; window width ≥ 500 px, use device emulation for 390). Check: no horizontal scroll, chips wrap, card hidden when `[weather]` is off.
- [ ] **Step 3:** `cargo test --workspace`, `cd crates/oikos-web && wasm-pack test --headless --chrome` → all green.
- [ ] **Step 4:** Commit `docs: home header, weather page and home settings`.
