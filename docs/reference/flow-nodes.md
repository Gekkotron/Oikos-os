---
title: Flow nodes
sidebar_position: 4
description: The flow document format and every node type with its parameters, outputs and rules.
---

A flow is an n8n-style automation: a small graph of nodes stored as one retained JSON document under `<automation_prefix>/<id>` (default `oikos/automation/<id>`) on the broker. The hub subscribes to that prefix, validates every document it reads, and runs the valid, enabled ones. An invalid document is kept and shown with the reason, but never runs. For a guided introduction, see [Flows](../users/flows.md).

The model is in [`crates/oikos-core/src/flow`](https://github.com/Gekkotron/Oikos-os/tree/main/crates/oikos-core/src/flow) (`model.rs`, `validate.rs`, `sun.rs`) and the engine in [`crates/oikos-os/src/flows`](https://github.com/Gekkotron/Oikos-os/tree/main/crates/oikos-os/src/flows).

## Flow document

```json
{
  "id": "living_led_motion", "name": "Living LED on motion", "enabled": true, "mode": "restart",
  "nodes": [
    { "id": "motion", "type": "trigger.state", "params": { "entity": "0x00158d00044c4d4c_occupancy_zigbee2mqtt" } },
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

| Field | Type | Description |
| --- | --- | --- |
| `id` | string | 1 to 64 characters of `a-z`, `0-9`, `_` and `-`. It is the last topic segment. |
| `name` | string | Free text shown in the UI. |
| `description` | string, optional | Free text. |
| `enabled` | bool, default `true` | A disabled flow keeps its definition but never runs. |
| `mode` | `single` (default) or `restart` | What a trigger does while a run of the same flow is still active. `single` ignores the new trigger and records a skipped run. `restart` cancels the active run and starts a new one, which is what makes "5 minutes after the last motion" work: motion coming back cancels the pending `delay`. |
| `area` | string, optional | The room (an area id) the Flows page lists the flow under. It does not change what the flow does. The hub checks it on save: an unknown room is refused, a room name is resolved to its id, and `Unassigned` is not a room for a flow. A room id already stored and since deleted is kept as it is, so it never blocks a save. |
| `nodes` | array | The steps. |
| `edges` | array | The connections, followed from a trigger onwards. |

Unknown keys, on the flow, on a node and in its params, are kept as they are, so a newer writer's fields survive. A `null` reads as absent.

Each node is `{"id", "type", "params", "name"?, "position"?}`:

- `id`: unique in the flow, 1 to 32 characters of `a-z`, `0-9`, `_` and `-`. Edges refer to it.
- `type`: one of the types below.
- `name`: an optional label shown on the canvas.
- `position`: optional `{x, y}` canvas coordinates. The hub ignores it.

Each edge is `{"from", "to", "port"?}`. An edge leaving a branching node must carry one of its ports: `"true"` or `"false"` for an `if`, `"1"` to `"10"` or `"otherwise"` for a `switch`, `"in"` or `"out"` for a `time.between`. Omit a port to do nothing there. An edge leaving any other node has no `port`.

A flow is refused when:

- an id is malformed or a node id is used twice;
- an edge names a missing node, a port that does not exist, or lacks the port a branching node (`if`, `switch`, `time.between`) needs;
- there is no trigger node;
- the graph has a cycle (the error names the path);
- a node's params are invalid (see each node below).

On save the hub also checks the house: every entity a node names must exist, a `command` must map for that entity, and a `flow.enable` target must be an existing flow.

### How a run goes

A run follows the graph from the trigger that fired, depth first. A failing node ends the run as `failed` at that step. A branching step (`if`, `switch`, `time.between`) that succeeds records the `ports` it followed, which can be none (a `switch` whose rules all missed); the web UI uses them to light the right wires. Other steps carry no `ports`. A non-trigger node with no incoming edge never runs. A run that is skipped (because of `single` mode, or because the broker link is offline when a schedule or sun trigger fires) is recorded as a skipped run.

The **scalar** is the one value of a state that triggers and `if` compare: the on/off bool, the number, the text, a light's `on`, a cover's `state`.

Runs live in memory only: a hub restart forgets them, the flow definitions stay on the broker. `[automation] runs_kept` (default 20) sets how many recent runs are remembered per flow.

### The house position

Sun nodes need the position of the house. Set `[flows] latitude` and `[flows] longitude` in `oikos.toml` (see [Configuration](configuration.md#flows)). With neither set, the hub uses Paris and saving a flow that has a `trigger.sun` or an `if` with `op: is_dark` returns a warning. Schedules use `[flows] timezone`.

## `trigger.state`

Starts a run when an entity's scalar changes to a different value.

| Param | Type | Description |
| --- | --- | --- |
| `entity` | string | The entity id (from `search` or `list_entities`). |
| `to` | JSON value, optional | Fire only when the new scalar equals it (`true`, `"ON"`, `21.5`). |
| `from` | JSON value, optional | Fire only when the previous scalar equalled it. |

Outputs: one. The run carries the entity's new value.

- The replayed value after boot or reconnect arms the entity silently. It never fires on that replay, on an availability-only change, or on an equal-value report. A change needs a known previous value and a known new one.
- A **button** (a virtual or discovered `button`) has no value: a press is an event and fires the trigger with the value `PRESS`. A button trigger takes no `to` or `from`, and a retained message is a replay, never a press.
- The trigger is refused on entities that send signals (remotes, events, tags): use `trigger.signal`.

## `trigger.signal`

Starts a run when a remote, an event entity or a tag scanner sends a signal. Like a press, a signal is an event the hub never replays from retained messages, and a repeated signal is a new one.

| Param | Type | Description |
| --- | --- | --- |
| `entity` | string | A `device_automation.*`, `event.*` or `tag.*` entity id. |
| `signal` | string, optional | Fire only for this signal name. Absent: any signal of the entity. |

Outputs: one. The trigger value is the signal name.

The entity must send signals (otherwise the hub says to use `trigger.state`), and a `signal` must be one the entity announces: its `capabilities.generic.signals`, or for an `event` the options of its last value.

## `trigger.schedule`

Starts a run at each wall-clock tick while the flow is enabled.

| Param | Type | Description |
| --- | --- | --- |
| `cron` | string | Five fields: minute, hour, day of month, month, day of week. Seconds and year forms are refused. |

Outputs: one. The cron runs in `[flows] timezone` (the system zone when unset). The first run is at the next tick. Ticks missed while the hub was busy or down are never replayed. `GET /api/v1/schedule?cron=…` previews the next ticks.

An `if` or a `switch` reachable from a schedule trigger (or a sun trigger) must name an `entity`, because these triggers carry no value; the exception is an `if` with `op: is_dark`. A `time.between` reads the clock and needs none.

## `trigger.sun`

Starts a run at a solar event.

| Param | Type | Description |
| --- | --- | --- |
| `which` | `sunrise`, `sunset`, `dusk` or `dawn` | The event. `sunrise` and `sunset` are the apparent horizon crossing (sun centre at -0.833 degrees). `dusk` and `dawn` are civil twilight (sun centre at -6 degrees). |
| `offset_minutes` | integer, optional | Minutes added to the event time; negative fires earlier. Between -720 and 720; 0 when absent. |

Outputs: one.

The hub arms the next event within the coming week, in the house zone. Events missed while the hub was busy are never replayed: the next one is searched from now. Where the sun never crosses the threshold (polar night, midnight sun) nothing is armed. While the broker link is offline when the event is due, the run is recorded as skipped.

## `if`

Branches on a comparison.

| Param | Type | Description |
| --- | --- | --- |
| `op` | `eq`, `ne`, `gt`, `lt`, `gte`, `lte` or `is_dark` | The comparison. |
| `value` | bool, number or string | The constant to compare with. Required for every op except `is_dark`, where it is refused. |
| `entity` | string, optional | The entity whose current scalar is read, live. Absent: the entity that triggered the run. Refused with `is_dark`. |
| `missing` | `fail`, `true` or `false`, optional | What to do when the value is unknown. Refused with `is_dark`. |
| `average` | duration, optional | A window such as `15m` or `1h`, at most `24h`. Compare the mean of the entity's readings over that window instead of its current value. `value` must then be a number. Refused with `is_dark`. |
| `offset_minutes` | integer in [-720, 720], optional | With `is_dark` only. Moves civil dusk by this many minutes and civil dawn by the opposite. Absent means 0. Refused with every other op. |

Outputs: two ports, `true` and `false`. Every edge leaving an `if` names one.

- `eq` and `ne` are typed: values of different types are never equal, and an unknown value equals nothing, so `ne` holds.
- `gt`, `lt`, `gte` and `lte` need both sides numeric; two known values that are not both numbers fail the node. An unknown value is `false`.
- `missing` set to `true` or `false` takes that port when the value is unknown, before any comparison.

:::caution
`missing: "fail"` (and leaving `missing` out) does not fail the node today: an unknown value goes through the comparison above, so `eq` and the ordering ops take `false` and `ne` takes `true`. Set `missing` to `true` or `false` when the branch matters.
:::
- `average` is the plain mean of the readings received in the window ending now, the newest included. While a flow uses it the hub keeps every reading of that entity, repeats included; for the part of the window before that (after a hub restart, or right after the flow is saved) it reads history, which holds one row per change, so a repeated reading there counts once. With no reading in the window the current value is the mean. The value is unknown, so `missing` applies, when the entity is unknown, unavailable or not a number. History is optional: without it the average starts from the readings received since the flow was loaded. The run detail shows what was compared, for example `average 30m = 18.6 → true`.
- `is_dark` is true between civil dusk and civil dawn (sun centre 6 degrees or more below the horizon) at the house position, now. It reads the sun, not an entity. With `offset_minutes: -30` it is true from 30 minutes before civil dusk until 30 minutes after civil dawn, which suits indoor spaces that get dark before the street does; a positive offset shortens the dark at both ends.

Example: a temperature sensor averaging 19 or less over 30 minutes takes `true`, so a single noisy reading does not trigger the branch.

```json
{ "id": "low", "type": "if", "params": { "entity": "sensor_temperature", "op": "lte", "value": 19, "average": "30m", "missing": "false" } }
```

## `switch`

Branches on several values at once: one output per rule, tried in order. It is an `if` with many comparisons.

| Param | Type | Description |
| --- | --- | --- |
| `rules` | array of rules | 1 to 10 rules. Required. See below. |
| `entity` | string, optional | The entity whose current scalar is read, live. Absent: the entity that triggered the run. |
| `average` | duration, optional | A window such as `15m` or `1h`, at most `24h`. Compare the mean of the entity's readings over that window, as for the [`if`](#if) `average`. Every rule `value` (and `to`) must then be a number. |
| `otherwise` | bool, default `false` | Adds an `otherwise` port, taken when no rule matches. |
| `all` | bool, default `false` | Take every matching rule instead of the first one only. |

A rule is `{"op", "value", "to"?}`:

| Field | Type | Description |
| --- | --- | --- |
| `op` | `eq`, `ne`, `gt`, `lt`, `gte`, `lte` or `between` | The comparison. |
| `value` | bool, number or string | The constant compared with. With `between` it is the lower bound. |
| `to` | number | The upper bound. Required with `between`, refused with every other op. |

Outputs: one port per rule, named by its 1-based position (`1`, `2`, ...), plus `otherwise` when set. Every edge leaving a `switch` names one.

- Rules are tried from the first. Without `all`, the first match is the only port taken. With `all`, every matching rule's port is taken, in rule order.
- `between` includes both bounds: `value <= x <= to`. It needs numbers, and `to` must be at least `value`.
- The operators compare as in an `if`: `eq` and `ne` are typed, the ordering ops and `between` need numbers.
- An unknown value (unknown or unavailable entity, or an `average` with a non-numeric reading) matches nothing except an `ne` rule, so `otherwise` is taken when no `ne` rule matches. A known value that is not a number, tested with an ordering op or `between`, fails the node.
- When nothing matches and `otherwise` is off, no port is taken and the branch ends there.
- The step detail shows the value and the ports taken, for example `20 → 2`, `unknown → otherwise`, `20 → none`, or `average 30m = 18.6 → 1`.

The ports follow the rule positions: when you add or remove a rule in the middle, the later rules change port number, and the edges leaving them have to follow. The web editor renumbers them for you.

Example: three bands of temperature, with a fallback.

```json
{ "id": "band", "type": "switch", "params": {
    "entity": "sensor_temperature",
    "rules": [
      { "op": "lt", "value": 18 },
      { "op": "between", "value": 18, "to": 22 },
      { "op": "gt", "value": 22 }
    ],
    "otherwise": true } }
```

Edges leave it with `"port": "1"`, `"2"`, `"3"` or `"otherwise"`. A reading of 22 takes port `2`; a reading of 22.5 takes port `3`; an unknown reading takes `otherwise`.

A `switch` refuses to save when `rules` is empty or has more than 10 entries, a rule has a `value` that is not a bool, number or string, `to` is missing with `between` or present with another op, `between` is given non-numbers or `to` is less than `value`, or `average` is badly formed, over `24h`, or used with a non-numeric rule value.

## `time.between`

Branches on the time of day: in or out of a daily range.

| Param | Type | Description |
| --- | --- | --- |
| `from` | string | Start of the range, `HH:MM`, 24-hour (`00:00` to `23:59`). Included. |
| `to` | string | End of the range, same format. Excluded. |

Outputs: two ports, `in` and `out`. Every edge leaving a `time.between` names one.

- The range is `from <= time < to`. A `to` earlier than `from` crosses midnight: `22:00` to `06:00` is in from 22:00 up to 05:59.
- `from` and `to` must differ. Times need two digits each (`07:00`, not `7:00`).
- It reads the clock of the house zone (`[flows] timezone`, the system zone when unset), by wall-clock time: on a day the clocks change, `07:00` still means 07:00 on the wall.
- It needs no entity and no trigger value, so it works after any trigger, schedule and sun triggers included.
- The step detail shows the time read and the port, for example `22:40 → out`.

```json
{ "id": "daytime", "type": "time.between", "params": { "from": "07:00", "to": "22:30" } }
```

A `time.between` refuses to save when `from` or `to` is not a `HH:MM` time (the reason reads ``from: `7:00` is not a HH:MM time``), or when `from` and `to` are equal (`from and to must differ`).

## `delay`

Pauses the run. A run that is cancelled (by `restart` mode, by disabling the flow, or by a hub restart) never reaches the node after it.

| Param | Type | Description |
| --- | --- | --- |
| `for` | string | A duration: whole seconds, minutes and hours, alone or combined, without spaces (`90s`, `5m`, `1h30m`). Every number needs a unit (`5` is refused) and the total must not be zero. |
| `up_to` | string, optional | Same format. When set, each run waits a random whole number of seconds between `for` and `up_to`, both included. It must not be shorter than `for`; equal to `for` is a fixed wait. |

Outputs: one. The step detail records the wait (`waited 5m`, or `waited 7m12s (5m to 10m)` with `up_to`).

```json
{ "id": "wait", "type": "delay", "params": { "for": "5m", "up_to": "10m" } }
```

## `command`

Sends a command to an entity.

| Param | Type | Description |
| --- | --- | --- |
| `entity` | string | The entity id. |
| `command` | object | The same JSON as the API's `Command`, for example `{"type": "turn_on", "brightness": 200}`. See [MCP tools](mcp-tools.md#commands). |

Outputs: one. The command goes through the hub's command path, confirmed by the entity's state echo within `[commands] confirm_timeout_ms`, exactly like `send_command`. No echo, a rejection or an offline entity fails the node.

On save the hub refuses a command the entity cannot take. It also refuses `arm` and `disarm` with a `code`: flows are retained on the broker in clear, so alarm codes are not allowed. Arm and disarm from a flow only on a panel that needs none.

## `notify`

Sends a phone push notification.

| Param | Type | Description |
| --- | --- | --- |
| `title` | string | Required, not empty. Takes `{entity_id}` placeholders, see [Values in a notification](../users/flows.md#values-in-a-notification). |
| `body` | string, optional | The alert body. Takes placeholders like `title`. |
| `channel` | string, optional | `alarm`, `security`, `camera`, `energy`, `climate`, `system` or `misc` (default). |
| `severity` | string, optional | `info` (default), `warning` or `critical`. |
| `entity_id` | string, optional | The entity the alert is about. |
| `image_url` | string, optional | An image to show with the alert. |
| `silent` | bool, optional | Deliver without sound. |
| `id` | string, optional | A fixed alert id. Absent: `flow-<flow id>-<run id>`, one alert per run. A fixed `id` makes each run replace the previous alert. |
| `lang` | string, optional | The language placeholder values are written in: `en` (default) or `fr`. The web editor sets it on save. |

Outputs: one. The step fails if the push service rejects the alert.

## `flow.enable`

Enables or disables a flow.

| Param | Type | Description |
| --- | --- | --- |
| `flow` | string, optional | The target flow id. Absent: the flow the node belongs to. |
| `enabled` | bool | `true` enables, `false` disables. |

Outputs: one. A flow that disables itself still finishes the current run. A target must be a valid flow id and, when it is another flow, an existing one.
