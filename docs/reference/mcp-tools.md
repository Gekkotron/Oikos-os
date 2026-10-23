---
title: MCP tools
sidebar_position: 3
description: The 34 tools and the one resource of the hub's MCP server, with their arguments.
---

The MCP server is at `http://<host>:8081/mcp` (streamable HTTP, stateful sessions) and offers **thirty-four tools** and **one resource**. It has the same trust model as the REST API on the same port: LAN-only, no authentication in v1. The tools are defined in [`crates/oikos-os/src/mcp`](https://github.com/Gekkotron/Oikos-os/tree/main/crates/oikos-os/src/mcp). For setup in Claude Code, Codex and Claude Desktop, see [Talking to the house (MCP)](../users/assistants-mcp.md).

A failed tool call answers in-band (`isError: true` with the reason as text) so the assistant can retry with better arguments. A refused publish (broker link down) is the same in-band error.

Claude Desktop only speaks stdio, so it launches `oikos-os mcp-stdio --server http://<host>:8081`, a proxy that relays every request to the running hub. It exits with `cannot reach Oikos-os MCP server at …` when the hub is down; its logs go to stderr and stdout carries the protocol.

## Resource

| URI | Content |
| --- | --- |
| `oikos://snapshot` | The whole house as one JSON document (`application/json`): connection, bridge, areas, devices, entities with their states, ignored discovery topics. |

## House

| Tool | What it does |
| --- | --- |
| `bridge_info` | The Zigbee2MQTT bridge (version, coordinator, permit join, online or not) and the state of the link to the broker. |
| `list_areas` | The rooms, each with its entity count and the count of entities shown by default (`primary_count`): declared areas first, then those only carried by entities, then `Unassigned`. |
| `set_areas` | Replaces the declared rooms (retained on `oikos/ui/_areas`) and returns the resulting list. Args: `areas` (each with `id`, `name`, optional `icon`, `order`, `groupNames`, `groupFavorites`). A room kept by id keeps the fields you leave out. |
| `list_devices` | The Zigbee devices with availability, battery and link quality. Args: optional `area` (an area id), `query` (case-insensitive, matched against name, model and vendor). |
| `list_entities` | A page of entities with their state. By default only what the app shows (`include: "primary"`: no diagnostic, config, disabled-by-default, folded or hidden entity); `include: "all"` for everything. Args: `area`, `kind`, `device_id`, `query` (entity name and id), `changed_since` (unix seconds), `include`, `limit` (100 by default, 500 at most), `offset`. The reply has `total` (matches before paging) and `items`. |
| `get_entity` | One entity with its state, its MQTT `bindings` (the expanded discovery config) and its `raw_override`. Args: `id`. |
| `search` | Devices (name, model, vendor) and entities (name, id, or belonging to a matching device) containing the query. Args: `query`, optional `limit` (20 by default, 500 at most). |
| `list_ignored` | The discovery topics that did not become an entity, with the reason. |

## Overrides

| Tool | What it does |
| --- | --- |
| `set_override` | Changes fields of the entity's `oikos/ui/<id>` override and returns the entity. Args: `id`, `patch` (an object; `null` clears a field). Unknown fields and invalid values are refused and nothing is written. The fields are listed in [Entity types](entity-types.md#overrides). |
| `clear_override` | Removes the override entirely and returns the entity as discovered. Args: `id`. |

## Commands

| Tool | What it does |
| --- | --- |
| `send_command` | Publishes a command to an entity and reports `accepted: true` once the next state echo confirms it, or `accepted: false` with the reason. Args: `id`, `command`. An unknown or offline entity, or a command the entity cannot take, is refused before anything is published. |

The `command` is tagged by `type`: `turn_on` (optional `brightness` 0-255, `color` `{h, s}`, `effect`), `turn_off`, `toggle`, `set_position` `{position 0-100}`, `open`, `close`, `stop`, `set_temperature` `{temperature}`, `set_hvac_mode` `{mode}`, `set_fan_mode` `{mode}`, `set_number` `{value}`, `select_option` `{option}`, `press`, `arm` `{mode, code?}`, `disarm` `{code?}`, plus the vacuum and water-heater commands the entity's capabilities list. A generic entity (lock, fan, siren, scene, text and so on) takes `{"type": "control", "control": "<name>", "value": …}` with a name from its `capabilities.generic.controls`; `value` is a bool, number or string by the control's shape and absent for a press. A control with no state to echo is accepted as soon as it is published.

## Flows

See [Flow nodes](flow-nodes.md) for the document format.

| Tool | What it does |
| --- | --- |
| `list_flows` | The flows, one summary each: `id`, `name`, `enabled`, `area`, `invalid` (why it cannot run), `nodes` (count) and `last_run`. |
| `get_flow` | One flow in full: definition, why it is invalid, the run in progress and the recent runs (newest first) with each node's outcome. Args: `id`. |
| `set_flow` | Creates or replaces a flow, validated against the house and saved retained on `oikos/automation/<id>`. Args: `flow` (the whole document). Returns `{flow, warning}`; `warning` is set when the flow reads the sun against the Paris fallback. |
| `delete_flow` | Removes a flow, valid or not. Args: `id`. |
| `set_flow_enabled` | Turns a flow on or off without touching its definition. Args: `id`, `enabled`. |
| `run_flow` | Starts a manual run and returns the run as it starts. Refused when the flow is disabled, invalid or already running. Args: `id`. |

:::note
The argument schema of `set_flow` that the server advertises still describes four node types (`trigger.state`, `if`, `delay`, `command`). The server accepts all nine; the server instructions it sends name the others (`trigger.schedule`, `trigger.sun`, `trigger.signal`, `flow.enable`, `notify`).
:::

## Groups

Groups need `[groups] enabled = true`; otherwise these tools answer `groups disabled`. See [Groups and virtual entities](../users/groups-and-virtual-entities.md).

| Tool | What it does |
| --- | --- |
| `list_groups` | Every group the hub stores, sorted by id, with its members and the flows that use it. |
| `set_group` | Creates or replaces a group, the entity `group_<id>`. Args: `id` (1 to 48 characters of `a-z`, `0-9`, `_`), `spec` (`kind`, `name`, `members`, optional `hideMembers`, true by default). Members must be of the group's kind and cannot be groups. |
| `delete_group` | Removes a group and clears what the broker retained for it. The UI override is kept. Args: `id`. |

## Virtual entities

| Tool | What it does |
| --- | --- |
| `set_virtual` | Creates or replaces a virtual `switch`, `number`, `button` or `select` the hub owns, as `virtual_<id>`. Args: `id` (1 to 48 characters of `a-z`, `0-9`, `_`), `spec` (`kind`, `name`; for a number `min`, `max`, `step`, `unit`; for a select `options`; `initial`). |
| `delete_virtual` | Removes a virtual entity. Args: `id` (without the `virtual_` prefix). |

## Notifications

| Tool | What it does |
| --- | --- |
| `send_notification` | Sends a phone push and returns its alert id. Args: `title` (required), optional `body`, `channel` (`alarm`, `security`, `camera`, `energy`, `climate`, `system`, `misc`; default `misc`), `severity` (`info`, `warning`, `critical`; default `info`), `entity_id`, `image_url`, `silent`, `id`. Sending again with the same `id` replaces the earlier alert; without one the id is `mcp-<unix seconds>-<n>`. |
| `list_alerts` | The recent push alerts, newest first. Args: optional `limit` (20 by default), `channel`. |

## History

| Tool | What it does |
| --- | --- |
| `get_history` | One slice of an entity's history, available only when `[history] url` is set. Args: `entity`, `what`, and by `what`: `series` needs `range` (`24h`, `7d`, `30d`, `1y`); `daily` needs `days` (7, 30 or 365) and `kind` (`counter` or `power`); `today` needs `kind`; `last_change` needs nothing more. The reply is a one-field object named after `what`. Errors read `history_unavailable: …` or `not_recorded: …`. |

## Climate

The climate engine is in beta. These nine tools answer `climate disabled` while `[climate] enabled` is `false`. See [Climate](../users/climate.md).

| Tool | What it does |
| --- | --- |
| `climate_status` | The house settings and every zone with its reading, decision and why (`source`, `reason`), overrides, pending proposal and learner state. |
| `get_climate_zone` | One zone as `climate_status` shows it, with its last 20 decisions. Args: `zone`. |
| `set_climate_settings` | Replaces the house settings. Read them with `climate_status` first and send the whole document back. Args: `settings`. |
| `set_climate_zone` | Replaces a zone's settings (`state` `off`, `pending` or `active`, targets, rules). New non-empty rules count as approved. A device another zone lists in `entities` is refused. Args: `zone`, `settings`. |
| `approve_climate_proposal` | Approves the pending proposal; its rules, or `rules` when you edit them, become the zone's schedule. Refused when there is none. Args: `zone`, optional `rules`. |
| `reject_climate_proposal` | Rejects the pending proposal; the same proposal is not made again for a week. Args: `zone`. |
| `set_climate_override` | Holds a temperature and/or a mode on a zone. Args: `zone`, `setpoint` and/or `mode`, and exactly one of `minutes` and `until` (unix seconds). Safety still acts. |
| `clear_climate_override` | Removes every override of a zone. Args: `zone`. |
| `learn_climate_zone` | Learns a zone's rules from its history now and returns where the learner stands. Args: `zone`. |

## Count

Nineteen tools live in `tools.rs` (house, overrides, command, groups, virtual, history, notifications), six in `flow_tools.rs` and nine in `climate_tools.rs`: 19 + 6 + 9 = 34.
