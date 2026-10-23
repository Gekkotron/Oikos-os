# Ambiances ("une ambiance, un geste")

Status: design, 2026-10-24. Project 4 of the Ivoire redesign (`2026-10-23-ivoire-theme-and-sidebar-design.md`).

## 1. Intent

The Ivoire mockup has a home row titled "Une ambiance, un geste": four tiles (Soirée tranquille, Bonne nuit, Je m'absente, Un peu de lecture). One tap puts the house in a known state. Nothing like this exists in Oikos today. A flow can be run by hand, but it is built on the canvas, needs a trigger node and is not shown as a tile.

The user's own example: **Bonne nuit turns off every light, the TV and every device.** For that house, nothing has to stay on. The user stressed that this is only an example. Any house builds its own ambiances ([feedback: project must be generic]).

"All" means all. A light or plug added next month is included without editing the ambiance. That is why an ambiance can address a *category* of devices, not only a fixed list.

## 2. Goals, non-goals, anti-goals

**Goals**
- An ambiance is a tile (name, icon, short subtitle). One tap runs its list of actions once (§3).
- Actions can address a whole category of devices ("every light"), for the whole house or one room, so new devices are covered automatically (§4.1).
- Actions can also set one device, switch an automation on or off or run it, and hold the heating (§4).
- After a tap the user sees that it is done, or which devices did not respond (§6.3).
- Home shows up to four pinned ambiances in the "Une ambiance, un geste" row. A new "Mes ambiances" page lists them all and is where they are created and edited, through a simple form, not the flow canvas (§6).
- The four mockup ambiances are offered as editable examples (§6.4).
- MCP can list, create, change, delete and apply ambiances (§5.3).

**Non-goals (this round)**
- **Active state.** A tile does not stay lit to show the house is "in" that ambiance. *(Claude's call, played back, user: "yes")*. Could come later as a derived state.
- "Capture the current state" to build an ambiance from what the house looks like now.
- An ambiance as a flow node or a flow trigger ("at 23:00 apply Bonne nuit"). For now a flow cannot apply an ambiance; an ambiance can run a flow (§4.4).
- Phone notifications as a direct action. They are available through a flow the ambiance runs.
- Delays and conditions inside an ambiance. Logic stays in flows (§4.4).
- The Android app. The contract (§A) is designed so it can follow; it is a separate project.
- Zigbee2MQTT native scenes (store/recall). A `scene.*` entity can still be activated by a device action (§4.2).

**Anti-goals**
- Turning on, opening or triggering something the user did not ask for. A category "off" must never touch sirens, door/gate relays or chimes (§4.1).
- An ambiance that half-applies silently. Every action that failed or was skipped is reported.
- Different behaviour between web and MCP: both call the same hub apply.
- A look that only works in one theme, or labels that are not translated (§7).

## 3. What an ambiance is (decided with the user)

- A **name**, a **subtitle** (one short line, optional) and an **icon** chosen from a fixed set (§6.2).
- **Pinned** or not, plus an **order**. The Home row shows the pinned ones in order, at most four.
- An ordered **list of actions**. A tap runs them once, top to bottom (§5.2).
- It belongs to the **house**, not to a room. A room appears only inside an action ("every light *in the bedroom*"). *(Claude's call)*
- It is fire-and-forget. Once applied, nothing in the hub remembers it, apart from the result shown to the user. *(played back, user: "yes")*

## 4. Actions

Four action types. All of them were played back to the user ("yes"). Their exact fields are Claude's calls (§A.2).

### 4.1 Category: "every light / plug / shutter"

- **What:**
  - `lights`: every light, including switches shown as a light (`showAs: light`);
  - `plugs`: every switch whose type (`SwitchType::of`) is `Switch` or `Plug`. This is an allow-list: door, siren and chime switches are never included, and any type added later stays out until it is added here on purpose. A switch with no `showAs` reads as `Switch` (existing rule) and is included. A relay the user does not want switched off goes in `except` or gets the door type;
  - `covers`: every cover.
- **Where:** the whole house, or one area.
- **To:**
  - `lights`: `off`, or `on` with an optional brightness (%). Lights that are not dimmable just turn on;
  - `plugs`: `off` or `on`;
  - `covers`: `close` or `open`.
- **Except:** an optional list of entity ids left alone (the fridge plug in another house).
- The set is resolved **when the ambiance is applied**, from the live house. Disabled entities, diagnostic/config entities and folded entities are not part of a category. Hidden-from-Home entities are included: "hidden" is a display choice. Hub-published group entities (`group_*`) are excluded so a member is not commanded twice. Entities whose availability is `Offline` are skipped and reported as skipped. `Unknown` availability (no availability topic) counts as reachable and is commanded. *(Claude's calls)*
- **Already there:** an entity already in the target state is not commanded and counts as `ok`. Examples: a light already off for `off`; a light on at the asked brightness, ±2 %, for `on`; a cover already closed for `close`. This keeps "Bonne nuit" quick and its report clean when half the house is already off. The same rule applies to device actions when the command maps to a comparable state (on/off, brightness, position, select option, setpoint). Other commands are always sent. *(Claude's call)*
- The TV in the user's example is reached through `plugs` if it hangs off a plug, or through a device action (§4.2). Oikos has no media-player kind today.

### 4.2 Device: "set this one device"

- One entity plus the same command JSON `send_command` takes (on/off, brightness, colour, cover position, climate setpoint or mode, a select option, a generic control such as a Z2M `scene.*` `activate`).
- This also covers the house-mode select (`mode_entity` in home settings) and thermostats driven directly.
- Validated against the entity's capabilities when the ambiance is saved, with the same rules as a flow `command` node.

### 4.3 Heating hold (climate engine)

- Holds a zone, or every zone, at a setpoint and/or mode for a duration (hours), using the climate engine's existing override (`OverrideRequest`, origin user). Or it clears the holds, so the schedules apply again.
- `setpoint` is a temperature or `"off"`, as `OverrideRequest` already allows. The editor offers both. `hours` (1 to 720) is sent as `minutes = hours × 60`.
- `zone: null` means every zone the engine knows at apply time. It is a loop, with one outcome line per zone in the report. `clear` on a zone with no hold is `ok`. *(Claude's calls)*
- The climate engine requires a duration, so "until I come back" is expressed as a long hold paired with a "clear holds" action in the coming-home ambiance. *(Claude's call)*
- This action is offered only when the climate engine is enabled. When it is disabled, a saved heating-hold action is reported as skipped at apply time.

### 4.4 Automation: "switch on / switch off / run a flow"

- `enable` and `disable` set a flow's `enabled`, like the `flow.enable` node.
- `run` starts a manual run, like `run_flow`. This is the escape hatch for anything an ambiance cannot do itself (delays, conditions, notifications). The ambiance does not wait for the run to finish. *(Claude's call)*

## 5. Hub (Claude's calls; the user left the technical side to Claude)

### 5.1 Storage

Retained MQTT, like dashboards and groups: one topic per ambiance, `<prefix>/<id>`, with the prefix configurable (`[ambiances] prefix`, default `oikos/ambiance`). An empty retained payload deletes the ambiance. The hub keeps a decoded copy. An invalid payload is kept with its reason and listed as invalid, never dropped, following the pattern of `crates/oikos-os/src/dashboards/store.rs`.

### 5.2 Apply

- `apply(id)` runs the actions in order. Within one action, the commands go out together. The next action starts once every command of the current one has been accepted or has failed. Commands go through `Hub::send_command`, so the echo/acceptance rules and timeouts are the existing ones.
- A failure never stops the run. Each action reports `ok`, `failed` (with the reason and the entities) or `skipped` (with why: offline, no longer exists, climate engine disabled).
- At most one apply per ambiance at a time. The lock is in hub memory, so it holds across web tabs and MCP, and a hub restart releases it. A second request while one runs is refused with "already running". Different ambiances may run at the same time; the last command wins.
- **References that disappear.** Save refuses unknown references (§5.3). If an entity, area, flow or zone is removed later, the ambiance stays valid. Apply reports that action as `skipped: no longer exists`, and the editor marks the row as missing so the user can fix or remove it. Only a payload that does not decode or breaks the schema is listed as invalid. *(Claude's call)*
- No auth: the hub has none in v1 (`http/mod.rs`), and ambiances follow flows and groups. *(existing rule)*
- The result is returned to the caller and broadcast on the WebSocket, so another open tab can show it too.

### 5.3 API and MCP

- HTTP:
  - `GET /api/v1/ambiances` lists the ambiances, including invalid ones with their reason;
  - `PUT /api/v1/ambiances/{id}` creates or replaces one. It is validated against the house, and unknown entities, areas, flows and zones are refused;
  - `DELETE /api/v1/ambiances/{id}`;
  - `POST /api/v1/ambiances/{id}/apply` returns the per-action report.
- WebSocket: definition changes and apply results are pushed, the same way flows and groups are today.
- MCP: `list_ambiances`, `set_ambiance`, `delete_ambiance` and `apply_ambiance`, plus a paragraph in the server instructions (`crates/oikos-os/src/mcp/mod.rs`) describing the action types.
- Shared types live in `oikos-api` (`Ambiance`, `AmbianceAction`, `ApplyReport`). The category resolution and the validation live in `oikos-core`, so the web can show "12 lights" in the editor with the same rules.

## 6. Web UI

### 6.1 Home row

- Title "Une ambiance, un geste" / "One mood, one tap". On the right, "Comment ça marche" / "How it works" opens a short explanation sheet: what an ambiance is, that categories include new devices, and a link to "Mes ambiances".
- Tiles as in the mockup: icon in a soft square, name, subtitle. Four across on wide screens. On a phone they wrap into a 2×2 grid. *(Claude's call; the builder may choose a horizontal scroll if it reads better)*
- A tap shows a pressed/busy state on the tile, then a toast: "Bonne nuit: done", or "Bonne nuit: 2 devices did not respond", which opens the report.
- With no pinned ambiance, the row shows a single invitation tile linking to "Mes ambiances". *(Claude's call)*

### 6.2 "Mes ambiances" page

- Sidebar entry "Mes ambiances" / "My moods" between Overview and Automations, with the sparkle icon from the mockup. On a phone it goes in the same place as the other sections. The Ivoire spec deferred this entry until this project exists.
- Route `/ambiances`. It lists every ambiance as a card, with a pin toggle and Apply, Edit and Delete. Invalid ones are shown with their reason. The list can be reordered with up/down buttons, which rewrite `order`.
- Pinning a fifth ambiance is allowed. Home shows the first four pinned ones by `order`, and the page says "Only the first 4 pinned show on Home". *(Claude's call)*
- The client sets `order` on create (current maximum + 1) and the id (slug of the name, with `_2`, `_3`… added if it is taken). The hub only validates them.
- Editor (`/ambiances/new`, `/ambiances/{id}`):
  - name, subtitle, icon picker (about 12 icons drawn in the `icons.rs` style: sofa, moon, door-out, book, sun, sparkle, film, plate, bath, leaf, flame, house) and a pin toggle;
  - the action list. "Add an action" asks for one of the four types, then shows a small form per type, reusing the entity picker and the per-kind command controls that already exist. Each row can be moved up or down and removed;
  - a category row shows its live count ("every light in the house: 12") and a link to see which ones;
  - Save, and "Try it" (applies the saved version).
- Not the flow canvas.

### 6.3 Report

A sheet that lists each action and its outcome, naming the entities that failed or were skipped and why. It is opened from the toast and from the card in "Mes ambiances" (last result of this session only; nothing is persisted).

### 6.4 Examples

When the house has no ambiance at all, the Home row invitation and the "Mes ambiances" empty state offer **"Add the four examples"**. This writes four ordinary, editable, pinned ambiances in the current UI language:

- **Bonne nuit** (La maison s'endort): every light off, every plug off, every shutter closed.
- **Je m'absente** (L'esprit tranquille): every light off, every plug off.
- **Soirée tranquille** (Lumières tamisées): every light on at 30 %.
- **Un peu de lecture** (Votre coin préféré): no action yet. Its card says "Add a lamp" and opens the editor.

*(Claude's call. The user agreed to "pre-filled" examples. Doing it from a button in the web, instead of a hub seed, gets the language right and stops the examples coming back after the user deletes them.)*

## 7. Constraints

- Every new label goes through `crates/oikos-web/src/i18n.rs`, FR and EN.
- Themes change only the look. The row, the page and the editor use theme tokens and work in Night Glass and Ivoire.
- Generic: no house-specific names, entity ids or hosts in tracked files, tests or docs. The examples use only categories.
- Docs: a user page (`docs/users/`) and a reference page (`docs/reference/`) for ambiances. The MCP tools are listed where the other tools are.
- Tests follow the existing patterns: store and validation unit tests, category resolution tests (showAs light counted as a light, door/siren/chime never in `plugs`, groups excluded, `except` honoured), and an apply test with a failing and an offline entity.

## 8. Left to the builder

- Module layout in `oikos-os` (an `ambiances/` module beside `groups/` and `dashboards/` is expected).
- How apply results reach the toast (HTTP response, WS event, or both).
- The exact icon drawings and the tile's busy animation.
- Whether the category count link opens a sheet or an inline list.
- Command timeouts: reuse whatever `send_command` uses.

## Appendix A — Contract

### A.1 Topic

`<prefix>/<id>`, retained, default prefix `oikos/ambiance`. `id` is `[a-z0-9_]+`, set by the client on create (slug of the name, made unique). Empty payload = deleted.

### A.2 Definition

```json
{
  "id": "bonne_nuit",
  "name": "Bonne nuit",
  "subtitle": "La maison s'endort",
  "icon": "moon",
  "pinned": true,
  "order": 1,
  "actions": [
    { "type": "category", "what": "lights", "to": "off", "area": null, "except": [] },
    { "type": "category", "what": "lights", "to": "on", "brightness": 30, "area": "living_room" },
    { "type": "category", "what": "plugs", "to": "off" },
    { "type": "category", "what": "covers", "to": "close" },
    { "type": "device", "entity": "select.house_mode", "command": { "type": "select", "option": "night" } },
    { "type": "climate", "zone": null, "setpoint": 16.0, "mode": null, "hours": 12 },
    { "type": "climate", "zone": null, "clear": true },
    { "type": "flow", "flow": "away_lights", "do": "enable" }
  ]
}
```

- `area: null` / `zone: null` means the whole house / every zone. `subtitle`, `icon`, `except`, `brightness`, `mode` and `setpoint` are optional. The `command` object is exactly what `send_command` accepts. The ids in this example are placeholders.
- Unknown top-level fields are kept on round trip, like `Flow.extra`, so the Android app can add display fields later.

### A.3 Apply report

```json
{
  "ambiance": "bonne_nuit",
  "started_at": 1761331200,
  "actions": [
    { "index": 0, "outcome": "ok", "entities": ["light.a", "light.b"] },
    { "index": 2, "outcome": "failed", "entities": ["switch.c"], "reason": "no state echo" },
    { "index": 3, "outcome": "skipped", "entities": ["cover.d"], "reason": "offline" }
  ]
}
```

A category action with some failures and some successes is reported as `failed` and lists only the failing entities, with the total count. The builder may split this into per-entity lines if it reads better.
