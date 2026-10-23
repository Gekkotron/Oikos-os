---
title: Flows (automations)
sidebar_position: 5
description: Automations as small graphs of triggers, checks, waits and actions, how a run walks the graph, and how to edit them in the web UI.
---

A flow is an n8n-style automation: a small graph of nodes stored as one retained JSON document under `oikos/automation/<id>` on the broker (the id is `[a-z0-9_-]{1,64}`). The hub subscribes to that prefix, validates every document it reads, and runs the valid, enabled ones. An invalid document is kept and shown with the reason, but never runs.

```json
{
  "id": "living_led_motion", "name": "Living LED on motion", "enabled": true, "mode": "restart",
  "nodes": [
    { "id": "motion", "type": "trigger.state", "params": { "entity": "<motion sensor id>" } },
    { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
    { "id": "on", "type": "command", "params": { "entity": "<light id>", "command": { "type": "turn_on" } } },
    { "id": "wait", "type": "delay", "params": { "for": "5m" } },
    { "id": "off", "type": "command", "params": { "entity": "<light id>", "command": { "type": "turn_off" } } }
  ],
  "edges": [
    { "from": "motion", "to": "moving" },
    { "from": "moving", "port": "true", "to": "on" },
    { "from": "moving", "port": "false", "to": "wait" },
    { "from": "wait", "to": "off" }
  ]
}
```

Entity ids come from the web UI or from the `search` and `list_entities` tools.

## Node kinds

The parameters of each node are in [Flow nodes](../reference/flow-nodes.md).

- **Triggers** start a run:
  - `trigger.state`: an entity's state changes.
  - `trigger.signal`: a remote press, an event entity or a tag scan.
  - `trigger.schedule`: a five-field cron expression in the house local time.
  - `trigger.sun`: sunrise, sunset, dusk or dawn, with an optional `offset_minutes`.
- **Checks** branch:
  - `if` compares a value with `value` using `op` (`eq`, `ne`, `gt`, `lt`, `gte`, `lte`), or uses `op: is_dark`, which is true between civil dusk and civil dawn at the configured point. The two outputs are `true` and `false`.
- **Actions** do something:
  - `command` sends a command to an entity.
  - `notify` sends a phone push notification.
  - `flow.enable` enables or disables a flow, its own by default.
- **Waits** pause the run:
  - `delay` waits for `for`, for example `90s`, `5m` or `1h30m`. With an optional `up_to` it waits a random time between the two.

The sun nodes and `is_dark` read the house location. Set `[flows] latitude` and `[flows] longitude` in `oikos.toml`; without them the hub uses Paris. See [Configuration](../reference/configuration.md).

## How a run goes

A run follows the graph from the trigger that fired, depth first. A failing node ends the run as `failed` at that step. A non-trigger node with no incoming edge never runs.

The scalar is the one value of a state that triggers and `if` compare: the on/off bool, the number, the text, a light's `on`, a cover's `state`.

`mode` says what a trigger does while a run of the same flow is still active. `single` (the default) ignores the new trigger and records a skipped run. `restart` cancels the active run and starts a new one, which is what makes "5 minutes after the last motion" work: motion coming back cancels the pending `delay`.

Two config keys: `[topics] automation_prefix` (default `oikos/automation`) and `[automation] runs_kept` (recent runs remembered per flow, default 20). Runs live in memory only: a hub restart forgets them, the flow definitions stay on the broker.

## Rooms

A flow can sit in a room: its optional `area` is an area id, and it only decides where the Flows page lists the flow; what the flow runs does not change. The hub checks it on save. An unknown room is refused, a room name is resolved to its id, and `Unassigned` is not a room for a flow. A room id already stored and since deleted is kept as it is, so it never blocks a save. Invalid flows keep their real name and room in listings.

## The Flows page

![The Flows page grouped by room, with filters and an enable switch per flow](/img/screens/flows.webp)

The Flows page (`/flows`) searches name, description, trigger devices and room (ignoring case and accents), filters by status (All, On, Off, ⚠ Needs attention) and trigger (Device state, Schedule, Sun, Button / event, Manual), sorts by name or last run, and groups by room (with a "No room" section last) or not at all. Room sections fold; a folded section that hides a flow needing attention shows ⚠. The **⋯** menu on a row or card moves a flow to another room, and the flow editor has the same room picker. The view, grouping, sort, filters and folded sections are remembered per browser; the search is not.

The assistant writes flows through the `set_flow` MCP tool (with `list_flows`, `run_flow` and friends), and the web UI shows them here. See [Talking to the house (MCP)](assistants-mcp.md).

## The flow canvas

![A flow on the canvas: sun and light triggers, checks and actions joined by wires](/img/screens/flow-canvas.webp)

The flow page (`/flows/<id>`) is a full-bleed canvas: each node is a pill with a coloured icon and a sentence title ("Motion at Entrée", "Turn off LED", "Wait 5 min"), joined by routed wires that go around the cards, with green "yes" and red "no" labels on the two outputs of a check. The last run, or one clicked in the Runs drawer, is replayed on the graph: visited cards and wires lit, the rest dimmed, failed steps red; a run in progress pulses live.

The **Edit** button switches to a React Flow-style editor: click a card to select it (amber ring) and edit its settings in the drawer, drag it to move it, drag from an output pin onto another card to wire it, click a wire for its ✕, press ⌫ to delete the selection and ⌘/Ctrl-Z to undo. **＋ Add** opens a searchable picker grouped WHEN / AND / THEN / WAIT / FLOW (type an entity name to list its triggers, checks and actions), **Tidy** re-lays the graph out, and nothing reaches the hub until **Save** (**Cancel** discards the draft).

**New flow** on `/flows` opens an unsaved draft in edit mode; its first Save creates the flow, disabled, under a slug of its name.

On a phone (narrow or touch-only screens) the canvas is view-only: Edit and New flow are hidden, while pan, pinch-zoom, the drawer, the switch and Run still work.

The REST routes for flows are in the [HTTP API](../reference/http-api.md).
