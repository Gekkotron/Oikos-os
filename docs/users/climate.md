---
title: Climate
sidebar_position: 10
description: How the climate engine decides each zone's heating or cooling, what it learns from history, and how eco, proposals, overrides and hand changes work.
---

:::caution Beta
The climate engine is still being tested. Its rules, settings and decisions may change between releases, and it commands real heating and cooling: watch what it decides before you leave a zone on it.
:::

With climate on, the hub heats and cools by zone. The engine is off by default. Turn it on with the **Climate** card in the Services step of the setup wizard (**Settings → Run setup again** reopens it; nothing is wiped), where it is marked Beta. It is the card's **Enabled** switch. Everything else is edited on the Climate screen, over REST and over MCP.

The one low-level field, the topic `prefix` (`oikos/climate` by default), has no screen: change it with `PUT /api/v1/settings/climate` or the `set_settings` MCP tool, which restarts the hub (see the [Settings reference](../reference/configuration.md#climate)).

Settings are retained documents under that prefix. While climate is off the REST routes answer `503` and the MCP tools answer `climate disabled`.

## The Climate screen

The **Climate** tab appears when climate is on. It lists the rooms with their state, the current reading and what the engine decided (the source and the reason). Open a room to see its panel:

- **Zone state**: Off, Pending or Active.
- **Room thermometer**: Automatic (the room's own temperature sensor when it has one, else the mean of its climate devices' readings) or a sensor you pick.
- **Hold temperature**: a zone override, **Until next change** or until a time you set. Active holds are listed with a **Clear** button.
- **Eco until…** and **Cancel eco** (see [Eco](#eco)).
- **This week**: the approved schedule as a week calendar, with a base temperature and coloured blocks. **Create schedule** starts one when there is none, **Edit schedule** changes it, and you tap an empty spot to add a block.
- **Proposed schedule**: what the learner found, with **Approve**, **Reject** and, after you edit it, **Approve edited**. **Learn now** runs the learner on demand.
- **Recent decisions**: the engine's last decisions for the room.

The **Settings** button opens **Climate settings**, the house document (see below). Banners explain when learning is off or [History](history.md) is not set up.

## House settings

These are the house-wide settings, with their defaults:

- **Season**: `auto` (default), `heat`, `cool` or `off`. In `auto` the mean outdoor temperature picks heat when it is below **Heat below** (`heat_below`, 16 °C) and cool when it is above **Cool above** (`cool_above`, 24 °C); in between the previous mode holds. With no outdoor reading it heats. **Outdoor temperature** (`outdoor_entity`) names the sensor.
- **Holiday** (`holiday_entity`): an entity that, while on, puts every zone in holiday: frost temperature in heat, off in cool.
- Safety limits: **Frost** (`frost`, 7 °C), **Overheat** (`overheat`, 30 °C), **Overcool** (`overcool`, 16 °C), and **Stale after** (`stale_after_min`, 60 minutes without a reading).
- Learner: **Learning window** (`learn_days`, 28), **Minimum days** (`learn_min_days`, 10), **Agreement needed** (`learn_support`, 0.6) and **Learn at** (`learn_at`, 03:30).
- **Presence**, **Away grace** and the eco settings (see [Eco](#eco)).

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

A temperature set by hand on a device holds until the zone's next scheduled change, at most `manual_hold_max_min` minutes (240 by default). Only an active zone watches for hand changes: what is set by hand while a zone is off is not held when it is switched back on, and the schedule applies.

The engine keeps deciding when history is off; only the learner pauses.

## Eco

Eco saves energy while a room, or the whole house, is empty. It is set once for the house, in the climate settings (`eco` in the house document):

- `heat` (16 °C by default) and `cool` (28 °C): what a zone holds in eco. Eco never uses more than the schedule: in heat the zone takes the lower of `heat` and its schedule, in cool the higher of `cool` and its schedule, a schedule `off` stays off, and a zone with no rules for the season stays idle.
- `actions`: extra settings for the zone's climate units while in eco. Today one kind exists, the fan speed: `[{"fan_mode": "low"}]`. Only units that list that fan mode get it; the others are left alone. When eco ends, each fan goes back to the speed it had, unless someone changed it in the meantime.

Eco starts two ways, and a zone is in eco while either one applies:

- **One room until a date** (`source: eco`). On the Climate screen, open the room and choose **Eco until…**, pick the end date and time, and start it. It ends by itself at that time, or with **Cancel eco**. The decision reads "Eco until Tue 3 Nov 19:00: 14.0 °C". Over REST it is `PUT /api/v1/climate/zones/{id}/eco` with `{"until": <unix seconds>}`; over MCP, `set_climate_eco`. Not available on a zone that is `off`.
- **The whole house while nobody is home** (`source: away`). The hub's own [alarm](alarm.md) armed away always means nobody home, even with an empty **Presence** list and even when a phone in the list still says home. Disarmed, it leaves the decision to the list. Another alarm panel can be added to **Presence** in the climate settings: armed away (or vacation) means nobody home, also over the phones, and disarmed or armed home or night means someone is. While a panel is arming, in its entry delay or triggered, it keeps its last answer. Phones and presence sensors can sit in the same list: the house is home when one of them says so. The Presence field opens the [entity picker](web-ui.md#picking-an-entity) on the alarm panels, phones and presence sensors; tick each one, then Done. When the phones and sensors all say away, `away_grace_min` (15 min) passes before the house goes to eco. An alarm armed away needs no grace: eco starts on the next pass, so a schedule block that begins after you arm starts in eco, and disarming brings the schedule straight back.

The two are independent: a bedroom in eco until Sunday stays in eco when the house is disarmed on Saturday. Holiday and overrides still come first, and a temperature set by hand during eco holds as usual.

## Thermometer correction

A climate unit often reads its own surroundings, which can sit far from the room's temperature. With **Thermometer correction** on, the hub nudges the unit's target from the room thermometer's reading so the room, not the unit, reaches the schedule target. It is off by default and set per room, on the room's card on the Climate screen; it needs a room thermometer (pick one in the same card).

It acts only on the **Schedule** row of an active room with a temperature target: never under holiday, eco or away, nor under a whole-room hold (the hold buttons on the room card). A hand change on one unit leaves only that unit out; the others keep being corrected, and if every unit is held the offset stays where it is.

The room reading is checked on every pass. Once it is `band` (0.5 °C) off the target, the hub moves the unit's target by `step` (0.5 °C) to close the gap, at most once per `interval_min` (15 min), and never more than `max_below` (3 °C) under or `max_above` (0.5 °C) over the target. `warmup_min` (60 min after the schedule target began) delays only the slow side: in heat, raising the unit's target when the room is too cold; in cool, the extra cooling when the room is too warm. The other direction can happen at once. Inside the band nothing changes, and the offset stays where it is. It goes back to 0 when the schedule target changes, the Schedule row stops applying, the room's settings change, the thermometer goes quiet, or the hub restarts.

If the thermometer has not reported for `quiet_limit_min` (6 h), the correction is dropped and the unit follows the schedule target alone. Under the room's reading, the screen shows the thermometer's value and its age once it is 15 min old or more.

Each correction is written in the decision's reason, for example "room above 19.5 °C: AC 18.5 °C (-0.5)", and appears under **Recent decisions**. The settings live in the zone's `feedback` block (`enabled`, `band`, `step`, `interval_min`, `warmup_min`, `max_below`, `max_above`, `quiet_limit_min`).

## The learner and proposals

The learner builds a weekly schedule from the devices' target history (time held in eco, away, holiday, safety or an override is not evidence) in the history database and proposes it with the evidence behind it. Once a schedule is approved, a part of it changes only where enough days (**Agreement needed**) held the same other value there: hand changes that vary from day to day keep the approved block, and days with no data keep theirs. Nothing applies until it is approved, through REST, MCP or the push button. The hub remembers a fingerprint of each rejected rule set. A room in eco is not learned until its eco ends (its learning state reads `eco`, "Learning paused while in eco"), so no new proposal arrives for it meanwhile; a proposal made before the eco stays. Requires [History](history.md).

The approval routes and the rest of the climate API are in the [HTTP API](../reference/http-api.md); the MCP tools are in [MCP tools](../reference/mcp-tools.md).
