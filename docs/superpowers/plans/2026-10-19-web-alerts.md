# Web UI push alerts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The web UI shows the push alerts the Android app shows (camera events first), read by the hub from OikosPush's retained history topic.

**Architecture:** `oikos-api` gains the `AlertView` wire type and the lenient parser of the history array. The hub intercepts the history topic before the registry, keeps the parsed list, broadcasts `AlertsChanged`, and exposes `GET /api/v1/alerts` and an action route that publishes the app's reply payload. The web adds an `alerts` signal and three screens (Home section, `/alerts`, `/alerts/:id`) built from pure, unit-tested helpers.

**Tech Stack:** Rust 2024, axum 0.8, rumqttc, serde/schemars, Leptos 0.8 CSR + leptos_router, gloo-timers, js-sys (new) for locale dates, hand-written CSS.

**Spec:** `docs/superpowers/specs/2026-10-19-web-alerts-design.md`

## Global Constraints

- Rust only; no Node. Trunk builds the web crate; `oikos-api` and `oikos-web` must pass `cargo check --target wasm32-unknown-unknown`.
- Config defaults: `[push] history_topic = "oikos-push/history"`, `responses_topic = "oikos-push/responses"`; env overrides `OIKOS_PUSH__HISTORY_TOPIC`, `OIKOS_PUSH__RESPONSES_TOPIC`.
- Reply payload, exact keys and order: `{"id","action","install_id":"oikos-web","ts"}` with `ts` in unix seconds, QoS 1, not retained.
- The hub never publishes on the history topic. No polling; no file or database.
- Channels: `alarm, security, camera, energy, climate, system, misc` (unknown → `misc`); severities `info, warning, critical` (unknown → `info`); at most 3 actions kept.
- Copy fixed by the spec: "Recent alerts", "See all", "No alerts yet", "No alerts in this channel", "Alerts", "Today", "Yesterday", "This alert is no longer available", "Media no longer available", "Open device", "Resolved", "Sent", "Couldn't send <label>", "Just now".
- Home section shows at most 5 alerts from `alarm`, `security`, `camera`, newest first.
- CSS classes follow `block__element--modifier`; phone width first.
- Author attribution: Gekkotron; commits with `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`.
- After every change run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`.

## Review Focus

1. A history payload that is valid JSON but not an array (an object, `null`) must keep the previous list and warn once, not empty it. (Task 3 test `non_array_history_keeps_previous_list`.)
2. A cancel-only stub (`cancel: true`, empty title) must never render as a blank row. (Task 1 test `cancel_stub_without_title_is_dropped`.)
3. A `sent_at` in the future, or far in the past, must still produce a sane relative time and day label, never a panic from an unsigned subtraction. (Task 5 tests `relative_time_future_is_just_now`, `day_label_handles_future`.)
4. An `entity_id` like `binary_sensor.front_door` whose entity has a different unique id must still match through the name slug, and an empty `entity_id` must match nothing. (Task 5 test `entity_match_rules`.)
5. An action posted while the broker link is down must answer 503 and the UI must toast "Couldn't send <label>", not "Sent". (Task 4 test `alert_action_refused_offline`; Task 8 wires the toast to the error path.)

---

### Task 1: `AlertView` and the history parser (`oikos-api`)

**Files:**
- Create: `crates/oikos-api/src/alerts.rs`
- Modify: `crates/oikos-api/src/lib.rs` (add `mod alerts;` and re-export `AlertAction, AlertChannel, AlertSeverity, AlertView, parse_history`)
- Modify: `crates/oikos-api/src/snapshot.rs` (field `alerts`)
- Modify: `crates/oikos-api/src/events.rs` (variant `AlertsChanged`)
- Test: unit tests inside `crates/oikos-api/src/alerts.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
  #[serde(rename_all = "snake_case")]
  pub enum AlertChannel { Alarm, Security, Camera, Energy, Climate, System, Misc }
  impl AlertChannel { pub const ALL: [AlertChannel; 7]; pub fn id(self) -> &'static str; pub fn name(self) -> &'static str /* "Alarm", "Security", … */ }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
  #[serde(rename_all = "snake_case")]
  pub enum AlertSeverity { Info, Warning, Critical }
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
  pub struct AlertAction { pub id: String, pub label: String }
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
  pub struct AlertView { pub id: String, pub channel: AlertChannel, pub severity: AlertSeverity, pub title: String, pub body: String, pub image_url: Option<String>, pub video_url: Option<String>, pub entity_id: Option<String>, pub actions: Vec<AlertAction>, pub source: Option<String>, pub silent: bool, pub resolved: bool, pub sent_at: u64 }
  /// The retained history payload as alerts, newest first as published. `None` when the payload is not a JSON array.
  pub fn parse_history(payload: &[u8]) -> Option<Vec<AlertView>>;
  ```
- `Snapshot.alerts: Vec<AlertView>` with `#[serde(default)]`; `WsEvent::AlertsChanged { alerts: Vec<AlertView> }`.
- **Existing bug fixed here:** `WsEvent` is `#[serde(tag = "type")]`, and serde_json refuses a tagged *newtype* variant holding a sequence ("cannot serialize tagged newtype variant WsEvent::AreasChanged containing a sequence"), so `AreasChanged(Vec<AreaView>)` and `DevicesChanged(Vec<DeviceView>)` are logged and dropped by `http/ws.rs::send` today and never reach the browser. Change them to struct variants `AreasChanged { areas: Vec<AreaView> }` and `DevicesChanged { devices: Vec<DeviceView> }` and update every match arm (`hub.rs::events_for`, `oikos-web/src/house.rs`, `house_state.rs` and its tests). The new variant uses the same struct shape.

- [ ] **Step 1: Write the failing tests** in `alerts.rs`:

```rust
const ENTRY: &str = r#"{"v":1,"id":"frigate-front-1696","channel":"camera","severity":"warning","title":"Front Door","body":"Person in Porch","image_url":"http://192.168.1.20:5000/api/events/1696/snapshot.jpg","video_url":"http://192.168.1.20:5000/api/events/1696/clip.mp4","entity_id":"binary_sensor.front_door","actions":[{"id":"disarm","label":"Disarm"}],"silent":false,"sent_at":1760880000,"source":"frigate","loc":{"key":"frigate.object","args":{"label":"person","zone":"Porch"}}}"#;

#[test] fn parses_a_full_entry()            // every field of ENTRY lands; resolved == false; loc ignored
#[test] fn unknown_channel_and_severity_fall_back() // "nvr" → Misc, "urgent" → Info
#[test] fn cancel_marks_resolved()           // ENTRY + "cancel":true → resolved == true, title kept
#[test] fn cancel_stub_without_title_is_dropped() // [{"v":1,"id":"x","cancel":true,"sent_at":1,"title":"","body":""}] → empty vec
#[test] fn bad_entries_are_skipped_not_fatal() // [42, {"id":7}, {"id":"ok","title":"T","sent_at":1}] → one alert "ok"
#[test] fn optional_fields_tolerate_wrong_types() // image_url: 5, actions: "x", sent_at: "soon" → None, empty, 0
#[test] fn at_most_three_actions()           // 4 actions → 3
#[test] fn non_array_is_none()               // b"{}" and b"nope" → None; b"[]" → Some(empty)
```

And in `events.rs` tests:

```rust
#[test] fn every_list_event_serializes()
// serde_json::to_value(WsEvent::AreasChanged { areas: vec![] }) == json!({"type":"areas_changed","areas":[]});
// same for DevicesChanged { devices } and AlertsChanged { alerts }; and each round-trips through from_value
```

- [ ] **Step 2: Run** `cargo test -p oikos-api alerts` → FAIL (module missing).
- [ ] **Step 3: Implement** the types and `parse_history` in `alerts.rs`. Parse into `serde_json::Value` first, then read fields leniently (`as_str`, `as_u64`); an entry needs a non-empty string `id`; an entry with `cancel == true` and a blank `title` is dropped.
- [ ] **Step 4: Add** `#[serde(default)] pub alerts: Vec<AlertView>` to `Snapshot` (built from `Vec::new()` in `snapshot()`, the hub fills it in Task 3) and `AlertsChanged { alerts: Vec<AlertView> }` to `WsEvent`; turn `AreasChanged` and `DevicesChanged` into the struct variants named in Interfaces and fix their match arms. Fix every `Snapshot { .. }` literal in the workspace (`grep -rn "Snapshot {" crates`).
- [ ] **Step 5: Run** `cargo test -p oikos-api && cargo check -p oikos-api --target wasm32-unknown-unknown && cargo clippy --workspace --all-targets -- -D warnings` → PASS.
- [ ] **Step 6: Commit** `feat(api): alert view, history parser, serialisable list events`.

### Task 2: `[push]` configuration

**Files:**
- Modify: `crates/oikos-os/src/config.rs` (new `PushConfig`, field `push` on `Config`)
- Modify: `oikos.example.toml`, `README.md` (config table / a "Push alerts" paragraph under "Web UI")
- Test: `crates/oikos-os/tests/config.rs`

**Interfaces:**
- Produces: `pub struct PushConfig { pub history_topic: String, pub responses_topic: String }` with the spec defaults; `Config.push: PushConfig`.

- [ ] **Step 1: Extend** `defaults_match_spec` with `assert_eq!(cfg.push.history_topic, "oikos-push/history"); assert_eq!(cfg.push.responses_topic, "oikos-push/responses");` and add to `numeric_looking_env_strings_stay_strings` a `jail.set_env("OIKOS_PUSH__HISTORY_TOPIC", "1")` with its assertion.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test config` → FAIL (no field `push`).
- [ ] **Step 3: Implement** `PushConfig` (`#[serde(default)]`, `Default` with the spec values, `Debug, Clone, Serialize, Deserialize, PartialEq`) and add it to `Config`.
- [ ] **Step 4: Document** the section in `oikos.example.toml` and README with the note: these become `oikos/history` and `oikos/push/responses` when the hub takes over push.
- [ ] **Step 5: Run** `cargo test -p oikos-os --test config` → PASS.
- [ ] **Step 6: Commit** `feat(os): push topics configuration`.

### Task 3: Hub alert store and ingest

**Files:**
- Create: `crates/oikos-os/src/alerts.rs` (the store: `AlertStore`)
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod alerts;`)
- Modify: `crates/oikos-os/src/hub.rs` (fields, `ingest` intercept, `snapshot`, `alerts`, `reply`)
- Modify: `crates/oikos-os/src/mqtt.rs` (`static_filters` gains the history topic; signature becomes `fn static_filters(topics: &TopicsConfig, push: &PushConfig)`)
- Test: `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Consumes: `oikos_api::{parse_history, AlertView, WsEvent::AlertsChanged}`, `PushConfig`.
- Produces:
  ```rust
  // alerts.rs
  pub struct AlertStore { /* list: Vec<AlertView>, last_bad_payload: Option<u64> */ }
  impl AlertStore {
      pub fn new() -> Self;
      /// Applies a history payload: `Changed` with the new list, `Unchanged` when equal, or
      /// `Invalid` (list kept) for a non-array payload, `first_time` false when the same bad
      /// payload (by hash) was seen before, so the caller logs once.
      pub fn apply(&mut self, payload: &[u8]) -> Applied;
      pub fn list(&self) -> &[AlertView];
      pub fn find(&self, id: &str) -> Option<&AlertView>;
  }
  pub enum Applied { Changed(Vec<AlertView>), Unchanged, Invalid { first_time: bool } }
  // hub.rs
  impl Hub {
      pub fn alerts(&self) -> Vec<AlertView>;
      /// Publishes the reply for `action` on `id`; `UnknownEntity`-style 404s become `HubError::UnknownAlert(String)` / `UnknownAction(String)`.
      pub fn reply(&self, id: &str, action: &str) -> Result<(), HubError>;
  }
  ```
  Add `HubError::UnknownAlert(String)` and `HubError::UnknownAction(String)` (both `#[error("unknown alert `{0}`")]` / `#[error("unknown action `{0}`")]`).

- [ ] **Step 1: Write the failing tests** in `tests/hub.rs` (use `ENTRY` from Task 1 as a string constant, wrapped in `[…]`):

```rust
#[tokio::test] async fn history_payload_fills_alerts_and_broadcasts()
// subscribe; ingest("oikos-push/history", b"[ENTRY]"); next_event == AlertsChanged { alerts: vec![alert] }; hub.alerts().len()==1; hub.snapshot().alerts == same; returned changes are empty (no registry change)
#[tokio::test] async fn identical_history_does_not_broadcast_again()
// ingest twice; after the first event, a second recv times out (use tokio::time::timeout 100 ms → Err)
#[tokio::test] async fn empty_history_payload_clears_the_list()
// ingest ENTRY then b"" → AlertsChanged { alerts: vec![] } and alerts().is_empty()
#[tokio::test] async fn non_array_history_keeps_previous_list()
// ingest ENTRY then b"{}" → alerts().len()==1, no new event
#[tokio::test] async fn reply_publishes_the_app_payload()
// online hub; ingest ENTRY; hub.reply("frigate-front-1696","disarm") ok; rx.recv() == Publish{topic:"oikos-push/responses", retain:false, payload} where serde_json::from_str::<Value>(payload) has id, action, install_id=="oikos-web", ts.as_u64() within 5 s of now; key order in the raw string: "id" before "action" before "install_id" before "ts"
#[tokio::test] async fn reply_rejects_unknown_alert_action_and_offline()
// unknown id → Err(HubError::UnknownAlert); known id + "nope" → Err(HubError::UnknownAction); set_connection(Offline) → Err(HubError::Publish(_))
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test hub history` → FAIL.
- [ ] **Step 3: Implement** `AlertStore` in `alerts.rs` with unit tests for `Applied::Invalid { first_time }` (second identical bad payload → `first_time == false`); hash with `std::hash::DefaultHasher`.
- [ ] **Step 4: Wire the hub**: `Hub::new` keeps `history_topic`, `responses_topic` and `alerts: Mutex<AlertStore>`; at the top of `ingest`, `if topic == self.history_topic { … return Vec::new(); }` applying the store, logging `tracing::warn!(%topic, "invalid history payload ignored")` on `Invalid { first_time: true }`, broadcasting `AlertsChanged` on `Changed`; `snapshot()` sets `alerts`; `reply` builds the payload with `serde_json::json!({"id": id, "action": action, "install_id": "oikos-web", "ts": clock::now()})` serialised through a struct with fields in that order (serde preserves struct field order; `json!` maps do not unless `preserve_order` is on, so use a small `#[derive(Serialize)] struct Reply<'a>`), then `self.publish(Publish { topic: responses_topic, payload, retain: false })`.
- [ ] **Step 5: Subscribe**: `static_filters` adds `push.history_topic.clone()`; update its callers and the existing unit test of `static_filters` if any (`grep -n static_filters crates/oikos-os/src/mqtt.rs`).
- [ ] **Step 6: Run** `cargo test -p oikos-os && cargo clippy --workspace --all-targets -- -D warnings` → PASS.
- [ ] **Step 7: Commit** `feat(os): read the push history and answer alert actions`.

### Task 4: REST routes and WebSocket coverage

**Files:**
- Modify: `crates/oikos-os/src/http/api.rs` (routes `/alerts`, `/alerts/{id}/actions/{action}`; `HttpError` mapping for the two new errors → 404)
- Modify: `README.md` HTTP API table (two rows)
- Test: `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Consumes: `Hub::alerts`, `Hub::reply`, `HubError::{UnknownAlert, UnknownAction}`.
- Produces: `GET /api/v1/alerts` → `Vec<AlertView>` JSON; `POST /api/v1/alerts/{id}/actions/{action}` → `202` empty, `404` JSON `ApiError`, `503` when offline.

- [ ] **Step 1: Write the failing tests** in `tests/http.rs`:

```rust
#[tokio::test] async fn alerts_route_and_ws_event()
// ws connect, read snapshot frame: json["alerts"] == []; server.hub.ingest("oikos-push/history", [ENTRY]); next frame json["type"] == "alerts_changed" and json["alerts"][0]["id"] == "frigate-front-1696"; GET /api/v1/alerts → 200, one entry
#[tokio::test] async fn areas_changed_reaches_the_socket()
// regression for the serialisation bug fixed in Task 1: ws connect, read the snapshot; PUT /api/v1/areas with one area; next frame json["type"] == "areas_changed" and json["areas"] is an array
#[tokio::test] async fn alert_action_publishes_and_404s()
// online; POST …/frigate-front-1696/actions/disarm → 202, server.publisher.recv() topic == "oikos-push/responses"; POST …/nope/actions/disarm → 404 with body.error containing "nope"; POST …/frigate-front-1696/actions/zzz → 404
#[tokio::test] async fn alert_action_refused_offline()
// not online; POST → 503
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test http alert` → FAIL.
- [ ] **Step 3: Implement** handlers `alerts(State) -> Json<Vec<AlertView>>` and `alert_action(State, Path<(String, String)>) -> Result<StatusCode, HttpError>` returning `StatusCode::ACCEPTED`; map `UnknownAlert | UnknownAction → NOT_FOUND`.
- [ ] **Step 4: README**: add the two routes to the HTTP API table.
- [ ] **Step 5: Run** `cargo test -p oikos-os && cargo clippy --workspace --all-targets -- -D warnings` → PASS.
- [ ] **Step 6: Commit** `feat(os): alerts REST routes`.

### Task 5: Web pure helpers (`alerts` module)

**Files:**
- Create: `crates/oikos-web/src/alerts.rs`
- Modify: `crates/oikos-web/src/lib.rs` (`pub mod alerts;`)
- Modify: `crates/oikos-web/src/house_state.rs`, `crates/oikos-web/src/house.rs` (field/signal `alerts: Vec<AlertView>`, set on `Snapshot` and `AlertsChanged`)
- Modify: `crates/oikos-web/src/api.rs` (`pub async fn send_alert_action(id: &str, action: &str) -> Result<(), ApiError>` using `check(Request::post(&url).build(), Some(BROKER_OFFLINE))`)
- Modify: `crates/oikos-web/src/icons.rs` (`pub fn channel_icon(channel: AlertChannel) -> &'static str`, seven distinct SVGs: bell, shield, camera, bolt, thermometer, gear, info circle)
- Test: unit tests in `alerts.rs`, `house_state.rs`, `icons.rs` (native `cargo test -p oikos-web`)

**Interfaces:**
- Produces:
  ```rust
  pub const RECENT_LIMIT: usize = 5;
  pub const RECENT_CHANNELS: [AlertChannel; 3] = [Alarm, Security, Camera];
  pub fn recent(alerts: &[AlertView]) -> Vec<AlertView>;                    // first 5 of RECENT_CHANNELS, order kept
  pub fn filtered(alerts: &[AlertView], channel: Option<AlertChannel>) -> Vec<AlertView>;
  pub fn relative_time(sent_at: u64, now: u64) -> String;                   // "Just now" (<60 s or future), "2 min ago", "3 h ago", "2 d ago"
  #[derive(Clone, Debug, PartialEq, Eq)] pub enum DayLabel { Today, Yesterday, On { year: i32, month: u8, day: u8 } }
  /// `tz_offset_min` is the browser's `Date.getTimezoneOffset()` (minutes to ADD to local to get UTC, so local = utc - offset).
  pub fn day_label(sent_at: u64, now: u64, tz_offset_min: i32) -> DayLabel;
  pub fn group_by_day(alerts: &[AlertView], now: u64, tz_offset_min: i32) -> Vec<(DayLabel, Vec<AlertView>)>; // order kept, one group per consecutive label
  pub fn match_entity(entity_id: Option<&str>, entities: &BTreeMap<String, EntityView>) -> Option<String>;
  pub fn slug(text: &str) -> String;                                         // lowercase, runs of non [a-z0-9] → "_", trimmed of "_"
  ```

- [ ] **Step 1: Write the failing tests** in `alerts.rs` (build `AlertView`s with a local `fn alert(id, channel, sent_at)` helper):

```rust
#[test] fn recent_keeps_five_from_the_three_channels()   // 7 alerts mixing energy/camera/alarm → 5, energy excluded, order kept
#[test] fn filtered_by_channel_or_all()
#[test] fn relative_time_steps()        // now-10 → "Just now"; now-120 → "2 min ago"; now-7200 → "2 h ago"; now-172800 → "2 d ago"
#[test] fn relative_time_future_is_just_now()  // now+600 → "Just now"
#[test] fn day_label_uses_local_midnight() // now = 2026-10-19T00:30:00Z, tz_offset_min = -120 (UTC+2): sent 2026-10-18T22:30Z (00:30 local) → Today; sent 2026-10-18T21:30Z (23:30 local on the 18th) → Yesterday; sent 2026-10-15T12:00Z → On{2026,10,15}
#[test] fn day_label_handles_future()   // sent_at > now → Today
#[test] fn group_by_day_keeps_order_and_merges_runs()
#[test] fn entity_match_rules()         // exact id; "binary_sensor.front_door" with entity id "front_door"; slug of name "Front Door" matches unique id "0x1_front_door"? NO: slug compares to slug(name) or slug(unique_id) exactly → test exact id, object-id part, name slug "Front Door"→"front_door", and None for None/""/no match
```

Add to `house_state.rs` tests: `alerts_changed_replaces_the_list` (apply `AlertsChanged` then `Snapshot` with empty alerts → empty). Add to `icons.rs` tests: `every_channel_has_a_distinct_svg`.

- [ ] **Step 2: Run** `cargo test -p oikos-web alerts` → FAIL.
- [ ] **Step 3: Implement** the helpers with `time::OffsetDateTime` (`from_unix_timestamp`, `+ Duration::minutes(-tz_offset_min)`, `.date()`); keep every signed/unsigned conversion through `saturating_sub` / `i64::try_from`.
- [ ] **Step 4: Implement** the `HouseState`/`House` field and `apply` arms, `send_alert_action`, `channel_icon`.
- [ ] **Step 5: Run** `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown` → PASS.
- [ ] **Step 6: Commit** `feat(web): alert helpers, signals and client`.

### Task 6: `AlertRow` and the Home "Recent alerts" section

**Files:**
- Create: `crates/oikos-web/src/components/alert_row.rs`, `crates/oikos-web/src/components/now.rs`
- Modify: `crates/oikos-web/src/components/mod.rs`, `crates/oikos-web/src/screens/home.rs`, `crates/oikos-web/styles/main.css`

**Interfaces:**
- Consumes: Task 5 helpers, `channel_icon`.
- Produces:
  ```rust
  /// Unix seconds, set from `js_sys::Date::now()/1000` and re-set every 60 s by a gloo_timers::callback::Interval on wasm; from `std::time::SystemTime` once, never ticking, off wasm. Provided once as context by `App`.
  #[derive(Clone, Copy)] pub struct Now(pub RwSignal<u64>);
  pub fn install_now() -> Now;
  #[component] pub fn AlertRow(alert: AlertView) -> impl IntoView;  // <A href="/alerts/<id>" class="alert-row">: .alert-row__thumb (img or glyph) .alert-row__title .alert-row__body .alert-row__time ("… · Resolved" when resolved)
  #[component] pub fn RecentAlerts() -> impl IntoView;              // section.alerts-recent with h2 "Recent alerts", <A href="/alerts">"See all"</A>, rows or p.alerts-recent__empty "No alerts yet"
  ```
- `App` calls `provide_context(install_now())`. Add `js-sys = "0.3"` to the workspace and web `Cargo.toml` (used for `Date::now()` and, in Tasks 7-8, locale formatting); add the web-sys feature `HtmlImageElement` only if needed by the error handler (`on:error` on `<img>` needs no feature).

- [ ] **Step 1: Implement** `Now` and `AlertRow`; render the thumbnail as `<img class="alert-row__thumb" src=image_url loading="lazy" alt="">` when set, else `<span class="alert-row__thumb alert-row__thumb--glyph" inner_html=channel_icon(..)>`.
- [ ] **Step 2: Implement** `RecentAlerts` from `Memo::new(|_| house.alerts.with(|a| recent(a)))` and the `Now` context; mount it in `HomeScreen` after the `<section class="room">` block (inside the `Show` so it appears with the loaded house).
- [ ] **Step 3: CSS**: `.alert-row` (card surface, flex row, gap 12px, min-height 0), `.alert-row__thumb` 56px square, radius 14px, `object-fit: cover`, glyph variant centred 26px; `.alert-row__title` `--type-card-name`, body/time `--type-label` muted, one line with ellipsis; `.alerts-recent` margin-top 16px, header row with the title muted and "See all" as a `.btn btn--compact`.
- [ ] **Step 4: Verify**: `cargo check -p oikos-web --target wasm32-unknown-unknown`, then `cd crates/oikos-web && trunk build` and run the hub (`cargo run -p oikos-os -- run`) against the house broker; the Home screen shows the section with real alerts. Screenshot with headless Chrome over CDP (see memory note; window ≥ 500 px wide) into the scratchpad.
- [ ] **Step 5: Commit** `feat(web): recent alerts on the home screen`.

### Task 7: `/alerts` page and navigation

**Files:**
- Create: `crates/oikos-web/src/screens/alerts.rs`
- Modify: `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/src/app.rs` (route `/alerts`, nav link "Alerts" between Home and Devices), `styles/main.css`

**Interfaces:**
- Consumes: `filtered`, `group_by_day`, `DayLabel`, `AlertRow`, `Now`, `AlertChannel::{ALL, name, id}`, `channel_icon`.
- Produces: `#[component] pub fn AlertsScreen() -> impl IntoView;` and `pub fn day_title(label: &DayLabel) -> String` ("Today", "Yesterday", or `js_sys::Date` → `to_locale_date_string("default", {month:"short", day:"numeric", year:"numeric"})` on wasm; `"{day} {month}/{year}"` fallback off-wasm so the function compiles natively).

- [ ] **Step 1: Implement** the screen: `header.sheet__header` with the back `<A href="/">` and `h1.sheet__title "Alerts"`; `nav.chips` of `button.chip` (`.chip--selected`) for All + the seven channels, each with its glyph and name, selection in an `RwSignal<Option<AlertChannel>>`; groups as `h2.alerts__day` then rows; empty copy per the constraints. The time zone offset comes from `js_sys::Date::new_0().get_timezone_offset() as i32` on wasm, `0` otherwise.
- [ ] **Step 2: Route and nav** in `app.rs`.
- [ ] **Step 3: CSS**: `.alerts` (column, gap 8px), `.alerts__day` (`--type-label`, muted, margin-top 8px, uppercase letter-spacing like `.room__title` siblings).
- [ ] **Step 4: Verify** with `trunk build`, the running hub and a CDP screenshot of `/alerts` with a filter applied.
- [ ] **Step 5: Commit** `feat(web): alerts page with channel filter and day groups`.

### Task 8: `/alerts/:id` detail

**Files:**
- Create: `crates/oikos-web/src/screens/alert_detail.rs`
- Modify: `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/src/app.rs` (route `/alerts/:id`), `styles/main.css`

**Interfaces:**
- Consumes: `match_entity`, `send_alert_action`, `Toasts`, `channel_icon`, `AlertChannel::name`.
- Produces: `#[component] pub fn AlertDetailScreen() -> impl IntoView;` and `pub fn full_date(sent_at: u64) -> String` (wasm: `js_sys::Date::to_locale_string("default", undefined)`; native: `format_timestamp`).

- [ ] **Step 1: Implement** the screen with `use_params_map` like `EntitySheetScreen`: missing id → `sheet__header` + `p "This alert is no longer available"`. Found: `div.alert-media` (16:9 via `aspect-ratio: 16 / 9`) containing, in a local `RwSignal<Media>` state machine `enum Media { Image, Playing, Failed }`: `<img>` with `on:error=|_| media.set(Failed)`; when `video_url` is set and `Image`, a `button.alert-media__play` overlay ("Play" label, aria-label "Play clip") that sets `Playing` and renders `<video controls autoplay playsinline src=video_url on:error=… >`; `Failed` → `p.alert-media__message "Media no longer available"`. Then `h1.sheet__title`, `p.alert-detail__body`, chips `span.badge` for the channel name and "Resolved", `p.sheet__updated` with `full_date`, action buttons `button.btn` (at most 3) calling `send_alert_action` in `spawn_local` and toasting "Sent" / "Couldn't send {label}" on `Err`, `A.btn "Open device"` to `/entity/<id>` when `match_entity` finds one, and `p.alert-detail__source "From {source} · {full_date}"` when `source` is set.
- [ ] **Step 2: Route** in `app.rs`; keep `/alerts` before `/alerts/:id` or rely on the router's static-first matching (verify both routes resolve).
- [ ] **Step 3: CSS**: `.alert-media` (glass card surface, radius 16px, overflow hidden, `aspect-ratio`), `.alert-media img, .alert-media video { width:100%; height:100%; object-fit:cover }`, `.alert-media__play` (centred 64px round `.btn--round`), `.alert-media__message` centred muted, `.alert-detail__actions` flex-wrap gap 8px.
- [ ] **Step 4: Verify** with `trunk build`, the hub, and CDP screenshots of a camera alert with its snapshot and after pressing play; check the action POST with a Jeedom-style alert if one is in the history, otherwise with `curl -X POST …/actions/<id>` against the hub and the broker log.
- [ ] **Step 5: Commit** `feat(web): alert detail with media, actions and open device`.

### Task 9: Docs, smoke test, embedded build

**Files:**
- Modify: `README.md` (Web UI screens list: Alerts and Alert detail; a "Push alerts" paragraph: source topic, future topics, media port assumption, follow-ups: media proxy, MCP tool)
- Modify: `crates/oikos-web/tests/smoke.rs` (assert the shell contains `"Alerts"` in the nav)
- Modify: `docs/superpowers/plans/2026-10-18-oikos-os-hub-deferred.md` (append: media proxy for Frigate login port; `list_alerts` MCP tool)

- [ ] **Step 1: Docs and smoke assertion.**
- [ ] **Step 2: Run the full gate**:
  ```sh
  cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
  cargo check -p oikos-api --target wasm32-unknown-unknown && cargo check -p oikos-web --target wasm32-unknown-unknown
  (cd crates/oikos-web && trunk build --release && wasm-pack test --headless --chrome)
  cargo build --release -p oikos-os
  ```
  Expected: all PASS; the hub binary embeds the new `dist/`.
- [ ] **Step 3: Commit** `docs: push alerts in the web UI` and leave the branch on `main`, unpushed (the user pushes).

### Task 10: Frigate media proxy (added during execution)

**Why (ledger finding):** the real history's `image_url`/`video_url` are all on `http://192.168.1.91:8971`, Frigate's login port, so the browser cannot load any snapshot or clip and every camera alert shows "Media no longer available". Spec §4.4 named this proxy as the follow-up; it is pulled in because without it the feature shows no camera picture in this house.

**Files:**
- Modify: `Cargo.toml` (reqwest features add `"stream"`), `crates/oikos-os/src/config.rs` (`FrigateConfig`, `Config.frigate`), `oikos.example.toml`, `README.md`
- Create: `crates/oikos-os/src/media.rs` (pure URL logic + the `MediaProxy` client)
- Modify: `crates/oikos-os/src/alerts.rs` (`AlertStore::with_media_origin`), `crates/oikos-os/src/hub.rs` (store construction, `Hub::media()`), `crates/oikos-os/src/http/api.rs` (route `/media`)
- Test: unit tests in `media.rs` and `alerts.rs`; `crates/oikos-os/tests/config.rs`; `crates/oikos-os/tests/http.rs` with a fake Frigate server

**Interfaces:**
```rust
// config.rs
#[serde(default)] pub struct FrigateConfig { pub url: Option<String>, pub user: Option<String>, pub password: Option<String> } // Debug redacts password; url normalised without trailing slash
// media.rs
pub fn origin_of(url: &str) -> Option<String>;                 // "http://h:8971" (scheme://host[:port], lowercase host, default port dropped)
pub fn proxied(url: &str, media_origin: &str) -> Option<String>; // Some("/api/v1/media?url=<percent-encoded url>") when same origin, else None
pub struct MediaProxy { client: reqwest::Client, cfg: FrigateConfig, origin: String, token: tokio::sync::Mutex<Option<String>> }
impl MediaProxy {
    pub fn new(cfg: FrigateConfig) -> Option<Arc<MediaProxy>>;   // None when url is unset
    /// Fetches `url` (must be on the origin, else `MediaError::Forbidden`) with the frigate_token cookie when a login is configured; passes `range` through; on 401 logs in again once.
    pub async fn fetch(&self, url: &str, range: Option<&str>) -> Result<Upstream, MediaError>;
}
pub struct Upstream { pub status: u16, pub content_type: Option<String>, pub content_length: Option<String>, pub content_range: Option<String>, pub accept_ranges: Option<String>, pub body: axum::body::Body }
pub enum MediaError { Forbidden, Unavailable(String) }
// alerts.rs
impl AlertStore { pub fn with_media_origin(origin: Option<String>) -> Self } // apply() rewrites image_url/video_url through `proxied` when the origin is set
// hub.rs
impl Hub { pub fn media(&self) -> Option<Arc<MediaProxy>> }
// http: GET /api/v1/media?url=…  → 400 missing/invalid url, 403 other origin or no proxy, else the upstream status with the four headers and the streamed body
```
Login: `POST <url>/api/login` with JSON `{"user","password"}`; the token is the `frigate_token` cookie of the reply's `Set-Cookie` headers; it is cached in memory; one re-login on a 401 from the media fetch, then the 401 is passed through. Without `user`/`password` the proxy fetches with no cookie (unauthenticated port behind a configured origin).

- [ ] **Step 1: Tests first.** `media.rs`: `origin_of_normalises` (`http://H:8971/x` → `http://h:8971`; `http://h:80/` → `http://h`; `https://h/` → `https://h`; `nope` → None), `proxied_only_same_origin` (same origin → `/api/v1/media?url=http%3A%2F%2Fh%3A8971%2Fapi%2Fevents%2F1%2Fsnapshot.jpg`; other port → None). `alerts.rs`: `apply_rewrites_media_on_the_frigate_origin` (image_url/video_url on the origin rewritten, an off-origin image_url untouched). `tests/config.rs`: defaults `frigate.url == None`; `OIKOS_FRIGATE__PASSWORD` stays a string and `Debug` redacts it. `tests/http.rs`: a `fake_frigate()` axum server with `POST /api/login` (checks body `{"user":"u","password":"p"}`, answers `Set-Cookie: frigate_token=tok; Path=/`), `GET /api/events/1/snapshot.jpg` (401 without `Cookie: frigate_token=tok`, else 200 `image/jpeg` body `JPEG`, echoing a `Range` header into `Content-Range: bytes 0-3/4` with 206); tests `media_proxy_logs_in_and_streams` (GET `/api/v1/media?url=<fake>/api/events/1/snapshot.jpg` → 200, content-type image/jpeg, body "JPEG"; the fake saw exactly one login), `media_proxy_passes_range_through` (206 + content-range), `media_proxy_relogs_once_on_401` (fake invalidates the token after the first fetch → second fetch gets 200 and the fake saw two logins), `media_proxy_refuses_other_origins` (403), `media_proxy_400_without_url`, `alerts_carry_proxied_media_urls` (ingest a history whose image_url is on the fake origin → `/api/v1/alerts` shows the `/api/v1/media?url=` form).
- [ ] **Step 2: Run** `cargo test -p oikos-os media` → FAIL.
- [ ] **Step 3: Implement** as in Interfaces; `Server::start_with(cfg: Config)` in tests/http.rs next to `start()`.
- [ ] **Step 4: Docs**: `[frigate]` in `oikos.example.toml` and README (why the login port needs it; the web never sees the password).
- [ ] **Step 5: Run** the full gate (fmt, clippy, workspace tests, wasm checks) → PASS.
- [ ] **Step 6: Commit** `feat(os): proxy Frigate media with the Frigate login`.
- [ ] **Step 7: Verify in the house**: put the Frigate `url`, `user`, `password` from the user's own setup payload into the git-ignored `oikos.toml` without printing them, rebuild the release hub, restart it, and confirm `curl -sI 'http://127.0.0.1:8080/api/v1/media?url=…'` on a real snapshot returns 200 `image/jpeg`, then a CDP screenshot of a camera alert's detail with its picture.
