# Schedule trigger, flow switch node and virtual entities

Author: Gekkotron
Date: 2026-10-20
Status: approved in conversation, awaiting plan

## Why

The water heater (WaterHeaterController, published by hand as retained discovery, see README "Zigbee2MQTT") accepts only a power level per channel and drops every channel to 0 % when no command arrives for 15 minutes. It has no target temperature. The user wants a "force heating" button on Home that heats the tank at full power until it reaches X °C, then stops by itself.

Nothing about this may be water-heater code. The user asked for three generic building blocks, and the force-heating feature is assembled from them in the house:

1. a **schedule trigger** for flows (cron underneath, a friendly editor on top);
2. a **flow switch node** that enables or disables a flow;
3. **virtual entities**: a switch (and number, button) with no device behind it, shown on Home like any other card.

Success: the user taps "Force heating" on Home, the heater runs at 100 % on its three channels, the power is resent every 10 minutes so the firmware timeout never cuts in, and when the tank reaches X the power drops to 0 %, the switch turns itself off and the loop flow disables itself.

## Goals

- `trigger.schedule` node: fires on a cron schedule while its flow is enabled; never fires while disabled.
- A schedule editor in the web flow canvas that a non-programmer can use without knowing cron, with a plain-English summary, and raw cron available under "Advanced".
- `flow.enable` node: enables or disables a flow (its own by default).
- Virtual entities that survive hub and broker restarts, usable from Home, flows and MCP.
- The house setup for force heating, created through MCP and verified live.

## Non-goals

- Loops or back-edges in flows. The schedule replaces them; validation keeps rejecting cycles.
- Comparing an entity with another entity in `if` (needed for a settable "Target temperature" virtual number). Follow-up, listed at the end.
- A Home tile for flows themselves. The virtual switch is the button.
- Any change to the water heater firmware.
- Per-device code in the hub or the web UI.

## Anti-goals

- A schedule that fires on hub boot, broker reconnect or retained replays in a way that sends commands twice in a burst. Firing must follow wall-clock ticks only.
- A schedule editor that is just a cron text box.
- A virtual switch that comes back "unknown" or "off" after a restart while its loop flow is still enabled (or the reverse): state must be retained.
- Force heating that can leave the heater at 100 % forever because a flow got stuck. Every path that stops the boost must send 0 %.

## Constraints

- Follow the flows design (`docs/superpowers/specs/2026-10-19-flows-design.md`): the flow document is retained JSON on `oikos/automation/<id>`, unknown keys are preserved (`extra`), nodes are `{type, params}`, `if` branches on `true`/`false` ports, flows are validated against the house on `set_flow`.
- Engine safety rules stay: triggers fire only on known-to-known state changes; replays never start runs; one active run per flow, governed by `mode` (`single` skips, `restart` cancels).
- The hub holds no per-device code (README; `docs/adding-a-device.md`). MQTT discovery is the plugin API.
- Commands are currently never published retained (`crates/oikos-core/src/command/mod.rs`: "never retained").
- Reserved topics directly under the discovery prefix today: `ui` and `automation`.
- `oikos-core` must keep building for wasm32 (the web UI uses it); time handling that needs the OS belongs in `oikos-os`.
- Home is in Europe/Paris; schedules are wall-clock local time, correct across DST changes.
- Attribution: Gekkotron only (no real name), commits as Gekkotron.

## Decisions made with the user

- Force heating stops by itself when the tank reaches X: power 0 %, the switch turns off, the loop flow disables itself.
- The button on Home is a generic virtual component, not a flow tile.
- The schedule is cron underneath with a friendly interface; enabling the flow activates the schedule (e.g. every 10 minutes).
- X is a fixed number written in the flow for now.

## Design

### 1. Schedule trigger (`trigger.schedule`)

A new trigger node kind next to `trigger.state`.

```json
{ "id": "tick", "type": "trigger.schedule", "params": { "cron": "*/10 * * * *" } }
```

- `cron`: a standard 5-field expression (minute, hour, day of month, month, day of week), local time. Validation rejects invalid expressions with a message the editor can show.
- While the flow is enabled the engine computes the next fire time and starts a run at each tick, following `mode` like any trigger (a run still active at the next tick is skipped under `single`, restarted under `restart`).
- No catch-up: ticks missed while the hub was down, the link was offline or the flow was disabled are not replayed. A tick is skipped (and recorded as skipped, with the reason) when the broker link is down, like manual runs are refused today.
- Enabling a flow arms its schedule from now; the first run is at the next tick, not immediately. Disabling, deleting or redefining a flow disarms it.
- A run started by a schedule has no trigger value; an `if` without `entity` after it is a validation error.
- A flow may hold both kinds of trigger; any of them starts a run.
- The MCP tool descriptions (server instructions in the oikos MCP server and `set_flow`) document the new node.

### 2. Flow switch node (`flow.enable`)

```json
{ "id": "stop", "type": "flow.enable", "params": { "enabled": false } }
{ "id": "start", "type": "flow.enable", "params": { "flow": "force-heating-loop", "enabled": true } }
```

- `flow`: target flow id; absent means the flow the node belongs to.
- `enabled`: required boolean.
- Same effect as `POST /api/v1/flows/{id}/enabled` (publishes the retained flow with the flag changed). Validation on `set_flow` rejects an unknown target id.
- Disabling the node's own flow does not abort the current run: the remaining steps of that run finish (so "power 0 %, switch off, disable self" works in any order). It only stops future triggers.
- Recorded as a step in the run history like a command.

### 3. Virtual entities

A virtual entity is an entity whose discovery config makes its own command topic its state topic, with `retain: true`:

```json
{ "unique_id": "virtual_force_heating", "name": "Force heating",
  "device": { "identifiers": ["oikos_virtual"], "name": "Virtual" },
  "command_topic": "<virtual state topic>", "state_topic": "<same topic>",
  "retain": true }
```

- **Hub change (generic, Home Assistant standard):** honour the `retain` discovery option when publishing a command, so the echo the hub hears back is also what the broker replays after a restart. Applies to every kind whose HA schema has `retain` (switch, number, select, light, …); default stays `false`.
- **Topics:** virtual state lives in a reserved namespace so it never collides with discovery or devices. Builder chooses the exact form; constraint: reserve it alongside `ui` and `automation` (e.g. `oikos/virtual/<id>`), and it must not be classified as a discovery topic or show up in `GET /api/v1/ignored`.
- **Creating them:** an MCP tool (and the matching REST route) creates or removes a virtual `switch`, `number` (min, max, step, unit) or `button` by publishing its retained discovery; the user configures the house through conversation. Virtual entities appear under one device ("Virtual") so they are easy to find; each can be moved to a room with the usual override.
- **Initial state (Claude's call):** creating a virtual switch or number also publishes a retained initial state (`OFF`, or the number's `min` unless an initial value is given). Without it the first tap would be an unknown-to-on change, which the engine's known-to-known rule swallows, so the headline "Force heating" tap would do nothing. No engine exception for virtual entities.
- **Button (Claude's call):** a press is an event, not a state. A virtual button publishes its press, not retained, on its topic. The engine fires `trigger.state` on a button for every press message on that topic, bypassing the known map (which exists to silence retained replays; a non-retained press is never replayed). `to`/`from` do not apply to buttons and are a validation error there.
- **Audit:** before shipping the `retain` change, check every discovery config the hub currently holds for `retain: true` (none expected: the water heater's hand-published discovery sets none) and list any hits in the PR.
- Documented in `docs/adding-a-device.md` (new "Virtual entities" section) and the README.

### 3b. `if` on a missing value (Claude's call)

Today an `if` whose entity has no value (unknown, unavailable, gone) fails the step and the run stops: neither branch runs. For force heating that would leave the switch on and the loop enabled while the firmware timeout silently cuts the power, which breaks the anti-goal "every path that stops the boost must send 0 %".

Add an optional `if` param `missing`: `fail` (the default, today's behaviour, so existing flows are unchanged), `true` or `false` (take that branch when the value is missing, recorded in the step detail as "missing → false"). The editor offers it as "When the value is unknown: fail / take Yes / take No". The house loop flow uses `missing: false`, so a silent tank sensor stops the boost cleanly.

### 4. Schedule editor (web flow canvas)

Built on the per-node editor sheet from `da17604` (`crates/oikos-web/src/screens/flow_canvas.rs`).

- Choices first, each producing a cron expression:
  - Every N minutes (1–59; N = 10 by default)
  - Every hour at :MM
  - Every day at HH:MM
  - On chosen weekdays at HH:MM (weekday chips)
  - Advanced: raw cron text field
- A plain-English summary always visible on the sheet and on the node in the canvas ("Every 10 minutes", "Weekdays at 07:30").
- Opening an existing node maps its cron back to the matching choice; anything that matches none opens in Advanced.
- Shows the next three fire times (local) so the user can check the schedule.
- Invalid cron in Advanced is flagged inline before saving; the hub's validation error is shown if the save is refused.
- The node is added from the same place the other node kinds are added; the canvas shows it with a clock glyph.
- `flow.enable` gets its own editor: target flow picker (defaults to "This flow") and an Enable / Disable choice.

### 5. The house setup (force heating)

Created through MCP once the pieces exist, then verified live. Entity ids are the current ones (`water_heater_power_0/1/2`, `water_heater_tank_temp`).

- Virtual switch **"Force heating"**, placed in the water heater's room, shown on Home.
- Flow **`force-heating-switch`** (always enabled, `mode: single`):
  - `trigger.state` on the virtual switch, `to: on` → `if` tank `lt` X (`missing: false`) (Claude's call: no 10-minute overshoot when the tank is already hot):
    - `true` → `flow.enable force-heating-loop` = true → power 100 % on the three channels (immediate start, without waiting for the first tick);
    - `false` → virtual switch off (nothing to do; the `to: off` path below then sends 0 %, which is harmless).
  - `trigger.state` on the virtual switch, `to: off` → `flow.enable force-heating-loop` = false → power 0 % on the three channels.
- Flow **`force-heating-loop`** (disabled at rest):
  - `trigger.schedule` `*/10 * * * *` → `if` tank `lt` X (`missing: false`):
    - `true` → power 100 % on the three channels (resent every tick, well inside the 15-minute firmware timeout);
    - `false` → power 0 % on the three channels → virtual switch off → `flow.enable` (self) = false.
- X is written in the `if` node (initial value chosen with the user at setup time).

## Calls made by Claude (check these)

- A virtual switch or number is created with a retained initial state, so its first change can fire a flow (§3).
- Button presses fire on every non-retained press message, outside the known-to-known rule (§3).
- New `if` param `missing` (`fail` default / `true` / `false`); the house flows use `false` (§3b).
- The switch flow checks the tank before starting, so a hot tank never gets 10 minutes of overshoot (§5).
- Audit existing discovery for `retain: true` before shipping the retain change (§3).

## Left to the builder

- Cron parsing and next-fire computation: crate choice (must handle the 5-field form and DST in Europe/Paris), and where the timezone comes from (system zone or an `oikos.toml` setting).
- Engine structure for timers (one task per armed schedule, or a single timer wheel).
- Exact virtual topic layout and the REST route shape for creating virtual entities.
- How a virtual button's presses reach the engine.
- Canvas glyphs, colours and sheet layout, matching the existing node styles in both themes and at phone width.
- Run history wording for schedule-started runs and `flow.enable` steps.

## Context and related work

- Water heater discovery and the tank-picture Home card: README "Zigbee2MQTT", `crates/oikos-web/src/cards/water_heater.rs`. The card's CSS landed in `da17604`; the card's Rust, the `format.rs` title fix and the docs note are still uncommitted at the time of writing.
- Flows design: `docs/superpowers/specs/2026-10-19-flows-design.md`.

## Follow-ups (not in this project)

- `if` comparing with another entity's value, so X can be a virtual "Target temperature" number set from Home (full temperature control).
- A Home tile for flows (enable toggle or Run button).
