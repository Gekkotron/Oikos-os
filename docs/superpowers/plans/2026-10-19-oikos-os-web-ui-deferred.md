# Oikos-os web UI — deferred findings after the v1 wave

Date: 2026-10-19 · Source: per-task reviews, the whole-branch review of 53b087b..1ac7c63 (Run 1), and the scoped re-review of the fix+polish wave 27637ab..acaf854 (Run 3). Everything Critical and Important is fixed; these are the Minor items left, triaged as 'can wait'.

## Deferred minors — from the per-task ledger

- RED-phase simulated with `todo!()` rather than tests-first; `House` has no native tests (`HouseState` does); `House.connection` conflates the WS link and the broker link (both shown as offline — acceptable for the pill).
- `is_on` treats an open cover as "on" (amber).
- `CardFrame` takes a non-reactive `EntityView` — Home must key/derive per entity. Per-card rebuild model works for ~100 entities but will hurt once alerts/automations push WS volume up. Worth a dedicated task.
- Climate value lacks unit (defaults to °C when the hub sends none).
- Cards sorted by `ui.order` then name (implementer choice, keep).
- Unknown brightness renders the slider at 255.
- ModeSelect capitalises select options.
- `motion_*` entities are illuminance sensors (naming in Z2M, not ours).
- Date shown without zone conversion; sensor value font 22px.
- Brightness override slider is 1..=255 (the hub rejects 0).
- "Clear override" relies on the WS `EntityUpserted` echo for the effective name.
- Offline-reason row unverified (hub was online during testing).
- Theme signal implemented as context (fine).

## Deferred minors — from the whole-branch review (Run 1)

- `AreaField` free-text mode is one-shot (`other` seeded once from `draft.with_untracked`): if the hub later adds an area matching the stored value, the field stays stuck in "Other…". Add an Effect that flips `other` back to false when `house.areas` grows to include the current value. (`override_editor.rs:181-183`)
- Helper duplication across cards: `with_climate(state, f)` and `with_water_heater(state, f)` have the same shape; `write_state` and the optimistic branch of `fire_command` are similar across `cards/climate.rs`, `cards/water_heater.rs`, `cards/vacuum.rs`. Worth a note before more kinds land.
- `availability_class` and `availability_word` live in `screens/entity_sheet.rs` and are re-exported into `settings.rs` and `devices.rs`; `card_frame.rs:9-15` duplicates `availability_class` outright. Unify in `format.rs` or a new `components/availability.rs`.
- `is_hashed` `Cache-Control: immutable` heuristic can misfire on asset names that happen to carry a hex run after a `-` (e.g. `my-abcdef12-icon.svg`). Current Trunk output is fine; add a brief doc-test note so the next person who tweaks hashing understands the contract.
- Theme toggle button in `app.rs:62-67` renders the raw lowercase `theme.as_str()` ("auto"/"night"/"warm"). The CSS `text-transform: capitalize` papers over this at render time, but the DOM text and screen-reader output are the raw strings. Pass the formatted word (or an accessible label), same way `ThemeSwitch` does.
- `ws::connect` resets back-off only on `WsEvent::Snapshot`, which per spec is always the first WS frame. If a buggy hub ever reconnects without a Snapshot leader, the back-off climbs to the 10 s cap and never relaxes. Low-risk — add a comment explaining the assumption.
- `build.rs` creates `../oikos-web/dist/` from the hub crate at `CARGO_MANIFEST_DIR` relative path — fine for the mono-repo layout, but if `oikos-os` is ever vendored on its own this breaks silently. Add a note.

## Deferred minors — from the scoped re-review (Run 3)

- `House::part_of_children` and `HouseState::part_of_children` are `pub` and have no production callers after the Cover badge was turned into a `Memo` over `house.entities`. A test in `house_state.rs:320-331` still exercises `HouseState::part_of_children`. Either remove both along with the test, or mark them `#[deprecated(note = "replaced by inline Memo over house.entities")]`.
- Same-named sibling suffix (polish 3) uses `eq_ignore_ascii_case` + non-empty guards; the brief asked for strict `==`. The loosening is realistic (handles `door_siren` vs `Door_Siren`) but slightly broader than specified. Keep unless the mismatch starts suffixing unrelated siblings.
- `format::wrap_id` inserts U+200B (ZWSP) into displayed ids. When the DOM text is copied, the ZWSP goes with it. Sites that are plausible copy targets: `components/toast.rs:72`, the ignored-topic rows in `screens/devices.rs:103,162` and sibling-link lists. Not blocking (standard tradeoff), but mild friction for anyone pasting a topic into mosquitto_sub. Consider stripping ZWSP on copy, or dropping the wrap on the toast and ignored-topic sites.
- `cargo clippy --workspace --all-targets` runs native only; no `.cargo/config.toml` pins `wasm32-unknown-unknown`. A dedicated `--target wasm32-unknown-unknown` clippy pass would catch wasm-only lints. Follow-up candidate.

## Seams for later waves

- The HS colour wheel (`components/hs_wheel.rs`) is driven live by pointer and keyboard; it does NOT round-trip through the override form. If an override field for a preset HS value is ever added, the wheel's `hs_at` / `marker_at` math is reusable.
- `OverrideEditor` is now mounted once per entity id via a one-shot `initial = house.entities.with_untracked(...)` snapshot in `screens/entity_sheet.rs`. Outside changes still merge into the form when it is clean (Review Focus 2); draft and error survive echoes. A future refactor that moves the editor back under a reactive closure must re-add the `tests/sheet.rs` regression check.
- Cover "Moving" badge is a `Memo` over `house.entities` (`cards/cover.rs:46-59`). The pattern is the model for any future badge whose state depends on a sibling entity.
- `encode_id` (`api.rs:56-66`) is the shared helper for percent-encoding entity and alert ids in URLs. Any new URL that interpolates an id must use it; grep for `format!("/api/`, `format!("/entity/`, `format!("/alert` to audit later waves.
