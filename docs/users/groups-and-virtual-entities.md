---
title: Groups and virtual entities
sidebar_position: 6
description: Control several devices as one with a group, and create switches, numbers and buttons the hub owns for your flows.
---

Two kinds of entity belong to the hub itself, with no device behind them. A **group** controls several devices as one. A **virtual entity** is a switch, a number or a button that exists only in the hub, so a flow has something to read and write.

## Groups

A group is an entity the hub publishes as `group_<id>`. It stands for a list of **member** entities of one kind. A command sent to the group goes to every member, and the group's state is the combination of the members' states. The group's definition is a retained message, so it survives a restart and the broker holds the truth.

Groups are off by default. Turn them on in `oikos.toml`:

```toml
[groups]
enabled = true
```

While `enabled` is `false`, the group routes and tools answer `groups disabled`, and group commands are rejected. See [Configuration](../reference/configuration.md#groups).

### Definition

| Field | Meaning |
| --- | --- |
| id | 1 to 48 characters of `a-z`, `0-9` and `_`; the entity is `group_<id>`. The id `_migrated` is reserved. |
| `kind` | What the group presents itself as: `light`, `switch`, `cover`, `climate`, `number`, `select`, `button`, `vacuum` or `water_heater`. |
| `name` | The display name. |
| `members` | The entity ids it controls. Every member must be of the group's kind, and a member cannot be another group. |
| `hideMembers` | `true` by default. The web UI then drops the members' own cards from Home, so the group shows instead (a member that is itself a favourite still shows under Favorites). A member of several groups is hidden when any of them says so. |

A switch you show as a light (override `showAs: "light"`) counts as a light for a light group. An id that no longer exists stays in the list and is flagged as absent; a member whose kind has changed since is flagged as a mismatch and is not commanded.

### How commands fan out

A command sent to a group (from the web UI, `send_command`, a flow `command` node or a REST call) is translated for each member and published to it:

- Offline members and members that are not of the group's kind are skipped.
- The group's own controls offer only what **every** member supports: brightness and colour only if all lights have them, a position only if all covers do, the modes that all climates share. If a member drifted since, it is skipped for that command.
- The hub waits up to two seconds for each targeted member to report. The command is accepted when they all did. With no reachable member it is rejected with `no reachable member`.
- The group's own state is not set by the command: it follows the members' echoes.

### How state combines

The hub republishes the group's state each time a member's value or availability changes.

| Kind | Group state |
| --- | --- |
| `light` | On if any member is on. Brightness, colour and colour temperature are the average of the lights that are on, and only shown when every member supports them. |
| `switch` | On if any member is on. |
| `cover` | `opening` or `closing` if any member moves, else `open` if any is open, else `closed`. The label reads "All open", "All closed" or "N open · M closed". The position is the average, when every member has one. |
| `climate`, `water_heater` | The target is the average of the members' targets, rounded to the step and kept inside the limits. When the targets differ, the lowest and highest are exposed as a range. The mode is the common one, and empty when members disagree. |
| `number` | The common value, else the average rounded to the step. |
| `select` | The common option, else none. |
| `vacuum` | `cleaning` if any member cleans, else `returning` if any returns, else `docked` if all are docked, else the first member's state. |
| `button` | No state. Pressing it presses every member. |

The group is `online` while at least one member of its kind is reachable (not offline), and `offline` otherwise.

### Managing groups

- **Web UI.** `/groups/new` opens a three-step wizard: choose the type, pick the members, then name the group, give it a room and decide whether to hide the members. A group's sheet lists its controls and members (with a flag on a mismatch), has a switch for "Show in Favorites" and "Hide members on Home", and offers Edit group and Delete group.
- **MCP.** `list_groups`, `set_group` and `delete_group`. See [MCP tools](../reference/mcp-tools.md#groups).
- **REST.** `GET /api/v1/groups`, `PUT /api/v1/groups/{id}` and `DELETE /api/v1/groups/{id}`. See [HTTP API](../reference/http-api.md#groups).

A group is an ordinary entity otherwise. It moves to a room with the usual override, and a flow can name it in a `trigger.state`, `if` or `command` node. Deleting a group clears everything the broker retained for it but keeps its UI override. The names of the flows that use a group are listed with it.

### Room groups

Earlier versions folded the covers and the lights of a room into one derived card per room. The first time groups are enabled on a house, the hub turns those cards into real groups, once: for each room with at least two covers, or at least two lights, it creates `cover_<room>` ("Shutters") or `light_<room>` ("Lights"), with the room's custom card name when it had one. Entities marked `ungrouped` and entities with no room are left out. A retained marker (`<first discovery prefix>/group/_migrated`) records that the migration ran, so it never runs twice.

## Virtual entities

A virtual entity has no device behind it: a "force heating" flag, a setpoint, a manual trigger for a flow. Three kinds exist:

| Kind | What it is |
| --- | --- |
| `switch` | An on/off value flows can read and write. Starts off. |
| `number` | A number between `min` and `max` (0 and 100 by default), with a `step` (1) and an optional `unit`. Starts at `min`. |
| `button` | A press with no state: an event, not a value. A flow can trigger on it. |

The entity is `virtual_<id>`, on a device named `Virtual`. The id is 1 to 48 characters of `a-z`, `0-9` and `_`.

Create one with MCP `set_virtual` or `PUT /api/v1/virtual/{id}`, with a body such as:

```json
{ "kind": "number", "name": "Target temperature", "min": 15, "max": 25, "step": 0.5, "unit": "°C", "initial": 19 }
```

Remove it with `delete_virtual` or `DELETE /api/v1/virtual/{id}`.

### Behaviour

- A switch and a number echo on their own topic and are retained, so a command is its own state and survives a restart.
- The hub publishes the initial state, retained, only the first time the entity is created: `OFF` for a switch, `min` for a number, or `initial`. A flow's trigger needs a known previous value to see a change, so a first tap would otherwise go unnoticed.
- Calling `set_virtual` again on an existing entity republishes its config (a rename, other bounds) and keeps its state. Changing the kind clears the old config.
- A button is not retained: a press is an event. A flow `trigger.state` on a button fires with the value `PRESS`.
- A virtual entity is an ordinary entity otherwise. Commands go through `send_command`, and it moves to a room with the usual override.

The topics are listed in [MQTT topics](../reference/mqtt-topics.md). Flows use virtual entities as flags and manual triggers: see [Flows](flows.md) and [Flow nodes](../reference/flow-nodes.md).
