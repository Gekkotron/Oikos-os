# Entity Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One searchable entity picker (a popup with a room column on a computer, a full-screen list grouped by room on a phone) replaces every entity choice in the web UI.

**Architecture:** A pure module `entity_picker::model` turns (entities, areas, filter) into a `Listing` and is tested natively. A view module renders that listing in a native `<dialog>` (top layer: it sits above any sheet, traps focus, Esc closes it). One DOM serves both layouts; CSS at the app's 640 px breakpoint swaps the room column for room headings. Thin field components (single, multi, add-button) wrap the dialog, and each screen swaps its `<select>` / text field for one of them.

**Tech Stack:** Rust, Leptos (CSR, wasm32), web-sys, `wasm-pack test` (headless Chrome) for view tests, `cargo test` for the model.

**Spec:** `docs/superpowers/specs/2026-10-25-entity-picker-design.md`

## Global Constraints

- Scope: `crates/oikos-web` and `docs/users/` only. No hub, API or Android change.
- Every user-visible string goes through i18n, in EN and FR. `tests/bare_text.rs` must stay green, so do not add new files to `bare_text_allow.txt`.
- The look comes from the theme tokens in `styles/tokens.css` (`--ink`, `--card-bg`, `--card-border`, `--chip-border`, `--scrim`, `--accent-ring`, `--accent-soft`, `--shadow`) so Night Glass and Ivoire both work. Never hard-code mockup colours.
- Breakpoint: `max-width: 640px` is the phone layout.
- Generic fixtures only: no real room, device or host names in code, tests or docs.
- Every existing pick filter keeps its meaning. What a field *accepts* becomes the picker's hard `pool`; the spec's pre-filter is the soft `suggest` chip. The All chip widens the suggestion to the pool, never past it.
- Commits use `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …` and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only (another session may share the worktree).
- Before any build, check `df -h ~` and `du -sh ~/dev-scratch/cargo-target`. Above 5 GB, run `cargo clean -p oikos-web`. Under 5 GB free on disk, run per-crate tests only.
- CI gates, which must pass before the PR: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test --workspace`, `wasm-pack test --headless --chrome crates/oikos-web`.

## Review Focus

1. **A state echo while the picker is open**: rows must not reorder or redraw, and the highlighted row and scroll position must stay put. The listing memo must compare equal when only `state` changed (Task 1 test `listing_ignores_state_changes`, Task 2 wasm test `echo_keeps_highlight`).
2. **A stored id the house no longer has**: it shows as a "missing" row in the closed field and at the top of the open list, and it can be cleared or replaced (Task 1 `missing_ids_lead`, Task 3 `missing_value_shows_raw_id`).
3. **A query with accents or the other language**: "temperature" finds "Température"; "interrupteur" finds a switch in EN mode (Task 1 `search_folds_accents_and_both_languages`).
4. **The phone Back button while the picker is open**: it closes the picker and leaves the page and its draft untouched (Task 2 `history_back_closes_picker`).
5. **The All chip with no query on a house of ~1 000 entities**: at most 200 rows render, followed by the "200 of N" line (Task 1 `all_without_query_caps_at_200`).

---

## File Structure

- Create `crates/oikos-web/src/entity_picker/mod.rs`: re-exports, plus `Keep` and `Suggest`.
- Create `crates/oikos-web/src/entity_picker/model.rs`: pure filtering, search, grouping and cap (native tests).
- Create `crates/oikos-web/src/entity_picker/needs.rs`: the shared predicates (thermometer, presence, on/off, siren, remote, tracker, signal, commandable).
- Create `crates/oikos-web/src/entity_picker/view.rs`: `PickerBody` (search, chips, rooms, list) and `EntityPicker` (the `<dialog>` shell).
- Create `crates/oikos-web/src/entity_picker/fields.rs`: `EntityPickField`, `EntityPickListField`, `EntityAddButton`.
- Create `crates/oikos-web/src/i18n/picker.rs`: the `PickerText` table.
- Create `crates/oikos-web/styles/picker.css`, linked from `index.html`.
- Create `crates/oikos-web/tests/entity_picker.rs` (wasm).
- Modify the call sites listed in Tasks 4–8, and delete the replaced components once nothing uses them.

---

### Task 1: Pure model, predicates and texts

**Files:**
- Create: `src/entity_picker/mod.rs`, `src/entity_picker/model.rs`, `src/entity_picker/needs.rs`, `src/i18n/picker.rs`
- Modify: `src/lib.rs` (add `pub mod entity_picker;`), `src/i18n/mod.rs` (add `pub mod picker;`)
- Move: `is_siren`, `is_remote`, `is_tracker` from `src/screens/alarm_settings.rs` into `needs.rs` (`pub`), and have alarm_settings import them. Move `is_thermometer` from `src/climate/entity_field.rs` the same way.

**Interfaces (Produces):**

```rust
// mod.rs
pub type Keep = std::sync::Arc<dyn Fn(&EntityView) -> bool + Send + Sync>;
#[derive(Clone)]
pub struct Suggest { pub text: PickerText, pub keep: Keep }

// model.rs
pub const CAP: usize = 200;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Chip { Suggested, All }
pub struct Filter<'a> {
    pub pool: Option<&'a Keep>,      // hard: what the field accepts; None = every entity
    pub suggest: Option<&'a Keep>,   // soft: the Suggested chip
    pub chip: Chip,                  // ignored when suggest is None
    pub diagnostic: bool,            // also list !is_primary() entities
    pub query: &'a str,
    pub room: Option<&'a str>,       // area id picked in the room column; None = all rooms
    pub current: &'a [String],       // chosen ids: always listed (pool, chip and diagnostic ignored; query applies)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row { pub id: String, pub title: String, pub device: Option<String>, pub room: Option<String>, pub icon: &'static str }
#[derive(Clone, Debug, PartialEq)]
pub struct Section { pub area: String, pub room: String, pub rows: Vec<Row> }
#[derive(Clone, Debug, PartialEq)]
pub struct RoomCount { pub area: String, pub room: String, pub count: usize }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Listing {
    pub missing: Vec<String>,     // current ids the house lacks, in `current` order
    pub sections: Vec<Section>,   // after the room narrowing and the cap
    pub rooms: Vec<RoomCount>,    // matches per room, before the room narrowing
    pub total: usize,             // matches before the cap (and before the room narrowing)
    pub shown: usize,             // rows in `sections`
}
pub fn listing(lang: Lang, entities: &BTreeMap<String, EntityView>, areas: &[AreaView], f: &Filter) -> Listing;
pub fn matches(lang: Lang, e: &EntityView, room: Option<&str>, needle: &str) -> bool; // needle already folded

// needs.rs (each `fn(&EntityView) -> bool`, plus `keep(f) -> Keep` to wrap one)
pub fn is_thermometer, is_presence, is_on_off, is_siren, is_remote, is_tracker, is_signal, is_commandable;
pub fn keep(f: fn(&EntityView) -> bool) -> Keep;

// i18n/picker.rs
pub enum PickerText { Search, All, Diagnostic, AllRooms, NoRoom, NoMatch, SearchAll, Unavailable,
    Missing, Close, Choose, Thermometers, Presence, OnOff }
pub fn picker_text(lang: Lang, k: PickerText) -> &'static str;
pub fn done_label(lang: Lang, n: usize) -> String;               // "Done (3)" / "Terminé (3)"
pub fn capped_label(lang: Lang, shown: usize, total: usize) -> String; // "200 of 812 shown, type to narrow" / "200 sur 812 affichés, tapez pour affiner"
pub fn use_picker_text() -> impl Fn(PickerText) -> &'static str + Copy;
```

Rules `listing` implements, from the spec:
- Candidates are `pool`-passing entities. On the Suggested chip (with `suggest` present), they also pass `suggest`. Unless `diagnostic` is set, they are `is_primary()`. Then `matches(...)` with the folded `query`. Ids in `current` that exist are added even when the chip, pool or diagnostic rule would drop them, but only if they match the query.
- `matches`: the folded needle is a substring of the folded `list_label`, `name`, `device_name`, room name, either of the `kind_search_words`, or the id. An empty needle matches everything.
- `Row.title` = `entity_title(lang, e)`; `device` = `device_name` when non-empty and not equal to the title; `room` = `room_name(areas, &e.area)`; `icon` = `icons::entity_icon(e)`.
- Grouping follows `group_wizard::group_by_room`: sections come in `areas` slice order, each sorted by folded title then id. Areas not in `areas` come next, then `UNASSIGNED_AREA` / empty last as "No room" (`PickerText::NoRoom`).
- `rooms` counts come from all matches. Then `room` narrows the sections. Then, when `chip == All || suggest.is_none()`, the query is empty and `room` is `None`, only the first `CAP` rows are kept, in section order.
- Derive `PartialEq` everywhere. No `state` field enters a `Row`, so a state echo yields an equal `Listing`.

Predicates (keep today's meaning for the moved ones):
- `is_presence`: (`kind == BinarySensor` and `device_class` ∈ {presence, occupancy, motion}) || `is_tracker(e)`.
- `is_on_off`: `kind` ∈ {Switch, BinarySensor}, or the id starts with `virtual_` and `kind == Switch`.
- `is_signal`: `component` ∈ {"device_automation", "event", "tag"}.
- `is_commandable`: `!crate::flow_picker::commands(e, None).is_empty()`.

- [ ] **Step 1: Write the failing tests in `model.rs`** (fixture helper `ent(id, name, device, area, kind, class)` building an `EntityView`; generic names only)

```rust
#[test] fn search_folds_accents_and_both_languages() {
    // "Température" (sensor, class temperature) found by "temperature" and "TEMPÉ";
    // a Switch found by "interrupteur" with lang = En and by "switch" with lang = Fr.
}
#[test] fn device_and_room_names_match() // query "kitchen plug" style: device_name "Plug A" found by "plug a"; room "Kitchen" found by "kitch"
#[test] fn look_alikes_differ_by_device() // two "Backlight mode" switches on devices "Plug A"/"Plug B": rows' `device` are Some("Plug A"), Some("Plug B")
#[test] fn suggested_chip_narrows_and_all_widens_to_pool_only() // suggest=is_thermometer → 1 row; All → every pool entity; pool=is_siren + All never lists a sensor
#[test] fn diagnostic_hidden_unless_flag_or_current() // EntityCategory::Diagnostic absent; present with diagnostic=true; present when its id is in current
#[test] fn missing_ids_lead() // current = ["gone", "a"] with "gone" absent → missing == ["gone"], "a" still in sections
#[test] fn sections_follow_area_order_no_room_last() // areas [b, a] → sections b, a, then unknown area "zz", then "No room"
#[test] fn room_narrows_sections_but_not_counts() // room=Some("a") → sections only a; rooms counts unchanged; total unchanged
#[test] fn all_without_query_caps_at_200() // 1 000 entities, Chip::All, empty query → shown == 200, total == 1 000; query "1" → no cap
#[test] fn listing_ignores_state_changes() // same house with a different state.value → listing(..) == listing(..)
```

In `needs.rs`: `presence_takes_motion_and_trackers`, `signal_components`, plus the moved predicates' existing tests (moved, not deleted). In `i18n/picker.rs`: `table_complete` using `assert_table` (same as other modules, `same: &[]`), and `done_and_capped_labels` asserting the exact EN/FR strings above.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p oikos-web --lib entity_picker i18n::picker`
Expected: compile errors (module missing).

- [ ] **Step 3: Implement** `mod.rs`, `model.rs`, `needs.rs` and `i18n/picker.rs` with the signatures above. Reuse `flow_edit::fold_accents`, `format::{entity_title, list_label, kind_search_words, room_name}`, `icons::entity_icon`. Move the four predicates with their doc comments.

- [ ] **Step 4: Run them and see them pass**

Run: `cargo test -p oikos-web --lib` → all pass, including alarm and climate tests that use the moved predicates.

- [ ] **Step 5: Commit**: `feat(web): entity picker model, predicates and texts`

---

### Task 2: The picker dialog and its body

**Files:**
- Create: `src/entity_picker/view.rs`, `styles/picker.css`, `tests/entity_picker.rs`
- Modify: `index.html` (link `styles/picker.css` after `widgets.css`), `Cargo.toml` (add `"History"` and `"HtmlDialogElement"` to the `[dependencies]` web-sys features, and `"HtmlDialogElement"` to `[dev-dependencies]`)

**Interfaces:**
- Consumes: Task 1 (`listing`, `Filter`, `Chip`, `Keep`, `Suggest`, `PickerText`, `capped_label`, `done_label`).
- Produces:

```rust
#[derive(Clone, Copy)]
pub enum Mode { Single, Multi }

/// The top row above the list: "None" / "Automatic · <entity>". Picking it reports `None`.
#[derive(Clone)]
pub struct Pinned { pub text: Signal<String> }

#[component]
pub fn PickerBody(
    mode: Mode,
    #[prop(into)] current: Signal<Vec<String>>,
    #[prop(optional)] pool: Option<Keep>,
    #[prop(optional)] suggest: Option<Suggest>,
    #[prop(optional)] pinned: Option<Pinned>,
    on_pick: Callback<Option<String>>,  // Single: the pick (None = pinned). Multi: Some(id) toggles id.
    #[prop(optional)] inline: bool,     // true: no room column at any width (group wizard)
) -> impl IntoView;

#[component]
pub fn EntityPicker(
    open: RwSignal<bool>,
    #[prop(into)] title: Signal<String>,
    mode: Mode,
    #[prop(into)] current: Signal<Vec<String>>,
    #[prop(optional)] pool: Option<Keep>,
    #[prop(optional)] suggest: Option<Suggest>,
    #[prop(optional)] pinned: Option<Pinned>,
    on_pick: Callback<Option<String>>,
) -> impl IntoView;
```

Behaviour to build (spec "Behaviour" section, all of it):
- **`EntityPicker`:** a `<dialog class="picker">` opened with `show_modal()` when `open` turns true, closed with `close()` when it turns false. The `cancel` event (Esc) sets `open` false. A click whose target is the dialog itself (the backdrop) also closes it. The header holds the title, a close button (`PickerText::Close`, "✕"), and in Multi mode a `done_label(n)` button. Single mode closes after `on_pick`.
- **Back closes the picker:** at every open (computer too: harmless, and it keeps one code path that the tests can reach; this is a plan call widening the spec's phone-only wording), call `history.push_state(&JsValue::NULL, "")`. There is no URL change, so the router and `BackStack` don't see it. Listen for `popstate` to set `open` false. When closing any other way, remove the listener first, then call `history.back()` to drop the pushed entry.
- **`PickerBody`:** the search `<input type="search">` is focused on open (placeholder `PickerText::Search`). Chips: Suggested (`suggest.text`, shown only with `suggest`), All, + Diagnostic (a toggle). The room column (`.picker__rooms`) lists All rooms plus each `RoomCount`, and typing resets `room` to None. The list is `role="listbox"` and holds, in order: the pinned row, the missing rows (raw id + `PickerText::Missing`), then the sections (`.picker__section-title` headings + rows), then `capped_label` when `shown < total` and no query. When there are no rows: `NoMatch`, plus a `SearchAll` button when on the Suggested chip.
- **Rows** (`role="option"`, `aria-selected`): the icon, the title, a second line with the device (and on a computer the room: render `.picker__row-room`, which CSS hides on phones), and on the right the live value `display_value(lang, &e.state, e.unit.as_deref())` above `relative_time(lang, last_update, now)` from the `Now` context. Read the value per row from `house.entities` by id, so only that text node updates. An unavailable entity (`Availability::Offline`) gets `.picker__row--off` and `PickerText::Unavailable`. Multi rows carry a checkbox reflecting `current`.
- **Listing:** compute it in a `Memo` (equal listings don't redraw). Keys for `<For>` are the row ids.
- **Keyboard:** ↑/↓ on the search box move the highlight over the flattened visible rows (pinned, missing, rows), shown with `aria-activedescendant` and scrolled into view. Enter picks (Single) or toggles (Multi).
- **CSS (`picker.css`):** dialog centred, `width: min(760px, 100vw - 32px)`, `max-height: min(640px, 100vh - 64px)`, `::backdrop { background: var(--scrim) }`. Two columns (rooms 160 px | list) above 640 px. At `max-width: 640px`: `inset: 0`, full width and height, no radius, rooms column hidden, `.picker__section-title` shown, `.picker__row-room` hidden. Above 640 px, section titles are hidden only when a room is picked. `.picker-body--inline` hides the room column at every width. The highlighted/selected row uses `--accent-soft` and an outline of `--accent-ring`. Chips reuse `.chip` / `.chip--selected`.

- [ ] **Step 1: Write the failing wasm tests** in `tests/entity_picker.rs`. Mount `EntityPicker` with a `House` holding generic fixtures (copy the `entity()` helper style from `tests/group_wizard.rs`) and a `Now` context:

```rust
#[wasm_bindgen_test] async fn opens_modal_with_search_focused()       // dialog.open == true; document.activeElement is the search input
#[wasm_bindgen_test] async fn typing_filters_rows()                   // type "plug b" → exactly one [role=option] besides pinned
#[wasm_bindgen_test] async fn single_pick_reports_and_closes()        // click row "b" → on_pick got Some("b"); open == false
#[wasm_bindgen_test] async fn pinned_row_reports_none()               // click pinned → on_pick got None
#[wasm_bindgen_test] async fn multi_toggles_and_stays_open()          // click two rows → two calls; open still true; Done shows "(2)" when current has 2
#[wasm_bindgen_test] async fn escape_closes()                         // dispatch cancel on the dialog → open == false
#[wasm_bindgen_test] async fn arrows_and_enter_pick()                 // ArrowDown ×2 + Enter on the input → on_pick got the 2nd visible row
#[wasm_bindgen_test] async fn echo_keeps_highlight()                  // ArrowDown, then update one entity's state in House → same aria-activedescendant, value text updated
#[wasm_bindgen_test] async fn suggested_chip_then_all()               // suggest = thermometers: 1 row; click All: every row
#[wasm_bindgen_test] async fn history_back_closes_picker()            // open → history().back() → await popstate → open == false; location.pathname unchanged
#[wasm_bindgen_test] async fn escape_pops_the_pushed_entry()         // open, Esc → exactly one popstate fires (from the picker's own history.back()); open stays false; pathname unchanged
```

- [ ] **Step 2: Run them and see them fail**

Run: `wasm-pack test --headless --chrome crates/oikos-web -- --test entity_picker`
Expected: compile failure (`view` module missing).

- [ ] **Step 3: Implement `view.rs` and `picker.css`** following the behaviour above.

- [ ] **Step 4: Run them and see them pass**, plus `cargo check -p oikos-web --target wasm32-unknown-unknown` and `cargo clippy -p oikos-web --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**: `feat(web): entity picker dialog`

---

### Task 3: The field components

**Files:**
- Create: `src/entity_picker/fields.rs`
- Test: add to `tests/entity_picker.rs`

**Interfaces:**
- Consumes: `EntityPicker`, `Mode`, `Pinned`, `Keep`, `Suggest` (Task 2).
- Produces:

```rust
/// One entity, or the pinned choice. Closed: the chosen entity's full row, or the pinned text.
#[component]
pub fn EntityPickField(
    #[prop(into)] label: Signal<String>,
    #[prop(into)] value: Signal<Option<String>>,
    on_change: Callback<Option<String>>,
    #[prop(optional)] pool: Option<Keep>,
    #[prop(optional)] suggest: Option<Suggest>,
    /// "None" / "Automatic · …": shown when `value` is None, and offered as the pinned row.
    /// Without it the field is required (no pinned row, closed text = PickerText::Choose).
    #[prop(optional, into)] pinned: Option<Signal<String>>,
    #[prop(optional)] compact: bool,
    /// CSS class of the wrapping element, so a host keeps its own field layout
    /// ("climate-field", "node-editor__field", "override__field").
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView;

/// A list of entities: removable chips (name + device), and an EntityAddButton.
#[component]
pub fn EntityPickListField(
    #[prop(into)] label: Signal<String>,
    #[prop(into)] value: Signal<Vec<String>>,
    on_change: Callback<Vec<String>>,
    #[prop(optional)] pool: Option<Keep>,
    #[prop(optional)] suggest: Option<Suggest>,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView;

/// A "+ Add…" button that opens the picker in Multi mode over `taken`; each tick reports the id.
#[component]
pub fn EntityAddButton(
    #[prop(into)] label: Signal<String>,
    #[prop(into)] taken: Signal<Vec<String>>,
    on_toggle: Callback<String>,
    #[prop(optional)] pool: Option<Keep>,
    #[prop(optional)] suggest: Option<Suggest>,
) -> impl IntoView;
```

- The closed `EntityPickField` is a `<button type="button" class="picker-field">` (Enter and Space open it natively). The full row shows icon, title, `device · room`, value and a chevron. `compact` drops the value and the icon. A value the house lacks shows the raw id and `PickerText::Missing`.
- In `EntityPickListField`, each chip toggle and each picker tick computes the new Vec (add if absent, remove if present) and calls `on_change` with it straight away.

- [ ] **Step 1: Write the failing wasm tests**:
  - `closed_field_shows_row_with_device_and_room`: the text contains the title, "Plug A" and the room name.
  - `missing_value_shows_raw_id`: value `Some("gone")` shows "gone" and the Missing text.
  - `list_field_ticks_add_and_remove`: two ticks then one untick give `on_change` values `[a]`, `[a,b]`, `[b]`.
  - `pinned_text_when_none`: value None shows the pinned text.
- [ ] **Step 2: Run, and see them fail** (`wasm-pack test … -- --test entity_picker`).
- [ ] **Step 3: Implement** `fields.rs` and its CSS (`.picker-field`, `.picker-field--compact`, chips reuse `.chip`).
- [ ] **Step 4: Run, and see them pass**, plus clippy and the wasm check.
- [ ] **Step 5: Commit**: `feat(web): entity picker fields`

---

### Task 4: Climate settings, room thermometer and home settings

**Files:**
- Modify: `src/climate/settings_sheet.rs` (Outdoor temperature, Presence, Holiday), `src/climate/room_panel.rs:542` and `src/screens/home_settings_panel.rs:369` (`ThermometerField` users), `src/screens/home_settings_panel.rs:330-360` (Mode selector), `src/climate/entity_field.rs`
- Test: the existing `tests/climate_*.rs` and `tests/home_settings_panel.rs` that drive these selects.

**Mapping:**

| Field | Component | pool | suggest | pinned |
|---|---|---|---|---|
| Outdoor temperature | `EntityPickField` | — | `Thermometers`, `is_thermometer` | `ClimateText::None` |
| Presence | `EntityPickListField` | — | `Presence`, `is_presence` | — |
| Holiday | `EntityPickField` | — | `OnOff`, `is_on_off` | `ClimateText::None` |
| Room thermometer (both hosts) | `EntityPickField` | — | `Thermometers`, `is_thermometer` | today's `auto_text` (the `automatic_with(...)` string; host overrides kept) |
| Home Mode selector | `EntityPickField` | `kind == Select` | — | `HomeText::NoneChoice` |

- `ThermometerField` keeps its public props (`value`, `auto`, `on_change`, `label`, `auto_text`) and becomes a thin wrapper over `EntityPickField`, so its two hosts don't change. Delete `EntityField`, `EntityListField`, `options`, `label_of`, and `thermometers_by_room` together with its test. The model's `sections_follow_area_order_no_room_last` replaces that test, which is the spec's ordering change.
- The Mode selector loses its "virtual first" sort (the hub's virtuals sit under "No room"). Delete `mode_options`.

- [ ] **Step 1: Update the wasm tests that drove these `<select>`s** so they drive the picker: open the field, click the row, check the draft or the PUT body as the old assertion did. Add `outdoor_picker_opens_on_thermometers` in `tests/climate_layout.rs` (or the file that mounts the settings sheet): open Outdoor temperature → only temperature rows plus the pinned None.
- [ ] **Step 2: Run them and see them fail** (`wasm-pack test … -- --test climate_layout --test climate_rooms --test home_settings_panel`).
- [ ] **Step 3: Swap the fields** as mapped, and delete the dead code.
- [ ] **Step 4: Run them and see them pass**, plus `cargo test -p oikos-web --lib` and clippy.
- [ ] **Step 5: Commit**: `feat(web): climate and home settings use the entity picker`

---

### Task 5: Alarm settings

**Files:**
- Modify: `src/screens/alarm_settings.rs` (the three `AddPicker` uses at ~544, ~737, ~952 and ~1005; `options_where`, `AddPicker`, `label_of`)
- Test: the alarm wasm test that adds a siren, remote or tracker (find it with `grep -l alarm__add crates/oikos-web/tests`; if there is none, add `tests/entity_picker.rs::alarm_adds_siren`).

- Each `AddPicker` becomes an `EntityAddButton`:
  - `pool = keep(is_siren | is_remote | is_tracker)` as today; no `suggest`.
  - `taken` = the ids already in the draft list.
  - `on_toggle` adds the id with today's defaults when absent, and removes it when present.
- The rows the alarm screen already renders for each chosen entity stay as they are, but their name uses `entity_title` plus the device on a second line, in place of `list_label`.
- Delete `AddPicker`, `options_where` and `Options` once unused.

- [ ] **Step 1: Write or update the failing test**: click "Add siren", tick one → the draft has it, the picker is still open; tick it again → removed.
- [ ] **Step 2: Run it and see it fail.**
- [ ] **Step 3: Swap the pickers.**
- [ ] **Step 4: Run it and see it pass**, plus `cargo test -p oikos-web --lib`.
- [ ] **Step 5: Commit**: `feat(web): alarm settings use the entity picker`

---

### Task 6: Flow card settings

**Files:**
- Modify: `src/screens/flow_canvas/settings.rs`: the entity `TextField`s of `TriggerState` (~594), `TriggerSignal` (~681), `If` (~783) and `Command` (~902); `src/i18n/flows.rs` (add `TriggerValue` to the enum, its `ALL` list and the table)
- Test: `tests/flow_canvas.rs`

| Node | pool | pinned | value read from the draft |
|---|---|---|---|
| `trigger.state` | — (any entity) | — | `TriggerState.entity` (`""` → None) |
| `trigger.signal` | `is_signal` | — | `TriggerSignal.entity` |
| `if` | — | new `FlowText::TriggerValue` = "Trigger value" / "Valeur du déclencheur" (what an empty `If.entity` means, per `EntityDefaults`) | `If.entity` |
| `command` | `is_commandable` | — | `Command.entity` |

- Use `EntityPickField` with `compact = true` and `class = "node-editor__field"`. `value` is a `Signal::derive` over `form.editor.draft` reading the node's entity, so it tracks edits. `on_change` calls the existing `entity_write` callback with the id (`None` for the pinned row).
- The Command JSON field's silent drop of bad input is out of scope (spec).

- [ ] **Step 1: Update or add the failing tests** in `tests/flow_canvas.rs`:
  - `command_entity_picked_from_picker`: open a command node, open the entity field, click a light → the node's `entity` is the light id.
  - `signal_picker_lists_only_signals`: a sensor is not offered.
- [ ] **Step 2: Run them and see them fail** (`-- --test flow_canvas`).
- [ ] **Step 3: Swap the four fields.**
- [ ] **Step 4: Run them and see them pass.**
- [ ] **Step 5: Commit**: `feat(web): flow card settings pick entities from the picker`

---

### Task 7: Ambiance device action and device-sheet energy counter

**Files:**
- Modify: `src/ambiances/action_forms.rs:650-760` (device action search), `src/override_editor.rs:372-420` (`CounterField`)
- Test: `tests/ambiance_editor.rs`; the device sheet / override editor wasm test that covers the counter, if any (otherwise add `entity_picker.rs::counter_automatic_and_pick`)

- **Ambiance:** replace the search input and results list with `EntityPickField`.
  - Pool: `is_primary() && !id.starts_with("group_")`, as today. No suggest, no pinned.
  - Value: the device row's `entity`. `on_change(Some(id))` runs today's `pick(id)` logic (switch to the first command when the current one doesn't fit).
  - Delete the `query`/`results` memos and `SEARCH_MAX` if unused.
  - The `ambiance-device__current` title line goes, because the closed field shows it.
- **Counter:** `EntityPickField` with `pool` = membership in the `counters` memo's ids (the closure reads the memo, so it tracks it), pinned `DeviceText::Automatic`, and `class = "override__field"`.

- [ ] **Step 1: Update the failing tests**: the ambiance device action picks a light through the picker and its command becomes the light's first command; the counter field offers only counters, and picking Automatic writes `energy_meter = None`.
- [ ] **Step 2: Run them and see them fail.**
- [ ] **Step 3: Swap the two fields.**
- [ ] **Step 4: Run them and see them pass.**
- [ ] **Step 5: Commit**: `feat(web): ambiance and energy counter use the entity picker`

---

### Task 8: Group wizard members

**Files:**
- Modify: `src/screens/group_wizard.rs` (member step: replace its search input and the `members_by_room` rendering with `PickerBody`)
- Test: `tests/group_wizard.rs`, and the native tests of `members_by_room`

- Use `<PickerBody mode=Mode::Multi inline=true current=selected pool=… on_pick=toggle/>`.
  - Pool: `e.kind == kind && !e.id.starts_with("group_")` (today's rule). The "shown only when already selected" rule is now the model's `current` rule.
  - `toggle` adds or removes the id in the wizard's `selected` set.
- Delete `members_by_room` if nothing else uses it, and move its distinct test cases (kind filter, group exclusion, selected-but-hidden kept) into `model.rs` tests if they aren't already covered. Keep `group_by_room` if another caller uses it (`ambiances/action_forms.rs:399`).

- [ ] **Step 1: Update `tests/group_wizard.rs`** so it ticks members through the picker rows (`[role=option]` inside `.picker-body--inline`), with the same expected PUT body as today.
- [ ] **Step 2: Run it and see it fail** (`-- --test group_wizard`).
- [ ] **Step 3: Swap the member list.**
- [ ] **Step 4: Run everything**: `cargo test -p oikos-web --lib` and `wasm-pack test --headless --chrome crates/oikos-web`. Expected: all pass.
- [ ] **Step 5: Commit**: `feat(web): group wizard members use the picker list`

---

### Task 9: Docs, full gates and on-screen check

**Files:**
- Modify: `docs/users/web-ui.md` (the settings section: entity fields open a searchable picker; describe the room column, the Suggested/All/+ Diagnostic chips, and Esc/↑↓/Enter), `docs/users/climate.md` (the Outdoor temperature, Presence and Holiday fields), `docs/users/alarm.md` (adding sirens, remotes and trackers), `docs/users/flows.md` (the entity field of card settings is a picker, not typed). One or two sentences each; generic wording.

- [ ] **Step 1: Write the doc lines.**
- [ ] **Step 2: Run every CI gate** from Global Constraints. Expected: all green.
- [ ] **Step 3: On-screen check.** Build and serve the web UI against the hub. Use headless Chrome over CDP (the extension times out on the wasm page; the window must be at least 500 px wide). Capture:
  - the climate settings Outdoor temperature picker at 1280 px and at 500 px, in Night Glass and Ivoire
  - a flow card's command entity picker at 1280 px
  - the alarm "Add siren" picker in Multi mode

  Check that rows show device + room + value, that the weather temperature appears under the Suggested chip, and that the 500 px view is full-screen with room headings.
- [ ] **Step 4: Commit the docs**: `docs(web): entity picker`
- [ ] **Step 5: Push the branch and open a PR against `main`** with the screenshots described in the body.
