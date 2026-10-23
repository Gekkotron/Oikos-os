---
title: Climate
sidebar_position: 10
description: How the climate engine decides each zone's heating or cooling, what it learns from history, and how proposals, overrides and hand changes work.
---

:::caution Beta
The climate engine is still being tested. Its rules, settings and decisions may change between releases, and it commands real heating and cooling: watch what it decides before you leave a zone on it.
:::

With `[climate] enabled = true` the hub heats and cools by zone. The engine is off by default:

```toml
[climate]
enabled = false
# prefix = "oikos/climate"
```

Settings are retained documents under `[climate] prefix` (`oikos/climate` by default), edited over REST and MCP. While `enabled` is false the REST routes answer `503` and the MCP tools answer `climate disabled`.

## Zones

Each area with a climate device is a zone. A zone is in one of three states:

- `off`: the engine does not command its devices.
- `pending`: waiting for its first approved schedule. A pending zone acts only on safety.
- `active`: running its schedule and overrides.

## How a zone is decided

The engine decides per zone from a priority tree, and every decision carries its `source` (`safety`, `override`, `holiday`, `away`, `schedule` or `idle`) and a `reason`:

1. **Safety** comes first and always acts: frost, overheat, overcool, and a stale reading (no usable temperature, so no schedule, override or away setpoint is sent to an active zone). A safety cause that holds stays until the reading is clearly back inside its limit.
2. **Pending**: a pending zone does nothing beyond safety.
3. **Zone override**: a whole-zone target and/or mode held for a number of minutes or until a time.
4. **Rules**: holiday, then away (the away setpoints when nobody is home), then the schedule.

Per-device overrides (hand changes) sit on top of the rules and hold their device whatever the zone does.

A temperature set by hand on a device holds until the zone's next scheduled change, at most `manual_hold_max_min` minutes (240 by default).

The engine keeps deciding when history is off; only the learner pauses.

## The learner and proposals

The learner builds a weekly schedule from the devices' target history in the history database and proposes it with the evidence behind it. Nothing applies until it is approved, through REST, MCP or the push button. The hub remembers a fingerprint of each rejected rule set. Requires [History](history.md).

The approval routes and the rest of the climate API are in the [HTTP API](../reference/http-api.md); the MCP tools are in [MCP tools](../reference/mcp-tools.md).
