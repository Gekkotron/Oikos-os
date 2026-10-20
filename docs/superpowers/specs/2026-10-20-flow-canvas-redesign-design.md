# Flow canvas redesign (Homey Advanced Flow × React Flow)

Author: Gekkotron
Date: 2026-10-20
Status: approved in conversation, awaiting plan

## Why

The web flow page (`/flows/{id}`, `crates/oikos-web/src/screens/flow_canvas.rs`) works but looks "dirty", in the user's word. Both the look and the use of space are the problem:

- The canvas is a fixed box under the recent-runs list. Wide flows are cut off at the right edge, and nothing fits the flow to the screen.
- Wires tangle. On *Living LED on motion* (23 nodes), five trigger wires converge on one point, some wires run through cards, and the `true`/`false` labels overlap card text.
- Cards say little ("If · = true", "Command · Power 1 · set number") and all look alike apart from a thin coloured stripe.
- Editing covers only moving cards and changing their settings. Adding a card or a wire needs MCP or raw JSON.

The user pointed to Homey Advanced Flow (https://homey.app/fr-fr/features/advanced-flow) and React Flow (https://reactflow.dev/) as the references. Success: the page reads as cleanly as those two, a 23-node flow is easy to follow, and a flow can be built from scratch in the browser without touching JSON.

Mockups from the brainstorm (git-ignored, for reference only): `.superpowers/brainstorm/36175-1792528445/content/` (`layout-v2.html`, `card-style.html`, `edit-mode-v2.html`).

## Decisions made with the user

### 1. Layout: full-bleed canvas (mockup "A") with the action buttons from "B"

- The canvas fills the whole viewport under the app bar. There is no page box and no runs list above it.
- **Top-left, floating:** back arrow to `/flows`, the flow name, the mode word (`single` / `restart`).
- **Top-right, floating, as one group:** the enable switch (`FlowSwitch`), Run, Edit, and a **Runs** button that toggles the runs drawer.
- **Bottom-left:** canvas controls stacked vertically: zoom in (+), zoom out (−), fit view (⤢).
- **Bottom-right:** a minimap of the whole flow with the visible viewport outlined. Clicking or dragging in it moves the view.
- **Right side:** a drawer, closed by default, holding recent runs. It overlays the canvas rather than pushing it.
- On open, the view fits the whole flow (clamped so a two-node flow doesn't zoom to 300 %).

### 2. Cards: compact pills (mockup "B", React Flow style)

- Each node is a rounded pill: a coloured round icon on the left, then a title line and a small grey sub-line.
- Colour by role: yellow = trigger (`trigger.state`, `trigger.schedule`), purple = check (`if`), green = action (`command`, `flow.enable`), grey-blue = wait (`delay`). The schedule trigger keeps its clock icon. Today's icons in `icons::node_icon` stay unless a better one is obvious.
- The title reads as a sentence about the house, not as the node type:
  - `if`: the node's `name` if set, otherwise "<entity label> is <value>?" (for example "Lumière TV is on?", "Entrée > 16 lx?").
  - `command`: "Turn off LED", "Set Power 1 to 100 %".
  - `trigger.state`: "Motion at Entrée", or the entity label plus the `to` filter.
  - `trigger.schedule`: the existing plain-English summary ("Every 10 minutes").
  - `delay`: "Wait 5 min".
  - `flow.enable`: "Disable Force heating loop" / "Disable this flow".
  - A user-set `name` always wins as the title. The sub-line then carries the generated detail (room · device).
- Pins: one input on the left edge (none on triggers). Outputs on the right edge: one for most kinds, two for `if` (`true` upper, `false` lower).
- Wires are smooth cubic curves. A wire leaving `true` is green and one leaving `false` is red, each with a small "yes"/"no" pill label near its start. Other wires are neutral grey.
- **Readability:** wires never run through cards, triggers don't all pile onto a single point, and labels never overlap card text.

### 3. Editing: React Flow-style, centred on "moment 2" of the edit mockup

The user chose the click-to-select model as the core:

- **Click a card:** it is selected (amber ring), and its settings open in the right drawer. The drawer shows runs in view mode and settings in edit mode, one drawer for both. The existing `NodeEditor` / `kind_fields` / `ScheduleField` forms move into this drawer.
- **Click a wire:** it is selected (thicker, amber) and shows a ✕ at its midpoint. Clicking ✕ deletes it.
- **⌫ / Delete** removes the selected card (and its wires) or the selected wire. **⌘/Ctrl-D** duplicates the selected card (offset, unwired).
- **Connecting:** drag from an output pin and drop on another card's input pin, or anywhere on that card, to create the wire. While dragging, a live curve follows the pointer and valid targets highlight. Dropping on empty canvas cancels: no picker opens there (the user preferred option 2 over the drop-to-create picker).
- **Adding cards:** "＋ Add" in the edit toolbar opens a searchable picker grouped WHEN / AND / THEN / WAIT / FLOW. Typing an entity name ("table") lists the matching triggers, checks and actions on that entity. The chosen card is placed in the visible area, unwired, and selected so its settings are open.
- **Edit toolbar** (replaces the view-mode group top-right while editing): ＋ Add, Tidy, Undo, Cancel, Save.
  - **Tidy** re-runs the automatic layout over the whole draft (overwrites positions, and is itself undoable).
  - **Undo** (button and ⌘/Ctrl-Z) steps back through draft changes: move, connect, delete, add, settings edit, tidy. Redo (⇧⌘Z) is nice-to-have.
  - Nothing reaches the hub until **Save** (`PUT /api/v1/flows/{id}`, as today). Cancel discards the draft. Hub validation errors still come back as a toast.
- Dragging cards and wheel/⌘-wheel zoom keep working as they do today.

### 4. Runs on the canvas

The user wants both of these:

- **Live:** while a run is active, the card being executed glows and each wire the run travels pulses in its colour. A `delay` card shows its countdown (the data is already there: `parked_node` / `parked_deadline`). This uses the existing WebSocket `FlowRunChanged` events, which the hub already sends after every step (`flows/hub.rs` `record_step`). **No hub change is needed.**
- **Replay:** clicking a run in the drawer highlights the path it took: visited cards and wires at full strength, the rest dimmed. Each visited card shows its step time, and failed steps are red. Clicking the run again, or pressing Esc, clears the highlight. Today's `highlight` / `step_offset` helpers cover part of this.

### 5. Phone: view only

No editor on the phone. On narrow or touch-only screens the Edit button is hidden. The canvas still pans (drag) and pinch-zooms, and the runs drawer, enable switch and Run work.

### 6. Uncommitted work in the tree

The working tree had an unfinished restyle (flow list as cards with trigger, mode and last run; icon-only Save/Cancel; a cycling zoom "loupe"). Agreed:

- Commit the **flow-list cards** (`screens/flows.rs`, `flows.rs::trigger_summary`, and their CSS) as they are, as the plan's first step.
- The flow-page part of that diff (loupe, icon buttons, "stay in edit mode after Save") is superseded by this design. The loupe goes away in favour of +/−/fit. Reuse its `CHECK` / `X_MARK` icons where they fit.

### 7. Calls made while writing this up (the user can override)

These came out of the builder review and were settled without the user:

- **Which wires the editor allows.** The editor itself blocks wires into a trigger, self-loops, duplicate wires (same from, port and to) and wires that would close a cycle; those targets don't highlight and a drop on them cancels. Fan-in and fan-out are allowed. Anything else is left to the hub's validation on Save (toast).
- **Picker defaults.** For a matching entity the picker offers: a trigger "When <entity> changes"; a check "<entity> is <its current state>?" (`eq`, current value); and the actions the existing command editor can express for that entity kind (on / off / toggle for lights and switches; open / close / stop for covers; set value for numbers; press for buttons; set temperature for climates). Each starts with a sensible default the user then adjusts in the settings drawer. The entity list comes from `House::entities`, as the existing editors already use it.
- **Drawer state.** In edit mode the drawer shows the selected card's settings. With nothing selected it shows runs, and the Runs button stays available while editing. In view mode clicking a card only selects it (ring), it doesn't open settings.
- **After Save** the page leaves edit mode, as the committed code does. The uncommitted "stay in edit mode" change is dropped. The undo stack is cleared on Save and on Cancel.
- **Live updates while editing.** The draft ignores incoming changes to the flow's definition, and Save overwrites. Runs keep updating in the drawer and on the canvas.
- **New node ids** are `<kind>_<n>` (`if_3`, `command_2`), the lowest free `n`, so they match the hub's `[a-z0-9_-]{1,32}`.
- **"Phone"** means the app's existing phone breakpoint, or `(hover: none) and (pointer: coarse)`. A touch laptop with a mouse keeps the editor.

### 8. New flow (user asked for it)

- The `/flows` list gets a **New flow** button.
- It opens the canvas on a **local, unsaved draft** (a route like `/flows/new`), already in edit mode, with an editable name ("New flow" by default) in the top-left. Nothing is sent to the hub yet: the hub rejects a flow with no trigger (`FlowError::NoTrigger`).
- The first **Save** creates the flow with `PUT /api/v1/flows/{id}`. The id is a slug of the name (`[a-z0-9_-]`, at most 32 characters), made unique against existing flow ids with a `_2`, `_3` suffix. The URL then switches to `/flows/{id}`.
- **My call:** a new flow is saved **disabled**, so a half-tested automation doesn't go live on its first save. The user turns it on with the switch.
- Cancel on an unsaved new flow goes back to `/flows`.
- Renaming an existing flow uses the same editable name in edit mode. The id never changes after creation.

## Goals

- Everything in "Decisions" above, in the Leptos web app.
- *Living LED on motion* (23 nodes) reads cleanly after Tidy: no wire crosses a card, and the 5 triggers enter their target without a single-point pile-up.
- A flow can be created end to end in the browser (New flow, add triggers, checks, actions, wire them, save) and the hub accepts it.

## Non-goals

- Editing on phones or touch-only screens.
- A drop-wire-on-empty-canvas picker (explicitly not chosen).
- Multi-select, box selection, copy/paste between flows, grouping or comment notes.
- New node kinds or any change to the flow model, engine or hub API.
- Changing the Android app.

## Anti-goals

- A port of React Flow itself, or any React/JS framework. The web app is Leptos (Rust → wasm), and the canvas stays hand-written Leptos + SVG/HTML. "Inspired by" means behaviour and look only.
- Saving to the hub on every edit: a half-built flow must never go live.
- Silently dropping parts of a flow the UI doesn't understand. `Node::extra` and unknown params must survive an edit and save round-trip, as they do today.
- A Tidy that runs by itself and moves cards the user placed. Automatic layout applies only to flows with no saved positions (as today) or when the user presses Tidy.
- Regressing the existing editors (schedule picker, flow-switch editor, `if.missing`, JSON fields).

## Constraints

- Crate: `crates/oikos-web` (Leptos, `trunk`). Styles in `styles/main.css` using the existing tokens (`--card-bg`, `--accent-amber`, …), in the app's dark glass look.
- Positions are stored per node (`Node::position`), and the hub ignores them. Auto layout is in `flows.rs::layout` (simple layered grid today).
- The page must stay usable at desktop widths down to about 1000 px. Below the phone breakpoint it's view only.
- The hub serves the built web assets: after the change, rebuild and restart the hub on :8081 (see project memory) to see it live.
- Debug wasm is large (about 13 MB), and the Chrome extension's screenshots time out on this page. Use headless Chrome over CDP for visual checks.

## Left to the builder

- The layout algorithm behind Tidy and the initial placement: a layered (Sugiyama-style) layout with crossing reduction is the expected shape. Pill size, gaps, and how incoming wires spread over the input side are yours to choose.
- Wire routing details (curve control points, label placement, avoiding cards).
- How pan works: drag on empty canvas, plain wheel scrolls and ⌘/Ctrl-wheel zooms (as today), or similar. Keep it React Flow-like.
- Minimap rendering, sizes and animation timings.
- Undo data structure (a stack of `Flow` snapshots is fine).
- Picker contents and ranking, as long as the WHEN / AND / THEN / WAIT / FLOW grouping and entity search hold.
- How the code is split across files (`flow_canvas.rs` is already 1,361 lines; splitting it is welcome).
- Test strategy: pure logic (layout, sentence titles, undo, edge edits, path-taken from steps) gets unit tests in `flows.rs` or new modules. UI gets headless-Chrome screenshots against the live hub.
