# TODOS

Running backlog of ideas and deferred follow-ups. Priority: **P1** = correctness or user-visible safety (do next), **P2** = polish or test coverage, **P3** = cosmetic or opportunistic.

## Dashboards (landed on `feat/dashboards`, 2026-10-22)

### P1 — correctness / forward-compat

- **Rich widget renderers for the 6 deferred kinds.** In v1 these render only the `Unavailable` placeholder (see spec "v1 widget renderer coverage"). Edit `crates/oikos-web/src/dashboards/widget.rs` for each:
  - `RoomGroup` → `cards::group::{CoverGroupCard,LightGroupCard}`
  - `FlowButton` → trigger via `api::run_flow`
  - `EnergyStat` → `cards::sensor::SensorCard` or a new metric card
  - `HistoryChart` → new chart card
  - `Camera` → `cards::camera::*` once it exists
  - `Lock` → `cards::lock::*` once it exists
- **FlowButton validator alignment, spec vs plan.** The hub validator used to require `WidgetConfig::Entity` for FlowButton (plan brief error). Fixed to require `WidgetConfig::Flow` per spec. Verify end-to-end that an in-browser FlowButton save round-trips. (Patched in 6c714cd — keep an eye on it under smoke.)
- **EditState resets on broker echo.** `EditState` is created inside the dashboards-tracking closure in `screens/dashboard.rs`. Any broker echo (including the hub's own echo after Save) rebuilds the state and discards unsaved draft edits. Fix: hoist state keyed by dashboard id outside the closure; only sync `original` from the hub when the draft isn't dirty.
- **Hub reorder is "all-or-nothing at validation", not at MQTT publish.** A mid-loop publish failure leaves broker + local store half-reordered (but mutually consistent). Rare on a healthy local broker; still worth a WHY comment, or publish all N messages before updating any local state so the local store rolls back on the first `Publish` error.
- **Two concurrent PUTs can pick the same `next_free_order`.** Lock is released between the read and the publish. Fine for single-user v1; add a serialization guard if the hub ever grows multi-writer.

### P2 — polish / test coverage

- Dashboards fetched once at startup; no retry/refresh on WS reconnect. Trigger a re-fetch on the WS reconnect event.
- Deep-linking an authored dashboard id shows "Unknown dashboard" briefly before the fetch lands. Add a loading state in `screens/dashboard.rs` or in the picker fallback.
- `.dashboards-picker__menu` uses `position: fixed` with no offsets — likely opens over the trigger. Anchor it under `.dashboards-picker` (`position: absolute`) and add top/left offsets. Outside-click and trigger-click don't close it. The fix needs both the anchoring and an outside-click handler.
- Grid comment claims spans wider than the column count "are clamped there" — no clamp rule exists. Rephrase to say WHY no clamp is needed (max span is 2 = min column count 2).
- `with_untracked` on `house.entities` means an entity arriving after the dashboard mounts leaves its widget stuck on `Unavailable` until remount. Memo on presence would fix it.
- `Grid` keys tiles by widget id alone — an edit that changes a widget's span or config under the same id does not re-render. Key on `(id, span, config)` or similar.
- Save button has no in-flight guard; a double click sends two PUTs. Disable while the request is in flight.
- Toggling pencil off while dirty keeps the unsaved draft on the grid with no indicator.
- `widget_with_wrong_span_rejected` test doesn't assert the full violation text ("span 2x2 not allowed"); tighten it.
- Add a tie-break-by-id test for `DashboardStore::list()` ordering.
- No test that `delete_dashboard` on an unknown id returns `UnknownDashboard`.
- `order_route_shadows_id_route` asserts `is_success()` only — tighten to `== NO_CONTENT`.
- Dashboard delete has no confirmation dialog. Route the `DashboardSettingsSheet` delete button through the `Confirming` context (`components::confirm::Confirming`) before calling the API.
- `DashboardSettingsSheet` delete/reorder swallow API errors with `is_ok()` and no feedback. Toast on failure.
- `WidgetConfigSheet` has no entity/target picker — swap-target requires delete + re-add. Add a picker scoped to the matching kind.
- HistoryChart entry filter in `AddWidgetPicker` uses Sensor entities only; spec says "any entity with history". Refine when the entity-history catalog is wired.
- Energy metric list is hard-coded in `add_picker.rs`; could drift from hub source. Expose a single source of truth.
- Overlay z-index 1 vs tools z-index 2 depends on no inner card element exceeding z-index 1 — audit each `WidgetView` variant.
- Save-success `selected_widget.set(None)` runs post-await — a widget selected mid-flight is cleared. Make the reset conditional on the index actually still matching the pre-flight widget.
- `widget_config_tagged_by_kind_shape` test name doesn't match what it does — rename or add the `to_value` keys assertion the name implies.
- Dashboard-store tests reach into `store.entries` directly instead of using the `invalid_reason()` helper.
- The DashboardScreen comment calls dashboards a "retained-topic fetch" — they're actually a one-shot HTTP fetch at start-up. Rephrase to "start-up fetch".

### P3 — cosmetic / backlog

- `path/body id mismatch` returns 422 while flows PUT returns 400 — intentional divergence per the dashboards brief, but worth normalizing across all PUT routes.
- Three dashboard HTTP handlers have no doc comments (only `put_dashboard` does).
- Widget kinds `Lock` and `Camera` have no matching `Kind` variants in `oikos-core`; their widget groups are empty. Add device-kind inventory when those entity classes land. See the P1 item "Rich widget renderers for the 6 deferred kinds".
- Hub.rs has one unrelated rustfmt-only hunk in the history_feed closure (harmless churn).
- `make_widget` in `AddWidgetPicker` panics natively (`js_sys::Date` is wasm-only); native tests use a fixed id instead. Fine, since those paths are wasm-only in production.
- Normalize commit attribution across the branch — 11 of 24 commits ended up with `Co-Authored-By: Claude Sonnet 5.5` or `Haiku 5.5` instead of the requested `Opus 4.7` line because sub-agents' own session rules overrode the dispatch brief. If the user wants a uniform trailer, a one-pass rebase can rewrite them.

## Other backlog ideas (unrelated to dashboards)

- **Bulk actions on the Flows page.** Enable, disable, delete or move several flows at once from the list (landed with the flow-list redesign on `feat/flow-list-redesign`, 2026-10-22; the list is single-item only).
- **Automatic room detection for flows.** Suggest a flow's `area` from its trigger and action entities instead of leaving it unassigned; today the room is set by hand only.
- **Android app support for a flow's `area`.** The app does not read or write a flow's `area` yet; the hub and web UI do.
