# Off-peak from a device, plus an off-peak trigger and check in flows — design

Date: 2026-10-30.

## Why

Today the hub knows off-peak hours only from the windows typed in Settings › Energy › Off-peak
(`HomeSettings.off_peak`, `oikos_core::energy::tariff`). Some houses have a device that reports
the grid's tariff period itself — a smart meter reader that publishes the current tariff as a
text state (`HC..` / `HP..`), or an on/off sensor. For those houses the device is the truth:
the utility can move the hours, and the windows go stale.

Flows also cannot react to off-peak: there is no way to say "when off-peak starts, turn the
water heater on", nor "only if we are in off-peak".

**Success:** a user with such a device picks it in the Off-peak card, says which of its values
mean off-peak, and builds a flow "when off-peak starts → switch the water heater on" with no
JSON. A user without such a device builds the same flow from the windows alone.

**Generic (user requirement):** nothing names a vendor, a meter model or a country. The device
is any entity the hub knows; the values that mean off-peak are chosen by the user.

## Goals

1. **Settings — an optional off-peak device**, beside the windows (user decision: keep the
   windows *and* support a device).
2. **One resolved "is it off-peak now?"** in the hub, used by flows.
3. **New flow trigger `trigger.off_peak`** — fires when off-peak starts or ends.
4. **New flow check `if` with `op: is_off_peak`** — the "value getter", modelled on `is_dark`:
   no entity, no value, ports `true` / `false`.
5. **The Energy day chart follows the device's history** (user decision): the off-peak /
   peak kWh split and the off-peak bands come from what the device reported at each moment.
6. Web editor, i18n (en/fr), MCP, docs follow.

## Non-goals

- Tariff prices, Tempo/day-colour logic, more than two periods. A device that reports several
  off-peak codes is handled by selecting all of them as off-peak values.
- Notify text tokens (`{…}`) for the off-peak state.
- The Android app.

## Anti-goals

- A house with no device set must behave exactly as today; saved settings and flows without the
  new fields must load and save unchanged.
- The hub must never fire `trigger.off_peak` at boot or on a flow save just because it learned
  the current state — only on a real change while running.
- No per-vendor list in the code or the web.

## Settings model

`HomeSettings` gains one optional field (absent / `null` = no device):

```json
"off_peak_device": { "entity": "sensor.meter_current_tariff", "values": ["HC.."] }
```

- `entity`: any entity id. `values`: the state values (as strings) that mean off-peak; every
  other value means peak. Non-empty.
- For an on/off entity the web preselects `["on"]`.
- Matching uses the entity's flow scalar (`oikos_core::flow::scalar`), compared the way an `if`
  with `op: eq` compares it to a string value, so a value picked in the Off-peak card matches
  exactly what the If value picker would offer (*my call*: no trimming, no case folding; meters
  publish fixed codes).
- Validation runs in `home::validate`, only on `set_home_settings` (as for `mode_entity` /
  `indoor_entity`; the retained document is never re-validated on load, so a device that has
  not announced yet at boot cannot block settings). An id the registry does not know, an empty
  `entity` or empty `values` is a 422 naming `off_peak_device`. Any entity kind is accepted.
- `off_peak_history` (dated windows) is unchanged and still only tracks windows.

## Resolving "off-peak now" (hub)

One function, used by the trigger and the check:

1. A device is set **and** its entity has a usable reading → off-peak ⇔ state ∈ `values`.
   *Usable* = the entity is available and its scalar is known; no reading yet, `unknown`,
   `unavailable` or an empty state are not usable (*my call*), so they fall back.
2. Otherwise → the windows in force today: `windows_for(off_peak_history(), today)`, the same
   windows the Energy day chart splits with, checked with `is_off_peak` at the local minute.
3. No device and no windows → never off-peak (single tariff).

So the windows are the backup when the device is missing, unavailable or has not reported yet
(user decision). **Both are optional** (user decision): the four set-ups are

| Device | Windows | Off-peak comes from |
|---|---|---|
| none | none | nothing — single tariff, never off-peak (today's behaviour) |
| none | set | the windows (today's behaviour) |
| set | none | the device; with no usable reading → peak |
| set | set | the device; with no usable reading → the windows |

Saving a device with no windows is valid; nothing in the hub, the API or the web requires a
window.

## Flow nodes

### `trigger.off_peak`

```json
{ "id": "t", "type": "trigger.off_peak", "params": { "edge": "start" } }
```

- `edge`: `start` (peak → off-peak) or `end` (off-peak → peak). Required.
- The engine keeps **one** resolved value and **one** boundary timer for the whole engine (not
  one per node, unlike `arm_suns`), and fans a change out to every enabled flow whose
  `trigger.off_peak` edge matches.
- It re-resolves when: the device entity's state or availability changes (the engine watches
  the device entity even when no flow names it), the next window boundary passes, and home
  settings change (the hub already broadcasts `WsEvent::HomeSettingsChanged` from
  `ingest_home`; the engine subscribes to that, or the builder adds an equivalent internal
  signal).
- The first resolution after boot only sets the baseline (anti-goal above).
- A home settings change (new device, new values, new windows) **re-baselines silently**: it
  does not fire, even if the answer flipped (*my call*: editing settings must not switch the
  water heater; same spirit as "no fire on flow save"). `is_off_peak` reflects the new answer
  at once.
- A change caused by falling back (device goes unavailable while the windows disagree) **does**
  fire (*my call*: the trigger follows the same answer `is_off_peak` gives; a flapping device
  will show up in the run log).
- The trigger value is `"start"` or `"end"`.

### `if` with `op: is_off_peak`

```json
{ "id": "c", "type": "if", "params": { "op": "is_off_peak" } }
```

Same shape as `is_dark`: no `entity`, `value`, `missing` or `average`; validation refuses them
as it does for `is_dark`. The run log line is `is_off_peak → true|false`.

## Web

- **Settings › Energy › Off-peak card**: both parts are optional and the card can be saved
  with either, both or neither. The windows editor stays; with a device set, its heading
  reads as a backup ("Backup windows — used when the device has no reading", en/fr). No
  window row is added by default. The card's Save puts back `off_peak` **and**
  `off_peak_device`, leaving the other home settings as they are. The shared `OffPeakEditor` is
  also used in `screens/home_settings_panel.rs`; the device row goes in both places (builder:
  put it in the shared component). A "Follow a device" row: the existing entity picker; once an entity is chosen, chips of the values it can
  report (from `oikos_core::vocabulary`, as the If value picker does) plus its current value,
  and a free-text add for a text sensor that announces no list (that is enough on its own). A one-line hint: "The windows
  are used when the device has no reading." Clear removes the device.
- **Flow editor**: the card library gets "Off-peak" under triggers (sheet: Starts / Ends) and
  "Is off-peak" under conditions, next to the sun ones; canvas card titles in `flow_text.rs`;
  en/fr strings in `i18n/flows.rs`.

## MCP and docs

- `get_home_settings` / `set_home_settings` carry the new field through the schema.
- The oikos MCP server instructions list `trigger.off_peak` and `op: is_off_peak`.
- `docs/reference/flow-nodes.md`, `docs/users/flows.md`, `docs/users/web-ui.md` (Off-peak card),
  `docs/reference/http-api.md` (home settings field).

## Testing

- core: resolution table (device on/off-peak value, unknown value, unavailable → windows, no
  device, no windows), node JSON round-trips, validation of both nodes.
- hub engine: edge fires once per change, not at boot, not on flow save; window boundary timer;
  fallback transition; disabled flow does not fire.
- api: settings round-trip with and without the field; 422 cases.
- grid day: pure `build_grid_day` cases — device off-peak mid-slot split, device unavailable
  gap falls back to windows, reading before the day carried in, rest of today on windows, no
  device = today's result unchanged; one DB test (`history_db.rs`) reading text rows back.
- web: settings card keeps the other home settings on save (as `setup_energy.rs` does today).

## Energy day chart (`GET /history/grid_day`)

The user decided the chart follows the device's history, not only the windows.

- **What is recorded already:** the history writer stores every state change, text included,
  in `entity_state.value` (jsonb; `num` is null for text) and every availability change in
  `entity_availability`. No storage change is needed. A new `Db` read returns an entity's value
  rows and availability rows in `[from, to)` plus the last of each before `from`; `samples` stays
  numeric-only.
- **Per-moment rule**, the same as the live resolver: at time `t`, if the device's reading in
  force at `t` is usable (available, known, see above) → off-peak ⇔ it matches `values`;
  otherwise → the windows in force on that day (`windows_for(history, date)`). A reading holds
  until the next value or availability change; it does not hold past `now` — the rest of today
  falls back to the windows (*my call*: the device cannot announce the future, and the windows
  are the user's best guess of it).
- **Which days:** the current `off_peak_device` applies to every day the chart shows, past ones
  included, wherever that entity's readings exist; days before it was recorded fall back to
  their windows (*my call*: no dated device history like `off_peak_history`; removing the device
  sends every day back to windows). The matching uses the *current* `values`.
- **Shape:** `build_grid_day` stays pure. `DayInput` gains an optional device timeline (readings
  already reduced to `(from, to, usable_off_peak: Option<bool>)` segments, or equivalent), and
  `off_peak_at` consults it before the windows. The minute-by-minute band scan then draws
  device-driven bands with no further change. `GridDay.tariffs` is true when a device is set
  or windows exist.
- **Converting a recorded `StateValue`** to the value matched against `values` uses the same
  scalar conversion as the live resolver, so live and chart never disagree on a reading.

## Left to the builder

Timer wiring inside the engine, where the resolver lives (core function + hub state lookup),
exact web layout of the device row, test file placement.
