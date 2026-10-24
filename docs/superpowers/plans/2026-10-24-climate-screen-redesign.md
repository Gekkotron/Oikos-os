# Climate screen redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Climate page fills the content width. On wide screens it shows a room list and the selected room's panel, built around a vertical week calendar whose blocks are coloured yellow, orange or red by temperature. On phones it shows room chips and the same seven-column calendar.

**Architecture:** Web crate only. Pure helpers (temperature band, room sub-line, selection fallback, block text) live in `climate/mod.rs` and `climate/timeline.rs` and are unit-tested natively. `WeekTimeline` keeps its props, its pure editing functions and its `BlockEditor`. Only its view turns vertical: day columns, a pointer mapping on Y, a now line and a tap-to-view line. `ZoneCard` becomes `RoomPanel`, with one panel per zone kept mounted. The screen shows only the selected panel, beside a new `RoomList`. One markup serves both widths, and CSS turns the list into chips below 1100 px.

**Tech Stack:** Rust 2024, Leptos (CSR wasm), `chrono`/`chrono-tz`, plain CSS in `crates/oikos-web/styles/main.css` + `styles/tokens.css`, `wasm-pack test --headless --chrome` for DOM tests.

**Spec:** `docs/superpowers/specs/2026-10-24-climate-screen-redesign-design.md`. The visual reference is `docs/superpowers/specs/2026-10-24-climate-screen-redesign-mockup.html`: desktop B in both themes, and the **left** phone, which is the approved A.

## Global Constraints

- Scope is `crates/oikos-web` (plus one docs line). No change to the hub, `oikos-climate`, `oikos-api`, the REST API or the MCP tools.
- The climate strings stay hard-coded English, as the climate screen is today. Do not move them to `i18n.rs`; the i18n project (`feat/i18n-all-screens`) owns that.
- Theme tokens only (`var(--…)`). New colour tokens go in `styles/tokens.css` for both Night Glass (`:root`) and Ivoire (`:root[data-theme="light"]`).
- Temperature bands: setpoint `< 17.0` is low (yellow), `17.0 ≤ t < 20.0` is mid (orange), `≥ 20.0` is high (red), and `Off` is off (neutral grey). The same bands apply in Heat and Cool mode and to the room-list dots.
- Token values: Night Glass `--temp-low: #ffd95a; --temp-mid: #ff9a3c; --temp-high: #ff5a4e; --temp-ink: #1a1206;` and Ivoire `--temp-low: #f2cc55; --temp-mid: #e58a3a; --temp-high: #d04a36; --temp-ink: #2a1a08;`. Contrast may be tuned, but the hues stay.
- A refetch (`house.climate_rev`) must never reset the selected room, an edit draft, a proposal edit, an open decision list or a hold being typed. Switching rooms must not reset them either.
- Generic project: no real room names, hosts or IPs in tracked files, tests or fixtures.
- Before any build: `du -sh ~/dev-scratch/cargo-target`, and over 5 GB run `cargo clean`. Check `df -h ~` and run per-crate tests only when under 5 GB free.
- `cargo fmt --all` and `cargo clippy -p oikos-web --all-targets -- -D warnings` are clean at every commit, and so is `cargo check -p oikos-web --target wasm32-unknown-unknown`.
- Commits are made as `Gekkotron`: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`. Each message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only, because another session may share the worktree.

## Review Focus

1. **A refetch while editing.** `climate_rev` bumps every engine tick. With "Edit schedule" open and a block's editor showing, a refetch must keep both the draft and the open editor (Task 3 browser test `refetch_keeps_selection_and_draft`).
2. **The selected room disappears** (a zone removed or renamed). The screen falls back to the first zone and does not panic or show an empty panel (`pick_zone` tests, Task 1).
3. **A very short block**, such as 15 minutes on a 360 px column: the temperature may be clipped, but the block stays clickable and the drag handles never cover all of it (Task 2 CSS keeps `height: min(10px, 25%)` handles and a `min-height: 4px` block).
4. **A day with no blocks, a Cool-only zone, or no approved rules at all**: the calendar still draws seven empty columns (or "No approved schedule yet"), and the now line still shows on today (Task 2 test `now_line_sits_on_today`).
5. **A zone with no reading or no decision** (an orphan, or one just created). The room row and status card show "No reading" / "No decision yet" and a neutral dot, not a blank or a panic (`room_subline` and `room_band` tests, Task 1).

---

### Task 1: Temperature bands, room sub-line and selection helpers

**Files:**
- Modify: `crates/oikos-web/src/climate/mod.rs` (new pure helpers and tests)
- Modify: `crates/oikos-web/styles/tokens.css` (the four `--temp-*` tokens in both theme blocks)

**Interfaces:**
- Produces, all in `crate::climate`:
  - `pub enum TempBand { Low, Mid, High, Off }` (`Clone, Copy, Debug, PartialEq, Eq`), with `impl TempBand { pub fn class(self) -> &'static str }`. It returns `"climate-temp--low" | "climate-temp--mid" | "climate-temp--high" | "climate-temp--off"`.
  - `pub fn temp_band(sp: Setpoint) -> TempBand`
  - `pub fn room_band(z: &ZoneView) -> Option<TempBand>`. It is the band of `z.decision`'s setpoint when that setpoint is a `Temp`, else `None` (neutral dot).
  - `pub fn room_subline(z: &ZoneView) -> String`
  - `pub fn pick_zone(ids: &[String], wanted: Option<&str>) -> Option<String>`. It returns `wanted` when it is in `ids`, else the first id, else `None`.

- [ ] **Step 1: Write the failing tests** in `climate/mod.rs`'s `tests` module. Build `ZoneView`s with a local helper, like `screens/climate.rs`'s `zone()`, plus a `decision(mode, setpoint)` builder.

```rust
#[test]
fn temp_band_cuts_at_17_and_20() {
    assert_eq!(temp_band(Setpoint::Temp(16.5)), TempBand::Low);
    assert_eq!(temp_band(Setpoint::Temp(17.0)), TempBand::Mid);
    assert_eq!(temp_band(Setpoint::Temp(19.5)), TempBand::Mid);
    assert_eq!(temp_band(Setpoint::Temp(20.0)), TempBand::High);
    assert_eq!(temp_band(Setpoint::Off), TempBand::Off);
    assert_eq!(TempBand::Mid.class(), "climate-temp--mid");
}

#[test]
fn room_subline_reads_each_state() {
    // orphan → "No climate device"; state Off → "Off";
    // Pending + empty rules → "Pending · no schedule"; Pending + rules → "Pending";
    // Active + no decision → "Active · no decision yet";
    // Active + decision Heat 16 → "Heat 16° · Active";
    // same with one override → "Heat 16° · Active · held".
}

#[test]
fn room_band_follows_the_decision_setpoint() {
    // decision Temp(18.5) → Some(Mid); decision Off or no decision → None.
}

#[test]
fn pick_zone_keeps_a_known_choice_and_falls_back() {
    let ids = vec!["a".to_owned(), "b".to_owned()];
    assert_eq!(pick_zone(&ids, Some("b")).as_deref(), Some("b"));
    assert_eq!(pick_zone(&ids, Some("gone")).as_deref(), Some("a"));
    assert_eq!(pick_zone(&ids, None).as_deref(), Some("a"));
    assert_eq!(pick_zone(&[], Some("a")), None);
}
```

Write the `room_subline_reads_each_state` assertions out in full, one per row of the table in the comment. The decision's mode/setpoint text uses the existing `mode_word` + `setpoint_text`.

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p oikos-web --lib climate::tests`. Expected: they fail to compile, because `temp_band`, `TempBand`, `room_subline`, `room_band` and `pick_zone` are not defined.
- [ ] **Step 3: Implement the five items** in `climate/mod.rs`, with one-line doc comments like their neighbours. The orphan check comes first, then `settings.state`, using the precedence of the table. Add the four tokens to `styles/tokens.css` in both theme blocks, with the values from Global Constraints.
- [ ] **Step 4: Run them and see them pass.** Run `cargo test -p oikos-web --lib climate::tests`. Expected: all pass.
- [ ] **Step 5: Commit.** Run `git add crates/oikos-web/src/climate/mod.rs crates/oikos-web/styles/tokens.css`, then commit with the message `feat(web): climate temperature bands and room helpers`.

---

### Task 2: The vertical week calendar

**Files:**
- Modify: `crates/oikos-web/src/climate/timeline.rs` (view only: `minute_on`, `minute_at`, `Bar`, `WeekTimeline`; plus `peek_text`)
- Modify: `crates/oikos-web/src/climate/proposal.rs` (`RulesTimeline`, `RulesPanel` and `ProposalPanel` gain and pass `now`)
- Modify: `crates/oikos-web/src/climate/zone_card.rs` (pass `now` to the two panels; computed below)
- Modify: `crates/oikos-web/styles/main.css` (replace the `.climate-timeline__*` horizontal rules)
- Test: `crates/oikos-web/tests/climate_timeline.rs`

**Interfaces:**
- Consumes: `TempBand`, `temp_band` (Task 1); `week_minute(now, tz) -> (Day, u16)` (existing); `Now` context (`components::now::Now(RwSignal<u64>)`, ticks every 60 s).
- Produces:
  - `WeekTimeline(rules: Signal<ModeRules>, editable: bool, on_change: Callback<ModeRules>, #[prop(optional, into)] now: Option<Signal<(Day, u16)>>)`. With `now`, today's column gets `climate-timeline__col--today`, holds one `.climate-timeline__now` at `top: minute/1440`, and its header gets `climate-timeline__dh--today`. Without `now` (the tests' existing mounts), there is no today marking.
  - `RulesPanel(zone, on_changed, now: Signal<(Day, u16)>)` and `ProposalPanel(zone, proposal, approved, on_changed, now: Signal<(Day, u16)>)`. `RulesTimeline` passes `now` through.
  - `pub fn peek_text(b: &Block) -> String`. It returns `"07:15 – 14:00 · 18.5°"`, plus `" · 3/5"` when the block has evidence, and `"Off"` in place of the temperature for an Off block.
  - DOM, used by Task 3/4 CSS and the tests: `.climate-timeline__cal` (the grid), `.climate-timeline__dh` (day headers; they hold `<span class="climate-timeline__dname">Mo</span><span class="climate-timeline__dname-rest">n</span>`), `.climate-timeline__hours` (the gutter; each label is a `span`, and the ones not on a multiple of 6 h also carry `climate-timeline__hour--minor`), `.climate-timeline__col` (one per day, Mon→Sun, the click target for adding), `.climate-timeline__bar` (plus the `TempBand::class()` of its setpoint), `.climate-timeline__temp`, `.climate-timeline__times`, `.climate-timeline__peek` (the tap-to-view line under the calendar, non-editable only) and `.climate-timeline__now`.

- [ ] **Step 1: Update the browser test helpers, then write the new failing tests** in `tests/climate_timeline.rs`.
  - `mount` appends, once, a `<style>` that gives `.climate-timeline__col` the properties `position: relative; height: 480px` and `.climate-timeline__bar` the property `position: absolute`. Tests load no stylesheet, and the vertical mapping needs a column height.
  - `track(host, day)` becomes `col(host, day)`, which selects `.climate-timeline__col`.
  - `click_at(target, fraction)` now sets `client_y = rect.top + rect.height * fraction` (and `client_x` to the column's middle).
  - Keep `tapping_an_empty_spot_adds_a_block_on_that_day`, with the same expected `18:00–20:00` at `0.75`, and `day_chips_extend_a_block_to_another_day` as they are.
  - Add three tests:

```rust
#[wasm_bindgen_test]
async fn blocks_stand_in_their_day_column_by_time() {
    // rules: Mon 07:15–14:00 at 18.5 and Mon 14:00–24:00 at 16.
    // The first bar in col(Mon) has style containing "top:30.208%" and "height:28.125%",
    // and class climate-temp--mid; the second has class climate-temp--low;
    // its .climate-timeline__temp text is "16°" and its .climate-timeline__times is "14:00 – 24:00".
}

#[wasm_bindgen_test]
async fn now_line_sits_on_today() {
    // mount with now = Signal::from((Day::Sat, 20 * 60 + 30)) and empty blocks:
    // exactly one .climate-timeline__now, inside the 6th column (Sat), style top:85.417%;
    // that column has climate-timeline__col--today.
}

#[wasm_bindgen_test]
async fn tapping_a_block_when_viewing_shows_its_times() {
    // editable=false, one block Mon 07:15–14:00 at 18.5 with evidence 3/5:
    // no .climate-timeline__peek before; click the bar → peek text "07:15 – 14:00 · 18.5° · 3/5";
    // click it again → peek gone; no .climate-timeline__editor ever appears.
}
```

  Add a mount variant, `mount_with(rules, editable, now)`, so that the viewing and now tests can pass those props.

- [ ] **Step 2: Add the native test** `peek_text_reads_times_temp_and_evidence` in `timeline.rs`'s `tests` module. It covers the three forms in the Interfaces block.
- [ ] **Step 3: Run them and see them fail.** Run `cargo test -p oikos-web --lib climate::timeline` (expected: compile failure on `peek_text`), then `cd crates/oikos-web && wasm-pack test --headless --chrome --test climate_timeline` (expected: the new tests fail and the old ones fail on `.climate-timeline__col`).
- [ ] **Step 4: Implement the view.**
  - `minute_on(col, client_y)` uses `rect.top()` and `rect.height()`. `minute_at` uses `closest(".climate-timeline__col")` and `client_y`.
  - `Bar` writes the style `top:{from/1440·100:.3}%;height:{(to−from)/1440·100:.3}%` and adds the band class. Inside, `.climate-timeline__temp` holds the setpoint text plus ` · a/b` evidence when present, then `.climate-timeline__times` and `title=peek_text(b)`. The handles keep their `Edge::From`/`Edge::To` meaning, at the top and bottom.
  - `WeekTimeline` renders the base line, then `.climate-timeline__cal`: an empty corner, 7 `.climate-timeline__dh`, the `.climate-timeline__hours` gutter with labels `00, 03 … 24`, and 7 `.climate-timeline__col`. After those come the existing hint and `BlockEditor` (still inline under the calendar), then the peek line when `!editable` and a block is peeked.
  - Viewing state: a local `peek: RwSignal<Option<usize>>`. Clicking a bar when `!editable` toggles it. When editable, keep today's `selected` behaviour.
  - Thread `now` through `RulesTimeline`, `RulesPanel` and `ProposalPanel`. In `ZoneCard`, build it as `Signal::derive(move || week_minute(expect_context::<Now>().0.get(), tz.get()))`; read the context once, outside the closure.
- [ ] **Step 5: Replace the timeline CSS** in `main.css`.
  - The `.climate-timeline__row`, `__track`, `__track--editable` and `__label` rules are replaced by `.climate-timeline__cal`, a grid with `36px repeat(7, minmax(0, 1fr))` columns and a `6px` gap.
  - `.climate-timeline__col` gets `position: relative; height: 420px`, the `var(--raised)` background, a `1px var(--card-border)` border, a radius of `12px`, and hour lines every 3 h via `repeating-linear-gradient`. `--today` takes the `var(--danger)` border.
  - `.climate-timeline__bar` is placed with `left: 4px; right: 4px`, gets a radius of `10px`, `min-height: 4px` and `color: var(--temp-ink)`.
  - `.climate-temp--low/mid/high` set the background to the matching token. `--off` takes the `var(--raised)` background, a dashed `var(--chip-border)` border and `var(--muted)` ink.
  - `.climate-timeline__handle` becomes a full-width strip of `height: min(10px, 25%)`, with `cursor: ns-resize`, `--from` at the top and `--to` at the bottom.
  - `.climate-timeline__times` is hidden when the block is short. Use a container query on the bar (`container-type: size`; `@container (max-height: 36px)`) to hide it.
  - `.climate-timeline__now` is `2px` in `var(--danger)`, with a small dot (`::before`).
  - `.climate-timeline__dh--today` is bold, with a `var(--danger)` underline.
  - The old `.climate-timeline__bar--off` and `--selected` rules are kept and adapted.
- [ ] **Step 6: Run the tests and see them pass.** Run `cargo test -p oikos-web --lib climate` and `cd crates/oikos-web && wasm-pack test --headless --chrome --test climate_timeline`. Expected: all pass. Then run `cargo check -p oikos-web --target wasm32-unknown-unknown` and clippy; both should be clean.
- [ ] **Step 7: Commit.** Run `git add` on the five files listed above, then commit with the message `feat(web): vertical climate week calendar with temperature colours`.

---

### Task 3: Room list, room panel and the full-width screen

**Files:**
- Create: `crates/oikos-web/src/climate/room_list.rs`
- Rename + modify: `crates/oikos-web/src/climate/zone_card.rs` → `room_panel.rs`, with `ZoneCard` renamed to `RoomPanel` (`git mv`, then edit)
- Modify: `crates/oikos-web/src/climate/mod.rs` (the `pub mod` lines)
- Modify: `crates/oikos-web/src/screens/climate.rs`
- Modify: `crates/oikos-web/styles/main.css` (screen layout; drop the old `.climate`/`.climate__zones`/`.climate-zone*` screen rules)
- Create: `crates/oikos-web/tests/climate_rooms.rs`

**Interfaces:**
- Consumes: `room_subline`, `room_band`, `TempBand::class`, `pick_zone` (Task 1); `degrees_text` (existing); `WeekTimeline`/`RulesPanel`/`ProposalPanel` with `now` (Task 2).
- Produces:
  - `RoomList(zones: Signal<Vec<ZoneView>>, selected: Signal<Option<String>>, on_select: Callback<String>)`. It renders a `<nav class="climate-rooms" aria-label="Rooms">` with one `<button class="climate-room">` per zone. A selected button carries `climate-room--selected` and `aria-current="true"`. Each button holds `.climate-room__dot` (plus the band class, or `climate-temp--none`), `.climate-room__name`, `.climate-room__temp` (reading via `degrees_text`, or "—") and `.climate-room__sub` (`room_subline`). An orphan zone also shows its "No climate device" badge.
  - `RoomPanel(zone: Signal<ZoneView>, tz: Signal<Tz>, on_changed: Callback<()>)`. The props are unchanged from `ZoneCard`. The root is `<section class="climate-panel">`, and its children are in order:
    1. `.card.climate-status`: the name, the reading in a large type with its age, the target + source chip + reason, the state segmented switch, the hold row, the overrides with Clear, and the settings-invalid badge.
    2. `.card.climate-week`: "This week", with the `RulesPanel` inside. It keeps its own Edit/Save/Cancel and mode tabs, and the base line `WeekTimeline` already draws. Under it sits a `.climate-legend` with the three band swatches `≤ 16°`, `17 – 19°`, `≥ 20°`.
    3. `.card` with the `ProposalPanel`, only when a proposal exists.
    4. `.climate-panel__lower`, holding `.card.climate-learn-card` (the learn line, Learn now and the `ThermometerField`) and `.card.climate-decisions-card` (today's decisions toggle and list).

- [ ] **Step 1: Write the failing browser tests** in `tests/climate_rooms.rs`. Mount `RoomList` directly with fixture `ZoneView`s named "Living room", "Bedroom 1" and "Bedroom 2".

```rust
#[wasm_bindgen_test]
async fn room_list_marks_the_selected_room_and_selects_on_click() {
    // selected = Some("bedroom-1"): that button has aria-current="true", the others have none;
    // its .climate-room__sub reads "Heat 18° · Active" and its dot has climate-temp--mid;
    // a zone with no decision has climate-temp--none;
    // clicking "Bedroom 2" calls on_select with "bedroom-2".
}
```

Also add `refetch_keeps_selection_and_draft`, from Review Focus 1, in the same file. Mount `RulesPanel` with a `RwSignal<ZoneView>` holding approved rules (it needs `provide_context(Toasts::new())` and the `now` prop: pass `Signal::from((Day::Mon, 0))`). Then:
  1. Click "Edit rules" and click a bar, so that `.climate-timeline__editor` is open.
  2. Set the zone signal to a clone with a different `reading`, which simulates a refetch.
  3. Run `tick().await` and assert that the editor is still open and that "Save" is still shown.

- [ ] **Step 2: Run them and see them fail.** Run `cd crates/oikos-web && wasm-pack test --headless --chrome --test climate_rooms`. Expected: a compile failure, because `room_list` does not exist.
- [ ] **Step 3: Implement `RoomList`** in `room_list.rs`, and do the `git mv` from `zone_card.rs` to `room_panel.rs` with the `ZoneCard` → `RoomPanel` rename. Rearrange its `view!` into the four children above. No behaviour changes: the hold, overrides, learn, decisions and proposal key memo stay as they are. The one markup change is the "Until next change" button, which holds `<span class="climate-hold__long">"Until next change"</span><span class="climate-hold__short">"Next change"</span>` so that Task 4 can switch to the short label on phones.
- [ ] **Step 4: Rebuild `ClimateScreen`.**
  - The root becomes `<div class="climate-screen">`, because `.climate` collides with the dashboard climate card's `.climate` in `styles/cards.css`.
  - The header and banners stay as they are.
  - Add `selected: RwSignal<Option<String>>` and `current = Memo::new(move |_| pick_zone(&ids.get(), selected.get().as_deref()))`.
  - Below the banners comes `.climate-screen__body`, holding `<RoomList zones=… selected=current.into() on_select=…/>` and `.climate-screen__panels`. That container keeps today's keyed `<For>` (one `RoomPanel` per zone id), with each panel wrapped in `<div hidden=move || current.get().as_deref() != Some(&id)>`. That way every panel stays mounted, and switching rooms keeps its state.
  - The empty-zones message stays, and replaces the body.
- [ ] **Step 5: Add the wide-layout CSS.**
  - `.screen:has(.climate-screen) { max-width: none; }`. Put it next to the `html:has(.flow-canvas-screen)` precedent.
  - `.climate-screen`: a flex column with a `12px` gap.
  - `.climate-screen__body`: a grid with `240px minmax(0, 1fr)` columns, a `16px` gap and `align-items: start`.
  - `.climate-rooms`: a `.card`-like surface, a column of buttons.
  - `.climate-room`: a grid of `1fr auto`, a radius of `14px` and no border. `--selected` takes the `var(--nav-active-bg)` background.
  - `.climate-room__dot`: an `8px` circle, and `climate-temp--none` takes the `var(--chip-border)` background.
  - `.climate-panel`: a flex column with a `12px` gap. `.climate-status`: flex-wrap rows.
  - `.climate-panel__lower`: a grid with `1fr 1fr` columns and a `12px` gap.
  - Delete the now-unused screen rules: `.climate {…}` in `main.css` (the screen one, not the one in `cards.css`), `.climate__zones`, `.climate-zone`, `.climate-zone__*` that no markup uses any more, and `.climate .sheet__header > .btn`, which is re-pointed to `.climate-screen`.
- [ ] **Step 6: Run the tests and see them pass.** Run `cd crates/oikos-web && wasm-pack test --headless --chrome --test climate_rooms --test climate_timeline` and `cargo test -p oikos-web --lib`. Expected: all pass. Then run the wasm check and clippy; both should be clean.
- [ ] **Step 7: Commit.** Stage the new, renamed and modified files explicitly, including the `git mv`, then commit with the message `feat(web): climate room list and full-width room panel`.

---

### Task 4: Narrow layout, docs and a visual check

**Files:**
- Modify: `crates/oikos-web/styles/main.css` (the narrow `@media`)
- Modify: `docs/users/web-ui.md:193` (the Climate paragraph)

- [ ] **Step 1: Add the narrow layout** under `@media (max-width: 1099px)`.
  - `.climate-screen__body` becomes one column.
  - `.climate-rooms` becomes a horizontal, scrollable row: `flex-direction: row; overflow-x: auto; scrollbar-width: none`, with no card surface.
  - `.climate-room` becomes a pill chip with a `1px var(--chip-border)` border and a radius of `99px`. `.climate-room__sub` is hidden. The selected chip is filled with `var(--nav-active-bg)`.
  - `.climate-panel__lower` becomes one column.
  - The status card stays compact: the reading and target sit on one row, and the state switch takes the full width.
  - Under `@media (max-width: 640px)`:
    - `.climate-timeline__col` and the gutter get `height: 360px`.
    - `.climate-timeline__cal` gets `22px repeat(7, minmax(0, 1fr))` columns and a `3px` gap.
    - `.climate-timeline__times`, `.climate-timeline__hour--minor` and `.climate-timeline__dname-rest` are hidden.
    - Bars get `left/right: 2px`, a font size of `11px` and centred text.
    - The hold row shows `.climate-hold__short` and hides `.climate-hold__long`. Above 640 px it is the other way round, so add the base rule `.climate-hold__short { display: none; }` outside the media query.
- [ ] **Step 2: Update `docs/users/web-ui.md:193`.** The paragraph says: Climate (`/climate`, beta) lists the rooms on the left, or as chips on a phone. The selected room shows its status (reading, target, state, hold), its week as a calendar of seven day columns with hours going down, with each block coloured by its temperature (yellow below 17 °C, orange to 20 °C, red above) and a line at the current time, and then its proposal, learning, thermometer and recent decisions. Keep the existing sentences about editing (tap an empty spot to add a two-hour block, day chips in a block's editor). Add that tapping a block while viewing shows its times.
- [ ] **Step 3: Check the result visually.**
  - Build and serve the web app: `cd crates/oikos-web && trunk serve` on `127.0.0.1:3000`, proxied to a hub.
  - Screenshot `/climate` at 1440×900 and at 390×844 (phone emulation), in both themes. Use headless Chrome over CDP, `Emulation.setDeviceMetricsOverride` for the phone, and `--window-size` no smaller than 500.
  - Compare with the mockup. Expected:
    - There are no empty sides at 1440 px.
    - The phone shows all seven columns with no horizontal page scroll.
    - The block colours match the bands.
    - The now line is on today.
  - If no hub is reachable, say so in the report, and do not claim the check passed.
- [ ] **Step 4: Run the full checks.** Run `cargo fmt --all --check`, `cargo clippy -p oikos-web --all-targets -- -D warnings`, `cargo test -p oikos-web` and `cd crates/oikos-web && wasm-pack test --headless --chrome`. Expected: all clean and passing.
- [ ] **Step 5: Commit.** Run `git add crates/oikos-web/styles/main.css docs/users/web-ui.md`, then commit with the message `feat(web): climate screen phone layout and docs`.
