# Climate room card and room settings popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the cluttered room status card with a thermostat-row card (− / + hold, eco pill, ⚙), move every set-once room setting into a ⚙ popup, and put the house settings behind a ⚙ popup too.

**Architecture:** Web-only (`crates/oikos-web`). A shared modal `Popup` component (native `<dialog>` + `BackClose`) hosts the room settings, the eco form and the house settings. `RoomPanel` keeps owning every local signal (drafts, popup `open` flags, decision log) so a refetch never resets them; it renders a new `RoomCard`, the unchanged week card and proposal card, and a `RoomSettings` popup. Hold logic lives in a `TargetStepper` component with pure helpers in `climate/mod.rs`.

**Tech Stack:** Rust, Leptos (CSR, wasm32), web-sys, gloo-timers, wasm-bindgen-test + `FetchStub`.

**Spec:** `docs/superpowers/specs/2026-10-30-climate-room-card-design.md` (mockup alongside). Read it before any task.

## Global Constraints

- No hub, API, engine or data-model change; only `crates/oikos-web` and `docs/users/climate.md`.
- The schedule editor, proposal card content and house settings fields are unchanged.
- Every new user-visible string is a `ClimateText` key with EN and FR, listed in `ClimateText::ALL` (the `every_word_has_both_languages` test).
- Refetch rule: popups, drafts and the pending target live in signals owned by `RoomPanel` / `ClimateScreen`, outside closures that react to `zone` / `climate`.
- Step 0.5 °C, clamp 0–40 °C; debounce 1.5 s; hold default = minutes to next change, else 120; duration chips hide after 10 s.
- − / + disabled when `can_hold(state)` is false (Off and Pending) or there is no start temperature.
- CSS goes in `crates/oikos-web/styles/main.css` next to the existing `.climate-*` rules, using the theme tokens (Ivoire + Night Glass), with a phone rule in the existing climate `@media` block.
- Gate before every commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test -p oikos-web`.
- Browser tests: `cd crates/oikos-web && wasm-pack test --headless --chrome --test climate_rooms` (add `--test popup` for Task 2).
- Commits as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.

## Review Focus

1. **Refetch while a change is pending**: a `zone` update inside the 1.5 s window or between send and refetch must not move the shown target back. Pinned by Task 3's `a_refetch_during_the_wait_keeps_the_new_target`.
2. **Rapid taps**: five taps produce one POST with the final value, not five. Pinned by Task 3's `taps_send_one_hold_after_the_pause`.
3. **Refetch while the room popup is open**: the popup stays open and the decisions list stays. Pinned by Task 5's `a_refetch_keeps_the_room_settings_open`.
4. **Escape / Back / backdrop close once**: one close callback per closing, Back pops only the popup's own history entry. Pinned by Task 2's `escape_and_backdrop_close_the_popup`.
5. **Pending room**: − / + disabled, target still shown. Pinned by Task 3's `a_pending_room_cannot_step`.

---

### Task 1: Pure helpers and new strings

**Files:**
- Modify: `crates/oikos-web/src/climate/mod.rs` (helpers + unit tests in its `mod tests`)
- Modify: `crates/oikos-web/src/i18n/climate.rs` (keys, `ALL`, EN/FR pairs)

**Interfaces:**
- Produces (in `crate::climate`):
  - `pub const TARGET_STEP: f64 = 0.5;`
  - `pub fn step_target(current: f64, delta: f64) -> f64` — `current + delta` rounded to the nearest 0.5, clamped to `0.0..=40.0`.
  - `pub fn hold_minutes(next_change: Option<u32>) -> u32` — `next_change` or `120`.
  - `pub fn start_target(z: &ZoneView) -> Option<f64>` — the decision's `Setpoint::Temp`, else `z.reading.map(f64::round)` (today's `start` rule in `room_panel.rs`).
  - `pub fn correction_summary(lang: Lang, f: &FeedbackSettings) -> String` — e.g. EN `"Step 0.5 °C · tolerance 0.5 °C · every 15 min"`, FR `"Pas 0,5 °C · écart 0,5 °C · toutes les 15 min"` (numbers via the module's `decimal(lang, v, 1)` with the trailing `.0` / `,0` trimmed, as `degrees_text` does).
  - New `ClimateText` keys: `HouseSettings` ("House settings" / "Réglages de la maison"), `RoomSettings` ("Room settings" / "Réglages de la pièce"), `RoomSettingsTitle` ("Room settings ·" / "Réglages ·", followed by the room name), `AdvancedSettings` ("Advanced settings" / "Réglages avancés"), `EcoPill` ("Eco…" / "Éco…"), `LowerTarget` ("Lower target" / "Baisser la consigne"), `RaiseTarget` ("Raise target" / "Monter la consigne"), `Learning` ("Learning" / "Apprentissage"), `TargetIn` ("Target ·" / "Consigne ·", followed by the mode word).

- [ ] **Step 1: Write failing unit tests** in `climate/mod.rs` `mod tests`:

```rust
#[test]
fn step_target_rounds_and_clamps() {
    assert_eq!(step_target(19.0, TARGET_STEP), 19.5);
    assert_eq!(step_target(19.3, TARGET_STEP), 20.0);   // 19.8 → nearest half
    assert_eq!(step_target(0.2, -TARGET_STEP), 0.0);
    assert_eq!(step_target(39.8, TARGET_STEP), 40.0);
}

#[test]
fn hold_minutes_defaults_to_two_hours() {
    assert_eq!(hold_minutes(Some(45)), 45);
    assert_eq!(hold_minutes(None), 120);
}

#[test]
fn start_target_prefers_the_decision() {
    // zone with decision Temp(18.5) and reading 19.6 → Some(18.5)
    // zone with no decision and reading 19.6 → Some(20.0)
    // zone with no decision and no reading → None
}

#[test]
fn correction_summary_reads_in_both_languages() {
    let f = FeedbackSettings::default(); // step 0.5, band 0.5, interval 15
    assert_eq!(correction_summary(Lang::En, &f), "Step 0.5 °C · tolerance 0.5 °C · every 15 min");
    assert_eq!(correction_summary(Lang::Fr, &f), "Pas 0,5 °C · écart 0,5 °C · toutes les 15 min");
}
```

Build the zones in `start_target_prefers_the_decision` with the module's existing test zone helper (or a minimal `ZoneView` literal as in `screens/climate.rs` tests). If `FeedbackSettings::default()` values differ from 0.5 / 0.5 / 15, set them explicitly in the test.

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib climate::tests` — expected: FAIL (functions missing).
- [ ] **Step 3: Implement** the four helpers and the nine keys (enum, `ALL`, `climate_text` arms).
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib` — expected: PASS, including `every_word_has_both_languages`.
- [ ] **Step 5: Commit** `feat(web): climate helpers and words for the room card`.

---

### Task 2: Shared `Popup` component

**Files:**
- Create: `crates/oikos-web/src/components/popup.rs`
- Modify: `crates/oikos-web/src/components/mod.rs` (`pub mod popup;`)
- Modify: `crates/oikos-web/styles/main.css` (`.popup`, `.popup--small`, `.popup__bar`, `.popup__title`, `.popup__close`, `.popup__body`, `::backdrop`; phone rule: `.popup:not(.popup--small)` fills the viewport)
- Test: `crates/oikos-web/tests/popup.rs`

**Interfaces:**
- Produces: `crate::components::popup::{Popup, PopupSize}`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupSize { #[default] Sheet, Small }

#[component]
pub fn Popup(
    /// Open while true; every close path sets it false.
    open: RwSignal<bool>,
    #[prop(into)] title: Signal<String>,
    #[prop(optional)] size: PopupSize,
    children: ChildrenFn,
) -> impl IntoView
```

Behaviour: renders nothing while `open` is false; when true, mounts `<dialog class="popup">`, calls `show_modal()` and attaches `BackClose` exactly as `components/camera_viewer.rs:69-103` does (one-shot close guard, `on:cancel` prevents default and closes, `on:close` closes, cleanup detaches). Additionally a `click` whose target is the dialog element itself (the backdrop) closes. The ✕ button's accessible name is the existing `DashText::Close`. Children are built when it opens.

- [ ] **Step 1: Write failing browser tests** in `tests/popup.rs` (copy the `host()` / `settle()` helpers pattern from `climate_rooms.rs`):
  - `the_popup_opens_and_the_cross_closes_it`: `open` false → no `dialog`; set true → `dialog.popup[open]` with the title text and the child; click `.popup__close` → `open.get_untracked() == false` and no `dialog`.
  - `escape_and_backdrop_close_the_popup`: dispatch a `cancel` event on the dialog → closed; reopen; dispatch `click` on the dialog element itself → closed; reopen; click on the child → still open.
  - `small_popups_carry_their_class`: `size=PopupSize::Small` → `dialog.popup--small`.
- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome --test popup` — expected: FAIL (module missing).
- [ ] **Step 3: Implement** `Popup` and its CSS.
- [ ] **Step 4: Run** the same command — expected: PASS.
- [ ] **Step 5: Commit** `feat(web): a shared modal popup`.

---

### Task 3: `TargetStepper` — hold with − / +

**Files:**
- Create: `crates/oikos-web/src/climate/target_stepper.rs`
- Modify: `crates/oikos-web/src/climate/mod.rs` (`pub mod target_stepper;`)
- Modify: `crates/oikos-web/styles/main.css` (`.climate-stepper`, `.climate-stepper__step`, `.climate-stepper__value`, `.climate-stepper__chips`)
- Test: `crates/oikos-web/tests/climate_rooms.rs`

**Interfaces:**
- Consumes: Task 1's `step_target`, `hold_minutes`, `start_target`, `TARGET_STEP`, `LowerTarget`/`RaiseTarget`; existing `can_hold`, `minutes_to_next_change`, `api::climate::{set_override, OverrideBody}`, `degrees_text`.
- Produces:

```rust
#[component]
pub fn TargetStepper(
    zone: Signal<ZoneView>,
    /// (day, minute) in the house time zone, as `RoomPanel`'s `now` memo.
    now: Signal<(Day, u16)>,
    on_changed: Callback<()>,
) -> impl IntoView
```

Renders: `button.climate-stepper__step` (−, aria-label `LowerTarget`), `strong.climate-stepper__value` (e.g. `19°` / `19,5°` via `degrees_text`), `button.climate-stepper__step` (+, `RaiseTarget`), and, while `chips_open`, `div.climate-stepper__chips` with **1 h**, **2 h**, and **Until next change** (existing `UntilNextChange` key, only when the next change exists).

State (all `RwSignal`s created once in the component body, never inside a reactive closure):
- `draft: Option<f64>` — Some from the first tap until cleared; the shown value is `draft` or `start_target(zone)`.
- `awaiting: bool` — set when a send returns Ok; an `Effect` tracking `zone` clears `draft` and `awaiting` on the first zone change while `awaiting` is true.
- `last_sent: Option<f64>`, `chips_open: bool`.
- Timers: a `gloo_timers::callback::Timeout` (1 500 ms) held in a `StoredValue<Option<Timeout>, LocalStorage>`, replaced on each tap (dropping cancels it); a second one (10 000 ms) hides the chips. Both dropped `on_cleanup`.
- Send: `set_override(id, OverrideBody { setpoint: Some(Setpoint::Temp(v)), minutes: Some(m), ..Default })`, `m = hold_minutes(minutes_to_next_change(&rules, mode, day, minute))` with `mode` = decision's mode or `Mode::Heat`. Ok → `last_sent = Some(v)`, `chips_open = true`, start 10 s timer, `awaiting = true`, `on_changed.run(())`. Err → toast (`Toasts`), `draft = None`.
- Chip: sends `last_sent` with that chip's minutes (60, 120, or next change), hides the row.
- Disabled (`disabled` attr on both buttons): `!can_hold(state) || shown.is_none()`.

- [ ] **Step 1: Write failing browser tests** in `climate_rooms.rs`, mounting `TargetStepper` alone with `Toasts`, `FetchStub` replying `200 {}` to `POST /api/v1/climate/zones/living-room/override`, and the existing `zone(...)` fixture (Active, setpoint 19.0, `now = (Day::Mon, 0)`, no rules):
  - `taps_send_one_hold_after_the_pause`: click + three times → value text `20,5°`/`20.5°` (assert under `Lang::En`: `"20.5°"`); `stub.requests()` has no POST; wait 1 600 ms (`gloo_timers::future::TimeoutFuture`) → exactly one POST whose body contains `"setpoint":20.5` and `"minutes":120`; `.climate-stepper__chips` present with "1 h" and "2 h" and no "Until next change".
  - `a_refetch_during_the_wait_keeps_the_new_target`: click + once (→ 19.5); `current.set(zone with reading 18.0)`; tick → still `19.5°`; after the POST, `current.set(zone with setpoint 19.5)` → `19.5°` and a further `current.set(setpoint 18.0)` now shows `18°` (draft cleared).
  - `a_chip_resends_with_its_duration`: after the first POST, click "1 h" → second POST with `"setpoint":19.5,"minutes":60`; chips gone.
  - `a_pending_room_cannot_step`: zone `ZoneState::Pending` → both buttons `disabled`, value `19°` shown.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome --test climate_rooms` — expected: FAIL (module missing).
- [ ] **Step 3: Implement** `TargetStepper` and its CSS.
- [ ] **Step 4: Run** the same command — expected: PASS.
- [ ] **Step 5: Commit** `feat(web): hold a room's target with − and +`.

---

### Task 4: `EcoPill` — eco start / cancel in a small popup

**Files:**
- Create: `crates/oikos-web/src/climate/eco_pill.rs`
- Modify: `crates/oikos-web/src/climate/mod.rs` (`pub mod eco_pill;`)
- Modify: `crates/oikos-web/styles/main.css` (`.climate-eco-pill`, reuse `.climate-eco__field`)
- Test: `crates/oikos-web/tests/climate_rooms.rs`

**Interfaces:**
- Consumes: Task 2's `Popup` (`PopupSize::Small`), Task 1's `EcoPill` key; existing `eco_until`, `eco_end_text`, `eco_until_line`, `api::climate::{set_eco, clear_eco}`, keys `EcoUntilDate`, `EcoUntilTime`, `StartEco`, `Cancel`, `CancelEco`, `EcoPickDate`.
- Produces:

```rust
#[component]
pub fn EcoPill(zone: Signal<ZoneView>, tz: Signal<Tz>, on_changed: Callback<()>) -> impl IntoView
```

Moves today's eco logic out of `room_panel.rs:274-375` unchanged in behaviour: hidden when the zone is Off; no live eco → `button.climate-eco-pill` "Éco…" opening the popup (date defaults to tomorrow in `tz`, time `00:00`; **Start eco** validates, sends, closes; **Cancel** closes); live eco → `span.climate-eco-pill` with `eco_until_line(...)` and a ✕ button (aria-label `CancelEco`) calling `clear_eco`. `eco_open`, `eco_date`, `eco_time` are created in the component body; the `Popup` is always in the view tree (its `open` signal decides).

- [ ] **Step 1: Write failing browser tests**:
  - `eco_starts_from_the_pill`: Active zone, no eco; click "Eco…" → `dialog.popup--small` with a date and a time input; set date to a future day, click "Start eco" → one `PUT …/living-room/eco`; no dialog.
  - `a_running_eco_cancels_in_one_tap`: zone with `eco: Some(until = far future)` → pill text contains the until line; click its ✕ → one `DELETE …/living-room/eco`.
  - `an_off_room_has_no_eco_pill`: Off zone → no `.climate-eco-pill`.
- [ ] **Step 2: Run** the climate tests — expected: FAIL.
- [ ] **Step 3: Implement** `EcoPill` and CSS.
- [ ] **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(web): eco as a pill with a small popup`.

---

### Task 5: `RoomCard`, `RoomSettings` popup, and the slimmer `RoomPanel`

**Files:**
- Create: `crates/oikos-web/src/climate/room_card.rs`, `crates/oikos-web/src/climate/room_settings.rs`
- Modify: `crates/oikos-web/src/climate/mod.rs` (two `pub mod`), `crates/oikos-web/src/climate/room_panel.rs`, `crates/oikos-web/src/climate/correction_card.rs`
- Modify: `crates/oikos-web/styles/main.css` (new `.climate-card__*` / `.climate-room-settings__*`; remove `.climate-hold*`, `.climate-eco` row, `.climate-panel__lower`, `.climate-learn-card` and their phone rules once unused)
- Modify: `crates/oikos-web/src/i18n/climate.rs` (drop keys left unused: `HoldTemperature`, `HideDecisions`, `NextChange`, `EcoUntilButton` if nothing else uses them; `grep` before removing)
- Test: `crates/oikos-web/tests/climate_rooms.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:

```rust
// room_card.rs
#[component]
pub fn RoomCard(
    zone: Signal<ZoneView>,
    tz: Signal<Tz>,
    now: Signal<(Day, u16)>,
    on_changed: Callback<()>,
    /// The ⚙ was pressed.
    on_settings: Callback<()>,
) -> impl IntoView

// room_settings.rs
#[component]
pub fn RoomSettings(
    zone: Signal<ZoneView>,
    tz: Signal<Tz>,
    on_changed: Callback<()>,
    open: RwSignal<bool>,
    /// Whether the panel is on screen; decisions are fetched only when open && shown.
    shown: Signal<bool>,
) -> impl IntoView

// correction_card.rs: CorrectionCard gains a fold
#[component]
pub fn CorrectionCard(zone: Signal<ZoneView>, on_changed: Callback<()>) -> impl IntoView
```

`RoomCard` (spec "The room card"): `header` with `h2` name, today's orphan / invalid badges, `button.climate-card__gear` ⚙ (aria-label `RoomSettings`); figures row: left = today's reading label, reading and thermometer line; right = `TargetIn` + mode word, then `TargetStepper`; `p.climate-card__why` = `source_word` + " · " + reason as plain text; holds = one chip per live override (`override_text`, moved from `room_panel.rs`) plus one ✕ (aria-label `Clear`) → `clear_override`; action row = today's state segmented buttons + `EcoPill`.

`RoomSettings`: a `Popup` (`Sheet`) titled `RoomSettingsTitle` + room name, with sections in order: `ThermometerField` (today's `set_sensor`), `CorrectionCard`, `PresenceCard`, learning (`Learning` heading, `learn_status_text`, **Learn now** as today), recent decisions (today's list; the fetch `Effect` runs when `open && shown`, re-runs on `zone` change; `log` signal created in `RoomSettings`' body so it survives refetches while open).

`CorrectionCard` change: under the switch, when enabled and a sensor exists, `p.climate-field__hint` with `correction_summary`, then a `<details class="climate-correction__more"><summary>` `AdvancedSettings` " (7)" `</summary>` holding the seven `NumField`s unchanged.

`RoomPanel` becomes: `settings_open = RwSignal::new(false)` in its body; `<RoomCard … on_settings=move || settings_open.set(true)/>`, the week card (unchanged), the proposal card (unchanged), `<RoomSettings open=settings_open shown=… />`. Everything moved out (hold, eco, learn, decisions, thermometer) is deleted from `room_panel.rs`.

- [ ] **Step 1: Update and write failing browser tests**:
  - Update `the_room_follows_the_language_without_a_remount`: the page shows "Consigne ·", "Maintenant", "Programmation", "Cette semaine", "Active", "Arrêt", "En attente" (FR) and their EN words after the flip; "Apprendre maintenant" / "Décisions récentes" are **not** on the page until the ⚙ is clicked; replace the `.climate-hold__setpoint` aria-label assertions with the + button's aria-label `"Monter la consigne"` → `"Raise target"`.
  - `the_gear_opens_the_room_settings`: click `.climate-card__gear` → `dialog.popup` containing "Room thermometer"-section text, "Learn now", "Recent decisions" and the correction switch; one `GET …/living-room/decisions?…` recorded.
  - `a_refetch_keeps_the_room_settings_open`: open the ⚙, `current.set(zone with reading 18.0)`, settle → dialog still open, decisions list still rendered.
  - `an_override_shows_as_a_chip_and_clears`: zone with one live `Override` (setpoint 21, until far future, `OverrideOrigin::User`) → one `.climate-card__hold` chip with today's `override_text`; click its ✕ → one `DELETE …/living-room/override`.
  - `correction_fields_fold_under_advanced`: zone with `feedback.enabled = true` and a `room_sensor` → the summary line; `details.climate-correction__more` not `open`; seven `input.field` inside it.
- [ ] **Step 2: Run** the climate tests — expected: the new ones FAIL.
- [ ] **Step 3: Implement** `RoomCard`, `RoomSettings`, the fold, the slimmer `RoomPanel`, CSS and dead-key / dead-CSS removal.
- [ ] **Step 4: Run** `wasm-pack test --headless --chrome --test climate_rooms` and the full gate — expected: PASS, clippy clean (no unused imports or keys).
- [ ] **Step 5: Commit** `feat(web): a calm room card with its settings behind a gear`.

---

### Task 6: House settings behind a ⚙, and the user docs

**Files:**
- Modify: `crates/oikos-web/src/screens/climate.rs`
- Modify: `crates/oikos-web/src/climate/settings_sheet.rs` (drop its own `card` / title wrapper if the popup title replaces it)
- Modify: `crates/oikos-web/styles/main.css` (`.climate-screen__gear`; remove `.climate-settings` card-only rules left unused)
- Modify: `docs/users/climate.md` (section "The Climate screen")
- Test: `crates/oikos-web/tests/climate_rooms.rs`

**Interfaces:**
- Consumes: Task 2's `Popup`, Task 1's `HouseSettings` key, existing `SettingsSheet { house, on_saved, on_close }`.

Change: the header's text **Settings** button becomes `button.climate-screen__gear` ⚙ (aria-label `HouseSettings`) toggling `settings_open`; the inline `<Show>` becomes `<Popup open=settings_open title=Key::Settings text>` whose child builds `SettingsSheet` from `climate.get_untracked()` (draft-on-open rule kept); `on_saved` closes and reloads, `on_close` closes.

Docs: rewrite the "The Climate screen" bullet list in `docs/users/climate.md` to describe the room card (reading, target with − / +, the 1 h / 2 h / Until next change chips, holds with ✕, state switch, Eco pill), the ⚙ room settings popup (thermometer, correction with Advanced settings, presence, learning, recent decisions), the week and proposal cards, and the ⚙ in the page header for **Climate settings**. Keep the rest of the page as is.

- [ ] **Step 1: Write failing browser test** `the_house_gear_opens_the_settings_popup`: mount `ClimateScreen` with `FetchStub` replying to `GET /api/v1/climate` with a one-zone `ClimateView` JSON (serialize the fixture with `serde_json::to_string`), `provide_context` for `Toasts`, `House`, `install_now()`, a `Lang` signal; click `.climate-screen__gear` → `dialog.popup` containing the "Season" field; dispatch `cancel` → closed. If mounting the whole screen needs more context than the test file already provides, mount only the header part by extracting a small `ClimateHeader` component in `screens/climate.rs` and test that.
- [ ] **Step 2: Run** — expected: FAIL.
- [ ] **Step 3: Implement** the header change, CSS and docs.
- [ ] **Step 4: Run** the climate tests and the full gate — expected: PASS.
- [ ] **Step 5: Commit** `feat(web): house climate settings behind a gear` (include `docs/users/climate.md`).

---

After Task 6: a whole-branch review, then open the PR against `main`. After merge: rebuild the web UI on the live hub and retake the Climate docs screenshot (docs screenshot recipe).
