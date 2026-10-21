# Flexible groups (Homey-style)

Status: design, 2026-10-21. Branch `feat/flexible-groups`.

## 1. Intent

Today a "group" in Oikos is derived: every room folds its lights, and its covers, into one card once it has two of them. Nothing is stored; the web (`crates/oikos-core/src/model/room_groups.rs`, `crates/oikos-web/src/groups.rs`) and the Android app (`RoomGroups.kt` and friends) each compute it. You cannot make a group across rooms, group plugs or heaters, or use a group in a flow.

The user wants groups **like Homey**: groups you build by hand, from devices in any room, that then behave as one device everywhere: on Home, in the device sheet, in flows, over MCP and in the Android app.

The project is generic ([feedback: project must be generic]): the rules below are for any house, not only the user's.

## 2. Goals, non-goals, anti-goals

**Goals**
- A group is built by hand: a name, one type, members from any room, a room it lives in.
- A group is one entity. Home shows it as a card, it has a sheet, flows can trigger on it, check it and command it, MCP can list, create, change, delete and command it.
- Every controllable kind can be grouped (§4).
- The automatic per-room groups are replaced; today's groups carry over once, so Home looks the same the first day (§9).
- The Android app shows and controls the same groups (§10).

**Non-goals (this round)**
- Sensor groups ("any window open", "average temperature"). User: "group control for now".
- Mixed-type groups ("everything in the living room off"). These are scenes, a separate Homey-gap item.
- Groups of groups.
- Zigbee2MQTT native groups (one Zigbee broadcast instead of N messages). Possible later optimisation behind the same group entity.

**Anti-goals (would make it a failure even if it works)**
- A group whose card or sheet is visually off the existing design: the user rejected a wireframe where text touched buttons; v2 of the mockup (real stylesheet) was approved: "yes it's beautiful".
- Different group behaviour between web, app, flows and MCP (each client re-implementing fan-out).
- A group disappearing by itself (today a derived group dissolves below 2 members; a hand-made one must not).

## 3. What a group is (decided with the user)

- **Hand-made only.** User picked "Homey style groups" over keeping the automatic ones.
- **One type per group.** User: "One type per group → Ok". A switch shown as a light (`showAs`) counts as a light.
- **Controls = what every member can do.** A command goes to every online member.
- **Combined state.** Lights and switches: "All on", "All off", "N of M on". Covers: today's text ("Opening…"/"Closing…", "All open"/"All closed", "N open · M closed"). Other kinds: the value when members agree, "Mixed" or a range ("19–21 °C") when they don't.
- **Availability.** Online when at least one member is online; greyed out when none is.
- **In flows, a group counts as "on" as soon as one member is on.** *(Claude's call; the user answered "ok" without choosing between any/all. Homey does the same.)*
- **Flows see the state the group publishes** (§4 table) for every kind: `trigger.state` and `if` read a group exactly like any entity; no extra "any member" rule beyond what the published state already encodes. *(Claude's call.)*

## 4. Which kinds can be grouped

Controllable kinds only: `light`, `switch`, `cover`, `climate`, `number`, `select`, `button`, `vacuum`, `water_heater`. Kinds added later by the entity-types work (fan, lock, valve, siren, humidifier…) follow the same rule when they land.

Excluded: `sensor`, `binary_sensor` (user: control only for now). **`alarm_control_panel` excluded too** *(Claude's call: arming several panels at once with codes is a security action that deserves its own design).*

Per kind *(Claude's calls, consistent with the HA payloads each kind already uses)*:

| Kind | Group controls | Group state published |
|---|---|---|
| light | on/off; brightness only if every member has it; colour/colour temp only if every member has it | `ON` if any member on; brightness = mean of the members that are on |
| switch | on/off | `ON` if any on |
| cover | open / stop / close; position only if every member has it | opening/closing if any moves (opening wins), else open if any open, else closed; position = mean |
| climate | target temperature (step = largest member step, bounds = intersection); HVAC modes in every member | target = mean of targets rounded to the step; mode = common mode, else none |
| water_heater | as climate (temperature + modes shared by all) | as climate |
| number | set value (bounds = intersection, step = largest) | common value, else mean |
| select | options in every member | common option, else none |
| button | press all | none (a press is an event) |
| vacuum | the commands every member supports | cleaning if any cleaning, else returning if any, else docked if all docked, else the first member's |

Climate and water heater stepping when targets differ (from the approved mockup): the first `+` or `−` sets every member to the rounded mean, then each step moves them all together.

## 5. How it is built (Claude's calls; the user left the technical side to Claude)

**The group is a hub-published entity, the same pattern as virtual entities** (`crates/oikos-os/src/virtual_entities.rs`). This is the key choice: the Android app reads the broker directly, not the hub API (`core/transport/DirectMqttTransport.kt` in the app repo), so a group must exist on MQTT as a normal discovery entity for every client to see it. Fan-out then lives in exactly one place, the hub, and flows, `send_command`, history and the app get it for free.

- **Definition:** one retained JSON document per group under a new reserved sub-tree of the first discovery prefix, e.g. `oikos/group/<id>`: `{ "name", "kind", "members": [entity ids], "hideMembers": bool }`. Reserve the sub-tree alongside `ui`, `automation` and `virtual` so it is never classified as discovery nor listed in `GET /api/v1/ignored`.
- **Entity:** from each definition the hub publishes a retained discovery config of the group's component, `unique_id: group_<id>`, device `{ "identifiers": ["oikos_group"], "name": "Groups" }`, with a command topic the hub subscribes to and a state topic it publishes the combined state on (retained), plus availability. The config announces only the capabilities in §4 (intersection of the present members); the hub republishes it when membership or a member's discovery changes.
- **Fan-out:** a command on the group's command topic is translated to each online member's own command (the same `Command` → member payload path `send_command` uses, so a `showAs: light` switch gets a switch payload). The hub recomputes and republishes the group state on every member state change.
- **Room, favourite, order, hidden:** the normal override `oikos/ui/group_<id>`, exactly like any entity, so the device sheet's existing settings, Home placement and the app's override handling apply unchanged. Name lives in the definition (it is the discovery `name`); a rename from the sheet writes the definition.
- **Ids:** `[a-z0-9_]{1,48}`, slugged from the name at creation, suffixed on collision; never changes after creation.
- **Members:** device entities only (no `group_*` members), of the group's kind (effective kind, `showAs` included). An entity can belong to several groups.
- **A member whose effective kind changes later** (its `showAs` edited so it no longer matches the group's kind) stays in the definition, shows as "Mismatch" on the sheet and is left out of the controls and the fan-out until it matches again or is removed. The `showAs` edit itself is not blocked. *(Claude's call.)*
- **A member that disappears** (device removed from Zigbee) stays in the definition and shows as "Missing" on the sheet until removed in Edit group. A group with no present member stays, greyed and unavailable. Groups never dissolve by themselves.
- **`send_command` / flow `command` on a group** reports `accepted: true` once every online member confirmed, otherwise `accepted: false` naming the members that did not (the group's own state may not change, e.g. turning on a group that is already "on").

## 6. Hub API and MCP

- REST: `GET /api/v1/groups`, `PUT /api/v1/groups/{id}`, `DELETE /api/v1/groups/{id}`, following the hub's conventions (422 on a rejected body, 503 while the broker is down). Validation: known kind from §4, every member exists or is kept as missing only when already in the stored definition, member kinds match.
- MCP: `list_groups`, `set_group`, `delete_group` (same style as `set_flow` / `set_virtual`); `send_command`, `get_entity`, `search` and `list_entities` see `group_<id>` like any entity. The server instructions text gains one sentence on groups.
- Remove `PATCH /api/v1/areas/{id}/groups/{kind}` once nothing calls it (after §9).

## 7. Web UI (decided with the user, mockups in `.superpowers/brainstorm/59397-1792589768/content/`)

- **Where you build one:** Devices screen, a "Groups" section above the device list with "＋ New group" (user picked option A, "Homey way"; device-sheet and Home-selection entry points were not picked).
- **Building:** three steps (user picked B over a one-page form): 1. type, 2. members (all rooms, grouped by room, filter box, only entities of that type), 3. name and room → Create.
- **Editing:** the group sheet's "Edit group" reopens the three steps pre-filled, type locked (change type = delete and rebuild). *(Shown in the approved mockup.)*
- **Card on Home** (`group-cards-v2.html`, approved): the type's normal card with the existing `×N` badge (`group__count`). Lights and covers keep today's group cards (`LightGroupCard`, `CoverGroupCard`). New kinds reuse their kind's card body with combined values: plugs = power toggle "All off"; buttons = "Press all"; climate = current range, Target stepper showing the range, Mode "Mixed". Must be built with the real classes (`card`, `card--wide`, `power`, `stepper`, `pick`, `group__*`), no new ad-hoc spacing.
- **Group sheet** (approved): header (name, room · N <type>), Controls panel, members panel (name, "room · state", own toggle, offline greyed, tap opens the member's sheet), Settings panel (Show in Favorites, Hide members on Home, Edit group, Delete group). Delete asks for confirmation in-page (no browser dialog) and, *(Claude's call)*, names the flows that use the group.
- **Member's device sheet** *(Claude's call)*: a read-only "In groups" line linking to each group it belongs to.
- **Route:** the group sheet replaces `/group/:area/:kind`; the derived-group code in `groups.rs` / `room_groups.rs` goes once §9 has run.
- **Flow editor:** groups appear in the entity picker, marked as groups (the `×N` badge or a "Group" tag).
- Energy and Security tabs ignore group entities *(Claude's call: members are already counted there)*.

## 8. Home placement

- A group shows in the room set by its override; Favorites as today.
- Inside a room, groups come before the unpinned single cards (today's rule, kept).
- **Hide members on Home: a setting on each group, yes by default** (user's choice, over "always show" and "hide only in the group's room"). Stored as `hideMembers` in the definition so the app reads the same rule.
  - Yes: each member's own card is left out of the room views on Home, in whatever room the member is. A member in several groups is hidden if any of them says yes.
  - No: members keep their own cards; the group is one more card.
  - Favorites *(Claude's call)*: a member the user marked as a favourite on its own still shows under Favorites; the setting only affects the room views.
  - Only Home is affected: the Devices screen, flows, Energy and Security still list every member.
  - Edited in the group sheet's Settings panel (a toggle next to Show in Favorites) and offered as a toggle in step 3 of the builder *(Claude's call for step 3)*.
  - Migrated groups (§9) get yes, so Home looks as it does today.

## 9. Changeover from derived groups (Claude's call, played back, user: "ok let's go")

- Once, at hub start, **after the initial retained sync has settled** (otherwise it sees an empty house and creates nothing), and only when a retained marker (e.g. `oikos/group/_migrated`) is absent: for every derived group the current rules produce (kinds light/cover, ≥2 members, not `ungrouped`, not Unassigned), create a hand-made group: name = the area's `groupNames[kind]` or today's default name, room = the area, favourite = kind in `groupFavorites`, `hideMembers: true`. Then publish the marker.
- After that, `ungrouped`, `groupNames` and `groupFavorites` are no longer read. They are left on the broker untouched (no destructive cleanup). The web's Customize / "Ungroup all" panel goes.
- Test the race explicitly: hub starting with the broker's retained messages arriving late must not migrate an empty set.

## 10. Android app (user: "yes")

- In scope, but built in its **own worktree of the app repo** (`<app-repo>`) with its **own session and plan**, created at build time.
- The app stops deriving room groups and reads `oikos/group/<id>` plus the `group_<id>` entities; group cards and the group sheet follow the web contract here (§3, §4, §7 card rules).
- **Building and editing groups in the app:** *(Claude's call, to check)* not in this round; build on the web or through MCP.
- **Contract first:** the hub plan's first task freezes the MQTT contract (definition schema, topics, marker, discovery config shape per kind, combined-state rules) in an appendix to this document and updates the app spec's §6.1 to point at it. The app plan is written from that appendix, not from §5's prose.
- **Release gate** *(Claude's call)*: a hub config switch (e.g. `[groups] enabled`, default off) gates publishing group entities and running §9. The hub can be rebuilt on the live Mac at any time without disturbing the current app; the user turns the switch on once the new app is installed.
- **Release order:** the app version that knows groups must be installed before or with the hub version that runs §9; otherwise the old app shows the new group entities as plain lights and folds them into its own derived groups. The app spec's §6.1 group contract is updated to point at this document.

## 11. Constraints

- Hub port 8081; hub runs on the user's Mac against the live broker; the live house is a test bed, not the scope.
- Follow the virtual-entities patterns for retained discovery, reserved topics and id validation.
- `target/` directories can fill the disk (the disk filled up during this design session); clean before big builds.
- Pre-existing failing wasm test `tests/sheet.rs` `state_echo_keeps_the_override_draft` is not this work's.

## 12. Left to the builder

- Exact topic layout under the reserved sub-tree (definition vs command vs state topics), and the marker topic name.
- Rounding and mean rules beyond §4, and how "Mixed" is worded per kind.
- How the hub tracks member confirmations for `accepted` on a group command.
- Wizard step styling (must use the existing sheet/card classes; no mockup of the steps in the real stylesheet yet — produce one before building if unsure).

## Appendix A — MQTT contract (frozen 2026-10-21)

Frozen. Later hub tasks and the Android app plan quote this appendix verbatim. A change needs a new dated revision of this appendix, not an edit in place. Where this appendix and §1–§12 differ, this appendix wins; the differences are listed in A.1 and A.6.

Notation: `<prefix>` is the first discovery prefix (default `oikos`); `<ui_prefix>` is the UI prefix (default `oikos/ui`); `<component>` is the Home Assistant component of the group's kind (§4); `<id>` is the group id.

### A.1 Topic layout

- **Definition:** `<prefix>/group/<id>/definition`, retained JSON (schema in A.2). *Deviation from §5's example path:* under the default prefix, `oikos/group/<id>` is the state and command topic, so the definition sits one level down. The sub-tree `<prefix>/group/` is reserved, alongside `ui`, `automation` and `virtual`: nothing under it is classified as discovery or listed by `GET /api/v1/ignored`.
- **Migration marker:** `oikos/group/_migrated` (default prefix; `<prefix>/group/_migrated` in general), retained. An empty payload means absent; any non-empty payload means present. Because `_migrated` matches the id pattern, the id `_migrated` is refused.
- **Discovery config:** `<prefix>/<component>/group/<id>/config`, retained (keys in A.3).
- **State and command:** `<prefix>/group/<id>`. The hub publishes the combined state here, retained (A.4). Commands arrive on the same topic (A.5).
- **Availability:** `<prefix>/group/<id>/avail`, retained, `online` or `offline` (§5: online when at least one member is online).
- **UI override:** `<ui_prefix>/group_<id>`, the same shape as any entity override (room, favourite, order, hidden, `hideMembers` is not here; it lives in the definition).
- **Id:** `[a-z0-9_]{1,48}`, slugged from the name at creation, suffixed on collision, never changed after creation, and not `_migrated`.

### A.2 Definition schema

```json
{ "name": "Living lights",
  "kind": "light",
  "members": ["0x00124b00_1_switch", "lamp_floor"],
  "hideMembers": true }
```

- `name`: required. Trimmed; 1 to 64 characters after trimming.
- `kind`: required. One of the controllable kinds of §4: `light`, `switch`, `cover`, `climate`, `number`, `select`, `button`, `vacuum`, `water_heater`. Fixed at creation: a PUT that changes it is refused with 422 (§7: changing the type means delete and rebuild).
- `members`: required. An array of unique entity ids, 0 or more. Each must be a device entity (no `group_*`) whose effective kind, `showAs` included, matches `kind` when it is added. A member already in the stored definition may stay while it is missing (§5).
- `hideMembers`: optional boolean, default `true` (§8).
- No other fields. The parser uses `deny_unknown_fields`: an unknown field is refused (422 on REST). A retained definition that fails the same check is not loaded and is logged.

### A.3 Discovery config per kind

Common keys on every config, for `<id>`:

- `name`: the definition's `name`.
- `unique_id`: `group_<id>`.
- `device`: `{ "identifiers": ["oikos_group"], "name": "Groups" }`.
- `availability_topic`: `<prefix>/group/<id>/avail`; `payload_available`: `online`; `payload_not_available`: `offline`.
- `retain`: `true`, except on `button` (see below), as on virtual entities.

Per kind. Commands are the Command JSON of A.5 (`{"type": ...}`). Every key marked "when shared" is present only when every present, matching member has that capability (§4's intersection rule). `command_topic` and `state_topic` are both `<prefix>/group/<id>` unless said otherwise.

- **light** (`light`): `state_value_template` `{{ value_json.state }}`; `state_on` `ON`, `state_off` `OFF`; `payload_on` `{"type":"turn_on"}`, `payload_off` `{"type":"turn_off"}`. When shared: `brightness` `true`, `brightness_state_topic` and `brightness_command_topic` (same topic), `brightness_value_template` `{{ value_json.brightness }}`, `brightness_scale` `255`, and a brightness command template that produces `{"type":"turn_on","brightness":<value>}`. When shared: colour as `hs_state_topic`, `hs_command_topic` (same topic), `hs_value_template` `{{ value_json.hs }}`; colour temperature as `color_temp_*` keys with `{{ value_json.color_temp }}` and the intersection of the members' mired bounds.
- **switch** (`switch`): `state_value_template` `{{ value_json.state }}`; `state_on` `ON`, `state_off` `OFF`; `payload_on` `{"type":"turn_on"}`, `payload_off` `{"type":"turn_off"}`.
- **cover** (`cover`): `payload_open` `{"type":"open"}`, `payload_close` `{"type":"close"}`, `payload_stop` `{"type":"stop"}`; `state_value_template` `{{ value_json.state }}`. When shared: `position_topic` (same topic), `position_template` `{{ value_json.position }}`, `set_position_topic` (same topic), `set_position_template` `{"type":"set_position","position":<position>}`.
- **climate** (`climate`): `temperature_command_topic` (same topic) with template `{"type":"set_temperature","temperature":<value>}`; `temperature_state_topic` (same topic) with template `{{ value_json.target }}`; `temp_step` = the largest member step; `min_temp` = the largest member minimum; `max_temp` = the smallest member maximum; `modes` = the HVAC modes every member has; `mode_command_topic` (same topic) with template `{"type":"set_hvac_mode","mode":"<value>"}`; `mode_state_topic` (same topic) with template `{{ value_json.mode }}`.
- **water_heater** (`water_heater`): the same keys as climate, with the `water_heater` component's names, as the hub's water heater tile already reads them (`temperature_*`, `mode_*`, `modes`, `min_temp`, `max_temp`, `temp_step`).
- **number** (`number`): `command_topic` with template `{"type":"set_number","value":<value>}`; `value_template` `{{ value_json.value }}`; `min` = the largest member minimum; `max` = the smallest member maximum; `step` = the largest member step; `unit_of_measurement` only when every member reports the same unit.
- **select** (`select`): `command_topic` with template `{"type":"select_option","option":"<value>"}`; `value_template` `{{ value_json.option }}`; `options` = the options every member has.
- **button** (`button`): `command_topic` (`<prefix>/group/<id>`); `payload_press` `{"type":"press"}`. No `state_topic`, and no `retain`, as on virtual buttons. Availability is still published.
- **vacuum** (`vacuum`): `schema` `state`; `state_topic` with `state_value_template` `{{ value_json.state }}`; `command_topic`, and one `payload_<action>` per action every member supports, each `{"type":"vacuum_action","action":"<action>"}` with `<action>` one of `start`, `pause`, `stop`, `return_to_base`, `locate`.

### A.4 Combined state rules per kind

The hub publishes the state on `<prefix>/group/<id>` as one retained JSON object. Only present, matching members count (a member that is missing or mismatched, §5, is left out of the state as it is out of the controls). "Mean" is the arithmetic mean of the counted members. Keys that depend on a shared capability (A.3) appear only when that capability is shared.

- **light:** `{"state":"ON"|"OFF"}`. `ON` if any member is on. `brightness` (0–255, mean of the members that are on, rounded to the nearest integer) when shared; omitted when `OFF`. `hs` (`[h, s]`, mean of the members that are on) and `color_temp` (mean of the members that are on) when shared.
- **switch:** `{"state":"ON"|"OFF"}`. `ON` if any member is on.
- **cover:** `{"state":"opening"|"closing"|"open"|"closed", "position":N, "text":"…"}`.
  - `state`: `opening` if any member opens; else `closing` if any closes; else `open` if any is open; else `closed`.
  - `position`: mean, rounded to the nearest integer, when shared.
  - `text` (the label, §3): `Opening…` when `state` is `opening`; `Closing…` when `state` is `closing`; `All open` when every counted member is open; `All closed` when every counted member is closed; otherwise `N open · M closed`, with N and M the counts of open and closed members.
- **climate** and **water_heater:** `{"target":N, "range":[lo, hi], "mode":"…" | null}`.
  - `target`: mean of the member targets, rounded to `temp_step` and clamped to `min_temp`..`max_temp`.
  - `range`: `[lowest, highest]` member target, present only when the targets differ. Clients show it as `19–21 °C` (§3).
  - `mode`: the common HVAC mode when every counted member with a known mode reports the same one; otherwise `null`. A `null` mode with members that disagree is shown as `Mixed` by clients (§7). The word `Mixed` is client text, never a state value.
- **number:** `{"value":N}`. The common value when every counted member has the same value; otherwise the mean, rounded to the group's `step`.
- **select:** `{"option":"…" | null}`. The common option when every counted member has the same one; otherwise `null`.
- **button:** nothing is published on the state topic (a press is an event, §4).
- **vacuum:** `{"state":"…"}`. `cleaning` if any counted member is cleaning; else `returning` if any is returning; else `docked` if every counted member is docked; else the state of the first counted member in definition order.

Member state is not counted until the member has reported a state. With no counted member, no state is published.

### A.5 Fan-out contract

- **Command shape.** A message on `<prefix>/group/<id>` is a command only when it is a JSON object with a `type` key: the Command JSON of `send_command` and the flow `command` node, for example `{"type":"turn_on","brightness":128}`. State payloads (A.4) never carry `type`, so the hub's own retained state echo is ignored on the same topic.
- **Translation.** The hub sends each targeted member its own command through the same path `send_command` uses, so a switch shown as a light (`showAs: light`) gets a switch payload. Targeted members are the present, matching members that are reachable. Members that are missing, mismatched or offline are not targeted.
- **Confirmation.** A targeted member has echoed when its state topic is published with the value the command asks for. A re-publish of that same value counts. The hub waits up to 2 seconds (2000 ms) from the command.
- **Result.** `accepted: true` only when every targeted member has echoed. Otherwise `accepted: false` with `reason: "<id1>, <id2>: no echo"`, listing the targeted members that did not echo, in definition order, separated by `, `. The group's own state may stay the same (turning on a group that is already on is accepted once its members echo).
- **No targeted member.** `accepted: false`, `reason: "no reachable member"`. This case is my addition; §5 is silent.
- **Kind mismatch.** A command the group's kind does not support is refused, as `send_command` refuses it for an entity.

### A.6 Lifecycle

- **Create** (new id): the PUT writes the definition (retained). The hub then publishes the retained config (A.3), the retained availability, and the retained state when any member has reported one (A.4).
- **Redefine** (PUT on an existing id): the definition is rewritten (retained) and the config is republished, because the shared capabilities may change. The state topic keeps its retained value and is not re-seeded: the hub publishes a new state only when the recomputed value differs. The initial state is published only when a group is new, as stated in §5. The UI override `<ui_prefix>/group_<id>` is not touched. `kind` cannot change (A.2).
- **Delete:** the hub publishes an empty retained payload on each of: `<prefix>/<component>/group/<id>/config` (the component of the group's kind), `<prefix>/group/<id>` (state), `<prefix>/group/<id>/avail` (availability), and `<prefix>/group/<id>/definition` (definition). The last one is my addition to the brief's list: without it the definition would be loaded again at the next start. The UI override is left in place, so a later group with the same id keeps its room and order; clearing it is a separate `clear_override` action.

TODO (app plan): point `<app-repo>/docs/superpowers/specs/2026-10-09-oikos-room-groups-design.md` §6.1 (lines 116–124, "Groups contract") at Appendix A of `docs/superpowers/specs/2026-10-21-flexible-groups-design.md` when the app plan is written. This worktree does not touch that repo.
