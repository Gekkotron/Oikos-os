# Climate eco: room eco until a date, house eco while away

Status: design, 2026-10-25. Branch `feat/climate-away-and-room-eco-mode`. Builds on the climate engine v1 (`2026-10-22-climate-engine-design.md`, "v1" below) and the native alarm (`oikos_alarm_house`).

## 1. Intent

Two ways to save energy with one idea, **eco**:

1. **Room eco until a date.** Someone is away for a week: the user puts that room in eco until the day they come back. The room holds the eco temperature, then goes back to its schedule by itself.
2. **House eco while away.** When the alarm is armed away, every room holds the eco temperature. Disarming brings the schedules back.

Eco is set once, house-wide: a temperature (heating and cooling) and optional extra actions on the room's AC units, such as "fan speed 1". The user's words: "set only one eco value, for example 15 °C, in settings; when I'm away, eco is set on every room".

The two triggers are independent. A bedroom in eco until Sunday stays in eco when the user disarms on Saturday; the other rooms go back to their schedules.

## 2. Goals, non-goals, anti-goals

**Goals**
- One house-wide eco setting: `heat` and `cool` setpoints and a list of extra actions.
- Per room: "Eco until <date/time>", which starts now and can be cancelled early.
- House eco follows the alarm: an `alarm_control_panel` in the climate `presence` list says "nobody home" when armed away.
- Extra actions (v1: fan speed) are applied to the room's climate units on entering eco, and undone on leaving it, staying generic: only values the device itself advertises.
- Every eco decision says why ("Eco until Sun 00:00", "Nobody home"), like every other decision.

**Non-goals**
- A different eco temperature per room (the user: "one eco for the whole house is enough").
- A scheduled start ("eco from Friday to Sunday"); eco starts when it is set.
- Extra actions on devices other than the room's climate units (heater plugs, lights). That is the job of ambiances and flows.
- Pre-heating before eco ends.
- Swing, preset or other climate settings as extra actions in v1 (the list form leaves room; there is no swing command in `oikos-core` today).
- A minus-X-degrees offset mode (discussed and dropped for the single eco value).
- Notifications when eco starts or ends.

**Anti-goals**
- A colder or less safe house: safety rows (frost, overheat, stale) stay above eco.
- The engine fighting the user: a fan or target changed by hand during eco is left alone, and not "restored" over the user's choice when eco ends.
- House-specific code: no room names, vendor fan values (`"1"`, `"silence"`) or entity ids in code.
- A room whose eco silently never ends: an expired room eco is removed, and the room is back on its schedule within one evaluation.

## 3. Decided with the user

- One eco temperature for the whole house, in the climate settings.
- Room eco lasts until a date the user picks.
- House eco is triggered by the alarm in **armed away** only (not home, not night).
- Room eco and house eco are independent; a room is in eco while either applies.
- Eco may send extra actions to the room's AC units; the user's case is fan speed 1.

## 4. Settings

### 4.1 House (`<prefix>/house`)

`HouseSettings` gains:

```json
"eco": {
  "heat": 16.0,
  "cool": 28.0,
  "actions": [ { "fan_mode": "1" } ]
}
```

- `heat` [16 °C], `cool` [28 °C]: the eco setpoint per mode. Validation: `heat` ≥ `frost`; `cool` ≤ `overheat`. The defaults equal v1's `away_setpoint` defaults, so a house that used away keeps the same temperatures.
- `actions` [empty]: extra climate settings for eco, in order. v1 knows one action, `{ "fan_mode": "<mode>" }`. An action is an object with exactly one key, so later kinds (`swing_mode`, …) are new keys. Unknown keys are refused by validation with a plain reason. Two actions setting the same thing are refused.

### 4.2 Per-zone `away_setpoint` goes

`ZoneSettings.away_setpoint` is removed; away uses `house.eco`. `ZoneSettings` does not deny unknown fields, so retained zone documents that still carry `away_setpoint` load fine and lose the key on their next save. The web and MCP stop showing it.

### 4.3 Room eco (`<prefix>/zone/<area_id>/eco`)

A new retained document per zone, written by the user (web, MCP, HTTP) and deleted by the engine when it expires:

```json
{ "until": 1793570400, "set_at": 1792950000 }
```

- `until` (Unix seconds) must be in the future when set. An empty payload deletes it.
- Kept apart from `…/override` on purpose: overrides are "latest `until` wins", so a 2-hour boost set during a week of eco would lose to the eco. Eco sits below overrides (§5).
- The topic does not end in `/config` (v1 §4.2), and joins the existing `<prefix>/zone/+/+` subscription.
- Allowed on `active` and `pending` zones (a `pending` zone only shows what it would do, as in v1); refused on an `off` zone.

## 5. The decision

The v1 priority tree becomes:

1. Safety (unchanged)
2. Override: hand changes and explicit overrides (unchanged)
3. Holiday (unchanged: frost in heat, off in cool)
4. **Eco**, when the room's eco is live (`source: eco`) or nobody is home (`source: away`): `house.eco.heat` / `house.eco.cool` for the mode.
5. Schedule
6. Idle

**Eco only ever saves.** The user's purpose is to reduce consumption, so eco never makes a room use more than its schedule would:
- in heat, the room gets the **lower** of `eco.heat` and what the schedule asks for now; in cool, the **higher** of `eco.cool` and the schedule;
- a schedule `off` stays `off`;
- a zone with no rules for the current mode stays idle (a heat-only zone is not switched on to cool while the house is away, and the other way round).

When the schedule is already at or below eco, the decision keeps `source: eco`/`away` (the reason says "schedule already lower") so the fan actions of §7 still apply; an idle zone gets no eco and no fan action.

- Room eco reason: `Eco until Sun 00:00: 15.0 °C` (the date and time short and local, as overrides show them).
- Away reason: `Nobody home: eco 15.0 °C`.
- When both apply, the source is `eco` (the room's own choice), the reason mentions both: `Eco until Sun 00:00 (nobody home): 15.0 °C`.
- Per-device hand changes still hold on top of eco (`hold_devices`, v1 §7), and a hand change during eco holds until the zone's next scheduled change or `manual_hold_max`, as today.
- `Source` gains `Eco`. `Away` keeps its meaning (now at the eco setpoint), so the history rows and the web's `history_source("away")` keep working.
- The learner treats `eco` like `away` and `holiday`: not evidence (`learn.rs`), so a week of room eco never teaches the room a 15 °C schedule.
- The pure crate's `ZoneInputs` gains the zone's room eco (`Option<RoomEco>`); `decide` treats an eco whose `until` ≤ `now` as absent, so the decision is right even before the hub deletes the document.

## 6. House eco from the alarm

No new setting: the alarm panel is added to the existing `presence` list, which already means "entity ids that say someone is home" and already applies `away_grace`.

`home_of` (`crates/oikos-os/src/climate/inputs.rs`) learns the alarm state (`StateValue::Alarm`):

| Alarm state | Says |
|---|---|
| `armed_away`, `armed_vacation` | nobody home |
| `disarmed`, `armed_home`, `armed_night`, `armed_custom_bypass` | home |
| `arming`, `disarming`, `pending`, `triggered`, `unknown`, `unavailable` | the panel's last definite answer (none at boot) |

The last-answer rule keeps a triggered alarm, or the entry delay when coming home, from flipping the heating back on and off. The hub keeps that last answer per presence entity in memory; at boot an undecided panel has no opinion, as an unknown tracker does today.

The existing rule then decides: home if any presence entity says home, or none says anything definite. With only the alarm in the list, armed away means away after `away_grace` (15 min, which also covers a quick "forgot my keys").

This stays generic: any `alarm_control_panel` works (the hub's own `oikos_alarm_house` or a Zigbee keypad's), and phones can be mixed in.

## 7. Extra actions (fan speed)

**Which units.** Only units the eco decision drives: in the decision's `entities`, not `held` by a hand change, and not `off`. Sending a fan mode to an off unit can wake it on some bridges, and a held unit is the user's. A unit that becomes driven later in the same eco period (its hold ends) gets the actions then.

On **entering eco** (the zone's decision source becomes `eco` or `away` from something else), for each driven unit, for each action:

- `fan_mode`: if the unit lists the value in its `fan_modes` and its current `fan_mode` differs, remember its current `fan_mode` and send `SetFanMode`. A unit that does not list the value, or has no fan control, is left alone; the web shows which units an action applies to (§9).

On **leaving eco** (source moves from `eco`/`away` to `schedule`, `idle` or `holiday`):

- For each remembered unit: if its `fan_mode` is still the value eco set, send the remembered value back. If the user changed it meanwhile, leave it and drop the memory.

Moving between `eco` and `away` (room eco ends while the alarm is still armed away, or the other way round) is not leaving eco: nothing is undone or re-sent.

Safety and overrides interrupt eco only for the setpoint: while a safety or override row wins, the fan is left as it is, and the eco memory is kept. If the next decision is eco again, nothing is re-sent; if it is the schedule, the fan is restored as above.

The fan is not re-asserted: one command on entry, one on exit, retried like the setpoint on `accepted: false` (v1 §5.5, same 3-failure limit and push). A fan changed by hand during eco is not a hand change for the hold logic; it is simply left alone.

**Editing eco during eco.** The setpoints follow at the next evaluation, as any setting does. For the fan: a changed value is sent to units still at the old eco value; a removed action restores those units as on leaving eco; an added action applies as on entering. Units the user changed by hand are left alone in every case.

**The zone stops being managed.** When a zone in eco is switched `off`, or a unit leaves the zone, the remembered fans are restored once (if still at the eco value) and forgotten; the room eco document is kept and simply expires, so switching the zone back on before `until` resumes it. Disabling climate stops the engine as in v1: nothing is sent, the memory stays in the retained decision, and the restore happens when eco ends after climate is enabled again.

**Surviving a restart.** The remembered fan modes live in the retained decision's `entities` record (v1 §5.3), each entity gaining an optional `eco_restore: { "fan_mode": { "was": "auto", "set": "1" } }`: what the unit had before eco and what eco set, so the "still at the eco value" check compares against what was actually sent even if the setting changed since.

## 8. HTTP and MCP

- `GET /api/v1/climate`: each zone gains `eco: { until, set_at } | null`; `house` carries `eco`.
- `PUT /api/v1/climate/zones/{id}/eco` with `{ "until": <unix> }` sets or replaces it; `DELETE /api/v1/climate/zones/{id}/eco` cancels it. Refusals in plain English: past `until`, `off` zone, unknown zone. 503 when climate is disabled, like the rest.
- `PUT /house` accepts `eco` with the validation of §4.1.
- MCP: `set_climate_eco` (`zone`, `until` as RFC 3339 local or Unix seconds), `clear_climate_eco` (`zone`); `set_climate_settings` takes `eco`; `climate_status` and `get_climate_zone` show it. The MCP instructions' climate paragraph gets one sentence on eco.

## 9. Web

- **Settings sheet** (`climate/settings_sheet.rs`): an **Eco** group with the heat and cool setpoints and **Extra actions**: a list with "+ Add action". v1 offers "Fan speed", whose value is picked from the union of `fan_modes` of the house's climate units, and each row says how many units offer it ("on 4 of 4 units"). The away fields (`presence`, `away_grace`) sit next to it, with the presence picker's hint saying an alarm panel can be added ("armed away = nobody home").
- **Room panel** (`climate/room_panel.rs`): next to the hold controls, an **Eco until…** button that opens a date and time input (time default 00:00, so "until Sunday" means the room is back on its schedule from Sunday morning). While eco is live, the panel shows "Eco until Sun 00:00" with a **Cancel eco** button.
- **Room list** (`climate/room_list.rs`): a room in eco shows a small eco marker, so rooms in eco are recognisable at a glance.
- `source_word` and `history_source` learn `eco`. All new text in English and French through `i18n/climate.rs`, like the rest of the screen.

## 10. Docs

`docs/users/climate.md` gains a short "Eco" section: what eco does, room eco until a date, house eco with the alarm (add the alarm panel to Presence), extra actions. The engine's `docs/reference` pages, if they list the climate documents or tools, get the new topic and tools.

## 11. Constraints

- Generic code: no vendor fan values or entity ids; the defaults in §4.1 are the only new numbers in code.
- The decision rows and the eco enter/leave/restore logic are pure and unit-tested in `oikos-climate`: eco vs override vs holiday vs safety order, room eco and away together, expiry, restore when unchanged, no restore when changed by hand, `eco`↔`away` without a re-send, unit without the fan mode, restart with `eco_restore` present.
- `home_of` for alarm states, including the last-answer rule, has hub tests.
- QoS 1 and retained for the new document, like every v1 document.

## 12. Left to the builder

- Module layout and the exact Rust shape of `EcoSettings`, `EcoAction` and `RoomEco`.
- Where the eco enter/leave transition is computed (pure crate from previous and new decision is preferred).
- The look of the eco controls and marker, within the existing climate styles.
- Exact wording of reasons and labels.

## 13. Claude's calls (for the user to check)

- Eco never raises use: lower of eco and schedule in heat, `off` stays `off`, idle zones stay idle (from the user's "to reduce consumption"; found by the builder check).
- Fan actions go only to units eco drives (not held by hand, not off).
- Editing eco during eco applies at once; units changed by hand are never touched.
- A zone switched off during eco gets its fans restored once; its room eco is kept and resumes if the zone comes back before the date.

- Eco defaults: heat 16 °C, cool 28 °C (the old away values). The user sets 15 °C.
- Room eco starts immediately; no planned start date.
- Room eco ends at the chosen date and time, time defaulting to 00:00 of that day.
- Extra actions only touch the room's own climate units; v1 has one action kind, fan speed.
- Fans are restored on leaving eco only if still at the eco value; otherwise left as the user set them.
- House eco reuses the `presence` list (alarm panel added there) instead of a new "alarm entity" setting; `away_grace` applies to it.
- Transient alarm states (`arming`, `pending`, `triggered`) keep the last definite answer.
- The per-zone `away_setpoint` is removed in favour of the house eco setpoint.
- Holiday stays above eco and keeps going down to frost.
- Room eco on a `pending` zone is allowed (shown only); on an `off` zone refused.
- Decision source `eco` for room eco, `away` kept for nobody home.
