# Silent trigger, Limit card and first-value triggers in flows — design

Date: 2026-10-30.

## Why

A smoke or carbon monoxide detector is only useful while it is alive. Today a flow can push an
alert when a detector goes off, but nothing notices a detector that has stopped talking
(dead battery, out of range, unpaired): it looks exactly like a quiet house.

Two gaps make this impossible with the cards that exist:

1. `trigger.state` fires only when a value *changes*. A detector that keeps reporting "no
   smoke, battery 100 %" changes nothing, so a watchdog built from `trigger.state` + `delay` in
   `restart` mode can never tell a healthy detector from a dead one. The hub does see every
   report (`StateSeen`), flows just cannot use it.
2. Nothing caps how often a branch runs. A "detector is silent" alert on a device that checks
   in about once an hour would push every hour, all day.

While wiring the detectors a third problem showed up: a detector whose value was never
reported (`unknown`) and then reports `true` is ignored by `trigger.state`, because the engine
records a value *appearing* silently. Some detectors only include their alarm field once it is
set, so their **first real alarm is skipped**. For a safety alert that is not acceptable.

**Success:** a user builds, with no JSON, a flow per detector:
*alarm → critical push*, *battery low → warning*, *silent for 50 min → Limit 4 per 24 h →
warning push*; and a detector whose alarm field was unknown still fires its alarm push.

**Generic:** nothing names a vendor, a model, a room or a house. The cards work on any entity.

## Goals

1. **New trigger `trigger.silent`**: fires when an entity has sent no report for a duration.
2. **New node `limit`**: lets a run through at most N times per rolling window.
3. **`trigger.state` fires on unknown → value** when it names an explicit `to`, under the rule
   below.
4. Web flow editor (card library, canvas card, editing sheet), i18n en/fr, MCP description and
   server instructions, user and reference docs.

## Non-goals

- A "back online" push or trigger when a silent entity reports again (user: nothing should
  happen then).
- Watching Zigbee2MQTT availability (`online` / `offline`); `trigger.silent` is about reports.
- Persisting Limit counts or silence clocks across a hub restart.
- A second "over the limit" output on `limit`.
- The Android app.

## Anti-goals

- No alert storm: a hub restart, a flow save or a broker reconnect must never make every
  `trigger.silent` fire at once.
- `trigger.state` must keep its promise of never firing at boot, on a broker replay or on a
  restore of a recorded value just because the engine learned a value.
- Saved flows without the new nodes load, validate and run exactly as today.

## `trigger.silent`

```json
{ "id": "t", "type": "trigger.silent", "params": { "entity": "<entity id>", "for": "50m" } }
```

- `entity`: any entity the house has (validated like `trigger.state`'s). `for`: a duration in
  the `delay` format (`parse_duration`, units `s`, `m`, `h`), at least `1m`, at most `168h`
  (*my call*: below a minute is noise, above a week is not a liveness check).
- **A report** is any state message the hub ingests for that entity in the current link epoch,
  changed or not: exactly what produces a `StateSeen` today. A restore of a recorded value is
  not a report (it emits no `StateSeen`). A retained replay after a reconnect counts as a
  report (*my call*: harmless, the clock restarts at reconnect anyway).
- **The clock** for each (flow, node) starts when the node is armed: engine start, flow save or
  enable, broker link back online. Each report of the entity restarts it. When it reaches
  `for`, the trigger fires **once**; it does not fire again until the entity reports and then
  goes silent for `for` again.
- Editing a flow keeps a node's clock when the node's `entity` and `for` are unchanged
  (same rule as `arm_schedules`: a node armed with the same parameters is not re-armed);
  a changed node is re-armed from now.
- **Broker link down:** no report can arrive, so clocks are frozen: nothing fires while the
  link is down, and every clock restarts from the moment the link is back (*my call*: a broker
  outage is not every device going silent; the existing "broker link is offline" skip would
  otherwise record a skipped run per detector).
- The trigger value is the `for` text (`"50m"`), so `{trigger}` in a notify prints it.
  The run's trigger info names the entity.
- Mode `single` applies as for any trigger. A fire that `single` records as skipped still
  uses up the one shot: the trigger stays latched until the entity reports again (*my call*).
- **Lost reports:** when the engine's `StateSeen` receiver lags, reports may have been missed,
  so every silent clock restarts from that moment instead of risking a false "silent"
  (*my call*; the lag hook today ignores `Lagged` on that channel).
- **Command echoes count:** a Zigbee2MQTT state echo after a `/set` is a report like any other
  (*my call*: it comes back only when the device answered, and detectors are rarely commanded).
  The clock is fed in the engine from `StateSeen`; no change to emission in `hub.rs`.

## `limit`

```json
{ "id": "l", "type": "limit", "params": { "max": 4, "per": "24h" } }
```

- `max`: 1 to 100. `per`: a duration in the `delay` format, `1m` to `168h`.
- A run reaching the node passes when fewer than `max` runs passed this node in the last `per`
  (rolling window, *my call*: no midnight reset, so no burst around midnight). A pass is
  counted when it happens. Over the limit, the run **ends at this node**: the step is recorded
  with outcome `ok` and detail like `limited: 4 in the last 24h` (no new outcome value, *my
  call*: the run did what it was told; the builder may add a distinct outcome if the web run
  log needs it to show the step differently), and the run status is `done`.
- One output, no port.
- The count is kept in the engine, in memory, per (flow id, node id). A hub restart forgets
  it. Saving the flow keeps it while the node exists with the same `per` and `max`; removing
  the node or deleting the flow drops it (*my call*). Disabling a flow keeps it.

## `trigger.state`: unknown → value

Today a change counts only "between two present, different values"
(`engine.rs` `state_changed`). New rule, added to that:

- A `trigger.state` with an explicit `to` and **no** `from` also fires when the watched value
  goes from unknown to a value equal to `to` — **only** if the unknown was confirmed by a real
  report: a `StateSeen` for that entity, in the current link epoch, arrived while the value
  was unknown.
- **The confirmation is per (entity, field), like `known`, and comes from an earlier report.**
  The engine handles `StateChanged` before the `StateSeen` of the same report (biased
  `select!`), so the confirming `StateSeen` is one processed *before* the change: when a
  `StateSeen` arrives (current epoch) and the engine's known value for a watched key is unknown,
  the key is marked "confirmed unknown". Only reports the engine processes while the key is
  watched count; a report from before a flow save is not seen. The mark is cleared when the key
  gets a value, when the link drops (with `known`) and when the key stops being watched.
- A value that vanishes and comes back (`true → unknown → true`, with a confirming report in
  between) fires again (*my call*: a duplicate alarm push is better than a missed one).
- Still silent: the first sighting after boot or reconnect, a restore of a recorded value
  (no `StateSeen`), a seed at flow save, a value vanishing (value → unknown), and every trigger
  without `to` or with a `from`.
- So a detector that has reported at least once without its alarm field, then reports
  `alarm: true`, fires; a hub that boots and restores `true` from the database does not.

## Web

- Card library: **"Silent for"** under triggers (sheet: the entity picker and a duration
  field), **"Limit"** under logic, next to Delay (sheet: "at most [N] times every [duration]").
- Canvas card titles in `flow_text.rs`, en/fr strings in `i18n/flows.rs`, like the off-peak
  cards. The run log shows the Limit step's detail text.
- Card colours/icons follow the existing trigger and logic families.

## MCP and docs

- `set_flow` schema and description list both nodes; the oikos MCP server instructions
  mention `trigger.silent`, `limit` and the unknown → value rule.
- `docs/reference/flow-nodes.md` (two new sections, `trigger.state` note), `docs/users/flows.md`
  (a short "Alive checks" example: silent → limit → notify).

## Testing

- core: JSON round-trips; validation (unknown entity, `for` / `per` out of bounds, `max` 0 and
  101, a `limit` with a port on its outgoing edge).
- engine, with the test clock:
  - silent: fires once after `for`; a report restarts the clock; does not repeat while still
    silent; re-arms after a report; nothing at boot; link down freezes, link up restarts;
    flow save with the same node keeps the clock, changed `for` re-arms.
  - limit: passes `max` times, ends the run after; a pass leaves the window after `per`;
    count survives a flow save, dropped when the node is removed.
  - unknown → value: fires after a confirming `StateSeen`; silent on restore, first sighting,
    seed, and with `from` set.
- web: card library entries, sheet edits round-trip into the definition.

## After merge (outside the code)

Rebuild the live hub and web, then extend each detector flow with
`trigger.silent (50m) → limit (4 / 24h) → notify (warning)`, and put a `limit` in front of any
battery warning that can repeat.

## Left to the builder

Timer wiring in the engine (one deadline across all silent clocks, as for schedules and suns),
where the Limit counts live, exact web sheet layout, test file placement.
