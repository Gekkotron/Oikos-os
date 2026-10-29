# Energy view: Classic redrawn as glowing circles (style C2)

Date: 2026-10-29. Status: design approved in brainstorming; awaiting written-spec review.

## Why

The web `/energy` page has two live views, chosen in Settings › Appearance › Energy view and
saved hub-wide in the home settings document (`energy_view`, `oikos_api::EnergyView`): **Classic**
(the circle diagram in `crates/oikos-web/src/screens/energy.rs`) and **Flux** (the Sankey, PR #84).
The user finds the information Classic gives right, but the drawing is not beautiful enough:
small 22 px discs, 10 px labels, straight dashed lines. This redraws Classic as **style C2,
"glowing circles"**, picked from a live mockup over C1 (plain circles) and today's Classic.

The look to reproduce is the throwaway mockup
`.superpowers/brainstorm/25189-1793268041/content/style-c-v1.html` (panel C2, wide) and
`phone-v2.html` (P2, phone). They are references for look and motion, not code to port.

## Goals

- Classic's view is replaced by C2. The setting keeps two choices, **Classic** and **Flux**,
  under the same names and the same stored value (`EnergyView::Classic`); no data migration.
- Same information as today, drawn better: every node Classic draws today, the same values,
  the same tap behaviour, the same fold into "+N others", the same accessible sentence.
- Motion that shows the direction and the size of each flow at a glance.

## Non-goals

- No change to the hub, the API, `oikos_core::energy`, the cards, the grid day chart, the
  "show all consumers" list, or the Flux view.
- No new data: no per-wire allocation beyond what `energy_sankey::build` already computes.
- No Android app change.

## Anti-goals (it would be a failure even if it works)

- **Solar never has a wire to the grid.** Solar has exactly one wire, to the house. Export is
  drawn on the house ↔ grid wire, running house → grid. Same for the battery: one wire, to the
  house.
- A drawing that loses information Classic shows today (a node, a value, a direction).
- Motion that cannot be turned off: `prefers-reduced-motion: reduce` must stop it.
- Janky phones: the animation must stay smooth on a mid-range phone.

## What it looks like

### Nodes

- **Solar, Grid, Battery** (when configured): big circles. Inside, top to bottom: the role
  icon (in the node colour), the value in large type, the unit and a short word (`production`,
  `↘ import` / `↗ export`, `↘ discharge` / `↗ charge`). The name sits under the circle.
- **House**: the biggest circle. Value and unit inside, plus `xx % solar` when solar covers part
  of it. Its rim is a **mix ring**: arcs in solar / grid / battery colours, in proportion to what
  feeds the house right now (the `* → house` links of `energy_sankey::build`). The ring glides
  when the mix changes.
- **Consumers**: small circles on the consumer side (right on wide, bottom row on phones), icon
  inside, value then name next to them (wide: to the right; phone: underneath). Up to
  `ARC_SLOTS` (3) slots; with more, the last slot is the existing `NodeKind::More` node
  ("+N others", neutral colour), toggling the list exactly as today.
- **Water meter** (drawn today only when there is no battery): a small circle with no wire and no
  dots, in Battery's slot, value in its own unit; dashed rim as today.
- Each circle has a radial-gradient fill (its colour, stronger at the top) and a coloured rim.
  A soft **halo** behind it grows and brightens with the node's power and slowly breathes
  (≈ 4 s). Unavailable nodes keep today's dimmed look and get no halo.

### Wires

- Curved (one quadratic bend), leaving and meeting each circle at its rim.
- Stroke is a gradient from the source colour to the destination colour; its width grows with
  power (log scale, as Classic's `stroke()`), and an idle wire (< `IDLE_W`) is thin and faint.
- **Dots** travel along each active wire in the flow direction: count by wire length, size and
  speed by power (log scale). Each dot glows softly and has a short two-step trail. They fade in
  and out at the ends. Idle wire: no dots.
- Grid wire colour follows direction: violet while importing, mint while exporting (the Grid
  circle turns mint too and says `↗ export`).

### What each wire carries

Every wire runs between one node and the house, so a wire shows that node's own net reading,
as Classic's edges do today (`EnergyNode::value` and `flow`), not Flux's cross-allocation:

- **Solar → house**: total production (the Solar circle's value). All of it goes "through the
  house" in this picture; the surplus leaves on the grid wire.
- **House ↔ grid**: the grid reading; `Flow::In` runs grid → house (import), `Flow::Out` runs
  house → grid (export).
- **House ↔ battery**: the battery reading; discharge runs battery → house, charge house →
  battery. Grid-charging a battery is drawn as import plus charge, both through the house.
- **House → consumer**: the consumer's reading; `More` carries the sum it folds.
- A node that is unavailable or has no value keeps Classic's look: dimmed circle, faint idle
  wire, no dots.

The **mix ring** is the one place that uses `energy_sankey::build`: its arcs are the
`solar → house`, `grid → house` and `battery → house` links, as fractions of their sum (what the
house is consuming right now and where it comes from). `xx % solar` is the solar fraction, and
the ring is an empty track when the sum is zero.

### Colours

Theme tokens as today (`--solar`, `--accent-violet` grid, `--accent-mint` battery and export,
`--ink` house). Consumers use Flux's palette (`--flux-c0..c4`, defined in `.energy-flux` today;
move them where both views can read them) so a consumer never shares a source colour. "+N
others" uses Flux's muted colour. Both themes: Night Glass and Ivoire; the halo is softer on
Ivoire (the mockup used about half the opacity).

### Layout

- **Wide** (≥ 600 px, today's `Layout::WIDE` breakpoint): a triangle on the left — Solar top-left,
  Grid bottom-left (Battery or Water below Grid's row, left) — the House centre, consumers in an
  arc on the right. Card spans the page like today.
- **Phone** (< 600 px): top to bottom, like Flux on phones. Top row Solar · (Battery) · Grid,
  House in the middle, consumers in a row at the bottom (1 to 3 slots, centred). Water, when
  drawn, sits in the top row in Battery's place.
- The diagram card keeps its glass card styling.

### Motion and accessibility

- `prefers-reduced-motion: reduce`: no dots, no breathing, no glides; wires keep their
  power-scaled width and the mix ring stays, so no information is lost.
- Animation pauses while the tab is hidden.
- The SVG keeps `role="img"`, the `aria_sentence` label and `<title>`; nodes stay clickable as
  today (`More` toggles the list, a single-sensor node opens `/entity/<id>`); clickable nodes are
  keyboard-focusable with the same behaviour on Enter/Space.

## Constraints

- Leptos/WASM web UI; styles in `crates/oikos-web/styles/main.css`; texts through the i18n
  modules (EN + FR). New words: the short words inside circles (`production`, `import`,
  `export`, `charge`, `discharge`, `% solar`) unless existing keys fit.
- The project is public and generic: no host names, rooms or family names in tracked files.
- Docs: update `docs/users/web-ui.md` where it describes the Classic view.

## Decided by the user

- C2 over C1 and today's Classic; P2 (top to bottom) on phones.
- Solar → house only; export runs house → grid.
- The playback of nodes, wires, colours, layout, battery, water, reduced motion and the
  unchanged setting was accepted as described above.

## Calls made in brainstorming (not asked of the user)

- C2 replaces Classic under the same name and stored value instead of becoming a third choice.
- Battery: its own circle, one wire to the house; dots toward the house when discharging, away
  when charging. Wide: under Grid on the left; phone: top row between Solar and Grid.
- Water: small circle, no wire, in Battery's slot (Classic's rule: water only without battery).
- Reduced motion behaviour as above.
- Wires carry each node's net reading (Classic's model); only the mix ring uses Flux's
  allocation. Following the user's rule, the solar wire carries all of production.
- Consumers come from `EnergyUi::diagram` exactly as Classic (3 slots, the last one `More` when
  `consumers.len() > ARC_SLOTS`); Flux's "Unitemised" node is not drawn. Consumer colours are
  `--flux-c0..c2` by slot; `More` uses the muted colour.
- Tap rules are Classic's unchanged (the House opens its sensor only when it has exactly one,
  as today).
- The wide drawing gets its own viewBox and coordinates (the mockup's), so `positions` and its
  tests change with it. Where a mockup and this document disagree, this document wins.
- Dots keep their position along a wire when a reading updates (the dot loop reads wire
  geometry and power from a non-reactive store, it is not rebuilt by each update); a wire that
  disappears takes its dots with it.
- Verification is unit tests on the pure geometry and value functions, plus headless-Chrome
  screenshots of the live page (wide and phone, both themes); no visual regression suite is
  added.

## Left to the builder

- How dots are animated (one `requestAnimationFrame` loop moving SVG circles along
  `getPointAtLength`, as in the mockup, or CSS `offset-path`), as long as it is smooth, pauses
  when hidden and respects reduced motion. Keep per-frame work small (no Leptos signal churn per
  frame).
- Exact radii, coordinates, bends and type sizes; follow the mockups.
- How the mix-ring fractions are computed, provided they come from the same allocation as Flux
  (`energy_sankey::build`) and add up to the house value.
- Whether wide/phone geometry lives in `energy.rs`'s `Layout` or a new module; keep pure
  geometry in plain functions with unit tests, like `positions` and `edge` today.
