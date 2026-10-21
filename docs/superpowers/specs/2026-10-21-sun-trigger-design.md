# Sun trigger and "is it dark" check for Oikos-os flows

Author: Gekkotron
Date: 2026-10-21
Status: approved in conversation, awaiting builder check

## Why

Homey's headline flow example is "turn on the light after sunset". Oikos-os flows today have `trigger.state`, `trigger.schedule` (5-field cron in the house local time), `if` (with `missing`), `delay`, `command`, `flow.enable` and virtual entities — but no notion of sunrise, sunset or darkness. This project adds that gap in a single pass, with the smallest possible vocabulary.

The user's words: "only on flow first node point with lat/long in settings" (the sun thing lives only as a trigger node, and the house's point goes in `oikos.toml`), then relaxed to "ok to ask is it dark too" (so darkness is also usable as an `if` condition, not just a trigger). Civil dusk/dawn — not pure sunset/sunrise — is what "dark" means at this house.

## Goals

- `trigger.sun` node that fires at sunrise, sunset, civil dusk or civil dawn, with a signed minute offset, at the house's point.
- `if` operand `op: "is_dark"` that is true between civil dusk and civil dawn.
- `[flows] latitude` and `[flows] longitude` in `oikos.toml`, with a Paris default and a save-time warning until the user sets their own point.
- Web canvas editor for the new trigger, mirroring the schedule editor's friendly picker.
- MCP tool docs (`set_flow` schema and oikos server instructions) teach the new node and operand so Claude authors sun flows without hand-written JSON.
- `oikos-core::flow::sun` holds the pure computation (lat, lon, date → sunrise, sunset, dusk, dawn) and stays wasm-clean.

## Non-goals

- Lux-based darkness: no reading of illuminance sensors, no threshold tuning. Pure sun geometry.
- Per-room or per-area sun points: one point for the whole house.
- `is_dark` as a snapshot scalar, MCP entity or Home tile: it is a flow-only `if` operand (user picked option A). No `sun_dark` virtual entity, no entry in `list_entities`, no `send_command` surface.
- A location editor on the Settings screen (that's a future project).
- Catch-up on missed events: a sunset that passed while the hub was down is not replayed at boot. Matches `trigger.schedule`.
- Any change to `trigger.state`, `trigger.schedule`, `delay`, `command`, `flow.enable` or virtual entities.
- Nautical dusk (-12°) or astronomical dusk (-18°): civil only.
- Solar noon or azimuth-based events.

## Anti-goals

- A sun flow silently running against the wrong house because the user forgot to set `latitude`/`longitude`. The hub ships Paris as a fallback but every `set_flow` affected flags a visible warning until a real point is set.
- A trigger storm at hub boot or after a broker reconnect. Sun events are instants, not retained state; the engine computes the next event in local time and arms a single `tokio::time::sleep`, so a boot replay of entity state can never fire a sun trigger.
- A `trigger.sun` or `op: is_dark` saved against a hub that doesn't recognise them being rewritten or dropped. Unknown keys continue to be preserved verbatim (`extra`), per the flows design.
- An `if is_dark` that behaves differently depending on the last entity that triggered the flow. The operand ignores the triggering entity entirely; it reads the sun angle at the moment of evaluation.
- Drift of several minutes because atmospheric refraction is ignored. The computation uses the standard "apparent sunrise/sunset" (refraction included, sun centre at -0.833° for sunrise/sunset and -6° for civil dusk/dawn).

## Constraints

- Follow the flows design (`docs/superpowers/specs/2026-10-19-flows-design.md`) and the schedule + virtual design (`docs/superpowers/specs/2026-10-20-flow-schedule-virtual-design.md`): one retained JSON doc per flow on `oikos/automation/<id>`, unknown keys preserved, nodes `{type, params}`, `set_flow` validation against the house.
- Engine safety rules stay: triggers fire only on known-to-known changes (vacuous for instant triggers), replays never start runs, one active run per flow governed by `mode` (`single` skips, `restart` cancels).
- `oikos-core` must keep building for wasm32: the sun computation is pure and lives there; the OS clock, timers and config loading stay in `oikos-os`.
- Home is in Europe/Paris. The time zone comes from the existing `[flows] timezone` (default: the system zone). Sun events are computed in UTC and converted to the configured zone for display and timer arming. DST is handled by the time-zone layer, not by the sun code.
- Attribution: Gekkotron only; commits as Gekkotron.
- Workspace gates: `cargo test --workspace`, clippy `-D warnings`, `cargo fmt`, the three wasm checks.

## Decisions made with the user

- The sun thing lives as a trigger node (first node of a flow). The user originally wanted it to live *only* there.
- Darkness is also available as an `if` operand (option A: new `op: "is_dark"` on the existing `if` node). Not a new node kind, not a virtual entity.
- Civil dusk / civil dawn (sun centre at -6° below horizon, refraction included) is what "dark" means — not sunset/sunrise, not nautical, not lux.
- Signed minute offsets on sun events (`sunset −30`, `sunrise +15`).
- One `trigger.sun` node with `which: sunrise | sunset | dusk | dawn` and `offset_minutes` — not two or four node kinds. Matches `trigger.schedule` carving everything into one node.
- `latitude`/`longitude` live in `oikos.toml` under `[flows]`, same section as `timezone`. No web Settings editor in this project.
- When `latitude`/`longitude` are absent, the hub falls back to Paris and `set_flow` returns a warning that the editor and MCP surface. The flow still runs.

## Design

### 1. Sun trigger (`trigger.sun`)

A new trigger kind alongside `trigger.state` and `trigger.schedule`:

```json
{
  "id": "evening",
  "type": "trigger.sun",
  "params": { "which": "sunset", "offset_minutes": -30 }
}
```

- `which`: one of `"sunrise"`, `"sunset"`, `"dusk"`, `"dawn"`. Required.
  - `sunrise`/`sunset`: sun centre at -0.833° below horizon (standard apparent sunrise/sunset, refraction included).
  - `dusk`/`dawn`: sun centre at -6° below horizon (civil twilight boundary). `dawn` precedes `sunrise`; `dusk` follows `sunset`.
- `offset_minutes`: signed integer, range `[-720, 720]`. Omitted = 0. Values outside the range are a validation error. *Claude's call: ±12 h is generous enough for "fire at sunset −2 h for a daytime chore" and tight enough that a typo of `72000` is caught at `set_flow`.*
- Fires once per calendar day in the configured time zone, at the event instant + offset. A run started by a sun trigger has no trigger value; an `if` without `entity` after it (except `op: is_dark`) is a validation error, same rule as schedule-started runs.
- No catch-up: an event that already passed when the hub boots, the flow is enabled, the broker reconnects, or the flow is redefined is never fired retroactively. The next fire is the next upcoming event + offset after `now()`.
- A flow may hold `trigger.state`, `trigger.schedule` and `trigger.sun` nodes side by side; any of them starts a run.
- Validation: `which` must be one of the four strings; `offset_minutes` must be in range; `params` with no other recognised keys may still carry an unknown key (preserved verbatim as `extra`).

### 2. `if` operand `is_dark`

The existing `if` node gains an operand:

```json
{ "id": "when_dark", "type": "if", "params": { "op": "is_dark" } }
```

- No `entity`, no `value`, no `missing`. Validation rejects a flow that sets any of them on this operand.
- True between the moment the sun centre drops below -6° (civil dusk) and the moment it rises back above -6° (civil dawn), at the house's point, in the configured time zone.
- The operand ignores the triggering entity; it reads the sun angle at the moment of evaluation. Works identically in state-triggered, schedule-triggered, sun-triggered and manual runs.
- The runtime value is always well-defined (the sun is either below the threshold or above it), so `missing` does not apply.
- Available in `set_flow` (hub-side) and in the web canvas's per-node parameter editor (web-side).

### 3. Configuration (`oikos.toml`)

New in `[flows]`:

```toml
[flows]
timezone = "Europe/Paris"       # already here
latitude = 45.75                # new, optional
longitude = 4.85                # new, optional
```

- `latitude`: `Option<f64>` in `[-90.0, 90.0]`. Absent = Paris fallback.
- `longitude`: `Option<f64>` in `[-180.0, 180.0]`. Absent = Paris fallback.
- Paris fallback: 48.8566, 2.3522 (city hall). `Claude's call.`
- Both must be present or both absent. If exactly one is present, the hub refuses to start and logs the config error. *Claude's call: a half-set point is a bug, not a feature.*
- Values outside their ranges are a startup error, same as any `figment` validation.
- No live reload: changing the point requires a hub restart. *Claude's call: matches the current behaviour of every other `[flows]` key.*

Save-time warning:

- When a `set_flow` or `PUT /api/v1/flows/{id}` saves a flow that mentions `trigger.sun` or `op: is_dark`, and both `latitude` and `longitude` were absent at hub startup (not: happen to equal the Paris default), the response is a success with a `warning` field attached. Wording: "Running sun flows against the Paris fallback. Set `[flows] latitude` and `[flows] longitude` in `oikos.toml` to use your house's point."
- The warning rides on the wire type `FlowWriteOutcome { flow: FlowView, warning: Option<String> }` returned by `PUT /api/v1/flows/{id}` and the MCP tool `set_flow`. These two are the only endpoints whose reply shape changes. `GET /api/v1/flows[/{id}]`, `DELETE`, `POST .../enabled`, `POST .../run`, `list_flows`, `get_flow`, `delete_flow`, `set_flow_enabled` and `run_flow` keep their current shapes. *Claude's call: always wrap the two writes (`{ flow, warning: null }` when there is no warning), one schema change, no `oneOf`.*
- A new config-visibility endpoint `GET /api/v1/config/flows` returns `{ latitude: Option<f64>, longitude: Option<f64>, timezone: String, fallback_active: bool }` (`fallback_active` is true iff both coordinates were absent at startup). The web canvas reads it on load, carries the result in the client-side app state, and uses it for (i) the "Paris fallback" badge on sun nodes and the `if is_dark` body, (ii) the next-three-fire-times preview, and (iii) the editor's live summary. Snapshot does **not** gain a flows-config field: the data is public-ish but orthogonal to the house state, and belongs behind its own endpoint. *Claude's call.*
- The web canvas surfaces the warning as a toast on save and as a badge on the sun node and the `if is_dark` body ("Paris fallback"). The badge persists across page reloads because it is derived from `GET /api/v1/config/flows`, not from the save response. The badge disappears once lat/long are set in `oikos.toml` and the hub restarts.
- `list_flows` and `GET /api/v1/flows` do not carry the warning. The warning is a save-time hint; the per-flow fallback state is derived from the config endpoint.

### 4. Sun computation (`oikos-core::flow::sun`)

Pure module next to `schedule.rs`, built around **one primitive**:

- `pub fn sun_altitude(lat: f64, lon: f64, at: DateTime<Utc>) -> f64` — apparent solar altitude in degrees at the point and instant, atmospheric refraction included. Everything else in the module derives from this one call. *Claude's call: a single shared evaluator guarantees `trigger.sun dusk` and `op: is_dark` can never disagree about where the -6° boundary sits.*
- `pub fn sun_events(lat: f64, lon: f64, date: NaiveDate) -> SunEvents` returning:
  ```rust
  pub struct SunEvents {
      pub sunrise: Option<DateTime<Utc>>,  // first crossing of altitude = -0.833° upward on `date` (local)
      pub sunset: Option<DateTime<Utc>>,   // first crossing of altitude = -0.833° downward on `date` (local)
      pub dawn: Option<DateTime<Utc>>,     // first crossing of altitude = -6°   upward on `date` (local)
      pub dusk: Option<DateTime<Utc>>,     // first crossing of altitude = -6°   downward on `date` (local)
  }
  ```
  Computed by root-finding on `sun_altitude` (same primitive). `None` means the event does not happen that day at that latitude (polar night / midnight sun / permanent civil twilight). Doesn't happen in France; included for correctness.
- `pub fn is_dark_at(lat: f64, lon: f64, at: DateTime<Utc>) -> bool` is `sun_altitude(lat, lon, at) <= -6.0`. The `<=` boundary matters for `trigger.sun which=dusk → if op: is_dark`: in practice the trigger fires at a tokio tick a few ms past the theoretical crossing, so altitude is already strictly below -6°, but using `<=` guarantees the branch holds even at the exact boundary. *Claude's call.*
- The dusk → is_dark consistency is tested as part of the module (see §8).
- Keeps `oikos-core` wasm-clean (no clock, no I/O, no OS).
- Builder picks the crate or writes the NOAA algorithm. Constraints: pure (no `std::time`), stable sign conventions (east-positive longitude, north-positive latitude), handles polar edge cases without panicking, and either exposes an altitude(lat, lon, utc) primitive or wraps one so `sun_altitude` is the single evaluator.

### 5. Engine (`oikos-os::flows`)

- When a flow is enabled or saved and contains at least one `trigger.sun` node, the engine computes, for each such node, the next fire time strictly after `now()` in the configured zone:
  1. Compute today's event instant for `which` at the house point.
  2. Add `offset_minutes`.
  3. If the result is in the past, repeat with tomorrow's date. Repeat until the result is in the future, bounded at seven days (polar edge safeguard; a flow whose next sun event is more than seven days out is logged and left disarmed until re-enabled).
- Arm one `tokio::time::sleep_until` per armed sun node. On fire, start a run (respecting `mode`), then recompute the next fire and re-arm.
- Disabling, deleting or redefining a flow disarms its sun nodes, matching the schedule timer's lifecycle.
- Re-arming of a sun node that hit the seven-day polar cap happens on exactly three events: a `set_flow_enabled` off→on toggle for its flow, a `set_flow` save of its flow, and a hub restart. No daily background recheck. *Claude's call: the cap is a France-never, polar-only safeguard; adding a midnight job for it would introduce engine infrastructure the schedule side also doesn't have.*
- Validation of "a run started by a sun trigger has no trigger value" is **static, at `set_flow` time** (walk the graph from each trigger root; reject when a reachable `if` lacks both `entity` and `op: is_dark` while the only path in is sun or schedule). Same locus as the schedule precedent in `2026-10-20-flow-schedule-virtual-design.md`. The runtime engine does not re-check.
- The broker link going offline does not disarm sun timers (they are time-based, not broker-based). A manual run of a sun-triggered flow is unaffected.
- The engine's known-to-known entity map and the retained-replay silence rules are untouched — sun events are instants with no state to replay.

### 6. Web canvas editor (`crates/oikos-web`)

Built on the per-node editor sheet from `da17604`, same as the schedule editor:

- The sheet for `trigger.sun` has:
  - A segmented control picking `which`: Sunrise / Sunset / Dawn / Dusk.
  - A plain-English offset picker: three chips "At the event", "Before", "After"; selecting Before/After reveals a minutes input. Advanced users can type any value in `[-720, 720]`.
  - A plain-English summary line that updates live: "At sunset", "15 min before sunrise", "30 min after civil dusk".
  - The next three fire times in local time, computed from the sun module.
  - A "Paris fallback" badge when the config has no explicit lat/long, with a one-line hint ("Set `[flows] latitude`/`longitude` in `oikos.toml`.").
- The sheet for `if` with `op: is_dark` collapses the entity picker and value field, showing only an "Is it dark now? (sun below -6°)" summary and the Yes/No port labels, consistent with the existing `if` sheet.
- Add buttons for both items appear in the "add node" menu in the same place the other node kinds are added. New glyphs in the icon set: a sun for `trigger.sun`, a moon for `op: is_dark` (shown on the `if` node when that operand is selected). *Builder's call: exact SVG; both glyphs are drawn flat, one colour, like the existing set.*
- The warning returned by the save API is shown as a toast ("Sun flows are running against the Paris fallback — set `[flows] latitude`/`longitude` in `oikos.toml`") once per save. The persistent badge is driven by `GET /api/v1/config/flows` (read on page load and after each save), not by the save response, so a browser refresh still shows the badge until a hub restart clears `fallback_active`.

### 8. Tests

- Unit tests in `oikos-core::flow::sun`: `sun_altitude` cross-checked against published tables for Paris and Lyon on 1 January and 21 June; `sun_events` returns the four events within ±60 s of the published values; `is_dark_at(dusk)` is true and `is_dark_at(dusk - 1s)` is false (and symmetric at dawn), proving the shared-primitive consistency; polar day/night returns `None` for the relevant events without panicking.
- Unit tests in `oikos-core::flow::validate`: `trigger.sun` with out-of-range `which` or `offset_minutes` is rejected; `op: is_dark` with any of `entity`/`value`/`missing` set is rejected; a flow whose only trigger is `trigger.sun` and whose downstream `if` lacks both `entity` and `op: is_dark` is rejected statically.
- Integration test in `oikos-os/tests`: a flow with `trigger.sun which=dusk offset_minutes=0` arms, fires at a mocked clock instant, and completes one run; disabling the flow mid-arm cancels the timer; the seven-day cap leaves a flow disarmed and re-arms on `set_flow_enabled` toggle.
- REST and MCP round trips: `PUT /api/v1/flows/{id}` and MCP `set_flow` return the `{ flow, warning }` wrapper, with `warning: null` when lat/long are set and the expected string when they are absent; `GET /api/v1/config/flows` returns the expected shape and `fallback_active` flips as the config changes.
- Web unit tests: the sun sheet writes a valid `trigger.sun` from each preset; the summary line and the next-three-fire-times derive from the shared `sun_events`; the "Paris fallback" badge appears iff the config endpoint returns `fallback_active: true`.
- Wasm smoke test: the sun module compiles for `wasm32-unknown-unknown` and `sun_events` returns a non-empty result in the browser build.

### 7. MCP (`oikos-os::mcp`)

- `set_flow` schema (JSON Schema derived from the Rust model) grows the tagged variant `trigger.sun` with `which` enum and `offset_minutes` integer, and the `if.params.op` enum grows `is_dark` with no required siblings.
- Server instructions get a paragraph after the existing one on flows: "Sun events use `trigger.sun` with `which` one of `sunrise`, `sunset`, `dusk` (civil), `dawn` (civil), and an optional `offset_minutes` in `[-720, 720]`. `if` with `op: is_dark` is true between civil dusk and civil dawn at the configured point. Set `[flows] latitude` and `[flows] longitude` in `oikos.toml`; the hub falls back to Paris otherwise and reports a warning on save."
- The `set_flow` reply is **always** `{ flow: FlowView, warning: Option<String> }`, same shape as the REST write — the wrapper is present even when there is no warning (`"warning": null`). One schema change, no `oneOf`. `get_flow`, `delete_flow`, `set_flow_enabled`, `run_flow` and `list_flows` keep their current shapes. *Claude's call: the only MCP client is this project's own assistant, which reads whatever the tool returns, so the churn is contained.*

## Calls made by Claude (check these)

- Offset range capped at ±720 minutes (±12 h), with values outside rejected at `set_flow`. §1.
- Civil dusk / civil dawn use sun centre at -6° with atmospheric refraction included; sunrise / sunset use -0.833° (apparent). §4.
- Dusk and dawn also accept `offset_minutes`, same semantics as sunrise/sunset. §1.
- `op: is_dark` rejects `entity`, `value` and `missing` at validation time. §2.
- Paris fallback coordinates: 48.8566, 2.3522 (city hall). §3.
- `latitude` and `longitude` must be both present or both absent; one-of-two is a startup error. §3.
- The save-time warning is attached only when both coordinates were absent at startup, never because they happen to equal the Paris numbers. §3.
- No live reload of lat/long: a change requires a hub restart. §3.
- No catch-up on missed sun events; the engine arms the next upcoming fire only. §5.
- Seven-day arming cap as a polar safeguard; a flow whose next fire is more than a week out is logged and left disarmed. §5.
- The warning field is on `set_flow` and `PUT /api/v1/flows/{id}` only; not on reads, not on runs, not on deletes. §3, §7.
- The two writes always return the `{ flow, warning }` wrapper (`"warning": null` when there is none), so the schema has one shape. The other seven flow endpoints and MCP tools keep their bare shapes. §7.
- A new endpoint `GET /api/v1/config/flows` exposes `{ latitude, longitude, timezone, fallback_active }` so the web can compute the next-three-fire-times preview locally (`oikos-core::flow::sun` is wasm-clean) and keep the "Paris fallback" badge visible across page reloads. §3.
- The badge is derived from this endpoint, not from the save response, so a page reload still shows it. §3.
- `oikos-core::flow::sun` is built around one primitive `sun_altitude(lat, lon, utc) -> f64`; `sun_events` and `is_dark_at` both derive from it, so `trigger.sun which=dusk` and `op: is_dark` cannot disagree about the -6° boundary. §4.
- `is_dark_at` uses `altitude <= -6.0` so the branch holds at the exact theoretical boundary; in practice the tokio tick lands a few ms past it. §4.
- The seven-day polar cap re-arms only on toggle, save, or restart. No daily background recheck. §5.
- "Run started by a sun trigger has no trigger value → downstream `if` needs `entity` or `op: is_dark`" is enforced **statically** at `set_flow` time, matching the schedule precedent. §5.
- Light to no visible UI for lat/long in this project: `oikos.toml` only. The Settings-screen editor is a follow-up project. §3.
- A sun trigger with `offset_minutes` applied puts the fire within the next-day window naturally; no cross-midnight edge case needs a special path beyond the "repeat until in future" loop. §5.

## Left to the builder

- Crate choice for sun computation (`sunrise`, `spa`, hand-written NOAA, …), with the constraints that it must be pure, must either expose or let us wrap a `sun_altitude(lat, lon, utc)` primitive, and must handle polar edge cases without panic.
- The exact engine data structure for timers (one per armed sun node, or a merged timer wheel across schedule + sun).
- Exact SVG for the sun glyph and the moon glyph on `is_dark`, matching the existing node glyph style in both themes.
- Where the "Paris fallback" badge lives on the sun node card and on the `if is_dark` body (corner badge, line under the title, etc.), and the toast's exact wording.
- The next-three-fire-times display's visual treatment on the sheet (list, pill row, inline sentence).
- Whether `flow_canvas.rs` or a new module owns the sun-node sheet; the schedule sheet is the precedent to copy.
- Where in the web client the `GET /api/v1/config/flows` result is cached (a single leptos signal in the root app state, or a per-screen refetch) and the exact JS/Rust key for it.

## Context and related work

- Flows design: `docs/superpowers/specs/2026-10-19-flows-design.md`.
- Schedule and virtual entities: `docs/superpowers/specs/2026-10-20-flow-schedule-virtual-design.md`.
- Flow canvas (editor sheet infrastructure): `crates/oikos-web/src/screens/flow_canvas/`, landed in `da17604`.
- Existing `[flows] timezone` config: `crates/oikos-os/src/config.rs`.
- Cron computation precedent: `crates/oikos-core/src/flow/schedule.rs` (uses `croner`; the sun module is its sibling).

## Follow-ups (not in this project)

- A Settings screen in the web UI to set lat/long interactively (would need `oikos.toml` writes, which the hub does not do today).
- `is_dark` as a hub-wide scalar entity (`sun_dark`), visible in snapshots, Home and MCP. Would need a virtual-entity-like discovery or a new system-entity concept.
- Lux-based darkness as an alternative or supplement: an `op: entity_above` / `entity_below` with a numeric threshold on an illuminance entity would cover this without a dedicated `is_bright_enough` operand.
- Nautical dusk (-12°) and astronomical dusk (-18°) if a user ever wants them.
- Azimuth-based triggers ("sun hits the kitchen window at 10:30"): a feature a few homes care about, out of scope here.
