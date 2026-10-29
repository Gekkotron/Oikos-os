# Energy view: Orbite (double ring) and a preview in Settings — design

Date: 2026-10-29. Status: approved in brainstorming; awaiting written-spec review.

## Why

The web `/energy` page has two live views, chosen in Settings › Appearance › Energy view and
saved hub-wide in the home settings document (`energy_view`, `oikos_api::EnergyView`):
**Classic** (glowing circles, `screens/energy.rs` + `energy_circles.rs`, PR #88) and **Flux**
(the Sankey, `screens/energy_flux.rs`, PR #84). The 2026-10-28 brainstorm offered three styles —
A Orbite, B Flux, C circles — and B and C are built. This adds the third, **Orbite**, and a small
moving **preview** of the chosen view in Settings so the user can see what each looks like
before leaving the page.

Both looks were picked from live mockups (untracked):
`.superpowers/brainstorm/25437-1793282101/content/orbite-v1.html` (the ring, Night Glass, wide and
phone), `orbite-ivoire-v2.html` (palette **B · Candy** picked for Ivoire) and
`settings-preview.html` (option **B · chips + one live preview** picked). They are references for
look and motion, not code to port.

## Goals

- A third choice **Orbite** (FR **Orbite**) in Settings › Appearance › Energy view, after Classic
  and Flux, stored as `energy_view: "orbite"`. Classic stays the default; missing = Classic.
- With Orbite chosen, `/energy` draws the double ring in place of the Classic / Flux diagram.
  Everything else on the page is unchanged and in the same order (title, subtitle, check-sign
  hint, "show all consumers" toggle, grid day chart, cards, list).
- Same information as Flux, from the same model: `energy_sankey::build(ui, fold_at(narrow))`.
- Under the Energy view chips in Settings, one live preview of the chosen view, drawn from
  fixed sample values.

## Non-goals

- No change to `oikos_core::energy`, `EnergyFlow::build`, `energy_sankey::build`, the cards, the
  grid day chart or the list.
- No change to how Classic and Flux look in Night Glass, nor to their shapes or motion. (In Ivoire
  they do change colour: see **Colours**.)
- No Android app change.

## Anti-goals (a failure even if it works)

- The two rings that do not add up: inner and outer rings must span the same total, so arcs line
  up.
- A number on the ring or in the lists that differs from what Flux shows for the same reading.
- Motion that cannot be stopped: `prefers-reduced-motion: reduce` stops the dots and the gliding.
- Ivoire that looks grey ("sad colours" was the user's reaction to the app's muted tokens).
- The Settings preview showing the house's live data or making requests.

## What Orbite looks like

### The ring

- One square SVG (viewBox 340 × 340 in the mockup), centre = the house.
- **Inner ring** (thinner, mockup r 104, width 14): **where power comes from**, clockwise from the
  top: Solar (column-0 `solar` node), Battery discharge (`battery`), Grid import (`grid`).
- **Outer ring** (thicker, mockup r 136, width 22): **where it goes**, clockwise from the top: Grid
  export (`export`), Battery charging (`battery-charge`), then each shown consumer in
  `Sankey::nodes` order, then **+N others** (`others`) and **Not itemised** (`unitemised`).
  Pick each ring's arcs **by role** (`SankeyRole`), not by raw position in `Sankey::nodes`:
  column 1 holds the house and the sinks in whatever order `build` pushes them. Only the
  consumers keep their `nodes` order.
- Each arc's angle = its watts / T × 360°, with one **T** for both rings: T = max(sum of inner,
  sum of outer). When the meters disagree, the shorter ring simply stops short and its track shows
  the rest; power is never invented to close it (my call; the anti-goal below is about the
  balanced case drawing wrong). This ordering lines the arcs up with the allocation rules (export is fed by solar
  first, charging by what solar has left): the solar arc sits over export and charging.
- Which watts an arc uses: the Sankey node's `size` (what its bar is drawn for), so the rings follow
  the same balancing Flux does. Labels show the node's measured `watts`, as Flux's do.
- Small gaps between arcs (mockup 1.6°), butt caps, a faint full-circle track behind each ring.
- **Centre**: a house icon, the house watts large (`Sankey::house_w`), the unit line
  ("W in the house" / FR "W dans la maison"), and `xx % solar` in the solar colour when the solar
  share is above 0: `Sankey::solar_share` (`Option<u8>`, rounded down), the same figure Flux's chip
  shows (`screens/energy_flux.rs`). Hidden when `None` or 0.
- When nothing flows (all zero / `Sankey::is_empty`): both tracks, centre shows the house value or
  "—", no arcs, no dots — the same empty case Flux handles.

### The lists

- Two lists, **Coming in** (FR **Entrant**) and **Going out** (FR **Sortant**). Each row: colour
  swatch, name (+ the short word Classic uses: `production`, `↘ import`, `↗ export`,
  `↘ discharge`, `↗ charge`), the value in W, and its share of its ring in %.
- Wide: three columns, Coming in | ring | Going out (inner list right-aligned heading). Narrow
  (the same `narrow()` breakpoint the screen already uses): ring on top, then Coming in, then
  Going out.
- Rows for nodes with no power are not shown.

### Colours

"Colour means place": grid is one colour on both rings (import inside, export outside), battery
one colour on both.

| Role | Night Glass | Ivoire (Candy) |
|---|---|---|
| Solar | `--solar` (`#ffc457`) | `#ffb21f` |
| Grid (import + export) | `--accent-violet` (`#8f7bff`) | `#9a7bff` |
| Battery (discharge + charge) | `--accent-mint` (`#6ff0bf`) | `#2fd3a0` |
| Consumer slot 0 | today's `--flux-c0` | `#35b9f0` |
| Consumer slot 1 | today's `--flux-c1` | `#ff6f91` |
| Consumer slot 2 | today's `--flux-c2` | `#7f9cff` |
| Consumer slot 3 | today's `--flux-c3` | `#ff9a55` |
| Consumer slot 4 | today's `--flux-c4` | `#3fd6c6` |
| Not itemised / +N others | today's `--flux-muted` | `#d9d1c3` |

**One palette for the three live views.** The user asked for Candy in Classic and Flux too, so the
table is not Orbite's own: it becomes a shared set of `--energy-*` variables (`--energy-solar`,
`--energy-grid`, `--energy-battery`, `--energy-c0`…`--energy-c4`, `--energy-muted`) defined on
`.energy` (and on the Settings preview box), with Night Glass values equal to what the views use
today (no visible change in Night) and the Candy values under `:root[data-theme="light"]`.
Classic's `--tone-*` (`.energy-circles`, incl. `--tone-water` → Candy cyan `#35b9f0` in Ivoire),
Flux's `--flux-c*` / `--flux-muted` and `color_var` (which today names `--solar`,
`--accent-violet`, `--accent-mint` directly) and Orbite all read these. The rest of the page (grid
day chart, cards, tariff colours) and the app's global tokens stay as they are; only the three
diagrams move to Candy. Classic keeps its Ivoire house tone (`--wire`) and its halo strength
(`--energy-glow: 0.45`); Flux gets the colours only, no glow (my calls).

Consumer slots rotate exactly as Flux's do (`slot % 5`, `screens/energy_flux.rs` `color_var`), so a
consumer has the same slot in both views; in Night Glass the five consumer colours may alias
today's `--flux-c0`…`--flux-c4` values outright (my call).

Glow: Night Glass arcs get `drop-shadow(0 0 6px currentColor)`; Ivoire arcs get a softer coloured
shadow, `drop-shadow(0 2px 6px color-mix(in srgb, currentColor 45%, transparent))`. Text stays in
the theme ink.

### Motion

- **Dots**: small light dots run clockwise round each arc (about one per 42° of arc), speed
  growing with the arc's watts (mockup: 10 + 0.9·√W degrees per second), fading in/out near the
  arc ends. None on Not itemised / +N others, none on arcs shorter than ~6°. `energy_dots.rs`'s
  `DotStore` works on wire curves, not arcs: add a separate, small arc-dot store for Orbite that
  shares only the approach (one rAF loop, no per-dot timers, stops when the page is hidden or the
  view unmounts) and `reduced_motion()`. Do not generalise `DotStore`; Classic stays untouched
  (my call).
- **Glide**: when readings change, arcs ease to their new angles (no jump), as Classic's mix ring
  does.
- `prefers-reduced-motion: reduce`: no dots, no glide (read the same way Classic reads it).

### Behaviour

- Tapping a row or an arc of a node with one sensor (`SankeyNode::sensor`) opens it with the
  screen's existing `open` callback, as Flux does. Tapping **+N others** toggles `expanded`, as
  Flux's "others" does.
- Hovering (or keyboard focus on) a row or an arc highlights that pair and dims the others.
- Rows are buttons (keyboard reachable); the SVG has the same accessible sentence Flux gives
  (`aria_sentence`), the arcs themselves are `aria-hidden`.
- Fold: same as Flux, `fold_at(narrow)` (8 wide, 4 narrow), and the screen's `folded` check uses
  `flux_folded` for Orbite too.

## The Settings preview

- On `/settings/appearance`, in the Energy view panel: the three chips (Classic / Flux / Orbite)
  stay as they are; under them, a box (rounded, faintly tinted, full panel width) holding a
  **small moving drawing of the chosen view**, captioned **Preview with sample values** (FR
  **Aperçu avec des valeurs d'exemple**); the existing hint stays under it.
- Tapping a chip saves at once, as today, and the preview switches with it.
- The drawing is a small, hand-drawn **schematic** per view (see `settings-preview.html`; that
  mockup is the reference for size, colours and motion, its exact numbers are not binding), not the
  real diagram fed with fake data: Classic = two source circles, the house, three consumer circles
  and curved wires with moving dashes; Flux = bars and bands; Orbite = the two rings with a slow
  rotation and a house value in the middle. Fixed sample values, no labels apart from Orbite's
  centre value. It follows the theme (the shared `--energy-*` palette, Candy in Ivoire), and is `aria-hidden`
  (the chip label is the accessible name).
- Reduced motion: still drawing.
- (Builder may choose to render the real components with a sample `EnergyUi` instead, if that is
  simpler and stays small; the look must match the mockup's size and calm.)

## Data, API and compatibility

- `oikos_api::EnergyView` gains `Orbite` (serialised `"orbite"`, `rename_all = "lowercase"`
  already). Update the doc comments, the API test (`"orbite"` accepted; an unknown value such as
  `"orbit"` still refused) and the MCP texts that list the values (`crates/oikos-os/src/mcp/mod.rs`,
  `tools.rs`, `params.rs`): `classic`, `flux` or `orbite`.
- Rollback risk: a hub older than this change reading a stored `"orbite"` logs "ignoring an
  undecodable home settings document" and runs with default home settings until the value is
  changed. Acceptable (single house, hub and web ship together); mention it in the PR.
- Web: `screens/energy.rs` switches on all three views (the `classic()` boolean becomes a match on
  `energy_view(house)`); a new `screens/energy_orbite.rs` holds `OrbiteDiagram(ui, narrow, open,
  expanded)` with the same signature as `FluxDiagram`; pure geometry (angles, arc paths, T, dot
  positions) in a `energy_orbite.rs` module next to `energy_circles.rs`, unit-tested.
- Settings: `EnergyViewSwitch` gets the third segment; the preview is its own component.
- i18n: `EnergyViewOrbite`, `EnergyPreview` (settings), `ComingIn`, `GoingOut`, `InTheHouse`
  (energy), EN + FR, added to the all-keys lists the i18n tests walk.

## Testing

- `energy_orbite` geometry: both rings sum to 360° minus gaps when balanced; T = max of the two
  sums when the meters disagree; arcs in the stated order; zero-watt nodes produce no arc; dot
  positions stay inside their arc.
- Web tests next to `tests/energy.rs`: with `energy_view = Orbite`, the Orbite diagram renders
  (and Flux/Classic do not), rows show the measured values Flux shows for the same `EnergyUi`,
  tapping a consumer row calls `open`, +N others toggles the list.
- Settings test (`tests/home_settings_panel.rs` or the settings tests): three chips, choosing
  Orbite saves `orbite`, the preview switches.
- Existing Classic / Flux tests that assert colour variable names are updated to the `--energy-*`
  names; a test checks Flux's `color_var` and Classic's tone classes map to the shared palette.
- API test as above. The existing gates (fmt, clippy, tests, wasm build) stay green.

## Docs

- `docs/users/web-ui.md`: the Energy view paragraph names Orbite and the Settings preview.
- Screenshot of Orbite for the docs site: follow-up, using the docs screenshot recipe.

## Calls made without the user (check these)

- Candy reaches Classic and Flux through one shared `--energy-*` palette (user's request); Night
  Glass is unchanged, Flux gets no new glow, Classic keeps its Ivoire house tone and halo.
- Stored value `"orbite"`, label "Orbite" in both languages.
- Fold like Flux (8 wide, 4 narrow).
- Unbalanced meters: the shorter ring stops short rather than being stretched.
- A fifth consumer colour, so Orbite and Flux colour each consumer the same.
- A separate arc-dot store; Classic's dots are not refactored.

## Left to the builder

- Exact sizes on narrow screens, spacing, the house icon (reuse the app's glyph), and whether the
  rings' SVG and the lists live in one grid or two.
- How the glide is implemented (CSS transition on `stroke-dasharray` or tween in the rAF loop),
  as long as reduced motion turns it off.
