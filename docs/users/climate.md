---
title: Climate
sidebar_position: 10
description: How the climate engine decides each zone's heating or cooling, what it learns from history, and how eco, proposals, overrides and hand changes work.
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

The engine decides per zone from a priority tree, and every decision carries its `source` (`safety`, `override`, `holiday`, `eco`, `away`, `schedule` or `idle`) and a `reason`:

1. **Safety** comes first and always acts: frost, overheat, overcool, and a stale reading (no usable temperature, so no schedule, override or eco setpoint is sent to an active zone). A safety cause that holds stays until the reading is clearly back inside its limit.
2. **Pending**: a pending zone does nothing beyond safety.
3. **Zone override**: a whole-zone target and/or mode held for a number of minutes or until a time.
4. **Rules**: holiday, then eco (the zone's own eco, or the whole house's while nobody is home), then the schedule.

Per-device overrides (hand changes) sit on top of the rules and hold their device whatever the zone does.

A temperature set by hand on a device holds until the zone's next scheduled change, at most `manual_hold_max_min` minutes (240 by default).

The engine keeps deciding when history is off; only the learner pauses.

## Eco

Eco saves energy while a room, or the whole house, is empty. It is set once for the house, in the climate settings (`eco` in the house document):

- `heat` (16 °C by default) and `cool` (28 °C): what a zone holds in eco. Eco never uses more than the schedule: in heat the zone takes the lower of `heat` and its schedule, in cool the higher of `cool` and its schedule, a schedule `off` stays off, and a zone with no rules for the season stays idle.
- `actions`: extra settings for the zone's climate units while in eco. Today one kind exists, the fan speed: `[{"fan_mode": "low"}]`. Only units that list that fan mode get it; the others are left alone. When eco ends, each fan goes back to the speed it had, unless someone changed it in the meantime.

Eco starts two ways, and a zone is in eco while either one applies:

- **One room until a date** (`source: eco`). On the Climate screen, open the room and choose **Eco until…**, pick the end date and time, and start it. It ends by itself at that time, or with **Cancel eco**. Over REST it is `PUT /api/v1/climate/zones/{id}/eco` with `{"until": <unix seconds>}`; over MCP, `set_climate_eco`. Not available on a zone that is `off`.
- **The whole house while nobody is home** (`source: away`). Add an alarm panel to **Presence** in the climate settings: armed away (or vacation) means nobody home, disarmed or armed home or night means someone is. While the panel is arming, in its entry delay or triggered, it keeps its last answer. Phones and presence sensors can sit in the same list. `away_grace_min` (15 min) applies before the house goes to eco.

The two are independent: a bedroom in eco until Sunday stays in eco when the house is disarmed on Saturday. Holiday and overrides still come first, and a temperature set by hand during eco holds as usual.

## The learner and proposals

The learner builds a weekly schedule from the devices' target history (time held in eco, away, holiday, safety or an override is not evidence) in the history database and proposes it with the evidence behind it. Nothing applies until it is approved, through REST, MCP or the push button. The hub remembers a fingerprint of each rejected rule set. Requires [History](history.md).

The approval routes and the rest of the climate API are in the [HTTP API](../reference/http-api.md); the MCP tools are in [MCP tools](../reference/mcp-tools.md).
