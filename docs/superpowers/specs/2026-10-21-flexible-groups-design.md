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
