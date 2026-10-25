# Native alarm

Status: design, 2026-10-24.

## 1. Intent

Today the Security page shows an alarm panel, but the panel only mirrors one. Another controller (Jeedom in the author's house) holds the alarm logic and publishes an MQTT `alarm_control_panel` through discovery. Oikos displays it and forwards arm and disarm commands. Everything that matters stays in the other system: which sensors count, when the sirens ring, what arms it.

Oikos is meant to replace that controller. The hub already sees every door, window and motion sensor, both kinds of siren and the phone's presence tracker. The alarm logic should live in the hub, so the other controller's alarm plugin, its discovery topic and its siren scenarios can be removed.

The feature is **optional**. Many houses don't want an alarm, and for them nothing changes ([feedback: project must be generic]).

## 2. Goals, non-goals, anti-goals

**Goals**
- A hub-owned alarm: one `alarm_control_panel` entity the hub publishes and drives, with the modes **away**, **home** and **night**, plus **disarmed**, **arming**, **pending** and **triggered** (§3).
- A per-mode rule decides what sets it off. A door or window opening sets it off at once, or after an entry delay. Motion needs N detections within a window (§4).
- On trigger: every configured siren rings for a bounded time, and a critical push with a **Disarm** button goes to the phones (§5).
- Arming and disarming from:
  - **presence:** the house arms "away" when every tracker is away and disarms when one comes home (§6.1)
  - **remotes:** arm-away, arm-home, disarm and SOS buttons (§6.2)
  - **web, app, MCP and flows:** the entity is an ordinary alarm panel (§6.3)
- A short confirmation chirp on arming (§5.3).
- One settings document, owned by the engine and edited on a **Settings → Alarm** page (§7, §8).
- Off by default. Turning it on and off is a switch on the settings page (§7.1).
- State survives a hub restart, including a running entry delay or siren period (§9).

**Non-goals (this round)**
- Alarm codes and PINs. *(Claude's call: the hub has no authentication yet, see `docs/users/security.md`, so a code would protect nothing. The entity announces `code_arm_required: false`.)*
- More than one alarm panel or partition per house.
- Camera-based triggers (Frigate person detection). Flows can react to the alarm entity and to camera events separately.
- Monitoring-centre or SMS dispatch.
- Battery and low-signal watchdogs for alarm sensors. The existing watchdogs crate covers that.
- Changes to the Android app. It already renders `alarm_control_panel` entities and push action buttons (§6.3, §5.2).

**Anti-goals**
- A siren with no end. Every ring stops after `siren_s`, and ringing again needs a new detection (§5.1).
- A false alarm from a single stray motion event (§4.2).
- An unknown or offline presence tracker that arms the house. Only a definite "away" from every tracker arms it (§6.1).
- Alarm logic spread across device-sheet flags. Safety rules live in one validated document (§7). *(Decided with the user.)*
- Any change for houses that leave the feature off, including houses whose own discovered alarm panel shows on the Security page today (§7.1).

## 3. States and modes

The entity uses the core alarm states (`oikos-core` `state/kinds/alarm.rs`):

| State | Meaning |
|---|---|
| `disarmed` | Nothing is watched. SOS still rings (§6.2). |
| `arming` | Exit delay after a manual arm. Nothing is watched yet. |
| `armed_away` / `armed_home` / `armed_night` | The mode's sensors are watched. |
| `pending` | A delayed sensor opened. The entry delay is running. |
| `triggered` | Sirens are ringing. |

Supported arm modes are `away`, `home` and `night` (no vacation or custom bypass). After the siren period ends (§5.1), the panel goes back to the state it was in before the trigger, so a new event rings again. The trigger alert stays open until the alarm is disarmed. On that return:

- the motion hits are cleared;
- a sensor still `on` (the one that triggered, or one opened since) is watched for closing, as in §4.3;
- an SOS from `disarmed` goes back to `disarmed` with its alert still open. A `DISARM` sent while already disarmed resolves any open alarm alert. *(Claude's call.)*

**No re-ring cap.** As long as nobody disarms, every new detection rings the sirens again for another `siren_s` and sends a new push. *(Decided with the user: "the alarm continues ringing".)*

## 4. What sets it off

### 4.1 Sensors

The settings list the watched sensors. Each entry is a binary-sensor entity id with:

- **kind:** `opening` (door or window) or `motion`. *(Claude's call: suggested from `device_class`, and the user can change it.)*
- **modes:** which of away / home / night watch it.
- **entry:** `instant` or `delayed`. This only applies to openings. Motion is always counted (§4.2).

Default when a sensor is added: opening → away and night, instant; motion → away only. The user ticks home on the outside doors and switches entry doors to delayed if wanted. The page says so next to the table. This matches the user's own rule:

- **away:** every sensor
- **night:** doors and windows only
- **home:** *(Claude's call)* outside doors only, which the user ticks per sensor

Each mode is editable per sensor, so any house can set its own.

### 4.2 The rule

While armed in mode M, for sensors watched in M:

- **Instant opening goes to `on`:** → `triggered`.
- **Delayed opening goes to `on`:** → `pending` for `entry_delay_s`. If the alarm is not disarmed in time → `triggered`. While pending, other delayed openings and motion are ignored, because the person walking in sets them off. An instant opening still triggers at once.
- **Motion goes to `on`:** count it. When `motion.count` detections (default **2**) fall within `motion.window_s` (default **120 s**) → `triggered`. Any two sensors count, or the same sensor twice. *(Decided with the user: "two motions or one door". The window is Claude's call.)*

Only a change to `on` counts. A replayed retained value after boot or reconnect never counts, which matches how `trigger.state` treats replays.

### 4.3 Arming with something open

The alarm arms anyway, and an open sensor is **never ignored**. Each watched opening that is `on` when the armed state is entered (the end of the exit delay, not the arm command, so leaving through a door doesn't count) is **watched for closing**: its change to `off` is an event, handled like an opening (instant or delayed, per its `entry`). Once closed, it is watched as usual, so reopening is an event too.

Why closing counts: a tilt-and-turn window left tilted reads open. To open it fully, someone has to close it first and then turn it open, so the close is the first sign of tampering. *(Decided with the user.)* The same rule protects any door left ajar: closing it while armed means someone is there.

The limit, stated in the user docs: a window or door that is ajar *and* can be pushed wider without closing (a plain casement or door) shows no change on a single contact. In away mode, motion still covers it. A second contact or a tilt sensor closes the gap.

*(Claude's call:)* Refusing to arm with something open would leave the house unarmed when the geofence fires and nobody is there to close the window.

### 4.4 Unavailable sensors

A watched sensor that goes unavailable while armed raises one warning alert, which is resolved when the sensor comes back. It does not trigger. *(Claude's call.)*

## 5. What happens on trigger

### 5.1 Sirens

The settings list sirens. Each siren is one of:

- **entity:** a `switch`, or a generic `siren` with the `power` control, driven through `Hub::send_command`. On means ring, off means stop.
- **mqtt:** `{topic, on, off, chirp?}` raw payloads published through `Hub::publish`. This covers devices whose siren output Zigbee2MQTT discovery doesn't expose as a command entity. The author's case is a DIY PTVO board driven by `<z2m>/<device>/l2/set {"trigger": <ms>}`: there `on` is a long trigger, `off` is `{"trigger":100}` (a short pulse that replaces the long one) and `chirp` is also `{"trigger":100}`. The topic must not contain `+` or `#`.

On `triggered`, every siren is switched on. After `siren_s` (default **180 s**, Claude's call, the usual limit in France), every siren is switched off and the panel returns to its armed mode (§3). Disarming at any point switches every siren off at once. One siren failing (rejected, offline, publish refused) never stops the others from ringing. The failure shows in the trigger alert.

### 5.2 Push

Through `Hub::notify`, source `"alarm"`, channel `alarm`:

| Event | Severity | Text (example) | Actions |
|---|---|---|---|
| Triggered | critical | "Alarm: front door opened" / "Alarm: 2 motions (hallway, kitchen)" / "Alarm: SOS" | Disarm |
| Armed (end of the exit delay) | info | "Armed night" / "Armed away (everyone left)" / "Armed home by remote", plus ". Office window open, watched for closing" when something is open | — |
| Disarmed (from any other state) | info | "Disarmed" / "Disarmed (someone came home)" / "Disarmed by remote" | — |
| Sensor unavailable while armed | warning | "Alarm sensor offline: back door" | — |

The trigger alert gets a new id each time and is resolved (`cancel`) on disarm. The armed and disarmed pushes also get a new id each time, so the alerts list keeps them as a log. *(Changed with the user on 2026-10-25: at first plain arm and disarm sent no push.)*

A **Disarm** action reply that arrives on `<push>/responses` disarms the alarm. That topic is currently subscribed only while climate is on (`hub.rs` `extra_filters`), so it must also be subscribed while the alarm is enabled. Replies are routed by alert id to the engine that raised the alert.

### 5.3 Chirp

On entering an armed state, every siren that has a chirp sounds once. For an mqtt siren the chirp is its `chirp` payload. For an entity siren it is an optional `chirp_ms`: on, then off after that many milliseconds. Sirens without a chirp stay silent.

## 6. Arming and disarming

### 6.1 Presence

`presence.trackers` lists entity ids whose state is `home` / `not_home` (a `device_tracker`, for example the one the Oikos Android app publishes at `oikos/presence/<slug>/state`).

- **Arm:** when every tracker has read a definite `not_home` for `presence.arm_after_s` (default **120 s**, Claude's call, to absorb geofence flapping), and the alarm is `disarmed`, it arms **away** with no exit delay. An `unknown` or unavailable tracker blocks auto-arming.
- **Edge, not level.** Auto-arm fires once per change to "everyone away". After any disarm (manual, remote, push action, or turning the feature on), presence won't arm again until a tracker reads `home` and then the house empties again. A disarm to let someone in while you're away therefore stays disarmed. *(Decided with the user.)* The latch is part of the runtime state (§9).
- **Disarm:** when any tracker changes to `home` and the alarm is in any state other than `disarmed`, it disarms, which also stops the sirens. Only the change counts. A tracker that is already `home` when someone arms "night" by hand doesn't disarm it.
- `presence.arm` and `presence.disarm` are separate switches (both default on once trackers are set). An empty tracker list turns presence off.

### 6.2 Remotes

`remotes` lists `device_automation.*` or `event.*` entities. Each has a map from signal name to action: `away`, `home`, `night`, `disarm` or `sos`. When a remote is added, known names are pre-filled. The Heiman/IAS names are `arm_all_zones` → away, `arm_day_zones` → home, `arm_night_zones` → night, `disarm` → disarm, and `emergency` / `panic` → sos. The builder can extend this list.

**SOS** goes to `triggered` from any state, including `disarmed`. Disarming stops it.

A remote only gets a `device_automation.*` entity once Zigbee2MQTT has seen a press. The settings page should say so when the remote picker has nothing to offer. User docs get a troubleshooting note: change the battery, re-pair close to the coordinator, and re-pair any device whose IEEE address reads `0x0000000000000000`. IAS remotes also need enrolment, which pairing redoes.

### 6.3 Web, app, MCP, flows

The hub publishes the entity with discovery, following the virtual-entity convention (`virtual_entities.rs`): a retained config under the hub's prefix with `component` `alarm_control_panel` and a fixed id (suggested `alarm_house` / unique id `oikos_alarm_house`), a retained state topic, a command topic that accepts the standard payloads (`ARM_AWAY`, `ARM_HOME`, `ARM_NIGHT`, `DISARM`), and `supported_features: [arm_away, arm_home, arm_night]`. As a result:

- the web Security page shows it through the existing `AlarmHero` with no new code (its id must not start with `group_`)
- the Android app arms and disarms it by publishing to the command topic, like any discovered panel
- `send_command` with `Command::Arm` / `Command::Disarm` short-circuits to the engine for this id, like `group_` ids do (`hub.rs` `send_command`), and a raw MQTT command on the command topic is parsed the way group commands are (`ingest_message`)
- flows can use `trigger.state` on it and `command` arm/disarm. No flow changes are needed.

A **manual arm** (web, app, MCP, flow, remote) goes through `arming` for `exit_delay_s` (default **60 s**, Claude's call). A presence arm skips it. The arm source is recorded (§9) and shown on the panel as `presence`, `remote`, `sos` or `command`. *(Claude's call: `send_command` carries no caller identity, and web, app, MCP and flows all count as `command`.)*

The entity carries JSON attributes: `source`, `since`, `deadline` (the end of the exit or entry delay, so the card can show a countdown), `cause` (the sensor(s) that set it off) and `open_at_arm` (the sensors watched for closing). Showing the countdown and cause in `AlarmHero` is a small web addition.

## 7. Settings document

### 7.1 On / off

`[alarm]` in `oikos.toml` holds only `prefix` (default `oikos/alarm`, validated like `climate.prefix`). The engine task always starts and stays idle while the settings document says `enabled: false`, which is the default. *(Claude's call: the user asked for a switch on the page, which a toml flag can't give.)*

- **Off:** no alarm entity is published (an empty retained config removes it), nothing is watched, no alerts are raised, and any discovered third-party panel shows on the Security page as today.
- **Turning off while armed or ringing:** sirens off, open alarm alerts resolved, entity removed. The settings are kept, so turning it back on restores them.

### 7.2 Shape

Retained at `<prefix>/settings` (JSON). It is validated on write like the climate docs (`climate/store.rs` `apply_validated`): an invalid document keeps the previous one and reports why.

```json
{
  "enabled": true,
  "entry_delay_s": 30,
  "exit_delay_s": 60,
  "siren_s": 180,
  "motion": { "count": 2, "window_s": 120 },
  "sensors": [
    { "entity": "<door contact id>", "kind": "opening", "modes": ["away", "home", "night"], "entry": "delayed" },
    { "entity": "<window contact id>", "kind": "opening", "modes": ["away", "night"], "entry": "instant" },
    { "entity": "<motion id>", "kind": "motion", "modes": ["away"] }
  ],
  "sirens": [
    { "entity": "<siren switch id>", "chirp_ms": null },
    { "mqtt": { "topic": "zigbee2mqtt/<device>/l2/set", "on": "{\"trigger\":6500000}", "off": "{\"trigger\":100}", "chirp": "{\"trigger\":100}" } }
  ],
  "remotes": [
    { "entity": "device_automation.<ieee>", "signals": { "arm_all_zones": "away", "arm_day_zones": "home", "disarm": "disarm", "emergency": "sos" } }
  ],
  "presence": { "trackers": ["<tracker id>"], "arm": true, "disarm": true, "arm_after_s": 120 }
}
```

Validation rules:
- The entity ids exist and have the right kind: binary_sensor for sensors; switch or a siren with `power` for entity sirens; a signal-sending entity for remotes; a tracker with home/not_home for presence.
- No duplicate entries.
- Delays are in range. *(Claude's call: entry 0–300 s, exit 0–300 s, siren 10–900 s, motion count 1–5, window 10–900 s.)*
- mqtt topics are non-empty with no wildcards.

The entity id named in a sensor, siren or remote may later disappear from the house (device removed). Validation on *write* refuses unknown ids. At *run time* a missing entity is skipped and shown as a warning on the settings page, never as a crash.

## 8. Web

- **Settings → Alarm** (`/settings/alarm`): a new `SettingsPage` row, after the existing ones, copying the climate settings sheet patterns: draft, merge, error mapping and the entity picker. The sections are:
  - an Enable switch
  - **Sensors:** a table with one row per sensor, kind, the three mode checkboxes and an instant/delayed toggle. "Add" offers binary sensors whose `device_class` is door, window, opening, garage_door, motion, occupancy or presence.
  - **Sirens:** an entity picker, or an "MQTT topic" form with the three payloads and a "Test chirp" button
  - **Remotes:** a picker plus a signal → action list
  - **Presence:** a tracker picker plus the two switches
  - **Timing:** the delays and the motion rule
  - Status text on the settings list row: "Off", "Disarmed", "Armed away", and so on
- **Security page:** no structural change. `AlarmHero` gains the countdown during `arming` and `pending`, and the cause line during `triggered` (§6.3).
- **Device sheet:** for an entity named in the alarm settings, a read-only line ("In alarm: delayed opening · away, home, night") linking to `/settings/alarm`.
- Every new label is in English and French through `i18n.rs`.

## 9. Restarts

The engine persists its runtime at `<prefix>/state` (retained): state, mode, source, since, deadline, the motion hits inside the window, the sensors watched for closing, the open trigger alert id and the siren end time. On boot, after the settings and state are ingested and the broker is online:

- In `pending` or `arming` with the deadline passed: a passed exit delay → armed. A passed entry delay → triggered, if the siren period would still be running. Otherwise go back to armed and keep the alert open. With the deadline not yet passed: resume the countdown.
- In `triggered` with sirens still due: switch them on again for the remaining time. With the period over: switch them off and go back to armed.
- Boot replays of sensor states never count as events (§4.2).
- **Presence is reconciled.** The runtime state also stores the last state seen for each tracker. At boot, a tracker whose replayed state differs from the stored one counts as a change. Someone who came home while the hub was down disarms it, and a house that emptied while the hub was down starts the `arm_after_s` timer. A tracker that matches the stored state is not a change. *(Claude's call.)*

## 10. MCP and HTTP

- **MCP:** `alarm_status` (read-only: enabled, state, mode, source, deadline, cause, open_at_arm, the settings and any settings warnings) and `set_alarm_settings` (replace the document, validated, returns the fresh status). They follow `mcp/climate_tools.rs`. Arming and disarming go through the existing `send_command` on the alarm entity. The server instructions text gets one paragraph.
- **HTTP:** `GET /api/v1/alarm` (status) and `PUT /api/v1/alarm/settings`. WebSocket: a new `AlarmChanged` event (the web refreshes the settings page on it, the way `ClimateChanged` bumps `climate_rev`). The entity's own state already flows through `StateChanged`.

## 11. Docs

- New `docs/users/alarm.md`: what it does, setting it up, modes, presence, remotes (with the troubleshooting from §6.2), and moving off another controller (enable here, check it, then remove the other controller's alarm and its discovery topic).
- `docs/reference/configuration.md` (`[alarm]`)
- `oikos.example.toml`
- `docs/reference/mqtt-topics.md` (`<prefix>/settings`, `<prefix>/state` and the entity topics)
- `docs/reference/mcp-tools.md`
- `docs/reference/http-api.md`
- `docs/users/security.md`: native alarm, no codes, LAN caveat
- `docs/users/push-notifications.md`: the alarm channel and the Disarm action

## 12. Testing

Hub integration tests in the style of `crates/oikos-os/tests/climate_hub.rs`: no broker, fed with `ingest`, `#[tokio::test(start_paused = true)]` for the delays. Each of these gets its own test:

- the rules (instant, delayed, motion count and window, per-mode filtering)
- an opening that is open at arm and closes while armed (a tilted window), including closing during the exit delay
- presence arm and disarm, including unknown trackers and the grace period
- remote signal mapping and SOS from disarmed
- siren on and off for both siren kinds, including a failing siren
- the push with Disarm and its reply
- restart resume for every state
- turning the alarm off while ringing
- the settings validation rules

The pure state machine should live in a small crate or module with no hub dependency (as `oikos-climate` does for decisions), so most of the rules are unit-tested on `(state, event, now) → (state, effects)`.

## 13. Left to the builder

- Crate or module layout: a new `oikos-alarm` crate for the pure machine and doc types plus `oikos-os/src/alarm/` for wiring is the suggestion, not a requirement.
- Exact discovery topic names, within the virtual-entity convention.
- The push text wording and its French translations.
- The settings page layout within the existing settings-page patterns.
- Extending the default remote signal names.

## 14. Claude's calls (for the user to check)

- No alarm codes (§2).
- home mode defaults to outside doors only, ticked per sensor (§4.1).
- Motion window 120 s (§4.2).
- Arm anyway with something open, and watch it for closing (§4.3). The armed push names it (§5.2).
- An unavailable sensor warns, it doesn't trigger (§4.4).
- Siren period 180 s, then back to armed (§3, §5.1).
- A push, with sound, on arm and on disarm, naming the mode and source (§5.2; changed 2026-10-25).
- A new opening defaults to away and night, instant (§4.1).
- Open sensors are checked at the end of the exit delay (§4.3).
- An SOS from disarmed returns to disarmed, with the alert open until a DISARM (§3).
- The arm source is coarse: `presence`, `remote`, `sos` or `command` (web, app, MCP and flows all go through the command path) (§6.3).
- Presence arms after 120 s of everyone away. A tracker coming home disarms at once (§6.1).
- Exit delay 60 s on a manual arm, none on a presence arm. Entry delay 30 s on delayed openings (§6.3, §7.2).
- On/off lives in the settings document, not the toml (§7.1).
