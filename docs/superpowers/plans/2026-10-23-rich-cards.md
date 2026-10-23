# Rich device cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every device card the mockup's frame (icon tile, room, name, state line, trailing control) and build the three rich cards — climate ring, temperature curve + freshness badge, wide vacuum — in both themes, FR/EN.

**Architecture:** `CardFrame` gains slots (trailing control, state line, tint) and reads a new `CardPlace` context (Overview / Room / Sheet) that room sections and sheets provide; the place decides the room line, the vacuum width and which controls are "sheet only". Pure helpers (state lines, ring geometry, freshness, sparkline path, vacuum sentences) are native-tested; components are wasm-tested. Rich-card CSS goes in a new `styles/cards.css`; theme values in `styles/tokens.css`.

**Tech Stack:** Rust, Leptos 0.8 (CSR, wasm32), leptos_router, Trunk, wasm-bindgen-test (headless Chrome), plain CSS + inline SVG.

**Spec:** `docs/superpowers/specs/2026-10-23-rich-cards-design.md` (mockup: `docs/superpowers/specs/2026-10-23-rich-cards-mockup.html` — open it, the ring drags).

## Global Constraints

- Web crate only: `crates/oikos-web`. No hub, API, `oikos-core` or Android change.
- A theme changes only the look: Ivoire and Night Glass get the same markup; every new token is defined in both theme blocks (`theme.rs::both_themes_define_the_same_tokens` enforces it).
- Every new label goes through `src/i18n.rs` (FR + EN). With no `Lang` in context text is English, so tests assert English.
- Text colours pass WCAG AA: use `--ink` / `--muted`; `#c9973d` (`--solar`) is never text.
- No brand on the robot drawing; no house-specific names, hosts or IPs anywhere (code, tests, fixtures, docs).
- Cards remount on every change of their entity (`RoomSection` memo → `card_for`); anything that must survive a state echo (a ring drag, a fetched curve) lives outside the component.
- Before each commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p oikos-web`, and from `crates/oikos-web`: `wasm-pack test --headless --chrome`. Check `~/dev-scratch/cargo-target` first; over 5 GB, `cargo clean`.
- Commits as `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A state echo for the climate entity arrives mid-drag (e.g. its room temperature updates): the card remounts but the ring keeps the dragged value and the release still sends exactly one `SetTemperature` — pinned in Task 5 (`drag_survives_a_remount`).
2. A Home with many temperature sensors and no history database: no curve, no console errors, at most 4 history requests in flight and none repeated within 5 min for the same id — pinned in Task 4 (`cache_reuses_and_caps_in_flight`).
3. A climate with mode `off` or with no target: the ring is disabled, shows "Arrêt"/"Off" or "—", and neither drag nor keys send anything — pinned in Task 5 (`off_ring_is_disabled`).
4. A sensor whose `last_update` is missing (fresh hub start, retained-only state): no freshness badge rather than a wrong "Live" — pinned in Task 4 (`freshness_cases`).
5. A device sheet for a climate, vacuum or water heater still offers every control the old card had (fan speed, extra vacuum actions, water-heater target and mode) — pinned per card in Tasks 3, 5, 6 (`*_sheet_has_full_controls`).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/components/card_place.rs` (create) | `CardPlace` enum, `provide_card_place`, `use_card_place` |
| `src/components/card_frame.rs` (modify) | New frame markup: tile, room, name, trailing, state line, tint |
| `src/components/dial.rs` (create) | Ring geometry (pure) + `Dial` component (drag, keys, drafts) |
| `src/components/sparkline.rs` (create) | `sparkline_points` (pure) + `Sparkline` component |
| `src/components/robot.rs` (create) | The generic robot SVG for the vacuum card |
| `src/api/series_cache.rs` (create) | Shared 24 h series cache: TTL, in-flight cap, coalescing |
| `src/cards/text.rs` (create) | Pure, language-aware card texts: state lines, freshness, mode labels, vacuum sentences, decimal formatting |
| `src/cards/mod.rs` (modify) | `ToggleSwitch` (compact trailing toggle) |
| `src/cards/{light,switch,water_heater,sensor,climate,vacuum}.rs` (modify) | Each card on the new frame |
| `src/screens/room.rs`, `src/screens/entity_sheet.rs`, `src/screens/group_sheet_v2.rs` (modify) | Provide `CardPlace` |
| `src/i18n.rs` (modify) | New `Key`s |
| `styles/tokens.css` (modify), `styles/cards.css` (create), `index.html` (modify) | Tokens per theme; rich-card CSS linked after `rooms.css` |
| `tests/card_frame.rs`, `tests/rich_cards.rs` (create); existing wasm tests (modify as needed) | wasm coverage |
| `docs/users/web-ui.md` (modify) | Describe the cards |

Spec amendment (made with this plan): the room line is hidden wherever the card sits under its own room's heading — a `RoomSection` on Home **or** on `/rooms/:id` (not the Favorites section) — and in sheets. Home today lists rooms as sections, so a room line there would repeat the heading.

---

### Task 1: Card place, new frame and tokens

**Files:**
- Create: `src/components/card_place.rs`, `styles/cards.css`, `tests/card_frame.rs`
- Modify: `src/components/mod.rs`, `src/components/card_frame.rs`, `src/screens/room.rs:33-96`, `src/screens/entity_sheet.rs:274`, `src/screens/group_sheet_v2.rs:168`, `styles/tokens.css`, `index.html:30`, existing wasm tests whose selectors break

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum CardPlace { #[default] Overview, Room, Sheet }`
  - `pub fn provide_card_place(place: CardPlace)` and `pub fn use_card_place() -> CardPlace` (Overview when absent).
  - `CardFrame` new optional props: `trailing: Option<AnyView>`, `state_line: Option<String>`, `tint: Option<&'static str>` (adds class `card--<tint>`, e.g. `"sage"`, `"cool"`), `wide: bool`, `row_header: bool` (tile beside room/name, as the climate card). Existing props unchanged.
  - Markup contract: `article.card > div.card__top > (button.card__header > span.card__tile[icon + dots] + span.card__titles > (span.card__room? + span.card__name)) + div.card__trailing?` then `p.card__state?` then `div.card__body`. The trailing control is never inside `.card__header`.
  - Tokens (both themes): `--tile-bg`, `--tile-on-bg`, `--card-sage-bg`, `--card-cool-bg`, `--ring-track`, `--ring-heat`, `--ring-cool`, `--ring-other`, `--badge-border`, `--toggle-on`. Ivoire values from the mockup file's `.ivoire` block; Night Glass from its `.night` block.

- [ ] **Step 1: Write failing wasm tests** in `tests/card_frame.rs` (mount helper as in `tests/generic_card.rs`, with an `areas` entry `{id:"salon", name:"Salon"}` in `House`, optional `provide_card_place`):
  - `room_line_shows_in_overview`: a switch in area `salon` → `.card__room` text `"Salon"`.
  - `room_line_hidden_in_room_and_sheet`: same with `CardPlace::Room`, then `CardPlace::Sheet` → no `.card__room`.
  - `unassigned_has_no_room_line`: area `UNASSIGNED_AREA` → no `.card__room`.
  - `trailing_sits_outside_the_header`: `.card__header .card__trailing` and `.card__header [role=switch]` both absent; `.card__trailing [role=switch]` present (after Task 2 this is the switch's toggle; for Task 1 assert with a test-only `CardFrame` mount passing `trailing`).
- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome --test card_frame` — FAIL (no `CardPlace`).
- [ ] **Step 3: Implement** `card_place.rs`; the new `CardFrame` markup (room text via `format::room_name(&house.areas, &entity.area)` only when `use_card_place() == Overview`); `RoomSection` calls `provide_card_place(CardPlace::Room)` unless `id == FAVORITES`; both sheets wrap their controls panel in a child component/scope that provides `CardPlace::Sheet`. Tokens in both theme blocks; `styles/cards.css` holds the frame layout (tile 40px rounded 12px, room 11px `--muted`, name 15px/600, state 13px `--muted`, honey `--card-on-ink` when `.card--on`), linked in `index.html` after `rooms.css`. Remove superseded `.card__header/.card__icon/.card__name` rules from `main.css` rather than overriding them.
- [ ] **Step 4: Run** `wasm-pack test --headless --chrome` (whole suite) and `cargo test -p oikos-web`. Fix every broken selector or count in existing tests in this same task (allowed by the spec); keep `.card__header` / `.card__name` / modifiers.
- [ ] **Step 5: Commit** `feat(web): card frame with room line, trailing slot and card place`

### Task 2: Card texts, light and switch

**Files:**
- Create: `src/cards/text.rs`
- Modify: `src/cards/mod.rs`, `src/cards/light.rs`, `src/cards/switch.rs`, `src/i18n.rs`, `styles/cards.css`, `tests/card_frame.rs`

**Interfaces:**
- Consumes: Task 1 `CardFrame` props and `CardPlace`.
- Produces (in `cards/text.rs`, all pure, `lang: Lang` first):
  - `pub fn decimal(lang: Lang, v: f64) -> String` — at most one decimal, none when the value rounds to a whole number: `15.5 → "15,5"` (Fr) / `"15.5"` (En), `20.0 → "20"`, `20.66 → "20,7"`.
  - `pub fn brightness_percent(level: u8) -> u8` — `round(level * 100 / 255)`, at least 1 when `level > 0`.
  - `pub fn light_state_line(lang: Lang, on: Option<bool>, brightness: Option<u8>) -> String` — Fr `"Allumé · 14 %"`, `"Allumé"` (no brightness), `"Éteint"`, `"—"` (unknown); En `"On · 14 %"`, `"On"`, `"Off"`, `"—"`.
  - `pub fn switch_state_line(lang: Lang, on: Option<bool>) -> String` — Fr `"Activé"` / `"Désactivé"`, En `"On"` / `"Off"`, `"—"`.
  - `ToggleSwitch` in `cards/mod.rs`: `#[component] pub fn ToggleSwitch(on: bool, #[prop(into)] aria_label: String, on_toggle: Callback<()>, #[prop(optional, into)] disabled: Signal<bool>)` → `button.power.power--compact[role=switch][aria-checked]`.
  - i18n keys: `On`, `Off`, `Activated`, `Deactivated`, `Brightness` (Fr "Luminosité").
- [ ] **Step 1: Write failing native tests** in `cards/text.rs`: `decimal_follows_language` (`(Fr,15.5)→"15,5"`, `(En,15.5)→"15.5"`, `(Fr,20.0)→"20"`, `(Fr,20.66)→"20,7"`), `brightness_percent_rounds` (`0→0`, `1→1`, `36→14`, `255→100`), `light_state_lines` (the six cases above), `switch_state_lines`.
- [ ] **Step 2: Run** `cargo test -p oikos-web cards::text` — FAIL.
- [ ] **Step 3: Implement** `text.rs` and keys. `LightCard`: trailing `ToggleSwitch` (aria-label = entity title), `state_line` from `light_state_line`, body = label row (`Brightness` · `NN %`) + existing slider + existing swatch. `SwitchCard`: trailing `ToggleSwitch`, `state_line` from `switch_state_line`, empty body. Group light/switch cards keep their bodies (they move with the frame).
- [ ] **Step 4: Add wasm tests** to `tests/card_frame.rs`: `light_card_shows_state_line_and_percent` (brightness 36, on → state `"On · 14 %"`, label row contains `"14 %"`); `switch_toggle_is_trailing` (`.card__trailing [role=switch][aria-checked=false]`). Run the whole wasm suite — PASS.
- [ ] **Step 5: Commit** `feat(web): light and switch cards on the new frame with FR/EN state lines`

### Task 3: Water heater card

**Files:**
- Modify: `src/cards/water_heater.rs`, `src/i18n.rs`, `styles/cards.css`; Test: `tests/rich_cards.rs` (create)

**Interfaces:**
- Consumes: `CardFrame` (Task 1), `use_card_place` (Task 1), `decimal` (Task 2).
- Produces: key `More` (Fr "Plus d'options", En "More options").

- [ ] **Step 1: Write failing wasm tests** in `tests/rich_cards.rs` (shared mount helper with an optional `CardPlace`): `water_heater_tile_has_menu_not_stepper` (Overview: `.card__trailing button[aria-label="More options"]` present, no `.stepper`, no `select`); `water_heater_sheet_has_full_controls` (Sheet: `.stepper` present, mode `select` present when `hvac_modes` is non-empty).
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome --test rich_cards` — FAIL.
- [ ] **Step 3: Implement**: trailing ⋯ button runs `open_entity(&id)`; body = existing tank + big current temperature + existing chip/inlet caption; stepper and mode select rendered only when `use_card_place() == CardPlace::Sheet`. Its own existing caption strings stay as they are (project 5 translates the rest).
- [ ] **Step 4: Run** the wasm suite — PASS.
- [ ] **Step 5: Commit** `feat(web): water heater card with a menu, full controls in its sheet`

### Task 4: Temperature card — curve, cache, freshness

**Files:**
- Create: `src/components/sparkline.rs`, `src/api/series_cache.rs`
- Modify: `src/components/mod.rs`, `src/api/mod.rs`, `src/cards/sensor.rs`, `src/cards/text.rs`, `src/i18n.rs`, `styles/cards.css`, `Cargo.toml` (web-sys features `IntersectionObserver`, `IntersectionObserverEntry`, `IntersectionObserverInit` if used); Test: `tests/rich_cards.rs`

**Interfaces:**
- Consumes: `fetch_series(entity, HistoryRange::Day)` (`src/api/history.rs`), `Now` context (`components/now.rs`, fall back to `unix_now()` when absent), `decimal`.
- Produces:
  - `pub const QUIET_AFTER_S: u64 = 3 * 3600;`
  - `pub fn freshness(lang: Lang, now: u64, last_update: Option<u64>) -> Option<String>` in `cards/text.rs`: `None` when absent; age `< QUIET_AFTER_S` → Fr `"En direct"` / En `"Live"`; `< 24 h` → `"Il y a N h"` / `"N h ago"` (whole hours, floor); else `"Il y a N j"` / `"N d ago"`. A `last_update` in the future counts as age 0.
  - `pub fn sparkline_points(points: &[(i64, f64)], width: f64, height: f64) -> Option<String>` — SVG `points` attribute, x by time across `width`, y inverted across `height` with 2 px padding; `None` for fewer than 2 points; a flat series draws a horizontal line at mid-height.
  - `pub async fn day_series(id: &str) -> Option<Vec<(i64, f64)>>` in `series_cache.rs` — `(t, mean)` pairs; module-level cache (`thread_local!` `RefCell<HashMap<String, Entry>>`), entries live `CACHE_TTL_S = 300` s (errors cached too, so a house without history asks once per 5 min per id), at most `MAX_IN_FLIGHT = 4` requests (others wait in a FIFO), concurrent callers for the same id share one request.
  - `#[component] pub fn Sparkline(entity_id: String)` — fetches lazily when first intersecting the viewport (or on mount where `IntersectionObserver` is missing), renders `svg.spark > polyline` or nothing.
- [ ] **Step 1: Write failing native tests**: `freshness_cases` (now=100 000: `None→None`; age 0 → "Live"; age 10 799 → "Live"; age 10 800 → "3 h ago"; age 86 399 → "23 h ago"; age 86 400 → "1 d ago"; future → "Live"; and Fr `"Il y a 3 h"`); `sparkline_points_cases` (1 point → `None`; flat 3 points → all y equal to `height/2`; rising 2 points → first y = `height-2`, last y = `2`, first x = 0, last x = `width`). For the cache, factor its bookkeeping into a pure `struct CacheState` with `fn begin(&mut self, id, now) -> Begin { Cached(..) | Join | Start | Queue }` / `fn finish(...)` and test `cache_reuses_and_caps_in_flight` (5th distinct id queues; same id joins; a fresh entry within 300 s is reused; after 300 s it starts again).
- [ ] **Step 2: Run** `cargo test -p oikos-web` — FAIL.
- [ ] **Step 3: Implement.** Temperature card (`device_class == "temperature"`): `tint = Some("cool")`; trailing = freshness badge `span.badge` (none when `freshness` is `None`); body = big value `decimal(lang, v)` + unit, then a bottom row with the paired humidity (`DROP` icon + `"57 %"`, still a link to its history) and `Sparkline` (wrapped in the link to `/history/<id>`). Keep the paired-humidity lookup as today; drop the humidity bar on temperature cards. Other sensor kinds only move to the frame. Keys: `Live`, `AgoHours`, `AgoDays` (format with `N` substituted in `text.rs`).
- [ ] **Step 4: wasm tests** in `tests/rich_cards.rs`: `temperature_card_without_history_has_no_curve` (no hub in the test env: after two `tick()`s and a 50 ms timeout, no `svg.spark`, value text `"20.7 °C"`); `temperature_badge_from_last_update` (`last_update = now − 60` → badge `"Live"`; `None` → no `.badge`). Run the suite — PASS.
- [ ] **Step 5: Commit** `feat(web): temperature card with a 24 h curve and a freshness badge`

### Task 5: Climate ring

**Files:**
- Create: `src/components/dial.rs`
- Modify: `src/components/mod.rs`, `src/cards/climate.rs`, `src/cards/text.rs`, `src/i18n.rs`, `styles/cards.css`; Test: `tests/rich_cards.rs`

**Interfaces:**
- Consumes: `stepped` (climate.rs), `decimal`, `CardFrame` with `row_header`, `tint = "sage"`, `use_card_place`.
- Produces (in `dial.rs`):
  - `pub const START_DEG: f64 = 135.0; pub const SWEEP_DEG: f64 = 270.0;`
  - `pub fn angle_of(value: f64, min: f64, max: f64) -> f64` (clamped, degrees, 135..=405).
  - `pub fn value_at(angle_deg: f64, min: f64, max: f64, step: f64) -> f64` — angle measured clockwise from +x as `atan2` gives; positions in the bottom gap snap to the nearer end; result snapped to `step` from `min` and clamped.
  - `pub fn arc_path(from_deg: f64, to_deg: f64, r: f64, c: f64) -> String` (SVG `d`, large-arc flag when the span > 180°).
  - `#[component] pub fn Dial(draft_key: String, value: Option<f64>, min: f64, max: f64, step: f64, disabled: bool, #[prop(into)] aria_label: String, #[prop(into)] center: String, #[prop(into)] caption: String, on_commit: Callback<f64>)` → `svg.dial[role=slider][tabindex=0][aria-valuemin|max|now|valuetext]`.
  - Drafts: `thread_local!` map `draft_key → f64`. Pointer down captures and starts a draft; moves update it (display follows); `pointerup`/`pointercancel` listened on `window` commits once and clears. A remounted `Dial` with a live draft shows the draft and keeps listening for moves while `buttons != 0`. Keys: ArrowUp/Right `+step`, ArrowDown/Left `−step`, PageUp/PageDown `±1.0`, Home/End → min/max; commit 600 ms after the last key (`gloo_timers::Timeout`, reset per key).
  - `pub fn mode_label(lang: Lang, mode: &str) -> String` in `text.rs`: `heat` Chauffage/Heat, `cool` Clim/Cool, `auto` and `heat_cool` Auto, `dry` Déshumidification/Dry, `fan_only` Ventilation/Fan, `off` Arrêt/Off, else `word(mode)`.
  - Keys: `TargetTemperature` ("Température souhaitée"/"Target temperature"), `ItIs` ("Il fait"/"It's"), `Decrease`, `Increase`, `Mode`.
- [ ] **Step 1: Native tests** in `dial.rs`: `angle_of_ends` (`min→135`, `max→405`, middle→270); `value_at_snaps_and_clamps` (7–30, step 0.5: 135°→7, 405° (=45°)→30, 270°→18.5, 90° (bottom gap, nearer end = max)→30); `arc_path_large_flag`. In `text.rs`: `mode_labels`.
- [ ] **Step 2: Run** `cargo test -p oikos-web` — FAIL.
- [ ] **Step 3: Implement** `Dial` and the climate card: `row_header`, `tint = "sage"`; trailing = mode badge — a native `<select class="badge badge--select" aria-label=Mode>` over `hvac_modes` with `mode_label` texts, or a static `span.badge` with the mode when `hvac_modes` is empty; body = `Dial` (range from `capabilities.min_temp/max_temp`, else 7–30; step `temp_step` else 0.5; `draft_key = entity.id`; center = `decimal(target)` + `"°"`, `"—"` without target, `mode_label("off")` when off; caption `TargetTemperature`; `disabled` when mode is `off` or target is `None`; class `dial--heat|--cool|--other|--off` from the mode) and a foot: `btn--round` − / + (existing `stepped`, sent at once as today), `"Il fait 22°"` (current, or the mode word when the unit reports none). Fan select only in `CardPlace::Sheet`. Commit = `SetTemperature` with the optimistic guess as the steppers do. Climate group cards keep their bodies.
- [ ] **Step 4: wasm tests** in `tests/rich_cards.rs`: `ring_is_an_accessible_slider` (aria-valuenow `"15.5"`, min/max from caps); `keys_send_one_command` (three ArrowUp → aria-valuenow `"17"`, `Pending` for the id false before 600 ms, true after); `drag_survives_a_remount` (dispatch `pointerdown` + `pointermove`, replace the entity in `House` with a new `current` to force a remount, dispatch `pointermove` + window `pointerup` → one pending command, aria-valuenow equals the drafted value); `off_ring_is_disabled` (mode `off`: `aria-disabled=true`, ArrowUp changes nothing, center text `"Off"`); `climate_sheet_has_full_controls` (Sheet: fan `select` present; Overview: absent). Run the suite — PASS.
- [ ] **Step 5: Commit** `feat(web): climate card with a draggable ring and a mode badge`

### Task 6: Vacuum card

**Files:**
- Create: `src/components/robot.rs`
- Modify: `src/components/mod.rs`, `src/cards/vacuum.rs`, `src/cards/text.rs`, `src/i18n.rs`, `styles/cards.css`; Test: `tests/rich_cards.rs`

**Interfaces:**
- Consumes: `CardFrame` (`wide`), `use_card_place`, existing `primary_action`, `vacuum_action_icon`.
- Produces:
  - `pub fn vacuum_sentence(lang: Lang, activity: Option<&str>) -> Option<&'static str>` and `pub fn vacuum_caption(lang: Lang, activity: Option<&str>, fan: Option<&str>) -> Option<String>` in `text.rs` — exactly the spec's table (docked, cleaning, returning, paused, idle, error; unknown → `None`; cleaning without fan → "En cours" / "Running").
  - `#[component] pub fn Robot(activity: Option<String>)` → `svg.robot.robot--<activity>`; concentric rings + round body + top bump, colours from tokens; `.robot--cleaning` pulses the outer rings, `.robot--error` tints `--danger`; `@media (prefers-reduced-motion: reduce)` stops the pulse.
  - Keys: `Start` ("Démarrer"), `Pause`, `Resume` ("Reprendre"), `SendHome` ("Renvoyer à la base"/"Send home"), `Battery` ("Batterie").
- [ ] **Step 1: Native tests**: `vacuum_sentences` (every row of the spec table, both languages) and update `primary_follows_the_activity` to the translated labels via `Lang::En` (`primary_action` gains a `lang` parameter).
- [ ] **Step 2: Run** `cargo test -p oikos-web` — FAIL.
- [ ] **Step 3: Implement**: `wide = use_card_place() != CardPlace::Sheet`; left column = battery (top row), sentence (`.vacuum__sentence`, 26px), primary dark button + round ⌂ (`return_to_base`, only when supported); right column = `Robot` + caption. Other actions and the fan select only in `CardPlace::Sheet`. Unknown activity: no sentence; the state line shows `display_value` as today.
- [ ] **Step 4: wasm tests**: `vacuum_tile_is_wide_with_sentence` (docked, Overview: `.card--wide`, text contains "At home.", no `select`, buttons = header + Start + ⌂); `vacuum_sheet_has_full_controls` (Sheet: not `.card--wide`, fan `select` present, `clean_spot` button present). Run the suite — PASS.
- [ ] **Step 5: Commit** `feat(web): wide vacuum card with a state sentence and a robot drawing`

### Task 7: Dashboards, phone width, both themes, docs

**Files:**
- Modify: `styles/cards.css`, `styles/main.css` (dashboard widget sizing only if needed), `docs/users/web-ui.md`

**Interfaces:**
- Consumes: everything above. `WidgetView` already renders `LightCard`, `ClimateCard`, `SensorCard`, `WaterHeaterCard` (Overview place by default) — no Rust change expected.

- [ ] **Step 1: Build and serve** the web UI against a hub or the fixtures (`trunk serve` from `crates/oikos-web`), with a house containing at least one light (on and off), switch, climate (heat, off), temperature sensor with humidity, vacuum (docked, cleaning), water heater.
- [ ] **Step 2: Screenshot** Home, a room page, an entity sheet (climate, vacuum) and a dashboard with 1×1 and 2×1 climate widgets, in Ivoire and Night Glass, at 1280 px and 390 px wide (headless Chrome over CDP; window width ≥ 500 px renders the phone layout via device metrics override). Compare with the mockup file.
- [ ] **Step 3: Fix CSS** until: no backdrop blur is added by the new cards (token-driven where needed); the ring fits a 160 px card and a 1×1 widget; − / + and toggles keep `--touch-min`; the vacuum takes a full row on phone; nothing overflows horizontally; text contrast holds in both themes.
- [ ] **Step 4: Docs**: describe the cards in `docs/users/web-ui.md` (ring drag + keys, curve needs the history database, freshness badge meaning, sheet-only controls). Run the full checks from Global Constraints — PASS.
- [ ] **Step 5: Commit** `style(web): rich cards at phone width and in both themes; docs`
