# Flow Canvas Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the web flow page as a full-bleed, React Flow-style canvas with compact sentence pills, routed wires, live and replayed runs, click-to-select editing, a card picker and a "New flow" button.

**Architecture:** All geometry, text, editing and picker logic lives in new pure modules (`flow_text`, `flow_layout`, `flow_viewport`, `flow_edit`, `flow_picker`) tested natively with `cargo test`. `screens/flow_canvas.rs` becomes a module directory whose Leptos components only wire signals to those functions. The hub, the flow model and the API are unchanged.

**Tech Stack:** Rust, Leptos (CSR, wasm32), leptos_router, web-sys pointer/wheel/keyboard events, inline SVG for wires, `wasm-pack test --headless --chrome` for DOM tests.

**Spec:** `docs/superpowers/specs/2026-10-20-flow-canvas-redesign-design.md`

## Global Constraints

- No React or JS libraries: hand-written Leptos and SVG only (spec, anti-goals).
- Nothing is sent to the hub until **Save** (`api::put_flow`). Cancel discards the draft.
- `Node::extra`, `Flow::extra` and unknown params survive an edit and save round-trip.
- Automatic layout applies only when not every node has a `position`, or when the user presses **Tidy**.
- Colours by role: trigger yellow, check (`if`) purple, action (`command`, `flow.enable`) green, wait (`delay`) grey-blue. A `true` wire is green with a "yes" label, a `false` wire red with "no", other wires neutral grey. Use existing CSS tokens in `styles/main.css`.
- Phone = `(max-width: 719px), (hover: none) and (pointer: coarse)`. On a phone, the Edit and New flow buttons are hidden, and pan, pinch-zoom, drawer, switch and Run still work.
- New flows are saved with `enabled: false`. Ids match `[a-z0-9_-]{1,32}`, are unique, and are never `new`.
- Commits use `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com` and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Every task ends green on `cargo test -p oikos-web`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` and `cargo check -p oikos-web --target wasm32-unknown-unknown`. UI tasks also run `cd crates/oikos-web && wasm-pack test --headless --chrome`.

## Review Focus

1. **Typing in a settings field while a card is selected:** ⌫ must delete a character, not the card. The keyboard handler ignores events whose target is an `input`, `textarea` or `select` (Task 8 test `backspace_in_input_keeps_card`).
2. **A flow named "New" or "Nouveau flow" with accents:** the slug must never be `new`, must fold `é→e`, and must not collide with an existing id (Task 5 tests `slug_avoids_reserved_and_taken`, `slug_folds_accents`).
3. **Undo after a drag:** one drag is one undo step, not one per pointermove (Task 5 `history_records_once_per_gesture` and Task 8 browser test `drag_is_one_undo_step`).
4. **Live update arriving mid-edit** (another run or an MCP edit): the draft and selection are untouched, and the drawer runs still refresh (Task 8 `ws_update_keeps_draft`).
5. **Flows with one node, or with all positions saved far off-screen:** fit view must still frame them, with zoom clamped to `[FIT_MIN, FIT_MAX]` (Task 4 `fit_single_node_caps_zoom`, `fit_far_positions`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/oikos-web/src/flow_text.rs` (new) | Sentence title, sub-line and colour role of a node. |
| `crates/oikos-web/src/flow_layout.rs` (new) | Pill geometry, layered auto layout, pin anchors, wire routing around cards, `tidy`. |
| `crates/oikos-web/src/flow_viewport.rs` (new) | Pan and zoom state, screen↔world mapping, fit, zoom-at-point, minimap transform. |
| `crates/oikos-web/src/flow_edit.rs` (new) | Draft operations (connect rules, delete, duplicate, add), node ids, flow-id slugs, undo history. |
| `crates/oikos-web/src/flow_picker.rs` (new) | Picker entries for a query against the house. |
| `crates/oikos-web/src/flows.rs` | Keeps highlight, parked, run helpers. Loses `layout`, `canvas_size`, `wire_path`, `port_offset`, `NODE_*`, `COL_GAP`, `ROW_GAP`, `with_resolved_positions`, `node_title`, `node_detail` (moved or replaced). |
| `crates/oikos-web/src/screens/flow_canvas/mod.rs` | `FlowCanvasScreen`, `NewFlowScreen`, `FlowBody` state wiring. |
| `crates/oikos-web/src/screens/flow_canvas/graph.rs` | `Graph` component: pills, wires, labels, pins, highlight classes. Used in both view and edit mode. |
| `crates/oikos-web/src/screens/flow_canvas/viewport.rs` | `Viewport` surface (pan, wheel, pinch), `CanvasControls`, `Minimap`. |
| `crates/oikos-web/src/screens/flow_canvas/toolbar.rs` | Top-left title pill (editable name in edit mode), top-right view and edit groups. |
| `crates/oikos-web/src/screens/flow_canvas/drawer.rs` | Right drawer: runs list (`RunItem`) or the selected card's settings. |
| `crates/oikos-web/src/screens/flow_canvas/settings.rs` | Today's `NodeEditor` body, `kind_fields`, `JsonField`, `TextField`, `flow_enable_fields`, moved verbatim plus a "Delete card" button. |
| `crates/oikos-web/src/screens/flow_canvas/picker.rs` | "＋ Add" picker popover. |
| `crates/oikos-web/tests/fixtures/living_led_motion.json` (new) | The real 23-node flow, for layout and routing tests. |
| `crates/oikos-web/tests/flow_canvas.rs`, `tests/flows_list.rs` | Browser tests, updated and extended. |
| `crates/oikos-web/styles/main.css` | Flow page styles rewritten under the existing `.flow-*` prefix. |

Class names kept for continuity: `.flow-node`, `.flow-node--{lit,failed,pending,orphan}`, `[data-node=…]`, `.flow-node__in`, `.flow-node__out`, `.flow-wire`, `.flow-wire--lit`, `.run-list`, `.flow-invalid`. New: `.flow-node--selected`, `.flow-node--target`, `.flow-node--dim`, `.flow-wire--yes`, `.flow-wire--no`, `.flow-wire--selected`, `.flow-wire--live`, `.flow-wire-hit`, `.flow-wire-label`, `.flow-wire-cut`, `.flow-drawer`, `.flow-picker`, `.flow-minimap`, `.flow-controls`, `.flow-toolbar`, `.flow-toolbar__edit`, `.flows-new`.

---

### Task 1: Commit the in-progress flow-list restyle

**Files:**
- Modify (already modified in the working tree): `crates/oikos-web/src/flows.rs`, `src/icons.rs`, `src/screens/flows.rs`, `src/screens/flow_canvas.rs`, `styles/main.css`
- Test: `crates/oikos-web/tests/flows_list.rs`, `tests/flow_canvas.rs`

**Interfaces:**
- Produces: `flows::trigger_summary(&Flow, &BTreeMap<String, EntityView>) -> String`, `icons::{CHECK, X_MARK}`, the `.flow-card*` list styles.

- [ ] **Step 1: Run the full check list from Global Constraints on the untouched working tree.** Expected: everything passes. If `tests/flows_list.rs` or `tests/flow_canvas.rs` fail only because the list became cards or the zoom bar became the loupe, update their selectors to the new markup and nothing else.
- [ ] **Step 2: Commit** all five files plus any test fixes as `feat(web): flow list as cards with trigger, mode and last run`. The canvas part of this diff is replaced in Tasks 7–8.

---

### Task 2: Sentence text for nodes (`flow_text.rs`)

**Files:**
- Create: `crates/oikos-web/src/flow_text.rs` (register in `lib.rs`)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `format::entity_title`, `schedule::describe`, `flows::value_text`, `oikos_core::flow::{Node, NodeKind, Op}`.
- Produces:
  ```rust
  pub enum Role { Trigger, Check, Action, Wait }
  pub fn role(kind: &NodeKind) -> Role;                       // TriggerState|TriggerSchedule→Trigger, If→Check, Command|FlowEnable→Action, Delay→Wait
  pub struct Ctx<'a> { pub entities: &'a BTreeMap<String, EntityView>, pub areas: &'a [AreaView], pub flows: &'a [(String, String)] /* (id, name) */ }
  pub fn sentence(node: &Node, ctx: &Ctx) -> String;          // generated, ignores node.name
  pub fn title(node: &Node, ctx: &Ctx) -> String;             // node.name if non-blank, else sentence
  pub fn subline(node: &Node, ctx: &Ctx) -> Option<String>;
  pub fn duration_words(spec: &str) -> String;                // "5m"→"5 min", "1h30m"→"1 h 30 min", "45s"→"45 s"; unparsable → spec unchanged
  ```

Rules (`E` = `entity_title` of the entity, or the raw id when unknown; `V` = value, with booleans written `on`/`off` and numbers followed by the entity's `unit` when it has one):
- `trigger.state`: no `to` → `When E changes`; `to` boolean → `When E turns on|off`; else `When E becomes V`.
- `trigger.schedule`: `describe(cron)`.
- `if` with an entity: `Eq` → `E is V?`, `Ne` → `E is not V?`, `Gt|Lt|Gte|Lte` → `E > V?` (using `>`, `<`, `≥`, `≤`). Without an entity, `E` is `Trigger value`.
- `delay`: `Wait {duration_words(for)}`.
- `command`: `TurnOn{brightness:None}` → `Turn on E`. `TurnOn{brightness:Some(b)}` → `Turn on E at {b*100/255 rounded} %`. `TurnOff` → `Turn off E`, `Toggle` → `Toggle E`, `Open|Close|Stop` → `Open|Close|Stop E`, `SetPosition{p}` → `Set E to p %`, `SetTemperature{t}` → `Set E to t °C`, `SetNumber{v}` → `Set E to V`, `SelectOption{o}` → `Set E to o`, `SetHvacMode{m}` → `Set E mode to m`, `Press` → `Press E`. Any other command → `{humanised command_name} E`.
- `flow.enable`: `Enable|Disable {flow name from ctx.flows, else the id}`, or `… this flow` when `flow` is `None`.
- `subline`: for nodes with an entity, `"{area name} · {device_name}"`, with either part dropped when missing and `None` when both are. For other nodes, `Some(sentence)` when the node has a non-blank name, else `None`.

- [ ] **Step 1: Write failing tests:**
  - `if_eq_bool_reads_on`: `if {entity: L1 light named "Lumière TV", op: eq, value: true}` → title `"Lumière TV is on?"`.
  - `if_gt_uses_unit`: `if {illuminance sensor "Entrée" unit "lx", op: gt, value: 16}` → `"Entrée > 16 lx?"`.
  - `command_turn_off`: `"Turn off LED"`. `command_set_number`: `{set_number 100}` on a number with unit `%` → `"Set Power 1 to 100 %"`.
  - `trigger_to_true` → `"When Motion turns on"`. `delay_words`: `"5m"` → `"Wait 5 min"`, `"1h30m"` → `"Wait 1 h 30 min"`.
  - `flow_enable_names`: `{enabled:false, flow: Some("force-heating-loop")}` with ctx flows `[("force-heating-loop","Force heating loop")]` → `"Disable Force heating loop"`. With `None` → `"Disable this flow"`.
  - `name_wins_and_subline`: a node named "LED is on" on an entity in area "Salon" with device "LED strip" → title `"LED is on"`, subline `Some("Salon · LED strip")`.
  - `unnamed_delay_has_no_subline` → `None`.
- [ ] **Step 2: Run** `cargo test -p oikos-web flow_text`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement** the functions above.
- [ ] **Step 4: Run** `cargo test -p oikos-web flow_text`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): sentence titles for flow nodes`.

---

### Task 3: Layout and wire routing (`flow_layout.rs`)

**Files:**
- Create: `crates/oikos-web/src/flow_layout.rs`, `crates/oikos-web/tests/fixtures/living_led_motion.json`
- Modify: `crates/oikos-web/src/flows.rs` (remove the moved items; repoint callers in the old canvas to `flow_layout` so the crate still builds)
- Test: `flow_layout.rs` `mod tests`

**Interfaces:**
- Produces:
  ```rust
  pub const NODE_W: f64 = 232.0; pub const NODE_H: f64 = 52.0;
  pub const COL_GAP: f64 = 96.0; pub const ROW_GAP: f64 = 28.0; pub const PAD: f64 = 40.0;
  #[derive(Clone, Copy, Debug, PartialEq)] pub struct Pt { pub x: f64, pub y: f64 }
  #[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
  pub fn positions(flow: &Flow) -> BTreeMap<String, Pt>;    // stored when every node has one, else auto_layout
  pub fn auto_layout(flow: &Flow) -> BTreeMap<String, Pt>;
  pub fn tidy(flow: &Flow) -> Flow;                         // every node.position = auto_layout
  pub fn with_resolved_positions(flow: &Flow) -> Flow;      // fills only missing positions
  pub fn bounds(pos: &BTreeMap<String, Pt>) -> Rect;        // all cards, no padding
  pub fn out_anchor(kind: &NodeKind, at: Pt, port: Option<&str>) -> Pt; // right edge; if: true at 30 %, false at 70 % of NODE_H; else 50 %
  pub fn in_anchors(flow: &Flow, pos: &BTreeMap<String, Pt>) -> BTreeMap<(String, Option<String>, String), Pt>;
  pub struct Route { pub d: String, pub label_at: Pt, pub mid: Pt }
  pub fn route(from: Pt, to: Pt, obstacles: &[Rect]) -> Route;
  pub fn routes(flow: &Flow, pos: &BTreeMap<String, Pt>) -> Vec<((String, Option<String>, String), Route)>;
  ```
- Consumes: `Flow::layers()` from `oikos-core` for columns.

Algorithm decisions:
- **auto_layout:** columns from `Flow::layers()`. An edge spanning more than one column gets a dummy entry in each column between its ends. Order each column by 4 down-and-up barycenter sweeps (mean row index of neighbours in the previous or next column; ties keep the current order). Then assign `y` per column with rows `NODE_H + ROW_GAP` apart, a dummy taking `NODE_H/2 + ROW_GAP`. Centre each column's block on the mean `y` of its predecessors, clamped so cards never overlap. `x = PAD + col*(NODE_W+COL_GAP)`, `y ≥ PAD`.
- **in_anchors:** the wires entering one node are sorted by their source anchor's `y` and spread evenly over the middle 60 % of the input edge (a single wire enters at 50 %).
- **route:** start with `M from C (from.x+k,from.y) (to.x-k,to.y) to`, where `k = max(40, |to.x-from.x|/2)`. Sample 24 points. If any sample falls inside an obstacle rect inflated by 6 px, use a lane: choose `y_lane` from the candidates {top − ROW_GAP/2, bottom + ROW_GAP/2} of every obstacle whose x-range overlaps the wire, minimising `|y_lane−from.y| + |y_lane−to.y|`, and draw cubic → horizontal line → cubic. `label_at` is 28 px along from `from`; `mid` is the path's middle point, where the ✕ goes.
- **routes:** obstacles are every card except the edge's two ends.

- [ ] **Step 1: Save the fixture:** `curl -s http://localhost:8081/api/v1/flows/living_led_motion | python3 -c "import json,sys; print(json.dumps(json.load(sys.stdin)['definition'], indent=2, ensure_ascii=False))" > crates/oikos-web/tests/fixtures/living_led_motion.json`. Remove every `position` key so the auto layout is exercised. Load it in tests with `include_str!("../tests/fixtures/living_led_motion.json")`.
- [ ] **Step 2: Write failing tests:**
  - `led_layout_has_no_overlaps`: no two card rects of `auto_layout(fixture)` intersect.
  - `led_routes_avoid_cards`: for every route of the fixture, none of 48 points sampled along `d` (parse the path or keep the samples in a test-only helper `route_samples`) lies inside a non-endpoint card rect.
  - `led_triggers_spread`: the 5 anchors into `tv_on` have 5 distinct `y` values, all inside the card's input edge.
  - `if_ports`: `out_anchor(If, (0,0), Some("true")).y == NODE_H*0.3` and `false` gives `NODE_H*0.7`.
  - `stored_positions_win` / `partial_positions_autolayout`: all stored → returned unchanged; one missing → auto layout.
  - `tidy_overwrites_positions`: `tidy` sets every node's position to `auto_layout`'s.
  - `crossings_reduced`: on a 2×2 crossing fixture (`a→d`, `b→c`, rows a,b / c,d), the layout puts `d` above `c`.
- [ ] **Step 3: Run** `cargo test -p oikos-web flow_layout`. Expected: FAIL.
- [ ] **Step 4: Implement** the module and remove the moved items from `flows.rs`, repointing the existing canvas to them (adjust the old `NODE_H*0.33/0.67` port maths to `out_anchor`).
- [ ] **Step 5: Run** `cargo test -p oikos-web` and the wasm check. Expected: PASS.
- [ ] **Step 6: Commit** `feat(web): layered flow layout with routed wires`.

---

### Task 4: Viewport maths (`flow_viewport.rs`)

**Files:**
- Create: `crates/oikos-web/src/flow_viewport.rs`
- Modify: `crates/oikos-web/src/flows.rs` (remove `ZOOM_*`, `clamp_zoom` once nothing uses them, or re-export from here)

**Interfaces:**
- Consumes: `flow_layout::{Pt, Rect, PAD}`.
- Produces:
  ```rust
  pub const ZOOM_MIN: f64 = 0.25; pub const ZOOM_MAX: f64 = 2.0; pub const FIT_MIN: f64 = 0.4; pub const FIT_MAX: f64 = 1.0;
  pub const ZOOM_STEP: f64 = 1.2; // multiplicative, for the +/− buttons
  #[derive(Clone, Copy, Debug, PartialEq)] pub struct View { pub x: f64, pub y: f64, pub zoom: f64 } // screen = world*zoom + (x,y)
  impl View {
      pub fn to_world(&self, screen: Pt) -> Pt; pub fn to_screen(&self, world: Pt) -> Pt;
      pub fn zoom_at(&self, screen: Pt, factor: f64) -> View; // keeps the world point under `screen` fixed, zoom clamped to [ZOOM_MIN, ZOOM_MAX]
      pub fn pan(&self, dx: f64, dy: f64) -> View;
      pub fn css_transform(&self) -> String;                  // "translate(Xpx,Ypx) scale(Z)"
  }
  pub fn fit(content: Rect, screen_w: f64, screen_h: f64) -> View; // content inflated by PAD, zoom in [FIT_MIN, FIT_MAX], centred
  pub fn visible_world(view: &View, screen_w: f64, screen_h: f64) -> Rect;
  pub struct Mini { pub scale: f64, pub ox: f64, pub oy: f64 }  // world→minimap: m = (w - o) * scale
  pub fn minimap(content: Rect, view_rect: Rect, w: f64, h: f64) -> Mini; // covers the union of content and view_rect
  pub fn mini_to_world(m: &Mini, p: Pt) -> Pt;
  pub fn centre_on(view: &View, world: Pt, screen_w: f64, screen_h: f64) -> View;
  ```

- [ ] **Step 1: Write failing tests:** `zoom_at_keeps_point_fixed`, `zoom_clamps`, `fit_centres_content`, `fit_single_node_caps_zoom` (one card in a 1400×800 screen → zoom == `FIT_MAX`), `fit_far_positions` (cards at x=5000 → `to_screen` puts them inside the screen), `minimap_roundtrip` (`mini_to_world` of the mapped centre returns the centre ±0.01), `centre_on_moves_point_to_middle`.
- [ ] **Step 2: Run** `cargo test -p oikos-web flow_viewport`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): pan, zoom, fit and minimap maths`.

---

### Task 5: Draft operations and history (`flow_edit.rs`)

**Files:**
- Create: `crates/oikos-web/src/flow_edit.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, PartialEq)] pub enum ConnectError { IntoTrigger, SelfLoop, Duplicate, Cycle, UnknownNode, BadPort }
  pub fn can_connect(flow: &Flow, from: &str, port: Option<&str>, to: &str) -> Result<(), ConnectError>;
  pub fn connect(flow: &mut Flow, from: &str, port: Option<&str>, to: &str) -> Result<(), ConnectError>;
  pub type EdgeKey = (String, Option<String>, String);
  pub fn remove_edge(flow: &mut Flow, key: &EdgeKey) -> bool;
  pub fn remove_node(flow: &mut Flow, id: &str) -> bool;          // drops its edges too
  pub fn duplicate_node(flow: &mut Flow, id: &str) -> Option<String>; // new id, same kind/params/name/extra, position +24,+24, no edges
  pub fn add_node(flow: &mut Flow, kind: NodeKind, at: Pt) -> String; // returns the new id
  pub fn next_node_id(flow: &Flow, kind: &NodeKind) -> String;    // "<prefix>_<n>", lowest free n ≥ 1; prefix: trigger.state→"trigger", trigger.schedule→"schedule", if→"if", delay→"delay", command→"command", flow.enable→"flow_switch"
  pub fn slug_id(name: &str, taken: &BTreeSet<String>) -> String;
  pub struct History { past: Vec<Flow>, future: Vec<Flow> }      // cap 100 each
  impl History { pub fn new() -> Self; pub fn record(&mut self, before: Flow); pub fn undo(&mut self, current: Flow) -> Option<Flow>; pub fn redo(&mut self, current: Flow) -> Option<Flow>; pub fn clear(&mut self); pub fn can_undo(&self) -> bool; }
  ```
- Rules: `BadPort` when `from` is an `if` and `port` isn't `Some("true"|"false")`, or `from` is not an `if` and `port` is `Some`. `Cycle` when `to` already reaches `from`. Fan-in and fan-out are allowed.
- `slug_id`: lowercase; fold `àâä→a, éèêë→e, îï→i, ôö→o, ùûü→u, ç→c, œ→oe, æ→ae`; every other char outside `[a-z0-9]` becomes `_`; collapse runs of `_`; trim `_`; truncate to 29; empty → `flow`; if the result is `new` or in `taken`, append `_2`, `_3`, … (lowest free).

- [ ] **Step 1: Write failing tests:** `rejects_into_trigger`, `rejects_self_loop`, `rejects_duplicate`, `rejects_cycle` (a→b→c, then c→a), `if_needs_port`, `allows_fan_in_and_out`, `remove_node_drops_edges`, `duplicate_offsets_and_unwires`, `next_id_fills_gap` (`if_1`, `if_3` exist → `if_2`), `slug_folds_accents` (`"Lumière été"` → `"lumiere_ete"`), `slug_avoids_reserved_and_taken` (`"New"` → `"new_2"`; `"Force heating loop"` with `force_heating_loop` taken → `force_heating_loop_2`), `slug_max_len` (≤ 32 with suffix), `history_undo_redo` (record A, current B: undo → A; redo → B; a new record clears the future), `history_records_once_per_gesture` (document by test: callers record the pre-gesture snapshot once, so three moves inside one gesture undo to the original in one step).
- [ ] **Step 2: Run** `cargo test -p oikos-web flow_edit`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): flow draft edits, ids and undo history`.

---

### Task 6: Picker entries (`flow_picker.rs`)

**Files:**
- Create: `crates/oikos-web/src/flow_picker.rs`

**Interfaces:**
- Consumes: `flow_text::{sentence, Ctx, Role}`, `oikos_core::flow::Scalar::of_state`, `EntityView::is_primary`.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Group { When, And, Then, Wait, Flow }
  #[derive(Clone, Debug, PartialEq)] pub struct Entry { pub group: Group, pub label: String, pub kind: NodeKind }
  pub fn entries(query: &str, ctx: &Ctx) -> Vec<Entry>; // ordered by Group, then entity title
  ```
- Always present (filtered by `query` against `label` when the query isn't empty): When "Schedule" (`cron "0 * * * *"`), Wait "Wait" (`for "5m"`), Flow "Disable this flow" (`flow.enable {enabled:false, flow:None}`), Flow "Enable a flow" (`{enabled:true, flow:None}`).
- Per matching entity (query non-empty, case- and accent-insensitive substring of `entity_title`, primary entities only, at most 8 entities): When `trigger.state{entity}`; And `if{entity, op: eq, value: current Scalar as JSON, else true}`; Then by `Kind`: Light|Switch → turn_on, turn_off, toggle; Cover → open, close, stop; Number → `set_number{value: current or 0}`; Button → press; Climate → `set_temperature{20.0}`. Other kinds get no Then. `label` = `sentence` of the would-be node.
- Empty query → only the always-present entries.

- [ ] **Step 1: Write failing tests:** `empty_query_has_generic_entries` (4 entries, no entity ones), `table_query_lists_light_entries` (a light "Lumière table" → 1 When, 1 And, 3 Then, labels `"When Lumière table changes"`, `"Lumière table is on?"` when on, `"Turn on Lumière table"`…), `accent_insensitive` (`"lumiere"` matches `"Lumière"`), `caps_at_eight_entities`, `sensor_has_no_actions`, `hidden_entities_excluded`.
- [ ] **Step 2: Run** `cargo test -p oikos-web flow_picker`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): card picker entries`.

---

### Task 7: Full-bleed view canvas (layout, pills, wires, controls, minimap, runs drawer)

**Files:**
- Replace `crates/oikos-web/src/screens/flow_canvas.rs` with the directory `screens/flow_canvas/{mod.rs, graph.rs, viewport.rs, toolbar.rs, drawer.rs, settings.rs}`. Move `NodeEditor` and its field helpers into `settings.rs` unchanged. Edit mode keeps working through the old drag code until Task 8, re-hosted on `Graph` and `Viewport`.
- Modify: `styles/main.css` (flow page section), `src/icons.rs` (drop `LOUPE`)
- Test: `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: Tasks 2–4.
- Produces (used by Tasks 8–10):
  ```rust
  // graph.rs
  #[derive(Clone, Copy, PartialEq)] pub enum Mode { View, Edit }
  #[derive(Clone, PartialEq)] pub enum Selection { Node(String), Edge(EdgeKey) }
  #[component] pub fn Graph(flow: Signal<Flow>, mode: Mode, lit: Signal<Highlight>, dim: Signal<bool>,
      steps: Signal<BTreeMap<String, String>> /* node → "+12s" */, parked: Signal<Option<(String, u64)>>,
      live: Signal<bool>, selection: RwSignal<Option<Selection>>) -> impl IntoView;
  // viewport.rs
  #[component] pub fn Viewport(view: RwSignal<View>, content: Signal<Rect>, children: Children) -> impl IntoView; // owns the screen size signal
  #[component] pub fn CanvasControls(view: RwSignal<View>, content: Signal<Rect>) -> impl IntoView;
  #[component] pub fn Minimap(view: RwSignal<View>, flow: Signal<Flow>) -> impl IntoView;
  // drawer.rs
  #[component] pub fn Drawer(open: RwSignal<bool>, runs: Signal<Vec<RunView>>, flow: Signal<Flow>, replay: RwSignal<Option<String>>, children: ChildrenFn) -> impl IntoView;
  ```

Behaviour decisions:
- `.flow-canvas-screen` fills the viewport below the sticky `.topbar` with no page scroll (`height: calc(100dvh - <topbar block>)`, measured from `.topbar`'s bounding box on mount into a CSS variable `--flow-top`). The `Viewport` surface is `overflow:hidden`, and one inner `div` carries `View::css_transform()` with `transform-origin: 0 0`. Cards and the SVG wire layer live inside it in world coordinates.
- Pan: pointer-drag on the empty surface (pointer capture). Wheel: `ctrlKey||metaKey` → `zoom_at(cursor, exp(-deltaY*0.01))` (covers trackpad pinch), else `pan(-deltaX, -deltaY)`. Touch: two active pointers → zoom by the ratio of distances about their midpoint, plus pan by the midpoint's motion.
- On first render and when the flow id changes: `fit(bounds, w, h)`. CanvasControls: + / − (`zoom_at(centre, ZOOM_STEP or 1/ZOOM_STEP)`), ⤢ fit. Minimap 180×120 bottom-right: cards as role-coloured rects, the viewport outline, click/drag → `centre_on`.
- Toolbar: top-left pill = back link (`href="/flows"`), name, mode word. Top-right group = `FlowSwitch`, Run, Edit (`.flow-toolbar__edit`), Runs (toggles the drawer). The edit group appears in Task 8.
- Pills: `.flow-node.flow-node--{trigger|check|action|wait}` (from `flow_text::role`), round icon (`icons::node_icon`), `.flow-node__title` = `flow_text::title`, `.flow-node__sub` = `subline`, `.flow-node__step` = step time or countdown. Pins: `.flow-node__in` (non-triggers), `.flow-node__out` once, or twice for `if` (`data-port="true|false"`). Wires: `.flow-wire` + `--yes`/`--no` by port, with `.flow-wire-label` ("yes"/"no") at `route.label_at`.
- Runs: no highlight by default. If `active_run` is set, `lit = highlight(active run)`, `live = true` (lit wires get `.flow-wire--live`, a dash-offset animation disabled under `prefers-reduced-motion`), no dimming, and the parked delay shows its countdown. Clicking a run in the drawer sets `replay = Some(run_id)`: `lit` = that run's highlight, `dim = true` (unlit cards and wires `.flow-node--dim`), and each visited card shows `step_offset(step.at, run.started_at)`. Clicking it again or pressing Esc clears it. Failed steps keep `.flow-node--failed`.
- Invalid flow: keep `.flow-invalid` and render no canvas.

- [ ] **Step 1: Update or extend browser tests** in `tests/flow_canvas.rs`:
  - `canvas_lights_the_active_run`: replaces `canvas_lights_the_last_run`. With `active_run` = the first run: 3 `.flow-wire--lit`, `[data-node=wait].flow-node--lit` ×1, 0 `.flow-node--dim`. With `active_run: None`: 0 lit.
  - `replay_dims_the_rest`: click `.run-list li:first-child` → `.flow-node--dim` count = nodes − visited, and the step text `+0s` is present. Click again → 0 dim.
  - `pills_read_as_sentences`: the reference flow shows `"Wait 5 min"`.
  - `yes_no_labels_on_wires`: 2 `.flow-wire-label`, one `.flow-wire--yes`, one `.flow-wire--no`.
  - `controls_and_minimap_present`: `.flow-controls button` ×3, `.flow-minimap` ×1, `.flow-toolbar__edit` ×1.
  - Keep `invalid_flow_shows_the_reason_and_no_canvas`, `unknown_flow_links_back`, `parked_delay_counts_down` (port assertion → `.flow-wire--lit` on the taken branch), and `cards_have_handles_and_orphans_fade` (its `.flow-node__out` count becomes one per non-`if` node plus two per `if`; `.flow-node__in` and orphan counts are unchanged).
- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome -- --test flow_canvas`. Expected: new tests FAIL.
- [ ] **Step 3: Implement** the module split, components and CSS.
- [ ] **Step 4: Run** the full check list plus the browser tests. Expected: PASS.
- [ ] **Step 5: Visual check:** `tools/restart.sh --debug`, then a headless Chrome screenshot (see project memory: CDP or `--screenshot`, window ≥ 1000 px wide) of `/flows/living_led_motion` and `/flows/force-heating-loop`. Expected: the whole flow fits, no wire crosses a card, and the controls, minimap and drawer appear as in mockup A. Also capture at 390 px via device emulation and expect no Edit button.
- [ ] **Step 6: Commit** `feat(web): full-bleed flow canvas with pills, routed wires and run replay`.

---

### Task 8: Editing (selection, drawer settings, connect, delete, duplicate, undo, tidy)

**Files:**
- Modify: `screens/flow_canvas/{mod.rs, graph.rs, toolbar.rs, drawer.rs, settings.rs}`, `styles/main.css`
- Test: `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `flow_edit::*`, `flow_layout::{tidy, with_resolved_positions, routes}`, `Graph`/`Selection` from Task 7.
- Produces: `FlowBody` state `draft: RwSignal<Flow>`, `history: StoredValue<History>`, `selection: RwSignal<Option<Selection>>`, a `fn edit(draft, history, f: impl FnOnce(&mut Flow))` helper that records before mutating (used by Tasks 9–10).

Behaviour decisions:
- Entering edit mode: `draft = with_resolved_positions(flow)`, empty history, drawer shows runs until something is selected. The draft ignores later `house.flows` changes (spec §7), but runs keep updating.
- Edit toolbar replaces the view group: ＋ Add (Task 9 wires it; render it disabled here), Tidy (`edit(|f| *f = tidy(f))`), Undo (disabled when `!can_undo`), Cancel (`.btn--icon` with `X_MARK`), Save (`CHECK`). Save → `api::put_flow` → on success leave edit mode, clear history and selection; on error show a toast and stay.
- Card click (no drag beyond 3 px) → `Selection::Node`, and the drawer opens on its settings. Card drag → one `record` at pointerdown, `position` updates while moving. Click empty canvas → clear selection.
- Wire: an invisible `.flow-wire-hit` path (stroke-width 14, `pointer-events: stroke`) per wire. Click → `Selection::Edge`, `.flow-wire--selected`, and a `.flow-wire-cut` ✕ at `route.mid` that calls `remove_edge`.
- Connect: pointerdown on `.flow-node__out` starts `Connecting { from, port, to_world: Pt }`. A live preview path is drawn with `route(from_anchor, pointer, &[])`. Cards where `can_connect` is `Ok` get `.flow-node--target`. Pointerup: hit-test `document.element_from_point` → closest `[data-node]`; if `Ok`, `edit(connect)`; otherwise cancel. No picker.
- Keys (document `keydown` while editing; ignored when the target is `input|textarea|select` or `isContentEditable`): `Backspace|Delete` → remove the selection, `Mod-D` → `duplicate_node` and select it, `Mod-Z` → undo, `Shift-Mod-Z` → redo, `Escape` → clear the selection. `Mod` = `metaKey || ctrlKey`.
- Drawer settings: `settings.rs` body for the selected node, plus a "Delete card" button. Field edits go through `edit` (one record per field change event).
- Phone: `.flow-toolbar__edit { display:none }` under the phone media query from Global Constraints.

- [ ] **Step 1: Write failing browser tests:** `click_card_opens_settings` (enter edit, click `[data-node=moving]` → `.flow-node--selected`, `.flow-drawer .field` present), `delete_key_removes_card_and_wires` (select `wait`, dispatch `keydown Backspace` → `[data-node=wait]` gone, wires 4→2), `backspace_in_input_keeps_card` (focus a drawer input, dispatch Backspace → node still there), `wire_click_then_cut` (click `.flow-wire-hit`, click `.flow-wire-cut` → wires −1), `connect_by_drag` (pointerdown on `[data-node=on] .flow-node__out`, pointermove, pointerup over `[data-node=off]` → wires +1), `connect_into_trigger_refused` (drop on `[data-node=motion]` → unchanged), `undo_restores` (delete then `Mod-Z` → back), `drag_is_one_undo_step` (pointerdown, three pointermoves, pointerup on a card, then one undo → original position), `ws_update_keeps_draft` (while editing with one card deleted, `house.flows.set(...)` with a new run → the draft still lacks the card and `.run-list li` count updates), `tidy_is_undoable`.
- [ ] **Step 2: Run** the browser tests. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full check list and browser tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): React Flow-style editing on the flow canvas`.

---

### Task 9: "＋ Add" picker

**Files:**
- Create: `screens/flow_canvas/picker.rs`
- Modify: `toolbar.rs` (enable ＋ Add), `styles/main.css`
- Test: `tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `flow_picker::entries`, `flow_edit::add_node`, the `edit` helper, `flow_viewport::visible_world`.
- Produces: `#[component] pub fn Picker(open: RwSignal<bool>, on_pick: Callback<NodeKind>) -> impl IntoView`.

Behaviour: a popover under ＋ Add with a focused search input, results grouped under headers `WHEN · TRIGGER`, `AND · CHECK`, `THEN · ACTION`, `WAIT`, `FLOW`. ↑/↓ move the highlight, Enter picks, Esc or an outside click closes. Picking adds the node at the centre of `visible_world` minus half a card, selects it (opening its settings) and closes the picker.

- [ ] **Step 1: Write failing browser tests:** `add_picker_adds_selected_card` (open, type "Wait", Enter → a new `[data-node=delay_1]` with `.flow-node--selected`), `picker_groups_entity_results` (with an entity "Lumière table" in `house.entities`, type "table" → headers `WHEN · TRIGGER`, `AND · CHECK`, `THEN · ACTION` present), `escape_closes_picker`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full check list and browser tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(web): card picker on the flow canvas`.

---

### Task 10: New flow and renaming

**Files:**
- Modify: `screens/flows.rs` (button), `screens/flow_canvas/{mod.rs, toolbar.rs}`, `src/app.rs` (route `/flows/new` declared before `/flows/:id`), `styles/main.css`
- Test: `tests/flows_list.rs`, `tests/flow_canvas.rs`

**Interfaces:**
- Consumes: `flow_edit::slug_id`, the Task 8 edit state, `api::put_flow`.
- Produces: `#[component] pub fn NewFlowScreen() -> impl IntoView`.

Behaviour: the "New flow" button (`.flows-new`, a link to `/flows/new`, hidden on phones) sits on the `/flows` list. `NewFlowScreen` starts `FlowBody` in edit mode on `Flow { id: "", name: "New flow", enabled: false, mode: Single, ..Default }`. The switch and Run are hidden until saved. In edit mode the top-left name is an `<input>` (for existing flows too). It writes `draft.name` and does not touch the id. On the first Save, `id = slug_id(name, existing ids)`, then `put_flow`; on success navigate (replace) to `/flows/{id}` in view mode. A Save with no trigger shows the hub's error toast and stays. Cancel on an unsaved flow navigates to `/flows`.

- [ ] **Step 1: Write failing browser tests:** `list_has_new_flow_button` (`a.flows-new[href="/flows/new"]`), `new_flow_starts_in_edit_mode` (mount `/flows/new` → the name input has value `"New flow"`, `.flow-node` count 0, no `.flow-switch`), `rename_updates_draft_name` (type in the name input → the header text changes; the draft id is unchanged on an existing flow).
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full check list and browser tests. Expected: PASS.
- [ ] **Step 5: Live check** against the rebuilt hub (`tools/restart.sh --debug`). In headless Chrome: New flow, name "Canvas smoke test", add the "Schedule" and "Wait" cards, connect them, Save. Expected: `GET /api/v1/flows/canvas_smoke_test` returns it with `enabled: false`. Then delete it through MCP `delete_flow` or `DELETE /api/v1/flows/canvas_smoke_test`.
- [ ] **Step 6: Commit** `feat(web): create and rename flows from the browser`.

---

### Task 11: Docs and final verification

**Files:**
- Modify: `README.md` (web UI flows paragraph: canvas, editing, picker, New flow, phone view-only)

- [ ] **Step 1: Update the README paragraph** that describes the flows page (currently "the web UI shows them at `/flows` with the last run lit on the canvas").
- [ ] **Step 2: Run** the full check list, all browser tests, then `tools/restart.sh` (release) and take headless screenshots of `/flows`, `/flows/living_led_motion` (view, replaying a run, edit with a card selected, picker open) and a 390 px phone view. Expected: matches the spec's decisions §1–§5 and §8.
- [ ] **Step 3: Commit** `docs(readme): flow canvas editing and new flows`.
