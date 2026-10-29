# Energy glowing circles (style C2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Classic live diagram on the web `/energy` page with the "glowing circles"
drawing (style C2): big gradient circles with halos, curved gradient wires, glowing dots that
travel in the flow direction, a solar/grid/battery mix ring on the house.

**Architecture:** Pure geometry and values live in a new `energy_circles.rs` (layouts, placement,
curves, wires, sizes, mix) and `energy_dots.rs` (dot phase and positions), both unit-tested
natively. A new Leptos component `CirclesDiagram` draws keyed SVG nodes and wires from them and
replaces Classic's SVG in `screens/energy.rs`. Dots are moved by one `requestAnimationFrame`
loop that reads a non-reactive store, so readings never restart the motion.

**Tech Stack:** Rust, Leptos (CSR, WASM), web-sys, inline SVG, CSS in `crates/oikos-web/styles/`.

**Spec:** `docs/superpowers/specs/2026-10-29-energy-circles-view-design.md`. Look-and-motion
references (throwaway, untracked): `.superpowers/brainstorm/25189-1793268041/content/style-c-v1.html`
(panel C2) and `phone-v2.html` (P2). Where they disagree with the spec or this plan, the plan wins.

## Global Constraints

- Solar has exactly one wire, to the house. No wire ever joins solar and grid. Export runs house → grid; the battery has one wire, to the house.
- The setting keeps `EnergyView::Classic` and its label "Classic" / "Classique"; Flux is untouched.
- Every text through the i18n modules, EN and FR. Tracked files stay generic: no host names, addresses, rooms or family names.
- Breakpoint: phone layout below 600 px window width (today's `layout()` rule in `screens/energy.rs`).
- Wires and dots read each node's own `EnergyNode::value` / `flow` (Classic's model); only the mix ring reads `energy_sankey::build`.
- Consumers come from `EnergyUi::diagram` (`ARC_SLOTS` = 3, last slot `NodeKind::More` when folded). Flux's Unitemised node is not drawn.
- No CSS `filter` on animated elements (phones): halos are radial-gradient fills, dot glow is a larger faint circle.
- `prefers-reduced-motion: reduce`: no dots in the DOM, no breathing, no transitions; wire widths and the mix ring stay.
- Commits as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Before a build, if `~/dev-scratch/cargo-target` is over 5 GB run `cargo clean`; with under 5 GB free (`df -h ~`), run per-crate tests only.
- Gates (all tasks): `cargo fmt --all --check`, `cargo clippy -p oikos-web --all-targets -- -D warnings`, `cargo test -p oikos-web`, `cargo check -p oikos-web --target wasm32-unknown-unknown`.

## Review Focus

1. **Import flips to export while the page is open**: the grid wire reverses (house → grid), turns mint, its dots carry on from the house end; no wire joins solar and grid. → Task 2 test `export_reverses_the_grid_wire`, Task 5 test `export_flips_the_grid_wire_in_place`.
2. **Night, or a sensor gone**: solar `None`/0 W or an unavailable node gives a dimmed circle, an idle wire, no halo, no dots, and never `NaN` in an SVG attribute (zero-length curve, zero mix sum). → Task 1 test `rim_curve_survives_coincident_centres`, Task 2 tests `unknown_and_unavailable_are_idle` and `mix_is_none_when_nothing_feeds_the_house`.
3. **Consumer set changes under the animation** (3 → 4 folds into More, a consumer drops to idle): stale dot wires are dropped, survivors keep their phase. → Task 4 test `sync_drops_gone_wires_and_keeps_phase`.
4. **Battery or water present or absent**, on both layouts: they share one slot; water gets a node but no wire. → Task 1 test `battery_or_water_takes_the_side_slot`, Task 2 test `water_has_no_wire`.
5. **Long names and big values**: a 5-digit wattage fits the circle in kW, consumer names are ellipsised with the existing `short()`. → Task 3 test `big_power_reads_in_kilowatts`.

---

### Task 1: Circle layouts, placement and curves

**Files:**
- Create: `crates/oikos-web/src/energy_circles.rs` (register `pub mod energy_circles;` in `crates/oikos-web/src/lib.rs`)
- Test: `#[cfg(test)] mod tests` in the same file

**Interfaces:**
- Produces:
  - `pub struct CirclesLayout { pub view_w: f64, pub view_h: f64, … }` with `pub const WIDE` and `pub const NARROW`, and `pub fn for_width(narrow: bool) -> &'static CirclesLayout`.
  - `pub struct Placed { pub at: (f64, f64), pub r: f64 }`
  - `pub fn place(diagram: &[EnergyNode], layout: &CirclesLayout) -> BTreeMap<String, Placed>` keyed by node id.
  - `pub struct Curve { pub from: (f64, f64), pub ctrl: (f64, f64), pub to: (f64, f64) }` with `path(&self) -> String` (`"M x y Q cx cy x y"`), `point(&self, t: f64) -> (f64, f64)`, `length(&self) -> f64` (16-segment polyline), `reversed(&self) -> Curve`.
  - `pub fn rim_curve(node: Placed, house: Placed, bend: f64) -> Curve` — from node rim to house rim along the centre line; control point = chord midpoint + `bend` px along the left normal of node → house.
  - `pub fn bend_for(layout: &CirclesLayout, node: &EnergyNode, slot: Option<usize>, slots: usize) -> f64`

Values (px, viewBox units; tune by eye afterwards only if the tests still hold):

| | WIDE (760 × 380) | NARROW (360 × 470) |
|---|---|---|
| radii big / house / small | 46 / 54 / 30 | 44 / 54 / 30 |
| house | (400, 214) | (180, 240) |
| solar | (260, 74) | (84, 74); with battery or water (60, 74) |
| grid | (96, 214) | (276, 74); with battery or water (300, 74) |
| battery or water | (190, 330) | (180, 66) |
| 1 consumer | (650, 214) | (180, 404) |
| 2 consumers | (640, 140), (640, 288) | (110, 404), (250, 404) |
| 3 consumers | (630, 80), (655, 214), (630, 348) | (56, 400), (180, 412), (304, 400) |
| bends solar / grid / battery | 26 / −22 / 0 | 22 / −22 / 0 |
| bends 3 consumers | −18, 0, 18 (1–2 consumers: 0) | 16, 0, −16 (1–2 consumers: 0) |

Solar, Grid, Battery use the big radius, House the house radius, Consumer / More / Water the small one.

- [ ] **Step 1: Write the failing tests** (reuse the `ent`/`consumer` fixture style of `energy.rs` tests with `EnergyFlow::build`)

```rust
#[test] fn wide_places_the_triangle_and_the_arc() {
    // solar, grid, water, 5 consumers → 3 slots (2 + More)
    let p = place(&ui.diagram, &CirclesLayout::WIDE);
    assert_eq!(p["solar"].at, (260.0, 74.0));
    assert_eq!(p["house"].r, 54.0);
    assert_eq!(arc_positions, vec![(630.0, 80.0), (655.0, 214.0), (630.0, 348.0)]);
}
#[test] fn narrow_runs_top_to_bottom() { /* same fixture on NARROW: solar (60,74), grid (300,74), water (180,66), arc y ≥ 400 */ }
#[test] fn battery_or_water_takes_the_side_slot() {
    // with a battery: p["battery"].at == (190.0, 330.0) and no "water" key (diagram drops water)
    // without battery or water on NARROW: solar (84,74), grid (276,74)
}
#[test] fn rim_curve_leaves_and_meets_the_rims() {
    // |from - node.at| == node.r and |to - house.at| == house.r (1e-9); bend 0 → ctrl is the chord midpoint
    // point(0) == from, point(1) == to; reversed() swaps from/to and keeps ctrl
}
#[test] fn rim_curve_survives_coincident_centres() {
    // node.at == house.at → every coordinate finite; length() finite and ≥ 0
}
```

- [ ] **Step 2: Run them to see them fail** — `cargo test -p oikos-web --lib energy_circles` → FAIL (module missing).
- [ ] **Step 3: Implement** the types and functions above in `energy_circles.rs`. Water is placed only when the diagram contains it (the core already drops it when a battery exists).
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib energy_circles` → PASS.
- [ ] **Step 5: Commit** `feat(web): glowing-circles layouts and curves`.

---

### Task 2: Wires, sizes and the house mix

**Files:**
- Modify: `crates/oikos-web/src/energy_circles.rs`
- Modify: `crates/oikos-web/src/screens/energy_flux.rs` (make `sanitise` `pub(crate)`, move it to `energy_circles.rs` and import it from there)

**Interfaces:**
- Consumes: Task 1 (`Placed`, `place`, `rim_curve`, `bend_for`, `CirclesLayout`); `crate::energy::level`; `crate::energy_sankey::Sankey`.
- Produces:
  - `pub fn tone(node: &EnergyNode, slot: Option<usize>) -> &'static str` — one of `"solar" "grid" "export" "battery" "house" "more" "water" "c0" "c1" "c2"`; grid with `Flow::Out` is `"export"`; a consumer's tone is `c{slot}`.
  - `pub fn consumer_slot(diagram: &[EnergyNode], id: &str) -> Option<usize>` — 0-based index among `NodeKind::Consumer` nodes in diagram order.
  - `pub struct Wire { pub id: String, pub from: String, pub to: String, pub kind: NodeKind, pub tone: &'static str, pub from_tone: &'static str, pub to_tone: &'static str, pub curve: Curve, pub watts: f64, pub active: bool, pub width: f64 }` — `id` is the node id; `from`/`to` are node ids in flow direction.
  - `pub fn wires(diagram: &[EnergyNode], placed: &BTreeMap<String, Placed>, layout: &CirclesLayout) -> Vec<Wire>` — one per Solar, Grid, Battery, Consumer, More node; none for House or Water.
  - `pub fn wire_width(watts: Option<f64>, active: bool) -> f64` — `1.5` when inactive, else `2.5 + 5.0 * level(watts)`.
  - `pub fn halo(watts: Option<f64>, live: bool) -> (f64, f64)` — `(extra_radius, opacity)` = `(8.0 + 12.0 * level, 0.18 + 0.5 * level)`; `(0.0, 0.0)` when not live or below `IDLE_W`.
  - `pub struct Mix { pub solar: f64, pub battery: f64, pub grid: f64 }` (fractions summing to 1) and `pub fn house_mix(s: &Sankey) -> Option<Mix>` from the `solar|battery|grid → house` links; `None` when their sum is ≤ 0.
  - `pub fn ring_dashes(mix: &Mix, r: f64, gap: f64) -> [(f64, f64); 3]` — `(dash_len, dash_offset)` for solar, battery, grid, clockwise from the top in that order; `gap` px between non-empty arcs only when more than one is non-empty; an empty part gets length 0.

Direction rules: Solar → house always. Grid and Battery: `Flow::In` node → house, `Flow::Out` house → node, `Idle` node → house. Consumer and More: house → node. Build every curve node → house with `rim_curve` and `reversed()` it when the flow runs house → node, so the bend never flips. `active` = status `Live`, value `Some(w)` with `w >= IDLE_W`, and flow not `Idle`. `from_tone`/`to_tone`: the house end is `"house"`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn solar_only_feeds_the_house() { /* every wire with from=="solar" has to=="house"; no wire joins "solar" and "grid" in either order — exporting fixture */ }
#[test] fn export_reverses_the_grid_wire() { /* grid -2000 W: from=="house", to=="grid", tone=="export", curve == rim_curve(grid,house,bend).reversed() */ }
#[test] fn battery_charge_runs_to_the_battery() { /* battery Out: from=="house", to=="battery"; In: reverse */ }
#[test] fn unknown_and_unavailable_are_idle() { /* solar None, grid unavailable → active == false, width == 1.5; halo == (0.0, 0.0) */ }
#[test] fn water_has_no_wire() { /* water in diagram → no Wire with id "water"; house never has a wire */ }
#[test] fn consumers_take_slot_tones() { /* 3 consumers → c0, c1, c2; 5 consumers → c0, c1, more */ }
#[test] fn widths_and_halo_scale_with_power() { /* wire_width(Some(10.0), true) == 2.5; wire_width(Some(10_000.0), true) == 7.5; halo(Some(10_000.0), true) == (20.0, 0.68) */ }
#[test] fn mix_follows_the_house_links() { /* solar 600 W, house 800 W, grid import 200 W → Mix{solar:.75, battery:0, grid:.25}; ring_dashes lengths sum to 2πr − 2·gap */ }
#[test] fn mix_is_none_when_nothing_feeds_the_house() { /* all idle → None; ring_dashes never NaN */ }
```

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib energy_circles` → the new tests FAIL.
- [ ] **Step 3: Implement** the functions above.
- [ ] **Step 4: Run** `cargo test -p oikos-web` → PASS (Flux tests still pass after the `sanitise` move).
- [ ] **Step 5: Commit** `feat(web): glowing-circles wires, sizes and house mix`.

---

### Task 3: Words inside the circles

**Files:**
- Modify: `crates/oikos-web/src/i18n/energy.rs` (new `EnergyText` keys and one helper)
- Modify: `crates/oikos-web/src/energy.rs` (power split and circle texts; tests in its `mod tests`)

**Interfaces:**
- Produces:
  - `EnergyText::{CircleProduction, CircleImport, CircleExport, CircleCharging, CircleDischarging}` = `("production","production")`, `("import","import")`, `("export","export")`, `("charging","charge")`, `("discharging","décharge")`.
  - `pub fn solar_share_line(lang: Lang, pct: u8) -> String` in `i18n/energy.rs`: EN `"72% solar"`, FR `"72\u{202F}% solaire"`.
  - `pub fn power_parts(lang: Lang, watts: Option<f64>) -> (String, &'static str)` in `energy.rs`: the number and its unit exactly as `power_text` formats them (`("3.2","kW")`, `("253","W")`, `("—","")`); `power_text` is rebuilt on it.
  - `pub fn circle_texts(lang: Lang, node: &EnergyNode) -> (String, String)` — the big line and the small line of a big circle. Unavailable → (`Unavailable` word, `""`); `None` → (`"—"`, `""`); Solar → (n, `"{unit} · production"`); Grid In / Out → (n, `"{unit} · ↘ import"` / `"{unit} · ↗ export"`); Battery In / Out → (n, `"{unit} · ↘ discharging"` / `"{unit} · ↗ charging"`); Grid or Battery Idle, House → (n, unit).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn circle_texts_name_the_direction() {
    // grid In 253 W, En → ("253", "W · ↘ import"); grid Out 1610 W, Fr → ("1,6", "kW · ↗ export")
    // solar 3460 W, En → ("3.5", "kW · production"); battery Out 500 W, Fr → ("500", "W · ↗ charge")
    // unavailable grid, En → ("Unavailable", "")
}
#[test] fn big_power_reads_in_kilowatts() { /* 12_345 W En → ("12.3","kW"); power_text unchanged for 999 W and 1000 W */ }
#[test] fn solar_share_line_in_both_languages() { /* En 72 → "72% solar"; Fr 72 → "72\u{202F}% solaire" */ }
```

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib energy` → FAIL.
- [ ] **Step 3: Implement** the keys and functions above.
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib energy` → PASS, existing `texts`/`texts_in_french` tests included.
- [ ] **Step 5: Commit** `feat(web): words inside the energy circles`.

---

### Task 4: Dot motion

**Files:**
- Create: `crates/oikos-web/src/energy_dots.rs` (register in `lib.rs`)

**Interfaces:**
- Consumes: `Curve`, `Wire` (Task 2), `crate::energy::level`.
- Produces:
  - `pub struct DotWire { pub curve: Curve, pub len: f64, pub speed: f64, pub radius: f64, pub count: usize, pub phase: f64 }`
  - `pub fn dot_wire(w: &Wire, phase: f64) -> DotWire` — `len = curve.length()`, `speed = 30.0 + 90.0 * level` (px/s), `radius = 2.2 + 2.6 * level`, `count = max(2, floor(len / 40))`.
  - `pub fn advance(phase: f64, dt_s: f64, speed: f64, len: f64) -> f64` — `(phase + dt_s * speed / len).rem_euclid(1.0)`; returns `phase` unchanged when `len <= 0`.
  - `pub struct Dot { pub x: f64, pub y: f64, pub r: f64, pub opacity: f64, pub halo: bool }`
  - `pub fn dots(w: &DotWire) -> Vec<Dot>` — for each of `count` dots at `q = (phase + i/count).rem_euclid(1)`: one halo dot (`r × 2.4`, opacity `0.25 × env`, `halo: true`) then the head and two trail dots at `q − 0.018 k` (k = 0, 1, 2) with radius `r × [1, 0.72, 0.44]` and opacity `env × [1, 0.62, 0.24]`, where `env = sin(π q)`. So `dots().len() == 4 × count`.
  - `pub type DotStore = Rc<RefCell<BTreeMap<String, DotWire>>>`
  - `pub fn sync(store: &DotStore, wires: &[Wire])` — keeps only active wires; an existing id keeps its `phase`; a new id starts at a phase derived from its id (stable hash → `[0, 1)`, no randomness).
  - `pub fn reduced_motion() -> bool` — `matchMedia("(prefers-reduced-motion: reduce)")` on wasm, `false` elsewhere.
  - `#[cfg(target_arch = "wasm32")] pub fn run(layer: web_sys::Element, store: DotStore) -> Rc<Cell<bool>>` — starts a rAF loop (re-arming `request_animation_frame` each frame; `dt` from `js_sys::Date::now()`, clamped to 0.1 s); each frame advances every phase and rewrites the layer's children: one `<g data-k="{id}">` per wire holding `4 × count` `<circle>`s with `cx cy r` and `style="opacity:…"`, created with `create_element_ns(SVG_NS, …)`, extras removed; `<g>`s of ids no longer in the store removed. The loop stops once the returned flag is set to `false`. Browsers already pause rAF in hidden tabs; the 0.1 s `dt` clamp keeps dots from jumping on return.

- [ ] **Step 1: Write the failing tests** (native)

```rust
#[test] fn advance_wraps_and_ignores_empty_curves() { /* advance(0.9, 1.0, 20.0, 100.0) ≈ 0.1; advance(0.3, 1.0, 50.0, 0.0) == 0.3 */ }
#[test] fn dots_fade_at_both_ends() { /* phase 0, count 2 → head at curve.from with opacity 0; head at q=.5 has opacity 1; len == 8 */ }
#[test] fn faster_and_bigger_with_power() { /* dot_wire for 10 W vs 10 kW: speed 30 vs 120, radius 2.2 vs 4.8; count ≥ 2 */ }
#[test] fn sync_drops_gone_wires_and_keeps_phase() {
    // sync with wires a,b; set a.phase = 0.42; sync with a (active) and c → keys {a, c}; a.phase == 0.42
    // an inactive wire is never stored; same id twice → same starting phase
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib energy_dots` → FAIL.
- [ ] **Step 3: Implement** the pure parts, then `reduced_motion` and `run` behind `cfg(target_arch = "wasm32")`; add web-sys features only if `cargo check --target wasm32-unknown-unknown` asks for them.
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib energy_dots` → PASS; `cargo check -p oikos-web --target wasm32-unknown-unknown` → clean.
- [ ] **Step 5: Commit** `feat(web): travelling dots for the energy wires`.

---

### Task 5: The CirclesDiagram component replaces Classic's drawing

**Files:**
- Create: `crates/oikos-web/src/screens/energy_circles.rs` (register in `screens/mod.rs`)
- Modify: `crates/oikos-web/src/screens/energy.rs` (swap the Classic `<svg>` for `<CirclesDiagram/>`; delete `DiagramEdge`, `DiagramNode`, `Positions`, `layout()`; replace with `fn narrow() -> bool` using the same 600 px rule)
- Modify: `crates/oikos-web/src/energy.rs` (delete `Layout`, `positions`, `Edge`, `edge`, `CLEARANCE`, `NODE_R`, `VIEW_H`, `stroke`, `speed` and their tests; keep `level`)
- Test: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: Tasks 1–4; `glyph`/`short`/`kind_class` from `screens/energy.rs` (make them `pub(crate)`), `node_value_text`, `node_label`, `aria_sentence`, `energy_sankey::build`.
- Produces: `#[component] pub fn CirclesDiagram(ui: Memo<EnergyUi>, narrow: bool, open: Callback<String>, expanded: RwSignal<bool>) -> impl IntoView`.

Markup (class names the tests and CSS rely on):
- `<div class="energy-diagram energy-circles" class:energy-circles--narrow=narrow>` around `<svg class="energy-diagram__svg" viewBox="0 0 {view_w} {view_h}" role="group" aria-label=…>` with `<title>` = `aria_sentence`. (Role `group`, as Flux, because the nodes inside are focusable.)
- `<defs>`: per tone a radial gradient `energy-disc-{tone}` (tone colour 34 % at the top → 8 %, mixed into the background) and `energy-halo-{tone}` (tone colour → transparent); per wire a `userSpaceOnUse` linear gradient `energy-wire-{sanitise(id)}` from `curve.from` to `curve.to`, stops `var(--tone-{from_tone})` → `var(--tone-{to_tone})`.
- `<g class="energy-circles__wires">`: keyed `<For>` by wire id; `<path class="energy-edge energy-edge--{kind_class} energy-tone--{tone}" class:energy-edge--idle=!active d=curve.path() stroke="url(#energy-wire-…)" data-from=from data-to=to style="stroke-width:{width}px">`.
- `<g class="energy-circles__dots" node_ref=…>`: empty in markup; when `!reduced_motion()`, an `Effect` calls `energy_dots::sync` on every `ui` change and `run` is started once on mount; `on_cleanup` sets the flag to `false`.
- `<g class="energy-circles__nodes">`: keyed `<For>` by node id; `<g class="energy-node energy-node--{kind_class} energy-tone--{tone}" class:energy-node--off=… class:energy-node--click=… transform="translate(x y)">` holding: halo `<circle class="energy-node__halo" r=r+extra fill="url(#energy-halo-{tone})" style="opacity:…">`, disc `<circle class="energy-node__disc" r fill="url(#energy-disc-{tone})">`, rim `<circle class="energy-node__rim" r>` (not on the house), the glyph `foreignObject` (Classic's), and texts:
  - big circles (Solar, Grid, Battery): `circle_texts` lines inside (`energy-node__big`, `energy-node__small`), name under the circle (`energy-node__label`);
  - House: `power_parts` inside, `solar_share_line(floor(mix.solar × 100))` when `house_mix` is `Some` and that share is > 0 (`energy-node__share`), name under; mix ring `<g class="energy-node__mix" transform="rotate(-90)">` = track circle + three arc circles `energy-node__arc--{solar|battery|grid}` with `stroke-dasharray`/`stroke-dashoffset` from `ring_dashes` (`gap` 4 px, ring width 7);
  - small circles (Consumer, More, Water): `node_value_text` then `short(node_label)`, right of the circle on wide, under it on narrow; Water's rim dashed.
- Clickable nodes (`More`, or exactly one sensor) get `tabindex="0"`, `role="button"`, `aria-label="{label} {value}"`; click and Enter/Space (with `prevent_default`) do what Classic's click did. Copy Flux's `on:keydown` handler shape.

- [ ] **Step 1: Write the failing browser tests** in `tests/energy.rs` (keep `live_house_shows_nodes_and_cards` and `four_consumers_fold_into_more` passing unchanged: still 6 `.energy-node`, 1 `.energy-node--water`, 4 `.energy-edge`)

```rust
#[wasm_bindgen_test] async fn circles_wire_solar_only_to_the_house() {
    // solar 3000 W, grid -1500 W (export), house derived, two consumers
    // every .energy-edge with data-from="solar" has data-to="house"; none joins solar and grid
    // .energy-edge--grid has data-from="house", data-to="grid" and class energy-tone--export
}
#[wasm_bindgen_test] async fn export_flips_the_grid_wire_in_place() {
    // start importing (grid +400 W): data-from="grid"; update the grid entity to -800 W; tick
    // the same <path> element (compare by reference) now has data-from="house"
}
#[wasm_bindgen_test] async fn house_shows_the_solar_share() {
    // solar 600 W, grid 200 W → .energy-node--house .energy-node__share text == "75% solar"
}
#[wasm_bindgen_test] async fn circle_opens_its_sensor_from_the_keyboard() {
    // consumer "oven": its .energy-node has tabindex="0", role="button"; Enter keydown → default_prevented and path "/entity/oven"
}
```

- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome -- --test energy` → the new tests FAIL.
- [ ] **Step 3: Implement** the component, swap it in, delete Classic's drawing code listed above.
- [ ] **Step 4: Run** the wasm tests → PASS; then the gates from Global Constraints → clean (clippy catches leftover dead code).
- [ ] **Step 5: Commit** `feat(web): Classic energy view drawn as glowing circles`.

---

### Task 6: Styles, both themes, reduced motion, visual check

**Files:**
- Modify: `crates/oikos-web/styles/main.css` (replace the `.energy-edge` / `.energy-node` / `.energy-diagram` Classic rules between `/* Energy */` and `/* /Energy */`; move `--flux-c0..c4` and `--flux-muted` from `.energy-flux` to `.energy` so both views read them)

Rules to write:
- `.energy-circles`: `--tone-solar: var(--solar)`, `--tone-grid: var(--accent-violet)`, `--tone-export: var(--accent-mint)`, `--tone-battery: var(--accent-mint)`, `--tone-house: var(--ink)`, `--tone-water: var(--accent-cyan)`, `--tone-more: var(--flux-muted)`, `--tone-c0..c2: var(--flux-c0..c2)`, `--energy-glow: 1`; `:root[data-theme="light"] .energy-circles { --energy-glow: 0.45 }`. Each `.energy-tone--x { --tone: var(--tone-x) }`. Glass card styling and `aspect-ratio` 760/380 (narrow 360/470).
- Wires: `fill: none; stroke-linecap: round; opacity: .4; transition: stroke-width .6s, opacity .4s`; idle opacity `.12`. Rims `stroke: var(--tone)`, width 2.5 (house none); halo opacity multiplied by `--energy-glow`, `animation: energy-breathe 4s ease-in-out infinite` (scale 1 → 1.06, `transform-box: fill-box; transform-origin: center`); node groups `transition: transform .6s`; mix-ring arcs `transition: stroke-dasharray .9s, stroke-dashoffset .9s`; dot fill `var(--tone)` via a `color` on each dot group (set by `run` from the wire's tone, or a class on the `<g>`). Off nodes opacity `.42`. Water rim `stroke-dasharray: 3 3`. Texts: big value Manrope 700 19 px, small line Inter 600 11 px muted, labels Inter 600 13 px muted (narrow: 17 / 10 / 12 px); share line `var(--solar)` 11 px.
- `@media (prefers-reduced-motion: reduce)`: no breathe animation, no transitions in `.energy-circles`.

- [ ] **Step 1: Build and serve** `cd crates/oikos-web && trunk serve` against a hub (a scratch copy of `Trunk.toml` with the proxy backends pointed at the live hub, never committed).
- [ ] **Step 2: Screenshot** with headless Chrome over CDP: 1440 wide and a 390 × 844 phone (`Emulation.setDeviceMetricsOverride`), Night Glass and Ivoire (`localStorage oikos.theme`), plus one run with `Emulation.setEmulatedMedia` `prefers-reduced-motion: reduce`. Compare with the mockups: circles, halos, gradient wires, dots moving, export wire mint, ring, no overlap of labels at 3 consumers. Keep screenshots in the scratchpad only (live house names).
- [ ] **Step 3: Run** the gates → clean.
- [ ] **Step 4: Commit** `style(web): glowing circles for the energy diagram`.

---

### Task 7: Docs and final verification

**Files:**
- Modify: `docs/users/web-ui.md:188` (the Energy paragraph)

- [ ] **Step 1: Rewrite** the first two sentences of the Energy paragraph for the new drawing: glowing circles, solar top-left, the grid on the left, the house in the centre with a ring showing where its power comes from, consumers on the right (top to bottom on phones: solar and grid on top, consumers below); dots travel along the wires in the flow direction, more and faster with more power; solar feeds the house only and export leaves from the house to the grid; motion stops when the system asks for reduced motion. Keep the rest of the paragraph (fold, cards, hint). Line 262 stays as is.
- [ ] **Step 2: Run** the full workspace gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cd crates/oikos-web && wasm-pack test --headless --chrome` → all green.
- [ ] **Step 3: Commit** `docs: glowing-circles energy view`.
