# Oikos-os hub — verification run against the house

Date: 2026-10-19 · Plan: `2026-10-18-oikos-os-hub.md` Task 15 · Commit under test: ba93da3

## 1. Gates

```
cargo fmt --all --check                                   ok
cargo clippy --workspace --all-targets -- -D warnings     clean
cargo test --workspace                                    135 passed, 0 failed, 0 ignored
cargo check -p oikos-core --target wasm32-unknown-unknown ok
cargo check -p oikos-api  --target wasm32-unknown-unknown ok
```

## 2. Run against the real broker (`oikos.toml`, prefixes `["oikos","homeassistant"]`)

| Observation | Value |
|---|---|
| Time to a complete snapshot after start | 1.6 s (discovery); state topics subscribed after the 500 ms debounce, 92 topics |
| Entities / devices / areas / ignored | 464 / 66 / 11 / 26 |
| Ignored reasons | `device_automation` 13, `update` 10, `device_tracker` 2, `text` 1 |
| Template failures | none |
| Warnings or errors in the log | none; password absent at info and debug level |
| Entities with unknown state at steady state | 115 of 464: 64 diagnostic/config entities whose keys are absent from the retained payloads, 10 buttons (never carry state), 41 primary entities of four custom multi-endpoint devices (`greenhouse/soil_sensors`, `water_heater`, `miscellaneous/heater_mobile`, `outdoor/light_sensors_door`) whose per-endpoint topics are not retained and fill in at the next report |

## 3. MCP (raw streamable HTTP, and `claude mcp list` → Connected)

- `list_areas`: living 143, bedroom1 32, bedroom2 33, bedroom3 22, office 33, bath 5, garage 79, dehors 35, stair 16, upstairs 1, Unassigned 65.
- `list_entities` area `living`: 27 primary entities.
- `send_command` `stop` on the office shutter (idle, no physical effect): accepted in 0.33 s, confirmed by the device's state echo (`closed`, position 0). A light was deliberately not toggled during the night run.
- `set_override` `{"area":"office"}` on a bath battery sensor without override → effective area `office`, retained `oikos/ui/<id>` written; `clear_override` → area `bath`, `raw_override` null. Both writes logged at info.
- HTTP API checked in Task 12: health, snapshot, WebSocket first frame, PATCH/DELETE override, 400/404 mapping.

## 4. Shutdown

SIGINT → `shutting down` → `stopped`, process gone within 2 s; rumqttc disconnect sent before the HTTP task is aborted.
