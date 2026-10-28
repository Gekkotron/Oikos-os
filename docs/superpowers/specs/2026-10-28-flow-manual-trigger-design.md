# Manual trigger card — design

Date: 2026-10-28. Status: reviewed, awaiting approval.

## Intent

A flow can only be started by its real trigger (a state change, a press, a schedule, the sun) or by
**Run now**, which always starts at the flow's *first* trigger node. There is no way to fire one
branch of a graph by hand, so testing "what happens on the *no* path of this `if`" or "does this
notify text read right" means waiting for the real event or faking a device.

The fix is Node-RED's inject node: a **Manual** trigger card you wire into any point of the graph,
with a ▶ button on the card itself. Pressing it starts a run from that card, so only the branch it
feeds runs, and it can carry a fixed value (Node-RED's payload) that the next cards read as the
trigger value.

## Goals

- A new node type `trigger.manual`, listed in the card library's WHEN group, with one unnamed output
  and no input, wired like any other trigger.
- A ▶ button on the card on the flow canvas; pressing it starts a run of the **saved** flow from
  that card.
- An optional fixed **value** on the card (a boolean, a number or a text, the `Scalar` kinds the
  `if` card compares) that the run carries as its trigger value.
- MCP `run_flow` and the HTTP run endpoint can name the card to start from, so an assistant can
  press the same button.
- A flow whose only triggers are Manual cards is valid ("button-only" flows).

## Non-goals

- Asking for a value at press time: the value is fixed on the card; two values = two cards.
- Repeat / interval injection (Node-RED's "repeat"): the Schedule card does that.
- A dashboard widget that presses one Manual card: the dashboard flow button keeps running the
  whole flow through `run_flow` without a card.
- Changing **Run now**: it keeps starting from the flow's first trigger node (which may now be a
  Manual card, in which case it behaves exactly like pressing that card) and keeps its current
  refusal rules.

## Anti-goals

- A press must never run an unsaved draft: what runs is what the hub holds.
- A Manual card must never fire on its own: it has no entity, schedule or event, so the engine's
  state, press, signal, schedule and sun paths must ignore it.
- A Manual card without a value must not let an `if`/`switch` with **Trigger value** slip through
  validation and fail at run time.

## Behaviour

### Wire format

```json
{ "id": "test_on", "type": "trigger.manual", "params": { "value": true } }
{ "id": "kick",    "type": "trigger.manual", "params": {} }
```

`params.value` is optional and is a bare JSON boolean, number or string (`Scalar`). Unknown params
are kept in `extra` like every other node. `NodeKind::is_trigger()` is true for it.

### Validation (`oikos-core/src/flow/validate.rs`)

- `trigger.manual` counts toward "at least one trigger".
- A Manual card **without** a value is a valueless trigger, exactly like `trigger.schedule` and
  `trigger.sun`: add it to the seed set of `check_ifs_after_valueless_triggers`, so an `if` or
  `switch` reachable from it must name an entity (`is_dark` excepted, as today).
- A Manual card **with** a value is a valued trigger: an `if`/`switch` without an entity reachable
  from it compares the value itself (see Running). But such an `if`/`switch` must not use
  `average`, since there is no entity whose readings could be averaged: reject it with a new error
  in the same style ("node `x`: an average after a manual trigger must name an entity").

### Running

- `run_flow` (hub, HTTP `POST /api/v1/flows/{id}/run`, MCP `run_flow`) takes an optional `node`:
  the id of a `trigger.manual` card in that flow.
  - With `node`: the run starts at that card and carries its value. Refused if the id is not a
    `trigger.manual` node of the flow (a 4xx with a reason, like the other refusals).
  - Without `node`: today's behaviour, from the first trigger node. If that node is a Manual card,
    its value is carried too.
- Refusals stay as today for both paths: broker offline, unknown, invalid or disabled flow.
- While a run is active, a **card press** follows the flow's `mode`: `single` refuses it (same
  refusal Run now gets today: "a run is active", no skipped run recorded); `restart` cancels the
  active run and starts the new one, as a real trigger would. **Run now** without `node` keeps
  refusing in both modes.
- The run is `manual: true`. It records which card started it and the value it carried, so the
  Runs drawer and the canvas replay can show "▶ Manual · on" (or "▶ Manual" without a value).
- `if` / `switch` without an entity, in a run started from a Manual card with a value, compare
  that value directly (today `node_left` reads the triggering entity's *current state*; a manual
  trigger has no entity, so it must use the carried value instead).
- `{trigger}` in a notify text prints the value (no unit, since no entity); `?` without a value,
  as today for schedule, sun and Run now.
- The engine's trigger loops (state changes, button presses, signals, schedule ticks, sun events,
  the watched-entity and averaged-window sets) skip `trigger.manual` nodes.

### Web UI

- **Card library**: a **Manual** card in WHEN; search finds it under "manual", "button", "inject",
  and the French "manuel", "bouton".
- **Canvas card**: title "Manual" or "Manual · <value>" (FR "Manuel"), its own icon, one output pin,
  no input pin. A round ▶ button on the card.
  - View mode (computer and phone): the button posts the run with the card's id. The run then
    shows live on the canvas the way any run in progress does. A refusal is reported the way Run
    now reports one today (`toolbar.rs` around `api::run_flow`).
  - Edit mode: the button is shown greyed out and does nothing, with a tooltip saying to save
    first.
  - The button is disabled on a disabled or invalid flow, like Run now.
- **Settings drawer**: one optional **Value** field: none, on/off, a number, or text.
- **`if`/`switch` value picker**: when **Trigger value** is fed by a Manual card, offer the kind of
  that card's value (on/off for a boolean, a number box for a number, Other value… for text).
- **Flow list**: the **Manual** chip, which today maps to "a valid flow with no trigger" (a set
  that is empty, since validation requires a trigger), now means "has a `trigger.manual` node", and
  rows show the Manual icon for it.
- All new strings in English and French in `crates/oikos-web/src/i18n/`.

### MCP and docs

- `run_flow` gets the optional `node` parameter; the server instructions and flow tool text name
  `trigger.manual` and its `value`.
- `docs/users/flows.md`: the trigger list, the run section (pressing a card, mode rules, the
  value), the editor section (the ▶ button, greyed in edit mode); `docs/reference/mcp-tools.md`
  for `run_flow`.

## Constraints

- Builds on the `switch` and `time.between` nodes of PR #77 (`feat/flow-switch-time-of-day`):
  validation and the engine paths touched here include `switch`. Start implementation once #77 is
  on main, branched from `origin/main` (not stacked).
- Generic: no house-specific names, rooms or hosts in tracked files, examples or tests.
- The hub and web are rebuilt and redeployed after merge; the web is the only client of
  `RunView`, and the Android app does not read flow runs, so the run contract may change.

## Calls made while writing this up (not discussed, check them)

- **Restart on a card press**: `Hub::run_flow` today refuses an active flow before the engine sees
  the request, and `run_manual` cancels the *new* run when one is active. For a card press on a
  `restart` flow the hub skips that check and the engine cancels the *active* run instead, as
  `fire` does; the run store must cope with the old run ending while the new one begins.
- **HTTP shape**: `node` goes in an optional JSON body (`{"node": "<id>"}`); a POST with no body
  keeps working as today.
- **Mixed paths in validation**: an entity-less `if`/`switch` is valid when every trigger that
  reaches it carries a value (a state, press or signal trigger, or a Manual card with a value).
  The `average` rule applies to any path from a Manual card with a value.

## Left to the builder

- How the run records its starting card and value on the wire (for example an optional `entity` on
  `TriggerInfo` plus a `node` field, or a new field on `RunView`), as long as old run JSON still
  parses and the web can show "▶ Manual · value".
- The icon, the exact button look and placement on the pill, and test layout.
- Whether `trigger_kinds`' old "no trigger → Manual" fallback is removed or kept.
