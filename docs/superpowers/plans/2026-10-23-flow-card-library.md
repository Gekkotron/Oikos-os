# Flow card library panel + Delete flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In flow edit mode, the right panel is always open and shows a card library (style A: rows shaped like canvas cards). Clicking or dragging a row adds a card and the library stays open. Separately, the flow list's ⋯ menu gains "Delete flow…".

**Architecture:** The ＋ Add popover (`screens/flow_canvas/picker.rs`) becomes a `Library` component rendered inside the existing `Drawer`. What the drawer shows is derived from one pure `panel(editing, card_selected, runs_open)` function. Placement and card descriptions are pure functions tested natively. Delete calls the hub's existing `DELETE /api/v1/flows/{id}`, and the list refreshes from the hub's `FlowsChanged` push.

**Tech Stack:** Rust, Leptos (wasm, Trunk), wasm-bindgen-test (`wasm-pack test --headless --chrome`), plain CSS in `crates/oikos-web/styles/main.css`.

**Spec:** approved in chat on 2026-10-23, summarised here (the chosen mockup is `.superpowers/brainstorm/99869-1792746533/content/library-panel-v3.html`, option A):

- Edit mode only: the right panel is always open and shows the library by default. The ＋ Add button and popover are removed.
- Rows: role-coloured round icon, title, one-line description, drag grip, under WHEN / AND / THEN / WAIT / FLOW headers in role colours. Device results from the search box use the same row, with the entity's `subline` (device · room) as the second line. ↑/↓ and Enter work while the search box has focus.
- Click or Enter adds the card, **keeps the library open**, does not select the new card, and flashes it briefly. Repeated adds don't overlap: each lands a step below the previous one, starting at the centre of the canvas area left of the panel.
- Dragging a row onto the canvas drops the card centred on the pointer.
- Selecting a canvas card shows its settings in the panel. Close, Esc or a click on empty canvas returns to the library. Runs keeps its toolbar button and opens in the same panel; closing it returns to the library while editing. Outside edit mode nothing changes.
- Phone and touch screens stay view-only (the existing CSS already hides Edit).
- Delete flow: the ⋯ menu on each flow row/card ends with "Delete flow…". It asks for confirmation inside the same popover ("Delete “<name>”? This can't be undone." + Cancel / Delete), then calls the hub.

## Global Constraints

- Web crate only (`crates/oikos-web`) plus `docs/users/flows.md`. No hub or `oikos-core` change.
- Must pass: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, and `cd crates/oikos-web && wasm-pack test --headless --chrome`.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Match the surrounding code: doc comments on every item, British-neutral plain English in UI copy, no new crates.
- `$DEV_SCRATCH` cargo target is shared; don't add per-repo cargo config.

## Review Focus

1. **Typing in the library search while a card is selected.** ⌫ must edit the text, not delete the card. The existing `typing(&ev)` guard covers inputs; the search box must stay an `<input>`. Test in Task 3.
2. **Adding many cards in a row on a crowded canvas.** No new card may sit exactly on an existing one. `free_spot` test in Task 1.
3. **A zoomed-out or panned canvas.** Click-add lands inside the visible area left of the panel, and drop lands under the pointer at any zoom. `add_spot` test in Task 1; the drop test in Task 4 uses a zoom ≠ 1.
4. **Undo after adding.** Each add is one undo step (it goes through `editor.edit`). Test in Task 3.
5. **Deleting a flow when the hub refuses.** The row stays and the toast shows the hub's error; the menu closes. Test in Task 5 (there's no hub in wasm tests, so the call fails).

---

### Task 1: Placement helpers

**Files:**
- Modify: `crates/oikos-web/src/flow_layout.rs` (add `free_spot` + tests)
- Modify: `crates/oikos-web/src/flow_viewport.rs` (add `add_spot` + tests)

**Interfaces:**
- Produces: `pub fn free_spot(taken: &BTreeMap<String, Pt>, start: Pt) -> Pt` in `flow_layout.rs`, plus `pub const STACK_GAP: f64 = 16.0;`.
- Produces: `pub fn add_spot(view: &View, screen_w: f64, screen_h: f64, panel_px: f64) -> Pt` in `flow_viewport.rs`, plus `pub const PANEL_RESERVE_PX: f64 = 384.0;` (doc: `--drawer-w` 360 px + 24 px of margin, keep in step with `main.css`).

- [ ] **Step 1: Write failing tests**

In `flow_layout.rs` tests:
```rust
#[test]
fn free_spot_keeps_a_free_start() {
    let taken = BTreeMap::from([("a".to_owned(), Pt { x: 500.0, y: 500.0 })]);
    assert_eq!(free_spot(&taken, Pt { x: 0.0, y: 0.0 }), Pt { x: 0.0, y: 0.0 });
}

#[test]
fn free_spot_steps_below_overlapping_cards() {
    let taken = BTreeMap::from([
        ("a".to_owned(), Pt { x: 0.0, y: 0.0 }),
        ("b".to_owned(), Pt { x: 10.0, y: NODE_H + STACK_GAP }),
    ]);
    // Two cards in the way: it lands two steps down.
    assert_eq!(
        free_spot(&taken, Pt { x: 0.0, y: 0.0 }),
        Pt { x: 0.0, y: 2.0 * (NODE_H + STACK_GAP) }
    );
}
```
In `flow_viewport.rs` tests:
```rust
#[test]
fn add_spot_centres_left_of_the_panel() {
    let view = View { x: 0.0, y: 0.0, zoom: 1.0 };
    // 1000×600 screen, 384 px panel: the free area is 616 wide.
    let p = add_spot(&view, 1000.0, 600.0, 384.0);
    assert_eq!(p, Pt { x: (616.0 - NODE_W) / 2.0, y: (600.0 - NODE_H) / 2.0 });
}

#[test]
fn add_spot_follows_pan_and_zoom() {
    let view = View { x: -100.0, y: -50.0, zoom: 2.0 };
    let screen = Pt { x: (616.0 - NODE_W * 2.0) / 2.0, y: (600.0 - NODE_H * 2.0) / 2.0 };
    assert_eq!(add_spot(&view, 1000.0, 600.0, 384.0), view.to_world(screen));
}

#[test]
fn add_spot_ignores_the_panel_when_too_narrow() {
    let view = View { x: 0.0, y: 0.0, zoom: 1.0 };
    assert_eq!(add_spot(&view, 500.0, 400.0, 384.0).x, (500.0 - NODE_W) / 2.0);
}
```
(`View::to_world` is `(screen - pan) / zoom`.)

- [ ] **Step 2: Run, expect FAIL** (`free_spot` / `add_spot` not defined)

Run: `cargo test -p oikos-web free_spot add_spot`

- [ ] **Step 3: Implement**

`free_spot`: a candidate overlaps when any taken point's `NODE_W × NODE_H` rectangle intersects the candidate's. While it overlaps, move the candidate down by `NODE_H + STACK_GAP`. Stop after 200 steps and return the last candidate (bounded loop).
`add_spot`: free width = `screen_w - panel_px` when that is at least `NODE_W * view.zoom`, else `screen_w`. The screen point is the centre of that width × `screen_h` minus half the card's on-screen size, mapped through `view.to_world`.

- [ ] **Step 4: Run, expect PASS** (same command)

- [ ] **Step 5: Commit** — `feat(web): placement helpers for cards added from the library`

---

### Task 2: Descriptions on library entries

**Files:**
- Modify: `crates/oikos-web/src/flow_picker.rs`

**Interfaces:**
- Produces: `Entry` gains `pub sub: Option<String>`, the row's second line. Generic cards get fixed copy; entity cards get `flow_text::subline(&node, ctx)`.

- [ ] **Step 1: Write failing tests**

Extend `empty_query_has_generic_entries` with:
```rust
let subs: Vec<_> = got.iter().map(|e| e.sub.as_deref()).collect();
assert_eq!(
    subs,
    [
        Some("At a set time, or on a repeat"),
        Some("Sunrise, sunset, dusk or dawn"),
        Some("Between dusk and dawn"),
        Some("A push to your phone"),
        Some("A fixed or random delay"),
        Some("Stops it from running again"),
        Some("Switches another flow on or off"),
    ]
);
```
Add `entity_entries_carry_the_subline`: it searches `"table"` over `ent("l1", "Lumière table", Kind::Light)` with an `AreaView` `salon` named `Salon` in `areas`, and asserts that every non-generic entry's `sub` equals `subline` of a node built from its kind (and is `Some`).

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p oikos-web flow_picker`

- [ ] **Step 3: Implement**

Add the description as a fourth tuple field of the `generic` array, set in the `entry` helper (generic → `Some(desc.to_owned())`, entity entries → `subline(&node, ctx)`). Update the doc comment of `MAX_ENTITIES` ("so the list stays short").

- [ ] **Step 4: Run, expect PASS**

- [ ] **Step 5: Commit** — `feat(web): one-line descriptions on flow library entries`

---

### Task 3: Library panel replaces the ＋ Add popover

**Files:**
- Create: `crates/oikos-web/src/screens/flow_canvas/library.rs` (replaces `picker.rs`)
- Delete: `crates/oikos-web/src/screens/flow_canvas/picker.rs`
- Modify: `crates/oikos-web/src/screens/flow_canvas/drawer.rs` (add `Panel`, `panel`; new `Drawer` props)
- Modify: `crates/oikos-web/src/screens/flow_canvas/mod.rs` (signals and effects, render `Library` + `NodeEditor` in the drawer)
- Modify: `crates/oikos-web/src/screens/flow_canvas/toolbar.rs` (remove ＋ Add and the `add` callback; Runs button)
- Modify: `crates/oikos-web/styles/main.css` (replace `.flow-picker*` rules with `.flow-library*`)
- Test: `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `free_spot`, `add_spot`, `PANEL_RESERVE_PX` (Task 1); `Entry.sub` (Task 2); `flow_text::role`, `icons::node_icon`, `flow_layout::positions`.
- Produces in `drawer.rs`:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Panel { Closed, Library, Settings, Runs }
  /// Settings beats Runs beats Library; outside edit mode only Runs opens it.
  pub fn panel(editing: bool, card_selected: bool, runs_open: bool) -> Panel
  ```
  `Drawer(panel: Signal<Panel>, runs_open: RwSignal<bool>, runs, flow, replay, children)`: open when `panel != Closed`; the runs header and list show only for `Panel::Runs`; `aria-label` is "Card library" / "Card settings" / "Recent runs".
- Produces in `library.rs`: `#[component] pub fn Library(on_add: Callback<NodeKind>) -> impl IntoView`, plus `pub struct Fresh(pub RwSignal<Option<String>>)` (the id of the card to flash, provided by `FlowBody`; Task 4's drop uses it too).
- `FlowBody`: rename the `drawer` signal to `runs_open`; `let panel = Memo::new(…panel(editing, matches!(selection, Some(Selection::Node(_))), runs_open))`. Remove the "selecting opens the drawer" effect. The close effect becomes "when `runs_open` turns false, `replay.set(None)`". `.flow-stage--drawer` follows `panel != Closed`. The add callback lives in `FlowBody`: `at = free_spot(&positions(&draft), add_spot(view, w, h, PANEL_RESERVE_PX))`, then `editor.edit(|f| add_node(f, kind, at))`, then `Fresh` is set to the new id and cleared after 1200 ms (`gloo_timers::callback::Timeout`). It does **not** touch `selection`.
- Toolbar Runs button: toggles `runs_open`; when it opens Runs while a card is selected in edit mode, it also clears `selection`.

- [ ] **Step 1: Write failing tests**

Native, in `drawer.rs`:
```rust
#[test]
fn panel_priorities() {
    assert_eq!(panel(false, false, false), Panel::Closed);
    assert_eq!(panel(false, true, false), Panel::Closed);
    assert_eq!(panel(false, false, true), Panel::Runs);
    assert_eq!(panel(true, false, false), Panel::Library);
    assert_eq!(panel(true, false, true), Panel::Runs);
    assert_eq!(panel(true, true, true), Panel::Settings);
}
```
Browser, in `tests/flow_canvas.rs`. Replace the `search` helper (no button to click; type into `.flow-library__search`). Replace `add_picker_adds_selected_card` and `escape_closes_picker`, and rename the selectors in `picker_groups_entity_results` (`.flow-library__header`, same header text):
- `library_open_in_edit_mode`: after `editing(..)`, `.flow-drawer--open` = 1, `.flow-library` = 1, `.flow-library__item` = 7, no `.flow-picker-wrap`. Before editing (plain `mount`), `.flow-drawer--open` = 0.
- `library_click_adds_and_stays_open`: click the "Wait" row, then click it again. `[data-node=delay_1]` and `[data-node=delay_2]` exist, `.flow-node--selected` = 0, `.flow-library` = 1, and the two cards' `style` tops differ. Press ⌘Z once and `delay_2` is gone (one undo step per add).
- `library_enter_adds_highlighted`: type "Wait" into the search, press Enter, and `delay_1` exists.
- `selecting_a_card_swaps_to_settings_and_back`: `select_card("moving")` shows `.flow-drawer .node-editor` = 1 and `.flow-library` = 0. Clicking `.node-editor__header .btn` (Close) gives `.flow-library` = 1.
- `backspace_in_library_search_keeps_card`: select `moving`, focus `.flow-library__search`, press Backspace on it, and `[data-node=moving]` still exists.
- `runs_from_edit_mode_returns_to_library`: click `.flow-toolbar__runs` and `.flow-drawer .run-list, .flow-drawer__empty` ≥ 1. Click `.flow-drawer__close` and `.flow-library` = 1.
- Update `click_card_opens_settings` so that after the empty-canvas click it asserts `.flow-library` = 1 instead of just "no fields".

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p oikos-web panel_priorities` and `cd crates/oikos-web && wasm-pack test --headless --chrome --test flow_canvas`

- [ ] **Step 3: Implement**

`Library` keeps the picker's query/highlight/results logic (move it, minus `open`, focus-on-open and the window Esc/outside listeners). Row markup:
```html
<li class="flow-library__item flow-library__item--{trigger|check|action|wait}" role="option" draggable="true">
  <span class="flow-library__icon" inner_html={node_icon}></span>
  <span class="flow-library__text"><span class="flow-library__title">{label}</span><span class="flow-library__sub">{sub}</span></span>
  <span class="flow-library__grip" aria-hidden="true">⋮⋮</span>
</li>
```
The header `<li class="flow-library__header flow-library__header--{role}">` keeps `header(group)` text (Wait and Flow headers take the wait / action role colours). Esc in the search box clears the query. Panel header: `<h2 class="flow-drawer__title">"Add a card"</h2>`.
CSS: reuse the mockup's look on the app tokens. Row: `border-radius: 14px`, background `color-mix(in srgb, var(--node-accent) 10%, var(--raised))`, `inset 0 0 0 1px var(--card-border)`, 28 px icon circle on `var(--node-accent)`, title `600 13px/17px`, sub `400 11px/14px` at 0.6 opacity, and `cursor: grab`. The highlight ring (`inset 0 0 0 1.5px var(--node-accent)`) shows only under `.flow-library:focus-within`. Headers are coloured `var(--node-accent)` with a 6 px dot. Add `.flow-node--fresh` (a 1.2 s accent glow keyframe, none under `prefers-reduced-motion`). `graph.rs` `card()` adds `flow-node--fresh` when `use_context::<Fresh>()` holds its id.

- [ ] **Step 4: Run, expect PASS** (both commands, plus the full `--test flow_canvas`)

- [ ] **Step 5: Commit** — `feat(web): card library panel in flow edit mode`

---

### Task 4: Drag a library row onto the canvas

**Files:**
- Modify: `crates/oikos-web/Cargo.toml` (web-sys features `DragEvent`, `DataTransfer` in deps; `DragEvent`, `DragEventInit` in dev-deps)
- Modify: `crates/oikos-web/src/screens/flow_canvas/library.rs` (`dragstart` / `dragend`)
- Modify: `crates/oikos-web/src/screens/flow_canvas/mod.rs` (`dragover` / `drop` on `.flow-stage`)
- Test: `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `Fresh` (Task 3), `View::to_world`, `NODE_W`, `NODE_H`.
- Produces: `pub struct Dragging(pub StoredValue<Option<NodeKind>>)` in `library.rs`, provided by `FlowBody`.

- [ ] **Step 1: Write failing test**

`drop_adds_under_the_pointer`: enter edit mode, set `camera` zoom to 2 by clicking the zoom-in control once (or the existing zoom helper), dispatch `dragstart` on the "Wait" row, then `dragover` and `drop` `DragEvent`s on `.flow-canvas` at client (300, 300). Assert that `[data-node=delay_1]` exists, its `style` places its centre at the world point under (300, 300) (compute from the `.flow-graph` layer's transform as the existing `drag_is_one_undo_step` test does), and that `.flow-library` is still 1.

- [ ] **Step 2: Run, expect FAIL**

- [ ] **Step 3: Implement**

`dragstart`: store the entry's kind in `Dragging`, call `data_transfer.set_data("text/plain", &label)` (Firefox needs data to start a drag), and set `effect_allowed = "copy"`. `dragend` clears `Dragging`. On `.flow-stage`: `dragover` calls `prevent_default()` when `Dragging` is set and the target is not inside `.flow-drawer`. `drop` takes the kind, converts the client point to a point relative to the `.flow-canvas` rect, maps it with `camera.view.to_world`, subtracts `NODE_W / 2`, `NODE_H / 2`, adds through `editor.edit(add_node)`, and sets `Fresh`. No `free_spot` here: the user chose the spot.

- [ ] **Step 4: Run, expect PASS**

- [ ] **Step 5: Commit** — `feat(web): drag cards from the library onto the flow canvas`

---

### Task 5: Delete a flow from the list

**Files:**
- Modify: `crates/oikos-web/src/api/core.rs` (add `delete_flow`)
- Modify: `crates/oikos-web/src/screens/flows/room_menu.rs` (prop `name`, confirm state, delete item)
- Modify: `crates/oikos-web/src/screens/flows/mod.rs` (`menu_for` passes `name=row.view.definition.name.clone()`)
- Modify: `crates/oikos-web/styles/main.css` (`.room-menu__item--danger`, `.room-menu__confirm`)
- Test: `crates/oikos-web/tests/flows_list.rs`

**Interfaces:**
- Produces: `pub async fn delete_flow(id: &str) -> Result<(), ApiError>`. Its shape is `delete_group`'s, on `/api/v1/flows/{id}`, and the hub answers `204`.
- `RoomMenu(id, name: String, href, current, invalid)`.

- [ ] **Step 1: Write failing tests**

In `flows_list.rs`:
- `menu_offers_delete_with_confirmation`: mount two flows and click the first row's `.room-menu__button`. `.room-menu__item--danger` has text "Delete flow…". Clicking it shows `[role=alertdialog]` with text containing `Delete “<name>”?` and `This can't be undone.`. Clicking Cancel brings back the menu items (`Move to room` header present), and `.room-menu__confirm` = 0.
- `failed_delete_keeps_the_row`: in confirm, click Delete and await ~100 ms. The row count is unchanged and the menu is closed (`.room-menu__pop` = 0). The call fails because there's no hub in the test.
- An invalid flow can still be deleted: its Delete item is not `disabled`.

- [ ] **Step 2: Run, expect FAIL** — `cd crates/oikos-web && wasm-pack test --headless --chrome --test flows_list`

- [ ] **Step 3: Implement**

`confirming: RwSignal<bool>`, reset to false whenever `open` turns false. The popover shows the confirm section `<div class="room-menu__confirm" role="alertdialog" aria-label="Delete flow">` with the prompt, `Cancel` (`btn btn--ghost`) and `Delete` (`btn btn--danger`). Delete closes the menu, then `spawn_local(api::delete_flow)`; on `Err(e)` it calls `toasts.push(e.error)`. Success needs nothing more, because the hub's `FlowsChanged` push removes the row. Update the module and component doc comments ("open the flow, move it to a room, or delete it").

- [ ] **Step 4: Run, expect PASS**

- [ ] **Step 5: Commit** — `feat(web): delete a flow from its ⋯ menu`

---

### Task 6: Docs, full checks, real-browser check

**Files:**
- Modify: `docs/users/flows.md:78` (replace the ＋ Add sentence; add one line on Delete flow… in the list section)

- [ ] **Step 1:** Rewrite the editor paragraph: "In edit mode the right panel holds the card library, grouped WHEN / AND / THEN / WAIT / FLOW. Click a card or drag it onto the canvas to add it; the library stays open so you can add several. Type an entity name to list its triggers, checks and actions. Selecting a card on the canvas shows its settings there instead." Then add the delete line to the flow list part of the page.
- [ ] **Step 2:** Run every check in Global Constraints. Expected: all green.
- [ ] **Step 3:** `cd crates/oikos-web && trunk serve`, open a flow, press Edit, and screenshot it over headless Chrome CDP (window ≥ 1200 px wide). Confirm that the library shows, two clicks add two non-overlapping cards, a drag drops under the pointer, and deleting a test flow from the ⋯ menu works against the local hub.
- [ ] **Step 4: Commit** — `docs(flows): card library panel and Delete flow`
