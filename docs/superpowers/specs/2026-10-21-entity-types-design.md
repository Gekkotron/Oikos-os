# More entity types — generic, table-driven

Author: Gekkotron
Date: 2026-10-21
Status: approved by the user 2026-10-21

## Why

Oikos-os turns Home Assistant MQTT discovery into entities. Any device that announces itself works with no Oikos code: a Zigbee2MQTT node, or a DIY device whose config was published by hand (the water heater, `oikos/device/water_heater/config`). That promise holds for 12 kinds today (`crates/oikos-core/src/model/kind.rs`). Seventeen other component names sit in `IGNORED_COMPONENTS` and are dropped on arrival, so a lock, fan, valve, siren or a remote's button press never reaches the house.

This project brings in every one of them. The user's rule: **no special code per type, generic and configurable**. Oikos is a generic Jeedom / Home Assistant replacement, not a one-house tool. The live house (3 remotes, 10 firmware `update` entries, a phone `device_tracker`, one `text`) is the test bed, not the scope.

Success: someone publishes a discovery config for any of the new types and, without an Oikos release for that type, the device appears in its room on a generic card with a sensible main control, every announced control is in its device sheet, it works in flows as trigger, check and action, its history is recorded, and the Android app shows the same thing.

## Scope

`IGNORED_COMPONENTS` lists 17 names. `device` is not a type: it is device-based discovery (one message carrying several components), which the hub already handles. That leaves **16 new types**, all in this project (the user: "yes 17 types for now").

| Group | Types | Where they show |
|---|---|---|
| Devices with state | `lock`, `fan`, `siren`, `valve`, `humidifier`, `lawn_mower`, `text`, `scene`, `device_tracker`, `camera`, `image` | room card (generic) + device sheet + flows |
| Momentary signals | `device_automation`, `event`, `tag` | flow triggers; last press/scan on the device sheet; no room card |
| Message target | `notify` | flow action ("send a message to …"); no room card |
| Firmware news | `update` | "update available" mark on the device sheet + one list in Settings; no room card |

Out of scope: the 12 existing kinds keep their current cards and behaviour (user: "new ones"). No install button for firmware updates (Claude's call; the user may ask for one later). No per-type designed cards (no lock picture, no fan animation).

## Decisions taken in conversation (the user's)

- **Generic, not per-type.** Adding or adjusting a type is data, not code. "Configurable" means per-entity settings in the spirit of today's overrides (`showAs`, `energyInvert`): show as another type, inverted, etc. More settings will come later.
- **All 16 types now**, split as in the table above (Claude proposed the split when the user said "I don't know"; the user approved it).
- **One generic card** for every new device type, controls drawn the generic way from what the config announces. No per-type card.
- **Card layout B** (visual companion, three options shown): the card shows icon, name, current state and **one main control** (a toggle, Unlock, Open, Start, Edit…). Every other control lives in the device sheet. This is how Homey does it: tap opens the full controls; a per-device quick action button sits on the tile, assigned automatically for toggle/press capabilities and changeable in device settings.
- **Main control is picked automatically and changeable per entity** in the device sheet, as a new setting next to "Show as". The user can also turn it off.
- **Attention states use the existing alert style** (rose border, `.card--alert`): an unlocked or jammed lock, a sounding siren.
- **Camera and image** show a still picture on the card, no main control.
- **Device tracker** shows home / away, no main control.
- **Flows:** every new type works as a trigger (state change), an `if` check and a `command` action, all coming from the same type description. Remote presses and tag scans are triggers; `notify` is an action.
- **The Android app is in scope** — new types and the generic card in the app too. The app lives in its own repo (`<app-repo>`) and its work runs in **its own worktree and its own session**, not in this one.
- **No anti-goals stated.** The user: "it's not make me wince… You'll fix it after." Iteration after landing is expected.

## The type table (the heart of it)

One data document describes every new type. Nothing else in the hub, the web or the app may branch on a new type's name; they read the table.

Each entry says, for one HA component:

- **States** — the fields the type reports, where each comes from in the discovery config (topic key + template key, e.g. `state_topic` + `value_template`; `percentage_state_topic`), the payload mapping (`state_locked` → `locked`), and the field's shape (on/off, number with min/max/step from config keys, choice from a config list such as `preset_modes`, text, picture, timestamp).
- **Primary state** — the field the card shows as its value, plus how to word it.
- **Controls** — the commands the type accepts, each tied to a command topic key and payload rule (`command_topic` + `payload_lock`; `percentage_command_topic` + number; `preset_mode_command_topic` + choice). A control is offered only when its topic key is present in the config (the same "announced" rule `capabilities.rs` uses today).
- **Default main control** — which control goes on the card, in order of preference (a lock: lock/unlock toggle; a valve: open/close; a mower: start; a text: edit). None for camera, image, device_tracker.
- **Alert states** — which values of which field put the card in alert style (lock `unlocked`/`jammed`, siren `on`).
- **Placement** — room card, flow-only signal, flow-only target, or update mark (the four groups above).
- **History** — which field is recorded and how it maps to the history store's `num` (on/off as 0/1, a percentage as-is, a choice not recorded).
- **Icon** — a default glyph, overridable by the config's `icon`.

**The table is data both sides read.** The app parses discovery itself in Kotlin (`core/source/.../discovery/DiscoveryParser.kt`), independently of the hub, so a Rust-only table would mean writing every type twice. Claude's call: the table is one JSON document kept in the hub repo, embedded in the hub build and published retained by the hub on MQTT (under the hub's prefix, next to `oikos/automation/…` and `oikos/ui/…`). The app reads it from there and falls back to a copy bundled at build time when the retained message is missing. The web reads it through the hub as usual.

The 12 existing kinds stay on their current per-kind code (`capabilities.rs`, `state/kinds`, `command/kinds.rs`). Moving them onto the table is a possible later project, not this one.

## Generic state and commands

- **State.** A new `StateValue` variant carries a generic record: the primary value plus named fields, each tagged with its shape from the table. The hub's state pipeline fills it from the table; it does not need a Rust struct per type.
- **Commands.** One generic command names a control from the table and carries its value (none for a press, a bool for a toggle, a number, a choice, a text). `send_command` (MCP and REST) accepts it, the hub resolves topic and payload from the table and the entity's config, and the usual echo check reports `accepted`.
- **Momentary signals** (`device_automation`, `event`, `tag`) produce events, not lasting state. The device sheet shows the last one ("button 1 hold, 2 min ago"). `device_automation` configs come one per action (`action_1_single`, `action_1_hold`…); the hub groups them by device so the remote is one thing with several actions, not one entity per action.
- **`notify`** has no state, only a "send message" control.
- **`update`** holds installed / latest version and whether an update is waiting.
- **Camera / image** hold a picture reference, not bytes in the state stream; the web and app fetch the picture the way the camera alerts do today (through the hub).

## The generic card and the device sheet

- **Card (web and app):** icon, name (override-aware), primary state worded from the table, one main control. Alert style when an alert state is met. Offline / pending styles as today. Tapping the card opens the device sheet.
- **Device sheet:** every control the config announces, drawn by shape: on/off → toggle, number → slider or stepper with the config's min/max/step, choice → chip row or picker, press → button, text → text field (honouring the config's min/max length and pattern), picture → image. The per-entity settings live here too.
- **New per-entity setting: main control.** Stored in the existing per-entity override (`oikos/ui/<id>`, `UiOverride`), shared by web and app, editable from the device sheet next to "Show as" and through MCP `set_override`. Values: a control name from the table, or "none". Absent means the table's default.
- **Momentary signals** show on the device sheet of their device (last event and time), not as room cards.
- **Firmware updates:** an "update available" mark on the device sheet (current → latest version) and one Settings list of devices with an update waiting.

## Flows

- **Trigger on state:** `trigger.state` works for every new device type unchanged in shape: the entity is a new-type entity, `to` / `from` compare against its primary value.
- **Trigger on a signal:** remote actions, `event` types and tag scans can start a flow ("when the living-room remote, button 1, is held"). Exact node shape is the builder's; it must validate the action / event type against what the config announces, like `set_flow` validation does today.
- **Check:** `if` reads a new-type entity's primary value like any other.
- **Action:** `command` sends the generic command; `notify` appears as "send a message to …".
- The web flow canvas (＋Add picker, per-node editor) offers the new triggers and controls; MCP tool descriptions and the `oikos` server instructions mention them.

## Settled after the builder check (Claude's calls)

A reviewer read this document as a builder would. None of its questions needed the user; each is settled here and listed under "Calls made by Claude".

- **Kind.** `Kind` gains one variant, `Generic`, and `Entity` gains a `component` string (`lock`, `fan`…) that names the table entry. Every new-type entity is `kind: generic`. This changes the REST / MCP snapshot / app contract; the app is single-user and free to change alongside the hub. `IGNORED_COMPONENTS` keeps only what the table does not describe (`device` stays handled as device-based discovery). `showAs` and room groups do not apply to generic entities in this project.
- **Table contract.** The table carries a `version` integer. The hub publishes it; the app reads it and uses its bundled copy when the retained message is absent or its `version` is newer than the app understands. The app work may amend the table (a hub-repo commit), since both sides share one author. The app builds entities for the 16 components from the table the same way the hub does; its existing per-kind parsing for the 12 kinds is untouched.
- **Momentary signals.** `device_automation` configs of one device fold into **one entity per device** (`kind: generic`, `component: device_automation`), whose actions are the union of the announced `type`/`subtype` pairs. `event` is one entity per config (it has a `unique_id` and `event_types`). `tag` is one entity per config. These entities appear in `list_entities` but never as room cards. The last signal (action / event type, time) is held as entity state and recorded in history so it survives a restart; **a retained or replayed signal at boot never starts a flow** — only signals received live do.
- **Values and comparisons.** Every field has a canonical value: the HA state word the table maps payloads to (`locked`, `unlocked`, `jammed`, `open`, `closed`, `home`, `not_home`…), a number, or a bool. Flows (`to`, `from`, `if`) compare canonical values only; translated labels (French in the app) are display-only. Choice and text fields support equality; numbers support the existing numeric ops; bools the existing bool ops. Picture, timestamp and version fields are not comparable.
- **Main control override.** New `UiOverride` field `mainControl` (camelCase on the wire like the others): a control name from the table, or the string `"none"` to hide it; absent or `null` means the table's default. The app's override parser is updated in the app work to read it.
- **Camera and image.** Both HA sources work: raw bytes on `topic` / `image_topic`, and a URL from `url_topic` / `url_template`. The hub keeps the latest picture per entity in memory (no disk cache) and serves it through one authenticated hub route; the web and app fetch it there. Pictures over a size cap set by the builder are dropped with a log line.
- **Secret text.** A `text` entity with `mode: password` is masked on the card and in the sheet, its value is left out of the snapshot and history, and the edit field is a password field.
- **History.** No schema change. The table maps text states to numbers where it means something (lock locked = 1 / unlocked = 0, valve and siren open/on = 1, tracker home = 1, fan on = 1, fan percentage as-is, humidifier target as-is); fields without a mapping are not recorded. Signals are recorded as events (see above).
- **Verification.** Each type gets one real-world discovery fixture (Zigbee2MQTT where one exists). The plan ends with a hub rebuild and restart on :8081 and a check against the live house: the 3 remotes as triggers, the 10 `update` marks, the phone tracker, the `text` entity, and `list_ignored` empty of the 16.

## Constraints

- `oikos-core` stays wasm-clean (the web shares it).
- The table is the single source: no `match` on a new type's name outside the table loader.
- The live house must not regress: today's 12 kinds, overrides, flows, history and Energy / Security screens keep working; `list_ignored` shrinks to genuinely unknown components only.
- Workspace gates: `cargo test --workspace`, clippy `-D warnings`, fmt, the three wasm checks. App: its own Gradle checks.
- Attribution: Gekkotron only on commits and doc bylines.
- Delivery: hub + web on `feat/entity-types` in `oikos-entity-types`; app in its own worktree of the app repo with its own session, after the hub publishes the table.

## Left to the builder

- The JSON shape of the table and its topic name, within the rules above.
- How the generic `StateValue` variant and generic command are spelled, and how the app's Kotlin model mirrors them.
- The signal-trigger node's exact shape and its canvas sheet.
- Which control widgets the sheet uses for each shape on phone vs desktop.
- Card wording for each type's primary state (French strings in the app, matching today's tone).
- Test fixtures: one real-world discovery config per type (Zigbee2MQTT where one exists).

## Calls made by Claude (check these)

1. The type table is a JSON document published retained by the hub, read by the app from MQTT with a bundled fallback — chosen because the app parses discovery on its own.
2. The 12 existing kinds stay on their current code; moving them onto the table is not in this project.
3. No firmware install button in v1.
4. `device_automation` configs are grouped per device into one remote with several actions.
5. Camera / image pictures go through one hub route, from MQTT bytes or a URL, latest picture held in memory.
6. One `Kind::Generic` plus a `component` string, instead of 16 new `Kind` variants; `showAs` and room groups don't apply to generic entities yet.
7. The table is versioned; the app may amend it; the app has a bundled fallback.
8. Remote presses, events and tag scans are entities with history; retained or replayed signals never start a flow at boot.
9. Flows compare canonical HA state words, never translated labels.
10. `mainControl` override, `"none"` hides the control.
11. Password `text` is masked and kept out of snapshot and history.
12. History maps text states to 0/1 per table entry, with no schema change.
13. The plan ends with a hub rebuild/restart and a live-house check.
