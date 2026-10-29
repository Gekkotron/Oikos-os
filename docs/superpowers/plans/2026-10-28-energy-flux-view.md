# Energy Flux (Sankey) View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A second live diagram for the web `/energy` page, a Sankey called **Flux**, chosen next to the theme in Settings › Appearance, with today's diagram kept as **Classic** (the default).

**Architecture:** A small setting module mirrors `theme.rs` (enum + `localStorage` + context signal). A pure model (`energy_sankey.rs`) turns the existing `EnergyUi` into balanced nodes and links; a pure geometry module (`sankey_layout.rs`) places bars, band paths and labels for a horizontal (wide) or vertical (narrow) frame. A Leptos component (`screens/energy_flux.rs`) renders that geometry as keyed inline SVG so CSS transitions animate changes; `EnergyScreen` picks Classic or Flux from the setting.

**Tech Stack:** Rust, Leptos (CSR, wasm32), inline SVG, CSS in `crates/oikos-web/styles/main.css`, `wasm-bindgen-test` for browser tests.

**Spec:** `docs/superpowers/specs/2026-10-28-energy-flux-view-design.md`

## Global Constraints

- Web crate only (`crates/oikos-web`); no change to `oikos-core`, the Android app, or the `energy-stat` dashboard widget.
- Classic is the default and is not changed visually or behaviourally.
- Setting persisted in `localStorage` key `oikos.energy_view`, values `classic` / `flux`; missing or unknown = Classic; every storage access fallible and silent (a private window must never panic).
- No new dependency, no chart library; inline SVG.
- Every user-visible word goes through `src/i18n/` (EN + FR); the `bare_text` test must stay green without adding `file:` lines to `bare_text_allow.txt`.
- Colours only from `styles/tokens.css` variables, so Night Glass and Ivoire both work.
- Nothing house-specific in code, tests, docs (no real device or room names; tests use generic names such as "Oven", "Heat pump").
- `IDLE_W` (10 W, `oikos_core::energy::IDLE_W`) is the threshold below which links, sources, sinks and consumers are not drawn.
- Fold: wide screens fold when more than **8** powered consumers (show 7 + "+N others"); narrow (< 600 px, `Layout::NARROW`) fold when more than **4** (show 3 + "+N others").
- The sweep animation is off under `prefers-reduced-motion: reduce`.
- Commits as `Gekkotron` (`git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Before each commit: `cargo fmt --all`, `cargo clippy -p oikos-web --all-targets -- -D warnings`, `cargo check -p oikos-web --target wasm32-unknown-unknown`.

## Review Focus

1. Night: solar sensor unavailable (inverter offline), grid importing → no solar node, house fed by the grid only, no panic. Test `night_with_solar_offline` in Task 2.
2. Battery discharging into export with no solar → battery → export link, battery → house gets the rest. Test `battery_feeds_export_without_solar` in Task 2.
3. Measured house sensor lower than what flows in (meter lag) → house bar = the larger figure, Not itemised never negative. Test `house_meter_lower_than_inflow` in Task 2.
4. A consumer's value changes and the consumer order is reshuffled → the same SVG elements are updated (keyed by id), not recreated, so transitions run. wasm test `flux_updates_in_place` in Task 4.
5. Twelve powered consumers on a phone → 3 shown + "+9 others", Not itemised still balances. Test `narrow_folds_at_four` in Task 2.

---

### Task 1: Energy view setting

**Files:**
- Create: `crates/oikos-web/src/energy_view.rs`
- Modify: `crates/oikos-web/src/lib.rs` (add `pub mod energy_view;` in alphabetical order after `energy_day`)
- Modify: `crates/oikos-web/src/app.rs:131-132` (install and provide next to the theme)
- Modify: `crates/oikos-web/src/screens/settings.rs` (`EnergyViewSwitch` + a second `Panel` in `AppearancePage`, ~line 384)
- Modify: `crates/oikos-web/src/i18n/settings.rs` (new variants, their EN/FR text, and the `ALL` list)

**Interfaces:**
- Produces:
  - `pub enum EnergyView { Classic, Flux }` (`Clone, Copy, Debug, PartialEq, Eq, Default` with `Classic` default)
  - `impl EnergyView { pub fn as_str(self) -> &'static str }` → `"classic"` / `"flux"`
  - `pub fn parse(value: Option<&str>) -> EnergyView`
  - `pub fn install_energy_view() -> RwSignal<EnergyView>` (provided as context in `App`)
  - `SettingsText::{EnergyView, EnergyViewHint, EnergyViewClassic, EnergyViewFlux}`

- [ ] **Step 1: Write the failing tests** in `energy_view.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn parses_known_values() {
    assert_eq!(parse(Some("classic")), EnergyView::Classic);
    assert_eq!(parse(Some("flux")), EnergyView::Flux);
    assert_eq!(parse(None), EnergyView::Classic);
    assert_eq!(parse(Some("orbit")), EnergyView::Classic);
}
#[test]
fn serialises_known_values() {
    assert_eq!(EnergyView::Classic.as_str(), "classic");
    assert_eq!(EnergyView::Flux.as_str(), "flux");
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib energy_view` — expected: FAIL (module missing).

- [ ] **Step 3: Implement** `energy_view.rs` mirroring `theme.rs`: `STORAGE_KEY = "oikos.energy_view"` (wasm32 only), `storage()` the same fallible way, `install_energy_view` reads the stored value on wasm32 (Classic natively) and an `Effect` writes it back on change. No DOM attribute to apply.

- [ ] **Step 4: Settings UI.** In `settings.rs` add `EnergyViewSwitch` built exactly like `ThemeSwitch` (`segmented` group, `chip segmented__option` buttons, `aria-pressed`), labelled by `settings_text(lang, T::EnergyViewClassic/Flux)`, `aria-label` = `T::EnergyView`. `AppearancePage` gets a second `<Panel title=T::EnergyView>` with the switch and `<p class="settings__hint">` = `T::EnergyViewHint`. Text (add to `ALL` too):
  - `EnergyView` → ("Energy view", "Vue énergie")
  - `EnergyViewHint` → ("How the Energy page draws the live power.", "Comment la page Énergie dessine la puissance en direct.")
  - `EnergyViewClassic` → ("Classic", "Classique")
  - `EnergyViewFlux` → ("Flux", "Flux")
  `HintAppearance` becomes ("Theme, energy view", "Thème, vue énergie"). The Appearance status line stays the theme name.

- [ ] **Step 5: App.** In `app.rs` after `provide_context(theme);`: `provide_context(crate::energy_view::install_energy_view());`.

- [ ] **Step 6: Run** `cargo test -p oikos-web` — expected: PASS, including the i18n table tests and `bare_text`.

- [ ] **Step 7: Commit** `feat(web): energy view setting in Appearance` (stage the five files).

---

### Task 2: Sankey model

**Files:**
- Create: `crates/oikos-web/src/energy_sankey.rs` (+ `pub mod energy_sankey;` in `lib.rs`)

**Interfaces:**
- Consumes: `oikos_core::energy::{EnergyUi, EnergyNode, NodeKind, Flow, NodeStatus, IDLE_W}`; `EnergyUi::node(id)` and `EnergyUi::consumers`.
- Produces:

```rust
pub const FOLD_WIDE: usize = 8;
pub const FOLD_NARROW: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SankeyRole { Solar, Grid, Battery, House, Export, BatteryCharge, Consumer, Others, Unitemised }

#[derive(Clone, Debug, PartialEq)]
pub struct SankeyNode {
    pub id: String,          // "solar" "grid" "battery" "house" "export" "battery-charge" "others" "unitemised", or the consumer's EnergyNode id
    pub role: SankeyRole,
    pub column: u8,          // 0 sources, 1 house + sinks, 2 consumers
    pub name: Option<String>,// consumer name; None for the others (UI words)
    pub watts: f64,          // what the label shows: the measured value (Others/Unitemised: their sum)
    pub size: f64,           // what the bar is drawn for: max(in, out) of drawn links, in watts
    pub sensor: Option<String>, // the single sensor id to open, if exactly one
    pub count: usize,        // Others only: how many consumers it folds
    pub slot: usize,         // Consumer only: 0-based position among shown consumers (colour rotation)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLink { pub from: String, pub to: String, pub watts: f64 }

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Sankey {
    pub nodes: Vec<SankeyNode>, // column 0 then 1 then 2, top to bottom within a column
    pub links: Vec<SankeyLink>, // ordered: by source node order, then target node order
    pub house_w: Option<f64>,
    pub solar_w: Option<f64>,   // measured production, None without a live solar node
    pub import_w: f64,
    pub export_w: f64,
    pub solar_share: Option<u8>,// 0..=100, rounded down
    pub folded: bool,           // an Others node exists
}

impl Sankey { pub fn is_empty(&self) -> bool } // no node with size >= IDLE_W
pub fn build(ui: &EnergyUi, fold_at: usize) -> Sankey
```

Allocation (the algorithm the tests pin; values under `IDLE_W`, `None` or `NodeStatus::Unavailable` count as 0):

```text
S = solar.value                         G = grid.value if grid.flow == In   else 0
B = battery.value if flow == In else 0  E = grid.value if grid.flow == Out  else 0
C = battery.value if flow == Out else 0 H = house.value (Option)
s,g,b = S,G,B
solar→export   = min(s, E);              s -= it
battery→export = min(b, E - solar→export); b -= it
solar→charge   = min(s, C);              s -= it
grid→charge    = min(g, C - solar→charge); g -= it
solar→house = s; grid→house = g; battery→house = b
house = H.unwrap_or(s + g + b)
shown = consumers (EnergyUi::consumers order) with value >= IDLE_W
if shown.len() > fold_at: keep first fold_at-1, Others = sum/count of the rest
unitemised = max(0, house - sum(shown kept) - others)
house.size = max(inflow, house, outflow)
drop links < IDLE_W; source size = sum of its drawn out-links; sink size = sum of in-links
solar_share = floor(solar→house / house * 100) when a solar node exists and house > 0
```

Column order: 0 = solar, grid, battery; 1 = house, battery-charge, export; 2 = shown consumers, others, unitemised.

- [ ] **Step 1: Write the failing tests** in `energy_sankey.rs` `mod tests`. Build `EnergyUi` values directly (helper `ui(nodes: &[EnergyNode], consumers: &[EnergyNode]) -> EnergyUi` and `node(id, kind, watts, flow) -> EnergyNode` filling the other fields with defaults). Tests and their assertions:
  - `import_only`: grid 500 In, house 500, consumers Oven 300, Lamp 5 → links grid→house 500, house→oven 300, house→unitemised 200; no lamp node; `import_w == 500.0`, `solar_share == None`.
  - `solar_surplus_exports`: solar 3000, grid 1200 Out, house 1800, consumer Heater 900 → solar→house 1800, solar→export 1200; export node column 1, size 1200; unitemised 900; `solar_share == Some(100)`.
  - `charge_from_solar_then_grid`: solar 300, grid 400 In, battery 500 Out, house 200 → solar→charge 300, grid→charge 200, grid→house 200; `solar_share == Some(0)`.
  - `battery_feeds_export_without_solar`: battery 800 In, grid 300 Out, house 500 → battery→export 300, battery→house 500.
  - `night_with_solar_offline`: solar `None` with `NodeStatus::Unavailable`, grid 250 In, house 250 → no "solar" node, grid→house 250, `solar_w == None`.
  - `house_meter_lower_than_inflow`: grid 1000 In, house 900, consumer Oven 950 → house node size 1000, `watts == 900.0`, no unitemised node (never negative).
  - `wide_folds_after_eight`: house 5000, nine consumers 500..100 W → 7 consumer nodes + Others with `count == 2`; `folded`.
  - `narrow_folds_at_four`: twelve consumers of 100 W, house 1500, `build(&ui, FOLD_NARROW)` → 3 consumers + Others `count == 9`, `watts == 900.0`, unitemised 300.
  - `every_drawn_node_balances`: for the fixtures above, for each column-1 node: in-links sum ≤ size and out-links sum ≤ size; for sources: out sum ≤ measured watts.
  - `nothing_known_is_empty`: only a house node with `value: None` → `is_empty()`.

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib energy_sankey` — expected: FAIL.

- [ ] **Step 3: Implement** `build` per the allocation above.

- [ ] **Step 4: Run** the same command — expected: PASS. Then `cargo clippy -p oikos-web --all-targets -- -D warnings`.

- [ ] **Step 5: Commit** `feat(web): Sankey model of the live energy flow`.

---

### Task 3: Sankey geometry

**Files:**
- Create: `crates/oikos-web/src/sankey_layout.rs` (+ `pub mod sankey_layout;` in `lib.rs`)

**Interfaces:**
- Consumes: `Sankey`, `SankeyNode`, `SankeyLink` from Task 2.
- Produces:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame { pub w: f64, pub h: f64, pub vertical: bool }
pub const WIDE: Frame = Frame { w: 980.0, h: 460.0, vertical: false };
pub const NARROW: Frame = Frame { w: 360.0, h: 620.0, vertical: true };
pub const BAR: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor { Start, Middle, End }

#[derive(Clone, Debug, PartialEq)]
pub struct Bar { pub id: String, pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
#[derive(Clone, Debug, PartialEq)]
pub struct Band { pub id: String, pub from: String, pub to: String, pub path: String, pub x0: f64, pub y0: f64, pub x1: f64, pub y1: f64 } // x0..y1: gradient axis (bar edge to bar edge, centre line)
#[derive(Clone, Debug, PartialEq)]
pub struct Label { pub id: String, pub x: f64, pub y: f64, pub anchor: Anchor }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Placed { pub bars: Vec<Bar>, pub bands: Vec<Band>, pub labels: Vec<Label> }

pub fn place(s: &Sankey, frame: Frame) -> Placed
```

Rules: band id = `"{from}>{to}"`. Main axis = x when horizontal, y when vertical; the three columns sit at 17 %, 49 %, 79 % of the main axis (wide) and 6 %, 42 %, 78 % (narrow). Cross-axis space = frame cross size minus 30 px top/bottom margin (wide) or 16 px (narrow); gap 20 px (wide) / 12 px (narrow) between bars; one scale `k = min over columns of (space - gap*(n-1)) / sum(size)`; bar length = `max(3.0, size*k)`; a column is centred on the cross axis. Bands leave the source bar's far edge and enter the target bar's near edge, stacked from the bar's top (left, in vertical) in link order, thickness `max(1.5, watts*k)`. Path, horizontal (swap x and y for vertical), always these five commands so CSS can interpolate it:

```text
M x0 y0 C xm y0 xm y1 x1 y1 L x1 y1+t C xm y1+t xm y0+t x0 y0+t Z
```

Labels: wide: column 0 → `End`, 14 px before the bar, vertically centred; column 2 and column-1 sinks → `Start`, 14 px after; house → `Middle`, 12 px above. Narrow: every label `Middle`, 18 px below its bar (house: above). Format numbers with at most 1 decimal.

- [ ] **Step 1: Write the failing tests** (`mod tests`, reusing a `Sankey` built by hand):
  - `paths_keep_one_shape`: every `Band::path` starts with `"M "`, ends with `" Z"`, contains exactly 2 `" C "`/`"C "` commands and 1 `"L "`.
  - `bars_fit_the_frame`: every bar inside `0..=w` × `0..=h` for `WIDE` and `NARROW`.
  - `bands_never_outgrow_their_bar`: per bar, the sum of leaving band thicknesses ≤ bar length + 0.01, same for entering.
  - `columns_sit_on_the_main_axis`: wide: column-0 bars share one `x`; vertical: they share one `y`.
  - `vertical_swaps_axes`: in `NARROW`, the house bar is wider than tall.
  - `labels_follow_the_side_rules`: wide solar label `End` and its `x` < solar bar `x`; consumer label `Start` and `x` > bar `x + BAR`.

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib sankey_layout` — expected: FAIL.
- [ ] **Step 3: Implement** `place`.
- [ ] **Step 4: Run** — expected: PASS; clippy clean.
- [ ] **Step 5: Commit** `feat(web): Sankey geometry for wide and narrow frames`.

---

### Task 4: Flux diagram on the Energy page (wide)

**Files:**
- Create: `crates/oikos-web/src/screens/energy_flux.rs` (+ `pub mod energy_flux;` in `screens/mod.rs`)
- Modify: `crates/oikos-web/src/screens/energy.rs` (choose Classic or Flux; `folded` per view)
- Modify: `crates/oikos-web/src/i18n/energy.rs` (new words + `ALL`)
- Modify: `crates/oikos-web/styles/main.css` (`.energy-flux*` next to `.energy-diagram`)
- Test: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: `energy_view::EnergyView` (context `RwSignal<EnergyView>`; read with `use_context` and fall back to Classic so existing mounts keep working), `energy_sankey::{build, Sankey, FOLD_WIDE, FOLD_NARROW}`, `sankey_layout::{place, WIDE, NARROW, Placed}`, `energy::power_text`.
- Produces: `#[component] pub fn FluxDiagram(ui: Memo<EnergyUi>, narrow: bool, open: Callback<String>, expanded: RwSignal<bool>) -> impl IntoView` and `pub fn flux_folded(ui: &EnergyUi, narrow: bool) -> bool`.

New `EnergyText` words (EN, FR): `Export` ("To the grid", "Vers le réseau"), `BatteryCharging` ("Battery charging", "Charge batterie"), `NodeSolar` ("Solar", "Solaire") and `NodeBattery` ("Battery", "Batterie") only if `node_label` has no such word already (reuse it if it does), `NodeHouse` likewise, `Unitemised` ("Not itemised", "Non détaillé"), `ChipConsumed` ("consumed now", "consommés maintenant"), `ChipSolar` ("solar production", "production solaire"), `ChipFromGrid` ("from the grid", "tirés du réseau"), `ChipToGrid` ("sent to the grid", "renvoyés au réseau"), `ChipSolarShare` ("solar share", "part solaire"), `NoLivePower` ("No live power reading yet", "Pas encore de mesure de puissance"). Helper `pub fn others(lang: Lang, n: usize) -> String` → "+3 others" / "+3 autres".

- [ ] **Step 1: Write the failing wasm tests** in `tests/energy.rs` with a `mount_view(entities, EnergyView)` helper that also provides `RwSignal::new(view)`; fixture: `solar` 2000 W, `grid` 300 W (import), two consumers "Oven" 1200 W and "Heat pump" 600 W:
  - `classic_is_the_default`: `mount` (no view context) → `.energy-diagram__svg` present, `.energy-flux` absent.
  - `flux_replaces_the_diagram`: Flux → `.energy-flux__svg` present, `.energy-diagram__svg` absent; bars with `data-id` `solar`, `grid`, `house`, and the two consumer ids; the chips row (`.energy-flux__chips`) has 4 chips and contains "consumed now"; the day chart and the cards are still rendered.
  - `flux_opens_a_sensor`: clicking the `Oven` bar navigates to `/entity/<oven id>` (same check the Classic node test uses).
  - `flux_updates_in_place`: keep a handle on the `Oven` bar element, apply a `WsEvent::EntityUpserted` with Oven at 400 W, `tick().await`, query again → `is_same_node` true and the `style` height changed.

- [ ] **Step 2: Run** `cd crates/oikos-web && wasm-pack test --headless --chrome -- --test energy` — expected: the new tests FAIL.

- [ ] **Step 3: Implement `FluxDiagram`.** `let model = Memo::new(move |_| ui.with(|u| build(u, if narrow { FOLD_NARROW } else { FOLD_WIDE })))`, `let placed = Memo::new(move |_| model.with(|m| place(m, if narrow { NARROW } else { WIDE })))`. Render:
  - `<div class="energy-flux__chips">` four chips (`<div class="energy-flux__chip"><b>value</b><span>word</span></div>`): house via `power_text`; solar (left out when `solar_w` is `None`); grid: `ChipToGrid` with "↗ " + `export_w` while exporting, else `ChipFromGrid` + `import_w`; solar share "N %" (left out when `None`).
  - `<svg class="energy-flux__svg" viewBox="0 0 {w} {h}" role="img" aria-label=aria_sentence(..)>` with `<title>`; `<defs>` holding one `linearGradient` per band (`id = "flux-g-" + sanitised band id`, `gradientUnits="userSpaceOnUse"`, `x1,y1,x2,y2` from `Band::x0..y1`, two stops coloured by the source and target node colours; links into the house take the source colour at both ends) and one shared sweep gradient `flux-sweep` animated with `<animateTransform attributeName="gradientTransform" type="translate" … dur="3.2s" repeatCount="indefinite">`.
  - Each layer is a `<For each=… key=|id| id.clone()>` over ids; the child reads its own entry from `placed`/`model` reactively and sets geometry through `style` (`d: path("…")` on bands, `x/y/width/height` on `<rect class="energy-flux__bar" rx="4">`, `transform: translate(…px, …px)` on `<text class="energy-flux__label">`), so updates reuse the element. Every bar carries `data-id`.
  - Label text: `<tspan class="energy-flux__value">` power + `<tspan class="energy-flux__name" dx="6">` name (consumer name truncated like Classic's `short`, role words from `EnergyText`, Others via `others()`); for `Anchor::End` the name comes first.
  - Colours per role through classes: `energy-flux__bar--solar|grid|battery|house|export|charge|unitemised|others` and `--c0…--c4` for consumers (`slot % 5`).
  - Bar or label click: `sensor` → `open.run(id)`; Others → `expanded.update(|e| *e = !*e)`. Clickable items get `tabindex="0"`, `role="button"`, and Enter does the same.
  - `model.is_empty()` → `<p class="energy-flux__empty">` `NoLivePower` instead of the svg (chips still shown when they have values).

- [ ] **Step 4: Wire the screen.** In `EnergyScreen`: `let view = use_context::<RwSignal<EnergyView>>().map(|s| s.read_only()).unwrap_or_else(|| RwSignal::new(EnergyView::Classic).read_only());` and `narrow = layout == Layout::NARROW`. Wrap the existing `.energy-diagram` block in a `Show`/branch on `view.get() == EnergyView::Classic`; otherwise `<FluxDiagram ui narrow open expanded />`. `folded` becomes Classic: `consumers.len() > ARC_SLOTS`; Flux: `flux_folded(&ui, narrow)`.

- [ ] **Step 5: CSS** in `main.css` after the `.energy-diagram` rules: `.energy-flux__svg` (full width, auto height); `.energy-flux__band { fill-opacity: .42; transition: d .9s var(--ease), fill-opacity .2s }` with hover/focus `.8`; `.energy-flux__sweep { pointer-events: none }`; `.energy-flux__bar { transition: x .9s, y .9s, width .9s, height .9s, opacity .4s }` with role colours from tokens (solar `--solar`, grid `--accent-violet`, battery/export/charge `--accent-mint`, house `--ink`, unitemised and others a muted mix of `--ink`), `--c0..4` a rotation of the theme accents; `.energy-flux__label { transition: transform .9s var(--ease) }`, value bold `--ink`, name `--muted`; chips like the mockup (`border-radius: 18px`, background `color-mix(in srgb, var(--ink) 8%, transparent)`). `@media (prefers-reduced-motion: reduce)` hides `.energy-flux__sweep` and sets the transitions to `none`.

- [ ] **Step 6: Run** the wasm tests (Step 2 command) — expected: PASS; then `cargo test -p oikos-web` (i18n tables, `bare_text`) — PASS.

- [ ] **Step 7: Look at it.** `trunk serve` per `docs/contributors/development.md` (or the fixtures dev loop there), switch Settings › Appearance › Energy view to Flux, check both themes and that values glide. Screenshot for the PR.

- [ ] **Step 8: Commit** `feat(web): Flux (Sankey) live energy view`.

---

### Task 5: Narrow screens

**Files:**
- Modify: `crates/oikos-web/src/screens/energy_flux.rs`, `crates/oikos-web/styles/main.css`
- Test: `crates/oikos-web/tests/energy.rs`

**Interfaces:**
- Consumes: `NARROW` frame and `FOLD_NARROW` (Tasks 2–3), the `narrow` prop (Task 4).

- [ ] **Step 1: Write the failing wasm test** `flux_narrow_runs_top_to_bottom`: mount `FluxDiagram` directly with `narrow: true` and six consumers → the `viewBox` is `0 0 360 620`, the house bar is wider than tall, 3 consumer bars + an Others bar, no element overflows the svg's viewBox.
- [ ] **Step 2: Run** it — expected: FAIL (labels/sizes not tuned for narrow yet, or the test's overflow check fails).
- [ ] **Step 3: Implement** the narrow details: labels `Middle` under each bar on two lines (value, then name truncated at Classic's `LABEL_CHARS`), smaller font sizes under a `.energy-flux--narrow` class, chips wrap (they already flex-wrap).
- [ ] **Step 4: Run** the wasm tests — expected: PASS. Check at 390 px width in the browser.
- [ ] **Step 5: Commit** `feat(web): Flux view top to bottom on phones`.

---

### Task 6: Docs and final check

**Files:**
- Modify: `docs/users/web-ui.md` (Energy section ~line 188; Appearance line ~line 260)

- [ ] **Step 1: Docs.** Energy: one paragraph after the Classic description: "**Flux**, chosen in Settings › Appearance, draws the same live power as a Sankey: sources on the left (solar, grid import, battery discharge), the house with export and battery charging in the middle, every consumer on the right with *Not itemised* for the rest; band thickness is the power, more than eight consumers fold into *+N others* (four on a phone), four chips above sum it up, and tapping a bar opens its sensor." Appearance: "the theme switch (Auto, Night Glass, Ivoire), the same choice as the header's theme button, and the **Energy view** (Classic, Flux), kept per browser."
- [ ] **Step 2: Full verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cd crates/oikos-web && wasm-pack test --headless --chrome` — all green.
- [ ] **Step 3: Commit** `docs: Flux energy view`.
