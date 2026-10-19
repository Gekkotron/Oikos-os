# Oikos-os flows: n8n-style automations authored through MCP, shown in the web UI

Author: Gekkotron. Date: 2026-10-19. Status: approved for planning.

## 1. Intent

Oikos-os replaces Jeedom, and Jeedom's last job is the automations. This design adds them as **flows**: node graphs the way n8n draws them, run by the hub in Rust, written by the assistant through MCP, and shown in the web UI as a canvas with the last run lit up.

The user's words: "Can I have an n8n editor inside Oikos-os, configurable through MCP, with visualization in the web UI?" and "add only what's needed for now in the n8n copy". The reference scenario, which the finished phase must run in the house:

> The living-room LED turns on when motion is detected and stays on for about 5 minutes once there is no more motion.

No n8n process, no JavaScript, no second config language: a flow is one retained JSON document on the broker, like an override is.

## 2. The model

### 2.1 A flow

```json
{
  "id": "living_led_motion",
  "name": "Living LED on motion",
  "enabled": true,
  "mode": "restart",
  "nodes": [
    { "id": "motion",  "type": "trigger.state", "name": "Motion",
      "params": { "entity": "0x00158d00044c4d4c_occupancy_zigbee2mqtt" } },
    { "id": "moving", "type": "if",
      "params": { "op": "eq", "value": true } },
    { "id": "on",  "type": "command",
      "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_on" } } },
    { "id": "wait", "type": "delay", "params": { "for": "5m" } },
    { "id": "off", "type": "command",
      "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_off" } } }
  ],
  "edges": [
    { "from": "motion", "to": "moving" },
    { "from": "moving", "port": "true",  "to": "on" },
    { "from": "moving", "port": "false", "to": "wait" },
    { "from": "wait",   "to": "off" }
  ]
}
```

- `id`: `[a-z0-9_-]{1,64}`, the last topic segment. `name`: free text, shown in the UI. `description` optional.
- `enabled`: a disabled flow keeps its definition, never starts a run, and cancels its active run when disabled.
- `mode`: what a trigger does while a run of the same flow is still active. `single` (default): the new trigger is ignored and recorded as a skipped run. `restart`: the active run is cancelled and a new one starts. These two are enough for the reference scenario (`restart` is what makes "5 minutes after the last motion" work: motion coming back cancels the pending `delay`).
- `nodes`: each has an `id` unique within the flow (`[a-z0-9_-]{1,32}`), a `type`, an optional `name` (the UI shows the type's label when absent), `params` by type, and an optional `position: {x, y}` the UI honours when present.
- `edges`: `from` and `to` are node ids; `port` names an output of `from` and is required when that node has more than one output (`if`), forbidden otherwise. The order of edges is the order in which several edges leaving the same port are followed.

### 2.2 Node types (the whole vocabulary of this phase)

| Type | Params | Outputs | Behaviour |
|---|---|---|---|
| `trigger.state` | `entity` (id), optional `to`, optional `from` | one | Starts a run when the entity's **scalar** (§2.3) changes to a different value. `to`/`from` filter on the new/previous scalar. Never fires on the first value after boot or reconnect (previous unknown), on an availability-only change, or on an equal-value report. |
| `if` | `op` one of `eq ne gt lt gte lte`, `value` (JSON bool, number or string), optional `entity` | `true`, `false` | Compares the scalar of `entity` (default: the entity that triggered the run, read live from the registry, not from the trigger payload) to `value`. `gt lt gte lte` need both sides numeric; otherwise the node fails. |
| `delay` | `for`: duration `"90s"`, `"5m"`, `"1h30m"` (seconds, minutes, hours; integers) | one | Pauses the run. Cancelled with the run. |
| `command` | `entity` (id), `command` (the API's `Command` JSON, e.g. `{"type":"turn_on","brightness":128}`) | one | Sends the command through the hub's command path (same confirmation as the API: echo within `confirm_timeout_ms`). Not confirmed, rejected or offline: the node fails. |

A run follows the graph from the trigger node that fired: a node runs, its outgoing edges on the chosen port are followed in order, depth first. A node that **fails** ends the run as `failed` at that step (n8n's default: stop on error). Nodes other than `trigger.state` with no incoming edge are allowed but never run (the UI shows them greyed).

What is deliberately **not** here (later phases, listed so the builder does not add them): time and sun triggers, MQTT and alert triggers, scenes, notify, a `wait_for` node, a `field` selector into a composite state, templates and expressions, a `switch` with n ports, parallel joins, a `repeat` node, a run-level variables bag.

### 2.3 The scalar of a state

Conditions and triggers compare one scalar derived from `StateValue`:

| `StateValue` | scalar |
|---|---|
| `OnOff(b)` | bool |
| `Number(n)` | number |
| `Text(s)` | string |
| `Light(l)` | bool `l.on` |
| `Cover(c)` | string `c.state` (`"open"`, `"closed"`, `"opening"`, `"closing"`, `"stopped"`), absent when none |
| `Climate(c)` | string `c.mode`, absent when none |
| `Alarm(s)` | string |
| `Vacuum(v)` | string `v.state`, absent when none |
| `WaterHeater(w)` | string `w.mode`, absent when none |
| `Unknown` | absent |

`eq`/`ne` compare JSON-typed values exactly (`true` ≠ `"true"`); an absent scalar is never equal to anything and makes `ne` true. `gt lt gte lte` need a number on both sides.

### 2.4 Validation

Two layers, so a flow written before its entities are discovered is never lost:

- **Structural** (pure, in `oikos-core`, run on every ingest and every write): the id or a node id is malformed or duplicated; a node type is unknown or its params do not decode; an edge names a missing node or port, or omits `port` on an `if`, or gives one on another node; the graph has a cycle; there is no `trigger.state` node; a duration does not parse or is zero. A retained payload failing this is `invalid` (§3.1).
- **Against the house** (only on `set_flow` and `PUT`, never on ingest): a trigger or node names an entity the registry does not know; a `command` cannot be mapped for its entity with its current state (dry-run through the command mapper, nothing published). The write is refused with the reason. At run time an entity that has gone missing simply fails that step (`failed`, "unknown entity"), and a trigger on a missing entity waits for it to appear.

A node may have several incoming edges (a diamond, or three motion triggers feeding one `if`); it runs once per arrival, and the canvas shows its latest step.

Unknown top-level keys and unknown `params` keys are kept verbatim (lenient, like the override), so a newer author's document survives a read-modify-write by this hub.

## 3. Hub (`oikos-core`, `oikos-api`, `oikos-os`)

### 3.1 Storage

Retained JSON on `<automation_prefix>/<flow_id>`; `[topics] automation_prefix = "oikos/automation"` (new config key; the registry already reserves the `automation` sub-tree under a discovery prefix, so these topics are never treated as discovery). An empty retained payload deletes the flow. Writes go through the hub like overrides: publish retained, then ingest locally so the reply already shows the result. The MQTT task already subscribes to `oikos/#`; the builder makes sure the subscription covers a prefix configured elsewhere.

A payload that does not decode or validate on ingest (edited by hand, or by a newer hub) is kept aside as `invalid` with its reason: listed, shown in the UI with the reason, never run, and overwritable.

Runs and their steps live in memory only, the last `[automation] runs_kept = 20` per flow; they are lost on restart, and the UI says "no runs yet" after one.

### 3.2 Engine

Pure parts in `oikos-core::flow`: the model, lenient decode and strict validation, duration parsing, the scalar of a state, comparison, cycle detection, the layering used for auto-layout (shared with the web), and a pure stepper: given a node outcome, which edges come next.

The engine proper in `oikos-os::flows`: one tokio task per active run, holding an abort handle so `restart`, disable, delete and shutdown cancel it; a `delay` is a `tokio::time::sleep`; a `command` awaits `Hub::send_command`. The engine subscribes to the hub's events and keeps its own map of the last known scalar per watched entity, seeded from the registry when a flow is loaded or changed, so a trigger fires only on a known-to-known change; the map is cleared when the broker link leaves `Online`, so the first value after a reconnect is never a trigger even when it differs (this is what keeps the retained replay at boot and after a reconnect silent: what moved while the hub was blind is not an event). Every step is recorded in the run (`node`, `at` in unix seconds, `outcome` one of `ok`, `failed`, `skipped`, with a `detail` string: the port taken, the command response, the failure reason). Run status: `running`, `done`, `failed`, `cancelled`, `skipped` (a `single` flow re-triggered while running; a one-step run so the UI shows it happened).

The hub exposes `flows()`, `flow(id)`, `set_flow(def)`, `delete_flow(id)`, `set_flow_enabled(id, bool)`, `run_flow(id)` (a manual run: starts at the first trigger node with no trigger data; an `if` without an explicit `entity` fails in a manual run with "no triggering entity").

### 3.3 Wire types (`oikos-api`)

`FlowView { definition (the §2.1 document), invalid: Option<String>, active_run: Option<run id>, runs: Vec<RunView> (newest first) }` and `RunView { id, flow_id, status, started_at, ended_at, trigger: Option<{entity, value, previous}>, steps: Vec<StepView> }`.

Snapshot gains `flows: Vec<FlowView>`. Events: `FlowsChanged { flows }` (definition, enabled, invalid or deletion changed) and `FlowRunChanged { flow_id, run }` (a run started, stepped or ended). Both list-carrying variants are struct variants (the tagged-newtype serialisation bug noted in the hub's deferred list).

### 3.4 REST

`GET /api/v1/flows`, `GET /api/v1/flows/{id}`, `PUT /api/v1/flows/{id}` (body: the definition; 422 with the validation reason), `DELETE /api/v1/flows/{id}`, `POST /api/v1/flows/{id}/enabled` (body `{"enabled": bool}`), `POST /api/v1/flows/{id}/run`. 503 when the broker link is down, as for overrides.

### 3.5 MCP

Tools: `list_flows` (name, enabled, invalid reason, node count, last run status and time), `get_flow` (the `FlowView`), `set_flow` (the whole definition; create or replace; the validation reason comes back in-band so the assistant fixes and retries), `delete_flow`, `set_flow_enabled`, `run_flow`. The `set_flow` parameter schema is the JSON Schema of the §2.1 document with the node types as a tagged enum, documented param by param, so the assistant learns the vocabulary from the tool itself. The server instructions get a paragraph on flows: what they are, where they live, the four node types, that entity ids come from `search`/`list_entities`, and that `mode: restart` is the way to express "N minutes after the last …".

## 4. Web UI (`oikos-web`)

Read-only canvas in this phase: no dragging, no node editing. The assistant is the editor.

### 4.1 Flows list (`/flows`, nav item "Flows" between Alerts and Devices)

One row per flow: name, a one-line summary built from the graph ("Motion → If → Turn on / Delay 5m → Turn off"), an enabled switch (the existing `.switch` affordance), the last run's status and relative time, an "invalid" badge with the reason when the document did not validate. Empty state: "No flows yet. Ask the assistant to create one."

### 4.2 Flow canvas (`/flows/:id`)

Header: name, enabled switch, "Run now" button (disabled while a run is active or the flow is invalid), mode chip.

The canvas copies n8n's look: a dotted-grid background; each node a rounded card with a type glyph on the left, the node name, and a one-line param summary ("Entrée", "= true", "5 m", "led_tv · turn on"); an input handle on the left edge, output handles on the right edge, two for `if` labelled `true` and `false`; wires as cubic Bézier curves in an inline SVG under the cards. Layout: `position` when every node has one, otherwise auto-layout by layer (longest-path depth → column, order of first appearance → row), left to right, trigger first. The canvas scrolls both ways on a phone and fits the width on a desktop.

The last run lights the canvas: nodes it visited and edges it followed in the accent colour, the failing node in rose, the step's `detail` as the node's second line, the pending `delay` of an active run pulsing with its remaining time. Nodes never reached stay in the resting colour.

Below the canvas: the runs, newest first, each with status, start time, trigger entity and value, and its steps on expand. A `FlowRunChanged` event updates the canvas and the list live.

### 4.3 Glyphs

Four new glyphs in the existing icon set: a bolt for the trigger, a fork for `if`, an hourglass for `delay`, a play arrow for `command`.

## 5. Goals, non-goals, anti-goals

**Goals.** The reference scenario runs in the house from a flow the assistant created through MCP with no hand-written JSON; the canvas shows it and its last run; disabling a flow stops it at once; a hub restart brings every flow back from the broker and starts nothing by itself.

**Non-goals (this phase).** Editing on the canvas. Any node beyond the four. Run history across restarts. Scenes. Importing Jeedom scenarios.

**Anti-goals.** A trigger storm at boot or after a reconnect (retained replay must stay silent). A run that keeps going after its flow was disabled, replaced or deleted. A flow document the app or a newer hub wrote being dropped or rewritten because this hub does not know a key. A second configuration language: no YAML, no rules file.

## 6. Constraints

- The seams listed in `docs/superpowers/plans/2026-10-18-oikos-os-hub-deferred.md` hold: `state_changed` means a real change; `asLight` switches are `kind: light` with an `on_off` value; `Unassigned` is an area id.
- Every web URL interpolating a flow or run id goes through `api::encode_id`.
- `oikos-core` stays wasm-clean (the web shares the model and the layering).
- Tests: the stepper, validation, scalar comparison, duration parsing and layering as unit tests in core; the engine against a hub with a stub publisher (trigger, restart cancelling a delay, disable cancelling a run, boot replay silent, single-mode skip) in `oikos-os/tests`; REST and MCP round trips; the canvas's layout function and run-highlight mapping as web unit tests; a wasm smoke test.
- Workspace gates as in the README: `cargo test --workspace`, clippy with `-D warnings`, fmt, the three wasm checks.

## 7. Decisions

| Decision | Who |
|---|---|
| Automations are n8n-style node graphs, not HA-style trigger/condition/action lists. | User |
| The assistant is the only editor (MCP); the web UI visualises. | User, Claude's reading of "visualization" |
| Only the four node types and two modes this scenario needs. | User ("only what's needed") |
| Flows are retained JSON under `oikos/automation/<id>`; runs are in memory. | Claude, from the v1 design spec §9 |
| `mode: restart` rather than a cancel-aware timer node. | Claude (Home Assistant's proven model; keeps the node set tiny) |
| Triggers fire only on known-to-known scalar changes. | Claude (boot and reconnect safety) |
| A failing node stops the run. | Claude (n8n default) |
| Auto-layout when positions are absent; MCP authors need not place nodes. | Claude |
| The first flow is created through MCP against the house as the phase's acceptance test, not hard-coded. | Claude |
| Ingest validates structure only; entity and command checks run on writes; a missing entity fails the step at run time. | Claude (builder check) |
| The known-scalar map is cleared when the link drops, so nothing fires on the first value after a reconnect. | Claude (builder check) |
| Several edges may enter one node; it runs once per arrival. | Claude (builder check) |

## 8. Left to the builder

Module and file layout; the exact JSON Schema derivation (`schemars`) for the tagged node enum; the run id scheme; how the engine task is started and stopped from `main`; the SVG wire geometry and the dotted grid; the glyph drawings; how the one-line summaries are worded; the run-list expand affordance; whether the Settings screen mentions flows.

## 9. The first flow

Created through MCP once the phase lands, as the acceptance test: trigger on a living-room motion sensor's `occupancy` entity, `if = true` → turn the LED on, `false` → delay 5m → turn it off, `mode: restart`. The living room has three motion sensors (`motion_south` named "Entrée", `motion_north`, `motion_corridor`) and two LEDs (`led_tv`, `led_ceiling`); which ones is the user's call and goes into the flow, not into the code.
