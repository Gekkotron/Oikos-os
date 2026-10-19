# Flows — deferred findings after the final fix wave

Date: 2026-10-19 · Source: the per-task reviews, the whole-branch review and the fix-wave re-review of f6432ed..029b90d. Everything Critical/Important is fixed; these are the Minor items the final reviewer triaged as 'can wait', plus one parked residual.

## Parked residual (user's call)

- **Lag recovery.** After a `Lagged` broadcast (more than 1024 events queued) the engine clears its known-value map; with no retained replay to re-seed it, the next real change of a quiet entity is treated as a first sighting and swallowed once. Better fix: on lag, re-seed every watched entity from the registry (which is authoritative) instead of clearing. One-line change plus a test.

## Deferred minors, by task

- **Task 1:** error types lack the schemars cfg_attr (not serialisable, defensible); `if` with `"value": null` fails decode untested; nulls inside `command`/edges not stripped; repeated/out-of-order duration units accepted; `Op`/`Position` lack doc comments.
- **Task 2:** self-loop and unreachable-cycle untested; no longest-path (diamond) layering test; cycle colours as u8 magic numbers; `layers_terminate_on_a_cycle` only asserts non-empty.
- **Task 3:** FlowRunChanged not in every_list_event_serializes (covered in flows::tests); FlowView field names unpinned by tests; some fields lack doc comments.
- **Task 4:** public-API missing-run test is non-discriminating (only the in-module ghost test guards the end_run ordering); a flow going valid→invalid keeps its `active` slot until end_run (engine must cancel the run on Reload); deque may hold runs_kept+1 until the next push.
- **Task 5:** delete test does not check FlowsChanged/Reload; run_flow test's Reload assertion is a leftover; no test that set_flow_enabled works with a vanished entity; offline test does not assert empty publisher; stray `let _ = LED_STATE_TOPIC`; set_flow_enabled read-then-write race; run_flow clones the whole FlowView under the lock; engine channel unbounded (Task 6 must consume; coalesce Reload); redundant topic.clone() in delete_flow.
- **Final:** request flood can starve events until lag; manual run during active run ends Cancelled without detail; paused-time tests lack a pre-advance "no OFF" assertion and a 4m59s negative; no-OFF checks rely on yield counts; no positive controls for the never-fires tests; shutdown aborts runs without end_run(Cancelled) and the engine keeps running during the 5 s grace; tests/flows.rs ~730 lines (split engine tests out).
- **Task 6:** delete-during-delay test does not discriminate the record_step guard (needs multi-thread or hook); panic path untested; seed-only-while-Online means a flow saved offline ignores the first change after reconnect (silence over a false fire); Lagged clears known and already-watched entities are not re-seeded until their next event.
- **Task 7:** non-string body id silently replaced by the path id; untested cases (invalid 422 body text, malformed body, enabled/DELETE on unknown id, enabled offline, run while active); setup PUTs ignore status; README error-summary sentence not updated for the flow codes; PUT re-serialises the Value before decode.
- **Task 8:** delete_flow idempotent_hint arguable; no multi-flow ordering test; ⚠️ Command brightness example unverified by test (Command is the API type, documented in send_command).
- **Task 8:** redaction test covers one node only; doc-test needles are weak substrings; three comment lines over 100 columns; delete_flow idempotent_hint; no multi-flow ordering test.
- **Task 9:** NODE_BOLT duplicates CHANNEL_ENERGY; `-0.0` in wire_path; late Running event re-activates a Done run; untested `>` comparison and string if-value in node_detail.
- **Task 10:** For key is the serialised FlowView (comment it as deliberate); toggle not optimistic; aria-checked evaluated once; title-only reason not reachable on touch (also shown inline); toggle click and Done-run row untested.
- **Task 11:** no prefers-reduced-motion guard on the pending pulse; lenient countdown assertion and weak `disabled` check; faint wires in the warm theme; ⚠️ 390 px scroll depends on `.screen` min-width (verify in Task 12 screenshot); ⚠️ route param decoding mirrors entity_sheet (slug ids, low risk).

## From the final whole-branch review (Minor)

- `house_state.rs` hard-codes `RUN_HISTORY = 20` while the hub's `runs_kept` is configurable.
- The MCP `set_flow` path deserialises with plain serde, not `Flow::decode`, so `"enabled": null` is an error over MCP but fine on the broker (error is in-band).
- An invalid flow's stub shows `name = id` instead of the name in the stored payload.
- A flapping sensor (false then true within a second) leaves a `Cancelled` run in the history; correct, but a 'superseded' label would read better.
- The 10-node living-room flow scrolls horizontally at 1280 px; a tighter column gap or a zoom-to-fit would keep it on one screen.
- Test name `lagged_then_identical_report_reseeds` tests first-sighting seeding, not lag; the stale-epoch drop in the engine has no test.

## Declined by the final reviewer (outside the spec)

- Self-triggering loops (a flow triggering on the light it commands) are author error; the API allows the same loop.
- Concurrent writers racing `set_flow_enabled` against `set_flow` on a single-household LAN hub.
- Run history across restarts, canvas editing: non-goals of this phase.
