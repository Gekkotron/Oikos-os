# Dashboards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fixed Home tab in the Oikos web UI with a `Dashboards ▾` picker fed by a hub-backed dashboards API — a fixed synthesized **House** entry (today's rooms overview, renamed) plus multiple user-authored, editable, column-grid dashboards built from built-in widget kinds.

**Architecture:** Pure dashboard model in `oikos-core` (crate), wire types in `oikos-api`, retained-MQTT-backed `DashboardStore` in the hub (`oikos-os`) mirroring the existing `FlowStore`, five HTTP routes under `/api/v1/dashboards`, and a Leptos UI in `oikos-web` with a top-nav picker, a column-grid renderer, an edit mode on the flow-canvas pattern (pencil button → drag → side sheet → Add picker → Save / Discard), and `screens::home::HomeScreen` kept verbatim as the House renderer.

**Tech Stack:** Rust (workspace), Axum (hub HTTP), rumqttc-driven MQTT retained topics, Leptos + leptos-router + WASM for the web client, `serde` + `schemars` for wire types, `tokio::sync::Mutex` for in-hub state (same as flows).

**Spec:** `docs/superpowers/specs/2026-10-21-dashboards-design.md`

## Global Constraints

- Rust workspace, 2024 edition; `#![forbid(unsafe_code)]` in every crate.
- The web crate is `wasm32-unknown-unknown` only; no `tokio` threads, no `std::fs`.
- All wire types in `oikos-api` derive `Serialize`, `Deserialize`, `JsonSchema`, `Clone`, `Debug`, `PartialEq`.
- Pure data crates (`oikos-core`, `oikos-api`) have no I/O, no clock, no logging.
- Nothing hard-codes the live test house; dashboards are generic for any house.
- Dashboards are user state and must not live in `oikos.toml`.
- The House dashboard is id `"house"`, name `"House"`, `order = 0`, non-writable, non-deletable, `widgets = []`.
- Allowed widget spans per kind, per spec §Widgets table — the hub validates on `PUT`, the client enforces on resize.
- `history-chart.range` is one of `1h`, `6h`, `24h`, `7d`, `30d`. `energy-stat.metric` is one of the metric ids the existing Energy page already exposes (reused verbatim; no new metrics in v1).
- Commit identity for every commit in this plan: `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`. Every commit ends with `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`.

## Review Focus

The five input classes most likely to bite a reader of this software, pinned to the task that owns their test.

- **Retained-payload decode of an unknown widget kind** (future hub seeing an older dashboard, or an older hub seeing a newer dashboard). Owner: Task 2 (wire types) — adds a decode test with an unknown `kind` variant that falls back to an "unknown" placeholder instead of 400-ing the whole dashboard.
- **Reorder with a stale id list** (client sends an ordered id list that includes a now-deleted dashboard, or omits one). Owner: Task 6 (reorder endpoint) — adds an integration test with a drifted list; the hub rejects with a clear error and does not partially apply.
- **PUT with an `order` colliding with another dashboard's `order`.** Owner: Task 5 (PUT endpoint) — adds a test confirming the hub normalizes `order` to the next free integer and reports the normalized value.
- **Phone collapse** of a wider-than-column widget. Owner: Task 10 (grid renderer) — adds a Leptos UI test at the phone breakpoint that a `2x2` widget renders at full column width without horizontal overflow.
- **Missing entity target in a widget** (an `entity_id` the hub no longer knows). Owner: Task 10 (grid renderer) — adds a test that the client renders a greyed "unavailable" placeholder tile with a delete affordance in edit mode.

---

## File Structure

Pure model and widget enums:
- Create: `crates/oikos-core/src/dashboard/mod.rs` — module root, re-exports.
- Create: `crates/oikos-core/src/dashboard/model.rs` — `Dashboard`, `Widget`, `WidgetKind`, `Span`, `WidgetConfig`, allowed-spans table, basic validation (`is_valid_dashboard_id`).
- Modify: `crates/oikos-core/src/lib.rs` — add `pub mod dashboard;`.

Wire types:
- Create: `crates/oikos-api/src/dashboards.rs` — `DashboardView` (one entry in the list), re-exports of the pure model types for the UI.
- Modify: `crates/oikos-api/src/lib.rs` — register the new module and re-exports.

Hub store + routes:
- Create: `crates/oikos-os/src/dashboards/mod.rs` — module root.
- Create: `crates/oikos-os/src/dashboards/store.rs` — `DashboardStore` (mirrors `FlowStore` — in-memory decoded copy of retained payloads, `apply`, `list`, `get`).
- Create: `crates/oikos-os/src/dashboards/house.rs` — `synthesize_house(areas, entities)` → `Dashboard` with id `"house"`.
- Modify: `crates/oikos-os/src/hub.rs` — add `dashboard_store`, `dashboards()`, `get_dashboard()`, `set_dashboard()`, `delete_dashboard()`, `reorder_dashboards()`, `dashboard_topic()`, replay dashboard topics on startup like flows.
- Modify: `crates/oikos-os/src/mqtt.rs` — subscribe to `<ui_prefix>/dashboards/+` and feed `DashboardStore::apply`.
- Modify: `crates/oikos-os/src/config.rs` — expose the dashboards topic prefix (default: derived from `ui_prefix`, matches `flows_prefix` convention).
- Modify: `crates/oikos-os/src/http/api.rs` — add five routes: `GET /dashboards`, `GET /dashboards/{id}`, `PUT /dashboards/{id}`, `DELETE /dashboards/{id}`, `POST /dashboards/order`.
- Modify: `crates/oikos-os/src/lib.rs` — add `mod dashboards;` and the `pub use dashboards::DashboardStore;` the hub needs.
- Test: `crates/oikos-os/tests/dashboards.rs` — end-to-end hub tests over the HTTP router.

Web (Leptos):
- Create: `crates/oikos-web/src/dashboards/mod.rs` — module root.
- Create: `crates/oikos-web/src/dashboards/picker.rs` — the `Dashboards ▾` picker component.
- Create: `crates/oikos-web/src/dashboards/grid.rs` — the responsive column grid that places widgets by span.
- Create: `crates/oikos-web/src/dashboards/widget.rs` — the widget renderer dispatching on `WidgetKind` to existing card components in `cards/`.
- Create: `crates/oikos-web/src/dashboards/edit.rs` — edit-mode state (dirty/clean, drag state, side-sheet), Save/Discard, dashboard settings sheet (name/delete/reorder).
- Create: `crates/oikos-web/src/dashboards/add_picker.rs` — the `＋ Add widget` picker, grouped by kind.
- Create: `crates/oikos-web/src/dashboards/config_sheet.rs` — per-widget config sheet (entity picker, title, size, delete).
- Create: `crates/oikos-web/src/screens/dashboard.rs` — the dashboard screen (picks the renderer: HomeScreen for House, grid for the rest).
- Modify: `crates/oikos-web/src/app.rs` — nav: remove the `/` Home link + `/` route; add `/dashboards/:id` route; redirect `/` → first dashboard; mount the Dashboards picker in the top bar.
- Modify: `crates/oikos-web/src/house.rs` — add a `dashboards: RwSignal<Vec<DashboardView>>` field and a fetcher for `GET /api/v1/dashboards`.
- Modify: `crates/oikos-web/src/api.rs` — add typed clients for the five dashboard endpoints.
- Modify: `crates/oikos-web/src/lib.rs` — register `pub mod dashboards;` and `pub mod screens::dashboard;`.

---

## Task 1: Core model — `Dashboard`, `Widget`, `Span`, `WidgetKind`, `WidgetConfig`

**Files:**
- Create: `crates/oikos-core/src/dashboard/mod.rs`
- Create: `crates/oikos-core/src/dashboard/model.rs`
- Modify: `crates/oikos-core/src/lib.rs`
- Test: in `crates/oikos-core/src/dashboard/model.rs` under `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing (new leaf module).
- Produces:
  - `pub struct Dashboard { pub id: String, pub name: String, pub order: u32, pub widgets: Vec<Widget> }`
  - `pub struct Widget { pub id: String, pub kind: WidgetKind, pub span: Span, pub config: WidgetConfig }`
  - `pub enum WidgetKind { Light, Cover, Climate, Lock, Sensor, RoomGroup, FlowButton, EnergyStat, HistoryChart, Camera, WaterHeater }`
  - `pub enum Span { X1x1, X2x1, X2x2, X1x2 }` with `pub fn as_str(&self) -> &'static str`
  - `pub enum WidgetConfig { Entity { entity_id: String, title: Option<String> }, Area { area_id: String, title: Option<String> }, Flow { flow_id: String, title: Option<String> }, Energy { metric: String, title: Option<String> }, History { entity_id: String, range: HistoryRange } }` — reuse `oikos_api::HistoryRange` by moving it to `oikos-core` if needed, else duplicate the enum in `dashboard::model`.
  - `pub fn allowed_spans(kind: WidgetKind) -> &'static [Span]` — the spec §Widgets table.
  - `pub fn is_valid_dashboard_id(id: &str) -> bool` — alnum + `_`/`-`, 1..=64 chars, not `house`-reserved.
  - `pub const HOUSE_ID: &str = "house"; pub const HOUSE_NAME: &str = "House";`

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn span_round_trips_as_str() { /* X1x1 ↔ "1x1" */ }
#[test] fn allowed_spans_match_spec() {
    assert_eq!(allowed_spans(WidgetKind::Light), &[Span::X1x1, Span::X2x1]);
    assert_eq!(allowed_spans(WidgetKind::RoomGroup), &[Span::X1x1, Span::X2x1, Span::X2x2]);
    assert_eq!(allowed_spans(WidgetKind::HistoryChart), &[Span::X2x1, Span::X2x2, Span::X1x2]);
    assert_eq!(allowed_spans(WidgetKind::Camera), &[Span::X2x1, Span::X2x2, Span::X1x2]);
}
#[test] fn house_id_reserved() {
    assert!(!is_valid_dashboard_id("house"));
    assert!(is_valid_dashboard_id("morning_glance"));
}
#[test] fn dashboard_serde_round_trip() { /* serde_json round trip over a two-widget Dashboard */ }
#[test] fn widget_config_tagged_by_kind_shape() { /* decode of `{ "kind": "light", "config": { "entity_id": "lamp" } }` */ }
```

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p oikos-core dashboard --`
Expected: FAIL (symbols undefined).

- [ ] **Step 3: Implement `dashboard::model`** with `#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]` on every type, `#[serde(rename_all = "snake_case")]` on enums, and `#[serde(tag = "kind", content = "config", rename_all = "snake_case")]` on an inner `WidgetBody` so the on-wire shape matches `{ "kind": "...", "config": {...} }` while the `Widget` struct carries `kind + config` as separate fields for the hub to validate kind↔config consistency on `PUT`.

- [ ] **Step 4: Run tests, confirm they pass**

Run: `cargo test -p oikos-core dashboard --`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(core): dashboard model (Dashboard, Widget, Span, WidgetKind)`.

## Task 2: Wire types in `oikos-api`

**Files:**
- Create: `crates/oikos-api/src/dashboards.rs`
- Modify: `crates/oikos-api/src/lib.rs`

**Interfaces:**
- Consumes: `oikos_core::dashboard::{Dashboard, Widget, WidgetKind, Span, WidgetConfig, HOUSE_ID, HOUSE_NAME, allowed_spans, is_valid_dashboard_id}`.
- Produces:
  - `pub struct DashboardView { pub dashboard: Dashboard, pub synthesized: bool }` — one entry in the list response; `synthesized: true` only for House.
  - `pub struct DashboardReorder { pub order: Vec<String> }` — body of `POST /dashboards/order`.
  - Re-exports: `Dashboard, Widget, WidgetKind, Span, WidgetConfig, HOUSE_ID, HOUSE_NAME, allowed_spans`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn dashboard_view_round_trips() { /* { "dashboard": {...}, "synthesized": false } */ }
#[test] fn reorder_body_round_trips() { /* { "order": ["a", "b", "c"] } */ }
#[test] fn unknown_widget_kind_decodes_as_placeholder() {
    // The REVIEW FOCUS test: a dashboard with an unknown kind decodes
    // without erroring the whole dashboard — the unknown widget becomes
    // WidgetKind::Unknown (or is dropped with a logged notice) depending on policy.
    // v1 policy: drop with a `#[serde(other)]` catch-all on WidgetKind and
    // skip serializing the dropped widget on re-encode.
}
```

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p oikos-api dashboards --`
Expected: FAIL.

- [ ] **Step 3: Implement `dashboards.rs`** and wire the module in `lib.rs`. Add `#[serde(other)]` on `WidgetKind` for the unknown-decode path; filter unknowns in `Dashboard::decode`.

- [ ] **Step 4: Run tests, confirm they pass**

Run: `cargo test -p oikos-api dashboards --`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(api): dashboard wire types + unknown-widget-kind decode`.

## Task 3: `DashboardStore` in the hub

**Files:**
- Create: `crates/oikos-os/src/dashboards/mod.rs`
- Create: `crates/oikos-os/src/dashboards/store.rs`
- Modify: `crates/oikos-os/src/lib.rs` — add `mod dashboards;` and `pub(crate) use dashboards::DashboardStore;`.
- Test: `#[cfg(test)] mod tests` inside `store.rs`.

**Interfaces:**
- Consumes: `oikos_api::{Dashboard, DashboardView}`, `oikos_core::dashboard::{HOUSE_ID, HOUSE_NAME, allowed_spans}`.
- Produces:
  - `pub struct DashboardStore { … }`
  - `pub fn new() -> Self`
  - `pub fn apply(&mut self, id: &str, payload: &[u8]) -> StoreApplied` — mirror `FlowStore::apply`: empty payload = `Removed`, byte-identical = `Unchanged`, else decode + validate.
  - `pub fn list(&self) -> Vec<Dashboard>` — authored only, sorted by `order` ascending.
  - `pub fn get(&self, id: &str) -> Option<&Dashboard>`
  - `pub fn next_free_order(&self) -> u32` — `max(order) + 1`, starting at 1.
  - `pub fn validate_widget(w: &Widget) -> Result<(), String>` — kind↔config shape and span-in-allowed-set.
- Reuses the `StoreApplied` enum shape from `FlowStore` (or defines a local one with the same four variants).

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn apply_empty_payload_removes() { /* apply "{}" then apply b"" → Removed */ }
#[test] fn apply_identical_is_unchanged() { /* same bytes twice → Changed, then Unchanged */ }
#[test] fn invalid_payload_is_rejected_with_reason() { /* bad JSON → Changed with Invalid entry carrying the reason */ }
#[test] fn widget_with_wrong_span_rejected() {
    // Light with 2x2 → validate_widget returns Err
}
#[test] fn widget_config_shape_mismatch_rejected() {
    // Light with Area config → validate_widget returns Err
}
#[test] fn list_sorted_by_order() { /* three dashboards in random insert order → sorted */ }
#[test] fn next_free_order_starts_at_one_and_fills_above_max() { }
#[test] fn house_id_rejected_on_apply() { /* apply("house", …) → Changed with Invalid */ }
```

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p oikos-os dashboards --`
Expected: FAIL.

- [ ] **Step 3: Implement `DashboardStore`** following the `FlowStore` shape (BTreeMap-of-decoded-entries + BTreeMap-of-last-payloads). `validate_widget` consults `allowed_spans(kind)` and the `WidgetConfig` variant.

- [ ] **Step 4: Run tests, confirm they pass**

Run: `cargo test -p oikos-os dashboards --`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(hub): DashboardStore (retained-topic mirror + validation)`.

## Task 4: House synthesis

**Files:**
- Create: `crates/oikos-os/src/dashboards/house.rs`
- Modify: `crates/oikos-os/src/dashboards/mod.rs` — re-export.
- Test: `#[cfg(test)] mod tests` in `house.rs`.

**Interfaces:**
- Consumes: `oikos_api::{AreaView, EntityView, Dashboard}`, `oikos_core::dashboard::{HOUSE_ID, HOUSE_NAME}`.
- Produces: `pub fn synthesize_house(_areas: &[AreaView], _entities: &[EntityView]) -> Dashboard` — returns `Dashboard { id: HOUSE_ID.to_owned(), name: HOUSE_NAME.to_owned(), order: 0, widgets: vec![] }`.

Rationale: per spec, House carries `widgets: []`; the client dispatches on `id == "house"` and renders `screens::home::HomeScreen` directly from the entity/area state it already has. The hub does not compute per-widget views for House.

- [ ] **Step 1: Write failing test**

```rust
#[test] fn synthesize_house_returns_fixed_shape() {
    let d = synthesize_house(&[], &[]);
    assert_eq!(d.id, "house");
    assert_eq!(d.name, "House");
    assert_eq!(d.order, 0);
    assert!(d.widgets.is_empty());
}
```

- [ ] **Step 2: Run test, confirm it fails**

Run: `cargo test -p oikos-os house --`
Expected: FAIL.

- [ ] **Step 3: Implement `synthesize_house`**.

- [ ] **Step 4: Run test, confirm it passes.**

- [ ] **Step 5: Commit** `feat(hub): synthesize_house dashboard (empty widgets, id "house")`.

## Task 5: Hub wiring — topic replay, API, in-hub mutators

**Files:**
- Modify: `crates/oikos-os/src/hub.rs`
- Modify: `crates/oikos-os/src/mqtt.rs`
- Modify: `crates/oikos-os/src/config.rs`
- Test: `#[cfg(test)] mod tests` inside `hub.rs` for the pure synchronous helpers.

**Interfaces:**
- Consumes: `DashboardStore`, `synthesize_house`, `oikos_api::DashboardView`.
- Produces (on `Hub`):
  - `pub(crate) fn dashboard_store(&self) -> MutexGuard<'_, DashboardStore>`
  - `pub fn dashboards(&self) -> Vec<DashboardView>` — [House synthesized at index 0] + authored sorted by `order`, each wrapped as `DashboardView`.
  - `pub fn dashboard(&self, id: &str) -> Option<DashboardView>` — including House for `id == "house"`.
  - `pub async fn set_dashboard(&self, mut body: Dashboard) -> Result<Dashboard, HubError>` — rejects id `"house"` with `HubError::Rejected`; rejects invalid widgets; normalizes `order` via `next_free_order` on insert, or keeps requested `order` on update if unique; publishes `<ui_prefix>/dashboards/<id>` retained; returns the normalized dashboard.
  - `pub async fn delete_dashboard(&self, id: &str) -> Result<(), HubError>` — rejects `"house"`; publishes empty retained payload.
  - `pub async fn reorder_dashboards(&self, ids: &[String]) -> Result<(), HubError>` — validates every id exists and no extras, assigns 1-based contiguous `order` by position, republishes each affected dashboard retained in a single batch.
  - `pub fn dashboard_topic(&self, id: &str) -> String` — `format!("{}/dashboards/{}", self.ui_prefix, id)`.
- `mqtt.rs`: subscription list adds `<ui_prefix>/dashboards/+`; message dispatch for topics matching that prefix calls `hub.dashboard_store().apply(id, payload)`.
- Extend `HubError` with a `UnknownDashboard(String)` variant mapped to 404.

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test(flavor = "current_thread")]
async fn set_dashboard_rejects_house_id() { /* HubError::Rejected */ }
#[tokio::test(flavor = "current_thread")]
async fn set_dashboard_normalizes_colliding_order() {
    // Pre-populate two dashboards with order 1 and 2.
    // PUT a new one requesting order = 1.
    // Expect: hub returns the new dashboard with its order updated to a free integer (next_free_order).
    // REVIEW FOCUS: PUT with a colliding `order`.
}
#[tokio::test(flavor = "current_thread")]
async fn delete_dashboard_rejects_house() { }
#[tokio::test(flavor = "current_thread")]
async fn reorder_rejects_drifted_list() {
    // Store has ids [a, b, c]. Reorder with [a, b, d] → Err; nothing republished.
    // REVIEW FOCUS: reorder with stale id list.
}
#[test]
fn dashboards_returns_house_at_index_zero() {
    // Hub with no authored dashboards and some areas+entities → list = [House].
}
```

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p oikos-os hub --`
Expected: FAIL.

- [ ] **Step 3: Implement the hub methods**, the subscription in `mqtt.rs`, and the `UnknownDashboard` variant. Reorder is **all-or-nothing**: validate first, build the new assignments, then publish each in one pass.

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(hub): dashboard mutators + retained-topic subscription`.

## Task 6: HTTP routes

**Files:**
- Modify: `crates/oikos-os/src/http/api.rs`
- Create: `crates/oikos-os/tests/dashboards.rs` — integration tests over the router.

**Interfaces:**
- Consumes: `Hub::{dashboards, dashboard, set_dashboard, delete_dashboard, reorder_dashboards}`, `oikos_api::{DashboardView, DashboardReorder}`.
- Produces (route handlers, all `async`):
  - `fn list_dashboards(State<Arc<Hub>>) -> Json<Vec<DashboardView>>`
  - `fn get_dashboard(State, Path<String>) -> Result<Json<DashboardView>, HttpError>` — 404 for unknown id.
  - `fn put_dashboard(State, Path<String>, Body<Dashboard>) -> Result<Json<Dashboard>, HttpError>` — 403 for id `"house"`, 422 for mismatched id in path vs body, 422 for invalid widgets.
  - `fn delete_dashboard(State, Path<String>) -> Result<StatusCode, HttpError>` — 403 for `"house"`, 204 otherwise.
  - `fn reorder_dashboards(State, Body<DashboardReorder>) -> Result<StatusCode, HttpError>` — 422 for drifted list.
- Route additions to `routes()`:
  ```
  .route("/dashboards", get(list_dashboards))
  .route("/dashboards/order", post(reorder_dashboards))
  .route("/dashboards/{id}", get(get_dashboard).put(put_dashboard).delete(delete_dashboard))
  ```
  (Register `/dashboards/order` **before** `/dashboards/{id}` or Axum will route `"order"` as an id.)

- [ ] **Step 1: Write failing integration tests**

In `tests/dashboards.rs`, spin up a hub-under-test (reuse the harness other tests use — see `tests/flows.rs` and `tests/http.rs`) and assert:

```rust
#[test] fn get_dashboards_returns_house_only_initially() { /* [{"dashboard": {"id":"house",...}, "synthesized": true}] */ }
#[test] fn put_dashboard_creates() { /* PUT /dashboards/morning → 200 with normalized body; GET then returns it */ }
#[test] fn put_dashboard_rejects_house() { /* 403 */ }
#[test] fn put_dashboard_rejects_bad_span() { /* light with 2x2 → 422 */ }
#[test] fn put_dashboard_normalizes_colliding_order() { /* REVIEW FOCUS */ }
#[test] fn delete_dashboard_removes() { /* 204; GET → 404 */ }
#[test] fn delete_house_forbidden() { /* 403 */ }
#[test] fn reorder_drifted_list_rejected() { /* REVIEW FOCUS */ }
#[test] fn reorder_happy_path() { /* three authored; POST new order; GET /dashboards returns them in the new order */ }
#[test] fn order_route_shadows_id_route() { /* POST /dashboards/order with a body → 200; not 404 "order" */ }
```

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p oikos-os --test dashboards`
Expected: FAIL.

- [ ] **Step 3: Implement handlers** and the three route lines in `routes()`. Reuse `HttpError::from(HubError)` for mapping.

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(hub): /api/v1/dashboards routes`.

## Task 7: Web API client

**Files:**
- Modify: `crates/oikos-web/src/api.rs`

**Interfaces:**
- Consumes: `oikos_api::{DashboardView, Dashboard, DashboardReorder}`.
- Produces:
  - `pub async fn fetch_dashboards() -> Result<Vec<DashboardView>, ApiError>`
  - `pub async fn fetch_dashboard(id: &str) -> Result<DashboardView, ApiError>`
  - `pub async fn put_dashboard(d: &Dashboard) -> Result<Dashboard, ApiError>`
  - `pub async fn delete_dashboard(id: &str) -> Result<(), ApiError>`
  - `pub async fn reorder_dashboards(order: &[String]) -> Result<(), ApiError>`

- [ ] **Step 1: Add signatures as `unimplemented!()` stubs so Task 9+ can compile against them**, with a compile-only guard (no runtime tests at this layer — the hub test suite covers the wire shape).

- [ ] **Step 2: Implement** the five functions using the existing `fetch` wrapper this file already uses for `fetch_flows`.

- [ ] **Step 3: Build to confirm compile** — `cargo build -p oikos-web --target wasm32-unknown-unknown`.

- [ ] **Step 4: Commit** `feat(web): api client for /dashboards`.

## Task 8: `house.rs` client state — add `dashboards`

**Files:**
- Modify: `crates/oikos-web/src/house.rs`

**Interfaces:**
- Consumes: `fetch_dashboards`, `DashboardView`.
- Produces:
  - `pub dashboards: RwSignal<Vec<DashboardView>>` on `House`.
  - On construction, kick off the fetch and populate the signal.
  - `pub fn dashboard(&self, id: &str) -> Option<DashboardView>` — read helper.

- [ ] **Step 1: Add the field and the initial fetch** following the pattern used for existing fetched state (snapshot, flows). No test; this is a thin signal plus a fetch call.

- [ ] **Step 2: Build to confirm compile**.

- [ ] **Step 3: Commit** `feat(web): fetch dashboards into House state`.

## Task 9: `screens::dashboard::DashboardScreen` with House special-case

**Files:**
- Create: `crates/oikos-web/src/screens/dashboard.rs`
- Modify: `crates/oikos-web/src/screens/mod.rs`
- Modify: `crates/oikos-web/src/lib.rs`

**Interfaces:**
- Consumes: `house.dashboards`, `HomeScreen`, `oikos_api::{DashboardView, HOUSE_ID}`.
- Produces: `#[component] pub fn DashboardScreen() -> impl IntoView` — reads the `:id` path param, looks up the matching `DashboardView` on `house.dashboards`, and renders:
  - If `id == HOUSE_ID` → `<HomeScreen />` (the existing rooms overview, used verbatim).
  - Otherwise → the grid component from Task 10 for the dashboard's widgets.
  - If no match → a short "Unknown dashboard" view with a link back to House.

- [ ] **Step 1: Write a Leptos UI test**

Use the testing pattern other screens already use (see `screens/home.rs` tests if present; otherwise a leptos test that mounts the component with provided context):

```rust
#[test] fn dashboard_screen_renders_homescreen_for_house_id() { /* id = "house" → renders rooms overview marker */ }
#[test] fn dashboard_screen_renders_grid_for_authored_id() { /* id = "morning" → renders the grid */ }
#[test] fn dashboard_screen_shows_unknown_for_missing_id() { }
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement `DashboardScreen`.**

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): DashboardScreen (House → HomeScreen, else grid)`.

## Task 10: Grid renderer + widget dispatch + missing-target placeholder

**Files:**
- Create: `crates/oikos-web/src/dashboards/mod.rs`
- Create: `crates/oikos-web/src/dashboards/grid.rs`
- Create: `crates/oikos-web/src/dashboards/widget.rs`
- Modify: `crates/oikos-web/src/lib.rs`
- Modify: `crates/oikos-web/src/screens/dashboard.rs` — use the grid.

**Interfaces:**
- Consumes: `House` (for entity/area/flow lookups), the per-kind card components in `crates/oikos-web/src/cards/*.rs`.
- Produces:
  - `#[component] pub fn Grid(cx, widgets: Signal<Vec<Widget>>) -> impl IntoView` — CSS grid with:
    - `grid-template-columns: repeat(2, 1fr)` at `≤ 600px`, `repeat(4, 1fr)` at `≤ 1200px`, `repeat(6, 1fr)` above.
    - Each widget takes `grid-column: span N` based on `Span` (phone collapses span >1 to full 2 cols; `1x2` stays 1 col, 2 rows).
  - `#[component] pub fn WidgetView(cx, widget: Widget) -> impl IntoView` — switch on `WidgetKind`, resolve the target from `House`, dispatch to the existing card.
  - `#[component] pub fn Unavailable(cx, label: String, delete_in_edit: Callback<()>) -> impl IntoView` — the greyed placeholder.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn phone_breakpoint_collapses_hero_to_full_width() {
    // REVIEW FOCUS: a 2x2 widget at ≤ 600px fits within the two-column grid without horizontal overflow.
}
#[test] fn widget_dispatches_to_cards_light_for_light_kind() { }
#[test] fn widget_with_missing_entity_renders_unavailable() {
    // REVIEW FOCUS: an entity_id the House state does not know.
}
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement the grid, `WidgetView` dispatch, and `Unavailable`.** Breakpoints use CSS; the Leptos component attaches a stylesheet or a `<style scoped>` block.

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): dashboard grid + widget dispatch + missing-target placeholder`.

## Task 11: Picker + nav rewire + `/` redirect

**Files:**
- Create: `crates/oikos-web/src/dashboards/picker.rs`
- Modify: `crates/oikos-web/src/app.rs`

**Interfaces:**
- Consumes: `house.dashboards`.
- Produces:
  - `#[component] pub fn DashboardsPicker() -> impl IntoView` — the top-nav dropdown showing dashboards in order with the current one marked, ending with `＋ New dashboard` that creates an empty dashboard with name "New dashboard" at `max(order) + 1` via Task 7's `put_dashboard`, then navigates to it in edit mode.
- `app.rs` changes:
  - Replace `<A href="/" exact=true>"Home"</A>` with `<DashboardsPicker />`.
  - Replace `<Route path=path!("/") view=HomeScreen />` with a redirect component that reads `house.dashboards` and navigates to `/dashboards/<first_id>` (`house` by default).
  - Add `<Route path=path!("/dashboards/:id") view=DashboardScreen />`.
  - Keep the fallback behavior (`fallback=HomeScreen` → change to `fallback=HouseRedirect`).

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn root_redirects_to_first_dashboard() { }
#[test] fn picker_lists_dashboards_in_order_with_house_first() { }
#[test] fn new_dashboard_button_creates_and_navigates_to_edit() { }
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement picker, redirect, and route changes.**

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): Dashboards picker + replace Home tab + / redirect`.

## Task 12: Edit mode — drag, resize, save/discard

**Files:**
- Create: `crates/oikos-web/src/dashboards/edit.rs`
- Modify: `crates/oikos-web/src/dashboards/grid.rs` — show drag handles and resize cycles when `edit.active`.
- Modify: `crates/oikos-web/src/screens/dashboard.rs` — mount the pencil button + Save/Discard bar when the dashboard is authored.

**Interfaces:**
- Consumes: `allowed_spans`, `put_dashboard`.
- Produces:
  - `pub struct EditState { pub active: RwSignal<bool>, pub draft: RwSignal<Dashboard>, pub original: Dashboard, pub dirty: Memo<bool> }`
  - `#[component] pub fn EditToolbar(edit: EditState) -> impl IntoView` — pencil toggle, Save, Discard, gear (settings sheet).
  - Drag reorder updates `edit.draft.widgets`'s order; resize cycles the widget's `span` through `allowed_spans(kind)`.
- House never enters edit mode: `DashboardScreen` doesn't render `EditToolbar` for id `"house"`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn pencil_absent_for_house() { }
#[test] fn save_round_trips_through_put_dashboard() { /* mock the api; after save, original = draft */ }
#[test] fn discard_resets_draft_to_original() { }
#[test] fn resize_cycles_only_allowed_spans_for_kind() { /* light cycles 1x1 ↔ 2x1, never 2x2 */ }
#[test] fn dirty_memo_true_after_reorder() { }
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement `EditState`, `EditToolbar`, drag reorder (HTML5 drag or pointer events — builder picks; cycle resize uses a corner button).**

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): dashboard edit mode (drag, cycle resize, save/discard)`.

## Task 13: `＋ Add widget` picker

**Files:**
- Create: `crates/oikos-web/src/dashboards/add_picker.rs`
- Modify: `crates/oikos-web/src/dashboards/edit.rs` — mount the Add picker.

**Interfaces:**
- Consumes: `House` (entities, areas, flows), `allowed_spans`.
- Produces: `#[component] pub fn AddWidgetPicker(on_add: Callback<Widget>) -> impl IntoView` — grouped by kind (Lights, Covers, Climates, Locks, Sensors, Room groups, Flow buttons, Energy, History, Cameras, Water heater). Picking an entry constructs the right `WidgetConfig` with a fresh widget id and the smallest allowed span, then calls `on_add`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn picker_groups_by_kind() { }
#[test] fn adding_a_light_builds_entity_config_with_smallest_allowed_span() { /* 1x1 */ }
#[test] fn adding_a_history_chart_defaults_to_2x1_and_24h_range() { }
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement the picker.**

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): dashboard add-widget picker`.

## Task 14: Per-widget config sheet + dashboard settings sheet

**Files:**
- Create: `crates/oikos-web/src/dashboards/config_sheet.rs`
- Modify: `crates/oikos-web/src/dashboards/edit.rs` — open config sheet on widget click; open dashboard settings on gear click.

**Interfaces:**
- Consumes: `delete_dashboard`, `put_dashboard`, `reorder_dashboards`, `allowed_spans`.
- Produces:
  - `#[component] pub fn WidgetConfigSheet(w: RwSignal<Widget>, on_delete: Callback<()>) -> impl IntoView` — fields appropriate to the kind (entity picker for entity widgets, area picker for `room-group`, flow picker for `flow-button`, metric picker for `energy-stat`, range picker for `history-chart`), title override, span, delete.
  - `#[component] pub fn DashboardSettingsSheet(d: RwSignal<Dashboard>, on_delete: Callback<()>, on_move_up: Callback<()>, on_move_down: Callback<()>) -> impl IntoView`.

- [ ] **Step 1: Write failing tests**

```rust
#[test] fn config_sheet_delete_removes_widget_from_draft() { }
#[test] fn settings_sheet_delete_calls_delete_dashboard() { }
#[test] fn move_up_calls_reorder_with_adjacent_swap() { }
```

- [ ] **Step 2: Run tests, confirm they fail.**

- [ ] **Step 3: Implement the two sheets.**

- [ ] **Step 4: Run tests, confirm they pass.**

- [ ] **Step 5: Commit** `feat(web): widget + dashboard settings sheets`.

## Task 15: End-to-end wiring and workspace build

**Files:**
- No new files; this task re-runs the whole workspace build and tests.

- [ ] **Step 1: Run the full workspace build**

Run: `cargo build --workspace` and `cargo build -p oikos-web --target wasm32-unknown-unknown`.
Expected: both succeed.

- [ ] **Step 2: Run the full test suite**

Run: `cargo test --workspace`.
Expected: PASS.

- [ ] **Step 3: Manual smoke in a browser** — start the hub, open the web UI, confirm:
  - `/` redirects to House (the rooms overview, unchanged visually).
  - `＋ New dashboard` creates one and enters edit mode.
  - Adding a light widget shows a working light card.
  - Resize cycles through allowed spans only; Save persists; reload keeps the dashboard.
  - Delete removes it; the picker falls back to House.

- [ ] **Step 4: Commit** `chore(dashboards): end-to-end smoke pass`.

---

## Self-Review

**Spec coverage.**
- Multiple named dashboards → Tasks 1, 5, 6, 7, 11.
- House fixed, synthesized, not editable → Tasks 4, 5, 6 (`PUT`/`DELETE` 403), 9 (special-case), 11 (picker), 12 (no edit toolbar).
- Column grid phone↔desktop → Task 10.
- Widget kinds and configs for already-rendered entities → Tasks 1, 2, 10, 13, 14.
- Edit mode (pencil, drag, side sheet, Add picker, Save/Discard) → Tasks 12, 13, 14.
- Dashboard management (name/delete/reorder, `＋ New dashboard`) → Tasks 11, 14, plus `POST /dashboards/order` in Task 6.
- Persistence as MQTT retained topics → Tasks 3 (store), 5 (hub mutators publish retained).
- `GET/PUT/DELETE /api/v1/dashboards` + `POST /dashboards/order` → Task 6.
- Allowed spans per kind + enumerations for `history-chart.range` and `energy-stat.metric` → Tasks 1 (span table), 13/14 (UI defaults), spec enumerations consumed by Task 2 wire types.
- Lenient missing-target handling → Task 10 (`Unavailable` placeholder).
- Realtime: widget state over existing entity WS, dashboard structure not pushed → implicit from Tasks 8 (fetch on init) and 10 (widgets read `House` signals already fed by the WS).
- Order field: House fixed at 0, authored ≥ 1 → Task 1 (reserved id), Task 3 (`next_free_order`), Task 5 (reorder algorithm).

**Step scan.** Every step is one checkable action (write test / run test / implement a named symbol / commit). No "handle edge cases" or TBD lines.

**Type consistency.** `Dashboard`, `Widget`, `WidgetKind`, `Span`, `WidgetConfig`, `DashboardView`, `DashboardReorder`, `HOUSE_ID`, `HOUSE_NAME`, `allowed_spans`, `is_valid_dashboard_id` are each defined in exactly one task and consumed by name in the later tasks' Interfaces blocks.

**Review Focus.** Five input classes with owning task + concrete test — unknown widget kind (Task 2), stale reorder list (Task 5/6), colliding `order` (Task 5), phone-collapse of a wider widget (Task 10), missing entity target (Task 10). All present as named tests in those tasks' Step 1 code blocks.

**Proportion.** The spec is ~250 lines; this plan is ~550 lines, roughly 2.2× — the ratio sits inside acceptable range for a 15-task, cross-crate feature.
