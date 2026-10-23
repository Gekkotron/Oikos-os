# Rich device cards — design

Date: 2026-10-23. Scope: the web UI (`crates/oikos-web`). The hub, the API, the dashboard model in `oikos-core` and the Android app do not change.

This is project 2 of the redesign toward the "Ivoire" mockup (Homey + Apple Home feel). It builds on project 1 (`docs/superpowers/specs/2026-10-23-ivoire-theme-and-sidebar-design.md`, PR #12, merged): its tokens, sidebar and FR/EN layer (`src/i18n.rs`). Projects 3 (home header + weather), 4 (ambiances) and 5 (translate every screen) are separate.

## Why

The device cards work but read like a control panel: icon, name, status dots and a body of controls. The mockup's cards read like a calm home: a soft icon tile, the room above the name, a plain state line ("Allumée · 14 %"), one obvious control. Three cards in the mockup carry more than looks: the climate card with its ring, the temperature card with its small curve, and the wide vacuum card with a sentence and a robot drawing. Those are what the user pointed at first.

## Decisions taken with the user

1. **Focus:** the climate, temperature and vacuum cards are the heart of the project. Light, switch and water heater get the same frame without being the focus.
2. **Climate:** the ring is **draggable** to set the target.
3. **Replace everywhere:** the new cards replace the old ones on Home, room pages, dashboards and wherever `card_for` is used. No opt-in, no per-device switch back to the old look.
4. **The switch footnote "Le programme prend le relais" is mockup text:** dropped.
5. **Freshness badge kept:** the temperature card's badge reads "En direct" while the sensor reports, and "Il y a 3 h" once it has gone quiet.
6. The user approved the visual direction from a browser mockup of all seven cards in both themes ("it's beautiful"). That mockup is committed next to this document as `2026-10-23-rich-cards-mockup.html` (open it in a browser — the ring drags).

Inherited from project 1 (do not reopen): a theme only changes the look; every feature exists in every theme. Themes: Night Glass and Ivoire (+ Auto). Every new label goes through `i18n.rs` in FR and EN.

## Goals

- One shared card frame, faithful to the mockup, in both themes.
- A climate card with a draggable ring, − / + buttons and the room temperature.
- A temperature card with a big value, humidity, a 24 h curve and the freshness badge.
- A wide vacuum card with a state sentence, a generic robot drawing and Start / Home buttons.
- No feature lost: whatever a card can do today it can still do (on the card or one tap away in its sheet, as noted below).

## Non-goals

- New features for other kinds (cover, groups, alarm, number, select, button, generic…): they only move onto the new frame.
- Adding Vacuum or Switch widgets to dashboards. `WidgetKind` (`oikos-core/src/dashboard/model.rs`) has no such variants, so they cannot be placed today; adding them is a core/hub change for a later project. Dashboards get the new look for the kinds they already have.
- A hub batch endpoint for history.
- Home's "Vos essentiels" section, its room filter chips and the header (project 3); ambiances (project 4).

## Anti-goals (would be a failure even if it works)

- A card that looks right in Ivoire and broken in Night Glass (or the reverse).
- A ring that sends a command on every pixel of a drag, or fights the state echo while dragging.
- A Home page with dozens of sensors that freezes or floods the hub with history requests.
- A brand name or the user's own device names baked into the drawing or the code (the project is generic).
- Text that fails WCAG AA (use project 1's `--muted`, never the mockup's light grey for text).

## How it fits the current code

- `card_for` (`src/screens/home.rs:56`) picks a card per kind; it is used by Home's `RoomSection`, `src/screens/room.rs`, and **inside the entity and group sheets** (`entity_sheet.rs:274`, `group_sheet_v2.rs:168`) as the sheet's controls. Dashboards use their own match in `src/dashboards/widget.rs` (Light, Cover, Climate, Sensor, WaterHeater).
- `CardFrame` (`src/components/card_frame.rs`) = `<article class="card">` with a `<button class="card__header">` (icon, name, group `×N`, pending + availability dots) and `card__body`. Modifiers: `--on/--offline/--pending/--alert/--wide`.
- Each card in `src/cards/*.rs` builds its body. Light: toggle, brightness slider, colour swatch. Climate: room temperature, target `Stepper`, mode and fan `ModeSelect`s. Sensor: value + paired humidity (same `device_id`), links to `/history/<id>`. Vacuum: activity, battery glyph, primary Start/Pause/Resume, icon buttons per other action (e.g. Dock), fan select. Water heater: chips, inlet temperature, tank.
- `EntityView.area` gives the room id; area names live in `House` (`AreaView`).
- `EntityState.last_update` (`oikos-core/src/model/state.rs`) is the unix time of the entity's last state message, when any.
- History: `fetch_series(entity, HistoryRange::Day)` (`src/api/history.rs`) returns ~10-minute buckets for 24 h. Without `[history] url` the hub answers Unavailable / NotRecorded. The recorder **dedupes**: an unchanged value writes no row (`oikos-core/src/history/mod.rs`).
- Vacuum activities the hub accepts: `cleaning, docked, paused, idle, returning, error` (`oikos-core/src/state/kinds/vacuum.rs`).
- wasm tests rely on `.card__header`, `.card__name`, `.card--alert`, `.card--pending`, `button.card` and on button counts (`tests/cover.rs`, `energy.rs`, `generic_card.rs`, `security.rs`, `group_wizard.rs`).

## The design

### Where a card is shown

A card knows where it is through a Leptos context, `CardPlace` (name is the builder's call), with three values:
- **Overview** (the default): Home, dashboards, anything that doesn't say otherwise.
- **Room**: provided by the room page (`/rooms/:id`).
- **Sheet**: provided by the entity sheet and the group sheet, where `card_for` renders the sheet's controls.

The sheet has no controls of its own: its "Controls" panel *is* the card. So every control a tile leaves out must reappear when the card is in a Sheet. In the rules below, "**sheet only**" means: rendered when `CardPlace` is Sheet, not on tiles. Nothing is lost — it is one tap away.

In a Sheet the room line is hidden and the vacuum card is not wide.

### The shared frame

Layout, top to bottom (see the mockup):

- **Top row:** an icon tile on the left (rounded square, `--tile` / `--tile-on` when lit). The **trailing control** on the right: a toggle, a badge or a ⋯ button, depending on the card.
- **Room** in small muted text, then the **name**.
- **State line** in muted text (honey when lit): "Allumé · 14 %", "Éteint", "Désactivé"…
- **Body** at the bottom (slider, ring, value, buttons).

Rules:
- The tap target that opens the sheet stays a `<button class="card__header">` holding the tile, room and name, with `.card__name` on the name. The trailing control sits **outside** that button (no nested buttons).
- Pending and offline signals stay visible (small, e.g. a dot on the tile corner or next to the name — builder's call), as do the group `×N` badge and `card--offline/--pending/--alert` modifiers.
- **Room label:** shown everywhere except on a room page and in a sheet, where the room is already obvious (my call). The name comes from `House` areas by `EntityView.area`. Unassigned entities show no room line.
- New tokens per theme in `styles/tokens.css` (names are the builder's call), e.g. tile, tile-on, card tints for climate (sage) and temperature (cool blue — `--cool-bg` exists), ring track, badge border. Values for Ivoire follow the mockup; Night Glass gets matching translucent values (the browser mockup has a starting set).
- Cards keep `--card-min-height`; Home grid and dashboard spans are unchanged except where noted.

### Light (and light groups on the frame)

Toggle top right. State line "Allumé · 14 %" / "Éteint" (masculine for every device — French names vary in gender, the device is the subject; my call). Brightness slider at the bottom with its label and %. The colour swatch stays (builder places it, e.g. next to the % label). Lit card uses `--card-on-bg`.

### Switch

Toggle top right, state line "Activé" / "Désactivé". No footnote. Switches re-typed by `showAs` (door, siren, chime, light) keep their own cards, moved onto the frame.

### Climate

- Header row: tile + room/name; top right a **mode badge** showing the current HVAC mode (translated). The badge is a native `<select>` styled as a badge, listing the HVAC modes (today's mode `ModeSelect` behaviour, keyboard and screen-reader friendly for free — my call). The fan-speed select is **sheet only** (my call).
- **Ring:** a 270° arc around the target value ("15,5°" + "Température souhaitée"), filled from the entity's `min_temp` to the target, with a knob. Range and step from `capabilities` (`min_temp`, `max_temp`, `temp_step`; fall back 7–30 °C, 0.5).
  - **Drag** with pointer (mouse and touch, `touch-action: none` on the ring only). The displayed value follows the finger; the `SetTemperature` command is sent **once on release**, with the optimistic guess as today. Values snap to the step.
  - **Keyboard / a11y:** `role="slider"`, `aria-valuemin/max/now`, `aria-valuetext` ("15,5 degrés"), arrows = one step, PageUp/PageDown = 1 °C; send after a short pause (same debounce as − / +).
  - While a drag is in progress, incoming state echoes do not move the knob. After release the knob shows the sent value as the optimistic guess, exactly like today's − / + `Stepper`; the echo then wins (including a clamped or rejected value).
  - Ring colour follows the mode: heat → honey, cool → cyan, other → sage (my call). Mode `off`: ring greyed, centre shows "Arrêt", drag disabled until a mode is picked.
- **Foot:** − and + round buttons (touch-size) either side of "Il fait 22°" (the current temperature; the mode name when the unit reports none, as today).
- Card tint: sage in Ivoire, its Night Glass counterpart.
- Climate groups (`ClimateGroupCard`) move to the frame with their current body; the ring is for single climates (builder may reuse it if trivial).

### Temperature sensor

Applies to sensors with device class `temperature` (they already absorb the paired humidity). Other sensors move to the frame with a big value and no curve.

- Big value "20,7 °C"; humidity "💧 57 %" bottom left when paired; the **24 h curve** bottom right (a small inline SVG polyline, no axes). Tapping the value or curve opens `/history/<id>` as today.
- **Fetch:** one `fetch_series(id, Day)` per card, started when the card is first on screen (lazy), through a shared in-memory cache keyed by entity id with a ~5 min lifetime and at most ~4 requests in flight. Remounting a card reuses the cache.
- **No history** (Unavailable / NotRecorded / error / empty): no curve; nothing else changes.
- **Freshness badge** top right, from `state.last_update` (the last state message, so it works with or without the history database and counts reports that didn't change the value — my call, better than the history's last point, which is deduped): "En direct" when it is less than 3 h old; otherwise "Il y a N h" (whole hours), "Il y a N j" from 24 h. Hidden when `last_update` is absent. It re-evaluates every minute or so. The threshold is a single constant.
- Card tint: cool blue (`--cool-bg`).

### Vacuum

- **Wide:** `card--wide` (full row on phone, two columns from 720px — existing rules) on Overview and Room; not wide in a Sheet. Not on dashboards (see non-goals).
- Left column: tile + battery (%), room, name, then a large **sentence** for the activity, then the buttons. Right column: a **generic robot drawing** (inline SVG: concentric rings, round body, a top bump — no brand) with a small caption under it.
- Sentences (FR / EN), my wording:
  | activity | sentence | caption |
  |---|---|---|
  | docked | À la maison. / At home. | Sur sa base · Prêt à nettoyer / On its dock · Ready to clean |
  | cleaning | En plein ménage. / Cleaning. | En cours · ventilateur <speed> / Running · fan <speed> |
  | returning | Rentre à la base. / Heading home. | — |
  | paused | En pause. / Paused. | — |
  | idle | Au repos. / Idle. | Hors de sa base / Off its dock |
  | error | Besoin d'aide. / Needs help. | Vérifiez le robot / Check the robot |
  | unknown | — (state line only) | — |
- The drawing changes subtly with the activity (e.g. the outer rings pulse while cleaning, error tints terracotta; reduced-motion = still). Builder's call how far to go.
- **Buttons:** the primary dark button keeps today's logic (Démarrer / Pause / Reprendre); a round ⌂ button sends `return_to_base` when supported; other supported actions (e.g. spot) and the fan select are **sheet only**. Vacuum groups move to the frame with their current body.

### Water heater

Tile, room, name; ⋯ top right opens the entity's sheet. Tank gauge on the left of the big temperature, the existing caption ("Au repos · Entrée à 16,3 °C") under it. Its other existing controls are **sheet only**.

### Everything else on the frame

Cover, binary sensor, number, select, button, alarm panel, generic, door, siren, chime and every group card: same frame (tile, trailing control where they have an obvious one, room, name, state line), body unchanged.

### Dashboards

"Replace everywhere" includes dashboards: `WidgetView` renders the same rich cards (ring, curve with the shared cache, badge) for the kinds it already supports, in the Overview place. A 1×1 climate widget gets the ring scaled down; a 2×1 one centres it.

### Language

Every new string (state lines, sentences, captions, badge, ring labels, aria texts) is a `Key` in `src/i18n.rs` in FR and EN. Numbers use the locale's decimal separator ("15,5" in FR, "15.5" in EN). Existing English literals inside the cards being touched move to `i18n.rs` while they're there (the full-screen sweep remains project 5).

### Phone

- 2 columns at phone width; the vacuum takes a full row. The ring scales to the card width (SVG with a viewBox) and stays usable at ~160 px; − / + and the toggle keep `--touch-min` targets.
- No backdrop blur added by these cards; heavy effects are token-driven so Ivoire can stay flat.

### Tests

- Keep `card__header`, `card__name` and the modifiers where it is natural; existing wasm selectors and count-based asserts may be rewritten in the same commit that changes the markup (single-user project, no external consumers).
- Unit tests (native): vacuum activity → sentence/caption keys; freshness text from a timestamp; ring angle ↔ value with min/max/step snapping; locale number formatting.
- wasm tests: a card in a Sheet shows its sheet-only controls and a tile doesn't; the ring has `role="slider"` with correct aria values and sends one command after a drag/keyboard change; the temperature card renders without curve or badge when history is unavailable; the room line is hidden on a room page.
- Screenshot check of every card in both themes at desktop and phone width (headless Chrome over CDP, see memory note).

## Left to the builder

Component structure (one `CardFrame` with slots vs a few frame variants), token names and Night Glass values, exact geometry of the ring and drawing, where pending/offline dots sit, the lazy-loading mechanism, the order of commits.

## Sequencing hint

Frame + tokens first (and the test fixes it forces); then light/switch/water heater on it; then temperature (curve, cache, badge), climate ring, vacuum — those three are independent of each other.

## Branch

PR #12 is merged into `main`. Work happens on `feat/rich-cards`, branched from `main` at 53d8858.
