# Energy Orbite View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third live energy view, Orbite (house in the centre, inner ring = where power comes
from, outer ring = where it goes), a Candy palette for all three views in Ivoire, and a live preview
of the chosen view in Settings › Appearance.

**Architecture:** `oikos_api::EnergyView` gains `Orbite`. A pure module `energy_orbite.rs` turns the
existing `energy_sankey::Sankey` into two rings of arcs (angles, paths, dot positions; native unit
tests). `screens/energy_orbite.rs` renders them as a Leptos component with the same props as
`FluxDiagram`, with a small rAF loop for the dots. One shared `--energy-*` CSS palette on `.energy`
(and the Settings preview) feeds Classic, Flux and Orbite; Night Glass values equal today's.

**Tech Stack:** Rust, Leptos (CSR, wasm32), SVG, CSS custom properties, `wasm-bindgen-test`.

**Spec:** `docs/superpowers/specs/2026-10-29-energy-orbite-view-design.md` (read it with this plan;
look-and-feel references are the untracked mockups under
`.superpowers/brainstorm/25437-1793282101/content/`: `orbite-v1.html`, `orbite-ivoire-v2.html`
(panel B · Candy), `settings-preview.html` (option B)).

## Global Constraints

- Stored value `energy_view: "orbite"`; label **Orbite** in EN and FR; Classic stays the default.
- Night Glass must look exactly as today for Classic and Flux (palette values = today's tokens).
- Ivoire (Candy): solar `#ffb21f`, grid `#9a7bff`, battery `#2fd3a0`, c0 `#35b9f0`, c1 `#ff6f91`,
  c2 `#7f9cff`, c3 `#ff9a55`, c4 `#3fd6c6`, muted `#d9d1c3`; Classic `--tone-water` → `#35b9f0`.
- Grid is one colour on both rings, battery one colour on both ("colour means place").
- Consumer colours rotate `slot % 5`, same slot as Flux.
- `prefers-reduced-motion: reduce` → no dots, no glide (Orbite and the Settings preview).
- Settings preview: fixed sample values, no live data, no requests, `aria-hidden`.
- No change to `oikos_core::energy`, `EnergyFlow::build`, `energy_sankey::build`, the cards, the grid
  day chart, the list, the Android app; Classic's `DotStore` is not refactored.
- Attribution: commits as `Gekkotron` (`-c user.name=Gekkotron -c
  user.email=60887050+Gekkotron@users.noreply.github.com`), message ends with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Never write the user's real name.
- Generic content only: no real rooms, hosts or family names in code, tests or docs.
- Gates (CI): `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo check -p oikos-web --target wasm32-unknown-unknown`; `cargo test --workspace`;
  `wasm-pack test --headless --chrome` in `crates/oikos-web`. Check `df -h ~` and
  `du -sh ~/dev-scratch/cargo-target` first (`cargo clean` if > 5 GB; per-crate tests when < 5 GB
  free).

## Review Focus

1. **Meters disagree** (outer sum ≠ inner sum, e.g. a house sensor reading higher than the
   sources): the shorter ring stops short, nothing stretched, no arc over 360°. Test in Task 3.
2. **Night, solar only, nothing else** (one inner arc, one outer "Not itemised" arc, or an empty
   Sankey): no NaN in paths, a full-circle arc does not vanish (an arc of 360° must not be drawn as
   a zero-length SVG arc). Test in Task 3.
3. **Direction flips live** (grid import → export, battery discharge → charge): the arc moves from
   inner to outer ring by id (`grid` → `export`), no stale DOM node left behind. Test in Task 4.
4. **Theme switch while on /energy** (Night ↔ Ivoire): colours follow at once, because everything
   reads CSS variables, never resolved hex in Rust. Test in Task 2.
5. **Hidden tab / leaving the page**: the dot loop stops (no rAF running after unmount). Test in
   Task 5.

---

### Task 1: `EnergyView::Orbite` in the API and MCP texts

**Files:**
- Modify: `crates/oikos-api/src/home.rs:27-40` (enum + doc comments), tests in the same file
- Modify: `crates/oikos-os/src/mcp/mod.rs:204`, `crates/oikos-os/src/mcp/tools.rs:67`,
  `crates/oikos-os/src/mcp/params.rs:594` (value lists)

**Interfaces:**
- Produces: `oikos_api::EnergyView::Orbite`, serialised `"orbite"`.

- [ ] **Step 1: Extend the test** `energy_view_is_lowercase_and_refuses_unknown_values`:

```rust
let s: HomeSettings = serde_json::from_value(json!({"energy_view": "orbite"})).unwrap();
assert_eq!(s.energy_view, EnergyView::Orbite);
assert_eq!(serde_json::to_value(EnergyView::Orbite).unwrap(), json!("orbite"));
assert!(serde_json::from_value::<HomeSettings>(json!({"energy_view": "orbit"})).is_err());
```

- [ ] **Step 2:** `cargo test -p oikos-api energy_view` → FAIL (no variant `Orbite`).
- [ ] **Step 3:** Add `/// The double ring. Orbite,` after `Flux`; the field's doc comment and the
  three MCP texts list `classic`, `flux` or `orbite`.
- [ ] **Step 4:** `cargo test -p oikos-api && cargo test -p oikos-os mcp` → PASS (fix any MCP
  snapshot/description test that pins the old text). `cargo check -p oikos-web --target
  wasm32-unknown-unknown` must still pass: any `match` on `EnergyView` in the web crate gets an
  `Orbite` arm that falls back to Flux for now (Task 4 replaces it).
- [ ] **Step 5: Commit** `feat(api): Orbite energy view value`.

### Task 2: One shared `--energy-*` palette, Candy in Ivoire

**Files:**
- Modify: `crates/oikos-web/styles/main.css` (`.energy` block ~4628, `.energy-flux__bar--*` ~4769,
  `.energy-circles` tone block ~4849, its light override ~4869)
- Modify: `crates/oikos-web/src/screens/energy_flux.rs:28-44` (`color_var`)
- Test: `crates/oikos-web/tests/energy.rs`, unit test in `energy_flux.rs`

**Interfaces:**
- Produces: CSS variables `--energy-solar`, `--energy-grid`, `--energy-battery`,
  `--energy-c0` … `--energy-c4`, `--energy-muted`, defined on `.energy, .energy-preview`, Night
  values first, Candy under `:root[data-theme="light"] :is(.energy, .energy-preview)`.
  Later tasks (Orbite, the preview) use only these names.

- [ ] **Step 1: Failing unit test** in `energy_flux.rs` tests:

```rust
#[test]
fn colours_come_from_the_shared_palette() {
    assert_eq!(color_var(SankeyRole::Solar, 0), "--energy-solar");
    assert_eq!(color_var(SankeyRole::Grid, 0), "--energy-grid");
    assert_eq!(color_var(SankeyRole::Export, 0), "--energy-battery");
    assert_eq!(color_var(SankeyRole::Consumer, 6), "--energy-c1");
    assert_eq!(color_var(SankeyRole::Others, 0), "--energy-muted");
    assert_eq!(color_var(SankeyRole::House, 0), "--ink");
}
```

  (Keep Flux's existing export colour: today Export maps to mint, so it stays `--energy-battery`
  in Flux; Orbite draws export in grid colour, see Task 3.)

- [ ] **Step 2: Failing wasm test** `ivoire_uses_the_candy_palette` in `tests/energy.rs`: mount
  Flux with `flux_fixture(1200.0)`; read `getComputedStyle(bar("solar")).fill` → `"rgb(255, 196, 87)"`
  (Night, unchanged); set `document.documentElement` attribute `data-theme="light"`, tick, read
  again → `"rgb(255, 178, 31)"`; same for a Classic `.energy-tone--solar` element's `--tone`
  resolved colour (read `getPropertyValue("--tone")` trimmed or the disc's `stroke`); remove the
  attribute at the end. Note the stylesheet must be loaded in the test page: check how existing
  wasm tests get CSS (if they don't, inject `main.css` + `tokens.css` via `include_str!` into a
  `<style>` in the test).
- [ ] **Step 3: Run** `cargo test -p oikos-web colours_come_from` and the wasm test → FAIL.
- [ ] **Step 4: Implement.** Define the palette (Night: `--solar`, `--accent-violet`,
  `--accent-mint`, today's `--flux-c0..c4` expressions, `--flux-muted`); make `--flux-c*` /
  `--flux-muted` aliases of `--energy-c*` / `--energy-muted` (or replace their uses); point
  `.energy-flux__bar--solar/grid/battery/export/charge/unitemised/others/c*` fills, `color_var`
  and `.energy-circles` `--tone-*` at the shared names; Ivoire `--tone-water: #35b9f0`; keep
  Classic's light `--tone-house: var(--wire)` and `--energy-glow: 0.45`.
- [ ] **Step 5: Run** both tests and the whole `tests/energy.rs` → PASS. Screenshot `/energy`
  Classic + Flux in both themes (CDP recipe, width ≥ 500 px) and compare Night against `main`:
  no visible change.
- [ ] **Step 6: Commit** `feat(web): shared energy palette, Candy colours in Ivoire`.

### Task 3: `energy_orbite` ring geometry (pure)

**Files:**
- Create: `crates/oikos-web/src/energy_orbite.rs` (register in `lib.rs` next to `energy_circles`)

**Interfaces:**
- Consumes: `energy_sankey::{Sankey, SankeyNode, SankeyRole}`.
- Produces:

```rust
pub const GAP_DEG: f64 = 1.6;
pub const MIN_DOT_ARC_DEG: f64 = 6.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ring { Inner, Outer }
#[derive(Clone, Debug, PartialEq)]
pub struct Arc {
    pub id: String,            // the SankeyNode id ("solar", "grid", "export", "battery-charge", "consumer:…", "others", "unitemised", "battery")
    pub ring: Ring,
    pub role: SankeyRole,
    pub slot: usize,           // SankeyNode::slot (consumers)
    pub name: Option<String>,  // SankeyNode::name
    pub watts: f64,            // measured, for labels
    pub size: f64,             // drawn, from SankeyNode::size
    pub share: u8,             // % of its ring's own sum, rounded
    pub a0: f64, pub a1: f64,  // degrees, 0 = top, clockwise, gaps applied
    pub sensor: Option<String>,
    pub count: usize,          // Others only
}
pub struct Rings { pub inner: Vec<Arc>, pub outer: Vec<Arc>, pub total: f64 }
pub fn rings(s: &Sankey) -> Rings;
pub fn colour_var(role: SankeyRole, slot: usize) -> &'static str; // Grid AND Export → "--energy-grid"; Battery AND BatteryCharge → "--energy-battery"; consumers "--energy-c{slot%5}"; Others/Unitemised "--energy-muted"; Solar "--energy-solar"
pub fn arc_path(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> String;
pub fn point(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64);
pub fn dot_speed(watts: f64) -> f64;                 // degrees per second: 10 + 0.9·√W
pub fn arc_dots(arc: &Arc, t_s: f64) -> Vec<(f64, f64)>; // (angle deg, opacity 0..1)
```

  Ring membership by role: Inner = Solar, Battery, Grid (in that order); Outer = Export,
  BatteryCharge, then Consumer nodes in `Sankey::nodes` order, then Others, then Unitemised. House
  is in neither. Nodes with `size <= 0` give no arc. `total = max(Σinner size, Σouter size)`; each
  arc spans `size / total · 360`, minus `GAP_DEG` split on both sides, never below 0.2°. A ring with
  a single arc spanning 360° draws without gaps. `arc_dots`: none for Others/Unitemised or spans
  < `MIN_DOT_ARC_DEG`; `n = max(1, round(span / 42))` dots at `a0 + f·span`,
  `f = (t·speed/span + j/n) mod 1`, opacity `min(1, min(f, 1−f)·span / 6)`.
  `arc_path` for a span ≥ 359.99° returns two half-arcs (so a full ring renders).

- [ ] **Step 1: Write failing tests** (build `Sankey` values through `energy_sankey::build` on an
  `EnergyUi` fixture, as `energy_sankey`'s own tests do):
  - `balanced_rings_line_up`: solar 2140 W, grid 380 W, battery charging 260 W, consumers 1200 /
    540 / 210, rest unitemised → inner ids `["solar","grid"]`, outer ids start
    `["battery-charge", consumer…]` end `"unitemised"`; last inner `a1` and last outer `a1` both ≈
    `360 − GAP_DEG/2` (±0.01).
  - `export_sits_first_and_in_grid_colour`: an exporting fixture → `outer[0].id == "export"` and
    `colour_var(outer[0].role, 0) == "--energy-grid"`.
  - `meters_disagree_the_short_ring_stops_short`: a house sensor above the sources' sum → outer
    ends at ≈ 360 − gap, inner's last `a1` < 350, every `a1 <= 360`.
  - `zero_size_nodes_give_no_arc` and `empty_sankey_gives_no_arcs` (no panic, no NaN, `total`
    finite).
  - `full_ring_path_is_drawn`: `arc_path(170.,170.,104.,0.,360.)` contains two `A` commands and no
    `NaN`.
  - `dots_stay_inside_their_arc` for t in 0..10 s step 0.37: every angle in `[a0, a1]`, opacity in
    `[0,1]`; `arc_dots` empty for an Unitemised arc and for a 5° arc.
  - `consumer_colours_match_flux_slots`: slot 6 → `"--energy-c1"`.
- [ ] **Step 2: Run** `cargo test -p oikos-web energy_orbite` → FAIL (module missing).
- [ ] **Step 3: Implement** `energy_orbite.rs` as specified.
- [ ] **Step 4: Run** → PASS; `cargo clippy -p oikos-web --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(web): Orbite ring geometry`.

### Task 4: `OrbiteDiagram` on the Energy page (static: rings, centre, lists, taps)

**Files:**
- Create: `crates/oikos-web/src/screens/energy_orbite.rs` (register in `screens/mod.rs`)
- Modify: `crates/oikos-web/src/screens/energy.rs:333-405` (view switch + `folded`)
- Modify: `crates/oikos-web/src/i18n/energy.rs` (new words + its `ALL` list at ~269)
- Modify: `crates/oikos-web/styles/main.css` (new `.energy-orbite*` block after the Flux block)
- Test: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: Task 3 (`rings`, `Arc`, `Ring`, `colour_var`, `arc_path`), Task 2 palette,
  `energy_sankey::{build, FOLD_WIDE, FOLD_NARROW}`, `screens::energy_flux::flux_folded`,
  `crate::energy::aria_sentence`, `crate::format::power_text` (as Flux uses it).
- Produces:
  `#[component] pub fn OrbiteDiagram(ui: Memo<EnergyUi>, narrow: bool, open: Callback<String>,
  expanded: RwSignal<bool>) -> impl IntoView` and DOM hooks the tests and Task 5 use:
  root `.energy-orbite` (+ `.energy-orbite--narrow`), `svg.energy-orbite__svg`
  (`role="group"`, `aria-label` = `aria_sentence`), arcs `path.energy-orbite__arc[data-id]`
  (`aria-hidden="true"`, `stroke` = `var(<colour_var>)`), a `<g class="energy-orbite__dots">`
  layer, centre `.energy-orbite__house` / `.energy-orbite__mix`, lists
  `.energy-orbite__list--in` / `--out`, rows `button.energy-orbite__row[data-id]`.
- New `EnergyText` words (EN / FR): `ComingIn` "Coming in" / "Entrant", `GoingOut`
  "Going out" / "Sortant", `InTheHouse` "W in the house" / "W dans la maison", `SolarShareLine`
  "{n} % solar" / "{n} % solaire" (or a format fn, matching how `others()` is done). Row words reuse
  Classic's existing `production` / `import` / `export` / `discharge` / `charge` words.

- [ ] **Step 1: Failing wasm tests** (use `mount_view(…, EnergyView::Orbite)` and `flux_fixture`):
  - `orbite_replaces_the_diagram`: `.energy-orbite__svg` 1, `.energy-flux` 0,
    `.energy-diagram__svg` 0; arcs with `data-id` `solar`, `grid`, `consumer:oven`,
    `consumer:pump`; grid day chart and cards still present.
  - `orbite_rows_show_the_flux_values`: each `.energy-orbite__row` text contains the same
    `power_text` Flux's label shows for that id (mount Flux, read labels, remount Orbite, compare).
  - `orbite_centre_shows_house_and_solar_share`: `.energy-orbite__house` = Flux's "consumed now"
    chip value; `.energy-orbite__mix` contains `%` and "solar".
  - `orbite_opens_a_sensor`: click the `consumer:oven` row → pathname `/entity/oven`; clicking its
    arc does the same.
  - `orbite_others_toggle_the_list`: 10 powered consumers → an `others` row; clicking it flips
    `expanded` (the "show all consumers" list appears).
  - `orbite_flips_grid_to_export_in_place` (Review Focus 3): grid sensor 300 → −500 via the house
    signal → `data-id='grid'` arc gone, `data-id='export'` arc present in the outer ring, exactly one
    arc per id.
  - `orbite_narrow_stacks_lists_under_the_ring` (copy `mount_flux_narrow`'s approach): root has
    `--narrow`; the in-list's top ≥ the svg's bottom.
  - `orbite_follows_the_language`: FR shows "Entrant" / "Sortant".
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome -- --test energy orbite` → FAIL.
- [ ] **Step 3: Implement** the component: `Memo` of `build(u, fold_at(narrow))` then `rings`;
  viewBox `0 0 340 340`, centre 170, inner r 104 / width 14, outer r 136 / width 22, faint
  `--track`-style full-circle tracks (`color-mix(in srgb, var(--ink) 7%, transparent)`), arcs keyed
  by id in `<For>`; centre is HTML over the svg (house glyph from `icons.rs`, value, `InTheHouse`,
  mix line from `Sankey::solar_share` when `Some(n) if n > 0`); rows are `<button>`s keyed by id
  (swatch, name + short word, value, `share` %); hover/focus on a row or arc sets a
  `RwSignal<Option<String>>` hot id → root gets `.energy-orbite--dim`, the pair gets `--hot`. Tap:
  `Others` toggles `expanded`, else `sensor` → `open`. Empty `Sankey` → tracks + the same
  `NoLivePower` text Flux uses. In `energy.rs`, replace the `classic` bool with a match on
  `energy_view(house)` (three arms) and use `flux_folded` for Orbite. CSS: wide grid
  `1fr minmax(260px, 340px) 1fr`, narrow single column; arc glow
  `drop-shadow(0 0 6px currentColor)` Night, `drop-shadow(0 2px 6px color-mix(in srgb,
  currentColor 45%, transparent))` Ivoire; `transition: d .6s, opacity .25s` on arcs for the glide
  (falls back to no glide where `d` transitions are unsupported; reduced motion → `transition:
  none`).
- [ ] **Step 4: Run** the Orbite tests and the whole `tests/energy.rs` → PASS; `cargo test -p
  oikos-web i18n` → PASS (all-keys lists updated).
- [ ] **Step 5:** Screenshot `/energy` with Orbite, wide and 390 px, Night and Ivoire; compare to
  `orbite-v1.html` / `orbite-ivoire-v2.html` panel B.
- [ ] **Step 6: Commit** `feat(web): Orbite energy view`.

### Task 5: Orbite motion (dots loop)

**Files:**
- Modify: `crates/oikos-web/src/screens/energy_orbite.rs`
- Test: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: `energy_orbite::{arc_dots, point}`, `energy_dots::reduced_motion()`, the
  `.energy-orbite__dots` layer from Task 4.
- Produces: `fn run_dots(layer: web_sys::Element, arcs: StoredValue<Vec<(Arc, f64 /*r*/, f64 /*width*/)>>) -> Rc<Cell<bool>>`
  (wasm only), returning a stop flag; follows `energy_dots::run`'s loop shape (one
  `request_animation_frame` chain, skip frames while `document.hidden`, stop when the flag is set).

- [ ] **Step 1: Failing wasm tests:**
  - `orbite_dots_move`: mount Orbite, wait two animation frames → `.energy-orbite__dots circle`
    count > 0; after ~200 ms the first circle's `cx` changed.
  - `orbite_dots_stop_after_unmount` (Review Focus 5): unmount the view (drop the mount handle /
    switch `energy_view` to Flux), wait two frames → no `.energy-orbite__dots` in the document and
    no panic in the console (the loop has stopped).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**: dots are `<circle r = width·0.17>` filled
  `rgb(255 255 255 / 90%)` (Night) / `rgb(255 253 248 / 95%)` (Ivoire) via a CSS variable; the arc
  list in the store updates from the rings memo; set the stop flag in `on_cleanup`; skip starting
  the loop when `reduced_motion()`.
- [ ] **Step 4: Run** → PASS; whole `tests/energy.rs` PASS.
- [ ] **Step 5: Commit** `feat(web): dots run round the Orbite arcs`.

### Task 6: Settings › Appearance: third chip and live preview

**Files:**
- Modify: `crates/oikos-web/src/screens/settings.rs:166-188` (switch), `:576-588` (page)
- Create: `crates/oikos-web/src/screens/energy_preview.rs` (register in `screens/mod.rs`)
- Modify: `crates/oikos-web/src/i18n/settings.rs` (`EnergyViewOrbite` in `ALL` and `SAME`,
  `EnergyPreview`; bump the `ALL.len()` assertion at ~350 from 66 to 68)
- Modify: `crates/oikos-web/styles/main.css` (`.energy-preview*`)
- Test: `crates/oikos-web/tests/settings_updates.rs` (or a new `tests/energy_preview.rs` copying its
  mount helpers)

**Interfaces:**
- Consumes: `energy_view(house)`, `set_energy_view`, Task 2 palette (`.energy-preview` carries
  it), `energy_dots::reduced_motion()`.
- Produces: `#[component] pub fn EnergyPreview(view: Signal<EnergyView>) -> impl IntoView`, root
  `div.energy-preview[data-view="classic|flux|orbite"]` with an `aria-hidden="true"` svg
  (viewBox `0 -8 150 112`) and the caption `p.energy-preview__caption`.
  Words: `EnergyViewOrbite` "Orbite" / "Orbite"; `EnergyPreview` "Preview with sample values" /
  "Aperçu avec des valeurs d'exemple".

- [ ] **Step 1: Failing wasm tests:**
  - `appearance_offers_three_energy_views`: on `/settings/appearance`, the Energy view group has 3
    `.segmented__option` buttons, texts `Classic`, `Flux`, `Orbite`.
  - `choosing_orbite_saves_and_switches_the_preview`: with the fetch stub, click `Orbite` → one PUT
    whose body contains `"energy_view":"orbite"`; `.energy-preview[data-view='orbite']` present.
  - `preview_follows_the_stored_view`: house settings `Flux` → `data-view='flux'`.
  - `preview_is_hidden_from_screen_readers`: the preview svg has `aria-hidden="true"`; caption in
    FR reads "Aperçu avec des valeurs d'exemple".
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**: third `segment(EnergyView::Orbite, T::EnergyViewOrbite)`;
  `EnergyPreview` under the switch, above the hint. Drawings are static SVG per view with fixed
  sample shapes, ported from `settings-preview.html` (`classic()`, `flux()`, `orbite()`): Classic =
  2 source circles + house + 3 consumer circles + 5 curved wires with a dashed overlay animated by
  CSS (`stroke-dasharray: 2 6`, offset −16 over 1.4 s); Flux = 7 bars + 6 bands at 38 % opacity;
  Orbite = two tracks, outer arcs (10/48/21/8/13 %) and inner arcs (85/15 %) via
  `stroke-dasharray`, rotating 360° over 18 s, centre text "2 260". Colours only from
  `--energy-*` (Orbite export/import pairs as in Task 3). Animations are CSS and switched off
  under `prefers-reduced-motion: reduce`.
- [ ] **Step 4: Run** → PASS; `cargo test -p oikos-web i18n` PASS.
- [ ] **Step 5:** Screenshot `/settings/appearance` in both themes; compare to
  `settings-preview.html` option B.
- [ ] **Step 6: Commit** `feat(web): Orbite choice and a live preview in Appearance`.

### Task 7: Docs and full gates

**Files:**
- Modify: `docs/users/web-ui.md:262` (and the Energy page paragraph in the same file, if it lists
  the views)

- [ ] **Step 1:** Update the text: Energy view is **Classic, Flux or Orbite**, with a moving preview
  under the choice; one sentence on Orbite ("the house in the centre, an inner ring for where the
  power comes from and an outer ring for where it goes"); Ivoire draws all three in brighter
  colours.
- [ ] **Step 2: Run all gates** (Global Constraints list) → all green. Also
  `cd website && npm run build` if the docs site build is quick locally (else rely on CI).
- [ ] **Step 3: Commit** `docs(web-ui): Orbite energy view and the Appearance preview`.
- [ ] **Step 4:** Push the branch and open a PR against `main` (title `feat(web): Orbite energy
  view, Candy Ivoire palette, Appearance preview`), body: summary, screenshots, the rollback note
  from the spec (an older hub ignores a stored `"orbite"` home settings document), follow-ups
  (docs-site screenshot of Orbite).
