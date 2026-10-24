---
title: Ambiances
sidebar_position: 7
description: One-tap house states made of actions, the four action types, how categories pick their devices, and how to read the report after a run.
---

An **ambiance** is a named list of actions that the hub runs once, top to bottom, when you apply it. "Bonne nuit" might turn off every light and every plug and close the shutters. Nothing stays set afterwards: the hub does not remember the ambiance, only the report of the last run.

An ambiance belongs to the house, not to a room. A room only appears inside an action, for example "every light in the living room".

```json
{
  "id": "good_night",
  "name": "Good night",
  "subtitle": "The house goes to sleep",
  "icon": "moon",
  "pinned": true,
  "order": 1,
  "actions": [
    { "type": "category", "what": "lights", "to": "off" },
    { "type": "category", "what": "plugs", "to": "off" },
    { "type": "category", "what": "covers", "to": "close" }
  ]
}
```

`id` is 1 to 48 characters of `a-z`, `0-9` and `_`. `name` is required (60 characters at most); `subtitle` (80 characters at most) and `icon` are optional. The icon is one of `sofa`, `moon`, `door_out`, `book`, `sun`, `sparkle`, `film`, `plate`, `bath`, `leaf`, `flame` or `house`. `pinned` and `order` are stored with the definition for the interfaces that show them.

## The four action types

| Type | What it does | Example |
| --- | --- | --- |
| `category` | Sets every entity of a category, for the whole house or one area. | Every light off |
| `device` | Sends one command to one entity, the same command `send_command` takes. | Set a select to `night` |
| `climate` | Holds a heating zone (or every zone) at a setpoint and/or mode for some hours, or clears those holds. | Hold every zone at 16 for 12 hours |
| `flow` | Enables, disables or runs one flow. | Run the "away lights" flow |

### Category

`what` is one of:

- **`lights`**: every light, including switches you show as a light. `to` is `on` or `off`. With `on` you can set `brightness` (1 to 100 %); a light that cannot dim just turns on.
- **`plugs`**: every switch that is a plain switch or a plug. `to` is `on` or `off`. Door, siren and chime switches are never plugs, so a house-wide "plugs off" never touches them. A relay you do not want switched off goes in `except`.
- **`covers`**: every shutter, blind or cover. `to` is `open` or `close`.

A category is not a fixed list. The devices are picked **when the ambiance is applied**, from the live house, so a light or plug you add next month is included without editing the ambiance. The rules behind the selection:

- Disabled, diagnostic, configuration and folded entities are not part of a category. Entities hidden from the home screen are included: hidden is a display choice.
- Groups are left out, so a light that belongs to a group is not commanded twice.
- `area` limits the category to one room. Leave it out for the whole house.
- `except` lists entity ids to leave alone.
- An entity that is **already** in the target state is not commanded. A light already off under "every light off" counts as done, so the report stays clean when half the house is already off.
- An entity that is offline is not commanded. It is listed as skipped with the reason `offline`.

### Device

`entity` is one entity id and `command` is the command `send_command` accepts, for example `{ "type": "select_option", "option": "night" }` for a select. The command is checked against the entity when you save the ambiance, with the same rules as a flow command.

Alarm codes are refused in an ambiance, because the broker would store them in clear.

### Climate

A climate action needs the climate engine on (`[climate] enabled = true`, see [Climate](climate.md)). While it is off, saving an ambiance with a climate action is refused. If the engine is turned off later, the action is reported as skipped.

- `zone` is a zone id, or `null` for every zone that is active when the ambiance is applied.
- `setpoint` is a temperature in °C, or `"off"`. `mode` is `heat` or `cool`. Give at least one of them, with `hours` (1 to 720) as the duration of the hold.
- `clear: true` removes the holds on the zone, so its schedule applies again. It does not take `setpoint`, `mode` or `hours`.

The engine needs a duration, so "until I come back" is written as a long hold, paired with a clear action in the coming-home ambiance.

### Flow

`flow` is a flow id and `do` is what to do with it:

- `enable` or `disable` sets the flow's `enabled`, like a `flow.enable` node does in a flow.
- `run` starts a manual run. The ambiance does not wait for the run to finish. Use it for anything an ambiance cannot do by itself, such as delays, conditions or notifications.

## Applying an ambiance

Actions run in order. Within one action, its commands go out together, and the next action starts once they have all been accepted or have failed. The commands use the same acceptance rules and timeouts as any other command.

- A failure never stops the run. Every action reports what happened.
- Only one run of the same ambiance can go at a time. A second request while it runs is refused with `already running`. Two different ambiances can run together.
- The broker link must be online, or the apply is refused.
- A restart of the hub ends a run, and clears the running state.

### References that disappear

Saving checks every reference: unknown entities, areas, flows, zones and `except` ids are refused. If something is removed **after** the ambiance was saved, the ambiance stays valid and listed. Applying it reports that action as skipped with `no longer exists`, and the other actions still run.

## Reading the report

Applying an ambiance returns a report, with one line per action, in order:

| Field | Meaning |
| --- | --- |
| `outcome` | `ok`, `failed` or `skipped` (see below). |
| `total` | How many entities (or zones) the action targeted. |
| `failed` | The entities that did not take the command, each with a `reason`. |
| `skipped` | The entities left out, each with a `reason`, such as `offline`. |
| `reason` | Why the whole action was skipped, when it was. |

The outcome is:

- **`failed`** when anything failed.
- **`skipped`** when nothing was attempted. The action's `reason` says why: `no matching entities` (a category with no device in it), `no active zone` (a climate action with no zone running), `the climate engine is off`, `no longer exists`.
- **`ok`** otherwise. An action that skipped some offline entities is still `ok`; the offline ones are listed under `skipped`, so look there if a device did not change.

The report is returned to whoever applied the ambiance and sent on the WebSocket to the other open clients. The hub does not keep it, so once the page is closed it is gone.

## Creating and applying ambiances

Ambiances are created and applied through the REST API or the MCP tools. See [HTTP API](../reference/http-api.md#ambiances) and [MCP tools](../reference/mcp-tools.md#ambiances). An assistant connected through MCP can list, create, change, delete and apply them with `list_ambiances`, `set_ambiance`, `delete_ambiance` and `apply_ambiance`.

Each ambiance is a retained message on `oikos/ui/ambiances/<id>`, so it survives a restart of the hub. An ambiance whose stored definition cannot be read is kept and listed with its reason, and it cannot be applied until it is fixed or deleted.
