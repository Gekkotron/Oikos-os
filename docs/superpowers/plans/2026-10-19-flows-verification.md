# Flows: house verification (2026-10-19)

Commit range: f6432ed..HEAD on main (plan at f6432ed, code from 15c183e), 17 commits including the docs commit that adds this file.

## Gates

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: every suite ok, 368 passed, 0 failed.
- `cargo check` for oikos-core, oikos-api and oikos-web on wasm32-unknown-unknown: clean.
- `wasm-pack test --headless --chrome` in crates/oikos-web: `test result: ok. 1 passed; 0 failed`.

## Hub restart

`trunk build --release` then `cargo build --release -p oikos-os`; the old debug hub (pid 68856) was stopped, the release binary started from the repo root (pid 6273).

- `/health`: `{"status":"ok","connection":{"type":"online"},"entities":464}`
- log: `subscribed, topics: 6` (the automation prefix is one of them).

## First flow

`set_flow` over MCP (streamable HTTP, session via `Mcp-Session-Id`) with `living_led_motion`: three `trigger.state` nodes (Entrée, Motion north, Motion corridor), one `if eq true`, `command turn_on` on 0x60a423fffe94d1c6_light_zigbee2mqtt, and on the false port `delay 5m` then `command turn_off`. Reply (trimmed): `isError: false`, `invalid: null`, `enabled: true`, `mode: restart`, 7 nodes, 6 edges, `runs: []`.

`list_flows`: `{"flows":[{"enabled":true,"id":"living_led_motion","invalid":null,"last_run":null,"name":"Living LED on motion","nodes":7}]}`. `GET /api/v1/flows` returns the same single flow, `invalid` null. `mosquitto_sub -t 'oikos/automation/#' -C 1 -v` printed the retained document on `oikos/automation/living_led_motion`.

## Screenshots (headless Chrome over CDP; kept in the session scratchpad outside the repo, not committed)

- `flow-canvas-1280.png`, `flow-canvas-run.png` (same state, recaptured after the run), `flow-canvas-390.png`, `flows-list-1280.png`; driver: `cdp-shot.py`.

What they show: at 1280 px the canvas has three yellow trigger cards in a column on the left (Entrée, Motion north, Motion corridor), the purple `If` card with a `true` and a `false` port on its right edge, the green `Command` turn on card on the true port, and `Delay 5m` then `Command` turn off on the false port. The path of the last run is lit: Motion north (glowing), the edge to `If`, the true edge, and the turn on card (glow, "accepted"). Below, the run list has two rows: Done (Cuisine = true) and Cancelled (Cuisine = false). The list page shows one row with the name, the node chain and the last run. At 390 px the canvas scrolls horizontally (the If card is cut at the right edge, the rest is reachable by scrolling).

## Run observed

A real motion event arrived about 30 s after the flow was created (Motion north, whose occupancy entity is named Cuisine in the app, 15:46:35Z), no wait needed. The sensor reported false then true within the same second:

- r1, `cancelled`: trigger false (previous true); steps motion_north ok, moving ok ("false"); cancelled while at the `delay` by r2 (mode restart working as designed).
- r2, `done`: trigger true (previous false); steps motion_north ok, moving ok ("true"), on ok ("accepted"). The LED turn_on was confirmed by the device echo.

No manual `run_flow`, no publish to sensors, no manual LED switch. The LED is left on by r2; the next motion-false starts the 5 minute delay and the turn off.

## For the deferred list

- Mobile (390 px): the flow title in the header wraps one or two characters per line because the back button, switch and Run now button take the row; the header should wrap the title onto its own line.
- The Run now button stretches across the whole header at 1280 px; it should hug its label.
- The switch glyph renders as an orange blob with a white knob rather than a clear on/off track.
- Run rows show the raw ISO timestamp (`2026-10-19T15:46:35Z`) and no duration; a relative time ("2 min ago") would read better.
- The Entrée card shows its name twice (name and entity label are identical); the trigger cards otherwise show an area name, which is good.
- The If card shows "= true" and "true" as two lines; the second looks redundant.
- A trigger that flaps (false, true in one second) leaves a cancelled run in the history; consider hiding cancelled runs with no side effect or labelling them "superseded".
- The wasm tests `flow_canvas` and `flows_list` cover the flow pages; the smoke test only mounts the shell. (Most items above were fixed in the final fix wave below.)

## Final fix wave

Fixes after the whole-branch review: engine re-seed from every report, alarm codes refused, run_flow refused offline, the switch pill, the parked delay, handles, relative times, the `--wire` token, and a late `Running` event no longer re-activating a finished run.

### Gates

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: 376 passed, 0 failed.
- `cargo test -p oikos-os --test flows` five times in a row: `33 passed; 0 failed` each time.
- `cargo check` for oikos-core, oikos-api and oikos-web on wasm32-unknown-unknown: clean.
- `wasm-pack test --headless --chrome` in crates/oikos-web, one result line per binary:
  - `flow_canvas`: `test result: ok. 5 passed; 0 failed`
  - `flows_list`: `test result: ok. 2 passed; 0 failed`
  - `smoke`: `test result: ok. 1 passed; 0 failed`
  - `cover`: `test result: ok. 1 passed; 0 failed`
  - `sheet`: `test result: ok. 1 passed; 0 failed`
  - `wheel`: `test result: ok. 1 passed; 0 failed`
- `identical_replay_then_real_motion_fires` was run against the old engine first and failed (no run within 2 s), then passed with the fix.

### Hub restart and the re-authored flow

`trunk build --release`, `cargo build --release -p oikos-os`, old hub stopped, new one started from the repo root; `/health`: `{"status":"ok","connection":{"type":"online"},"entities":464}`.

`set_flow` over MCP replaced `living_led_motion`: same three triggers and `moving` if; true goes to `on`; false goes to `clear_entree` (if Entrée occupancy = false) then `clear_north` (north = false) then `clear_corridor` (corridor = false) then `wait` (delay 5m) then `off`; the false ports of the three clear-ifs have no edges; mode `restart`. Reply (trimmed): `isError: false`, `invalid: null`, `active_run: null`, `enabled: true`, `mode: restart`, 10 nodes, 9 edges, `runs: []` (runs are in memory and the restart emptied them).

### Screenshots (session scratchpad, not committed)

`ff-canvas-1280.png`, `ff-canvas-390.png`, `ff-list-1280.png`, `ff-list-390.png`, `ff-canvas-light-1280.png`, `ff-list-light-390.png`.

What they show:

- The enable switch is a proper pill: an amber track with the white knob inside it at the right (Night and Warm). The hit area is the invisible 48 px pseudo-element.
- Canvas at 1280: the title "Living LED on motion" is on one line; the switch, a compact "Run now" and the `restart` chip sit on the right; "Run now" no longer stretches.
- Canvas at 390: the back button is on its own row, the title on its own full line, then the switch, Run now and the chip.
- Handles: an input dot on the left edge of every non-trigger card, an output dot on the right edge of every card, and one output dot next to each of the `true` and `false` labels of every `if`. Wires start and end on the dots. In the Warm theme the wires are now clearly visible.
- Trigger cards show the node title; the entity title appears on a second line only when it differs (Motion north / Cuisine, Motion corridor / Couloir; Entrée has no second line). The canvas is wider than the viewport, so the last cards are reached by scrolling.
- The list page shows one row with the summary chain and "No runs yet". Relative times could not be seen on a real run (see below); they are covered by the wasm tests.

### Run observed

No motion event reached the hub between the restart and the screenshots, so no real run was observed in this wave, and the lit canvas, the countdown and the relative times were not screenshotted on the house (the wasm tests cover them with the run shape the engine produces). No `run_flow`, no publish to sensors, no manual LED switch.
