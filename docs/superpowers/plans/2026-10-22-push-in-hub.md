# Push in the hub Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The hub sends the Oikos app's FCM pushes itself, with the Frigate bridge and the two watchdogs, so the OikosPush containers can be switched off.

**Architecture:** Three new crates. `oikos-push` is the push service: normalisation, history, tokens, the retry queue and the FCM client. It is driven by the hub through `on_message` / `on_connected` / `on_subscribed` and publishes through the hub's `mpsc::Sender<Publish>`. `oikos-frigate` and `oikos-watchdog` are pure translators, each turning its input (Frigate events, gateway health states, Mosquitto log lines) into v1 alert JSON. The hub hands that JSON to `PushService::submit`. Everything goes through the one MQTT connection in `crates/oikos-os/src/mqtt.rs`. The hub's existing `AlertStore` keeps reading the retained history topic, which is now `<prefix>/history`, published by the service.

**Tech Stack:** Rust 2024, tokio, rumqttc 0.25 (the hub's link), reqwest 0.13, serde_json, `jsonwebtoken` 10 (RS256 for the Google OAuth assertion), axum (dev-dependency for the fake FCM server), Leptos 0.8 (web node editor).

**Spec:** `docs/superpowers/specs/2026-10-22-push-in-hub-design.md`. Behaviour reference: `~/Documents/SourcesPerso/OikosPush` at `f122031` (read-only). Its Python modules and `tests/` are the detailed spec for Tasks 1–5, 7 and 8.

## Global Constraints

- New crates: `crates/oikos-push`, `crates/oikos-frigate`, `crates/oikos-watchdog`. Each has `edition.workspace`, `version.workspace`, `authors.workspace`, `license.workspace` (Apache-2.0) and `repository.workspace`, and uses `#![forbid(unsafe_code)]`. Dependencies come through `[workspace.dependencies]` (add `jsonwebtoken` there).
- Topic prefix is `oikos/push`, configurable as `[push] prefix`. The topics are `<prefix>/notification/<source>`, `<prefix>/tokens/<installId>` (retained), `<prefix>/responses` and `<prefix>/history` (retained).
- Legacy intake prefix is `oikos-push`, configurable as `[push] legacy_prefix`; empty disables it. The hub reads `<legacy>/notification/+`, `<legacy>/tokens/+` and `<legacy>/history`. It never publishes there, except an empty retained payload clearing a dead token on `<legacy>/tokens/<id>`.
- Constants from OikosPush:
  - `DEFAULT_TTL = 43200`, `CANCEL_TTL = 300`, `MAX_TTL = 2_419_200`, `FCM_MAX_BYTES = 4096`, `ELLIPSIS = "…"`.
  - Settle delay 2 s, `retry_interval` 60 s, `max_retries` 10, `history_size` 100, `history_days` 7.
  - Channels `alarm, security, camera, energy, climate, system, misc`; severities `info, warning, critical`.
- With no `[push] credentials` the service keeps the history and sends nothing. It logs one `warn` at start. A missing or unreadable key file is the same, plus that warning naming the path, and the hub still starts.
- Secrets (the service-account key, the Frigate login) never appear in logs, `Debug` output or commits. `oikos.toml` stays git-ignored.
- Do not add per-repo `.cargo/config.toml` or target-dir overrides.
- Commits go out as Gekkotron: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`. Every message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Gate for every task: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` all pass.

## Review Focus

1. **The first boot must not wipe the retained history.** The service publishes nothing on `<prefix>/history` before its first sync. Alerts received before then are held and applied after the retained list is adopted (Task 5 test `first_boot_never_publishes_an_empty_history`).
2. **A reconnect during the settle window must not mark the service synced early.** A stale settle timer from a previous connection is ignored (Task 5 test `stale_settle_timer_is_ignored`).
3. **The hub's own history echo must not be re-adopted.** A non-retained message on `<prefix>/history` never replaces the service's list (Task 5 test `own_history_echo_is_ignored`).
4. **OAuth failures count as transient.** A bad key, network loss or a 401 are retried later for every token, never clear tokens and never panic. A 401 drops the cached access token so the next attempt signs in again (Task 3 test `unauthorized_drops_the_cached_token_and_is_transient`).
5. **A phone registered on both the old and the new token topic with the same token gets one push.** When FCM says that token is dead, both topics are cleared (Task 5 test `same_token_on_legacy_and_new_topic_is_sent_once_and_cleared_on_both`).

---

### Task 1: `oikos-push` crate and normalisation

**Files:**
- Create: `crates/oikos-push/Cargo.toml`, `crates/oikos-push/src/lib.rs`, `crates/oikos-push/src/normalize.rs`
- Modify: `Cargo.toml` (workspace dep `jsonwebtoken = { version = "10", default-features = false, features = ["aws_lc_rs"] }`; `aws-lc-rs` is already in the lock; use `rust_crypto` if `aws_lc_rs` fails to build)
- Test: `crates/oikos-push/src/normalize.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub struct Alert { id: String, channel: String, severity: String, title: String, body: String, image_url: Option<String>, video_url: Option<String>, entity_id: Option<String>, actions: Vec<Action>, silent: bool, cancel: bool, ttl: u32, sent_at: u64, source: String, loc: Option<serde_json::Value> }`, with all fields `pub` and `Clone, Debug, PartialEq`.
  - `pub struct Action { pub id: String, pub label: String }`.
  - `pub struct Rejected { pub reason: String }`.
  - `pub fn normalize(source: &str, payload: &serde_json::Value, now: u64) -> Result<Alert, Rejected>`. The source is the topic segment; the caller extracts it.
  - `pub fn to_fcm_data(alert: &Alert) -> BTreeMap<String, String>`.
  - `pub enum Priority { High, Normal }`, `pub fn android_priority(alert: &Alert) -> Priority` and `pub fn ttl_seconds(alert: &Alert) -> u32`.
  - The constants in Global Constraints, as `pub const`.

- [ ] **Step 1: Write the failing tests.** Port every test in `OikosPush/tests/test_normalize.py` (42) one for one, with the same names in snake_case and the same input payloads and expected values. Add the `android_priority` and `ttl_seconds` cases from `tests/test_fcm_sender.py`. Python's `normalize(topic, …)` takes a topic; the Rust version takes the `<source>` segment, so pass `"frigate"` where Python passes `"oikos-push/notification/frigate"`.
- [ ] **Step 2: Run `cargo test -p oikos-push`.** Expected: compile failure (`normalize` is not defined).
- [ ] **Step 3: Implement `normalize.rs`** as a line-by-line port of `normalize.py`: v1 and legacy paths, the `LEGACY_CHANNELS` table, the `_loc` validation regexes, and `_fit`'s drop/shrink order (loc, body, title, actions, entity_id, video_url, image_url). Truncate UTF-8 safely: cut the bytes, then back off to a char boundary.
- [ ] **Step 4: Run `cargo test -p oikos-push`.** Expected: all pass.
- [ ] **Step 5: Commit.** Message: `feat(push): oikos-push crate with OikosPush normalisation`.

### Task 2: History and tokens

**Files:**
- Create: `crates/oikos-push/src/history.rs`, `crates/oikos-push/src/tokens.rs`
- Test: in-file `mod tests`

**Interfaces:**
- Consumes: `Alert` (Task 1).
- Produces:
  - `pub struct History`, with these methods:
    - `new(size: usize, days: u64)`.
    - `apply(&mut self, alert: &Alert, now: u64)`.
    - `trim(&mut self, now: u64)`.
    - `entries(&self) -> &[serde_json::Value]`.
    - `to_json(&self) -> String`.
    - `replace_from_json(&mut self, payload: &[u8]) -> bool`: false when the payload is not a JSON array, and the list is then kept.
    - `is_empty(&self) -> bool`.
  - `pub fn to_history_entry(alert: &Alert) -> serde_json::Value`. It produces the typed keys of `history.py::to_history_entry`, including `loc`.
  - `pub struct Tokens`, keyed by the **full token topic** (so a legacy and a new registration of the same install are two entries), with these methods:
    - `handle(&mut self, topic: &str, payload: &[u8]) -> TokenStatus`, where `TokenStatus` is `Registered | Updated | Removed | Noop | Ignored`.
    - `remove(&mut self, topic: &str) -> bool`.
    - `clear(&mut self)`.
    - `tokens(&self) -> Vec<String>`: unique, in registration order.
    - `topics_for_token(&self, token: &str) -> Vec<String>`.
    - `len(&self) -> usize`.

- [ ] **Step 1: Write the failing tests.** Port `tests/test_history.py` and `tests/test_tokens.py`. Skip the tests about `history.json` loading and saving; persistence is the broker now. Add:
  - `replace_from_json_rejects_non_array_and_keeps_list`;
  - `tokens_on_two_topics_with_one_token_yield_it_once`: `oikos/push/tokens/a` and `oikos-push/tokens/a`, both with token `t1`, give `tokens() == ["t1"]` and `topics_for_token("t1").len() == 2`.
- [ ] **Step 2: Run `cargo test -p oikos-push`.** Expected: FAIL (undefined types).
- [ ] **Step 3: Implement both modules,** ported from `history.py` and `tokens.py`. The clock comes in as the `now` argument.
- [ ] **Step 4: Run `cargo test -p oikos-push`.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(push): alert history and token registry`.

### Task 3: FCM HTTP v1 client

**Files:**
- Create: `crates/oikos-push/src/fcm.rs`
- Modify: `crates/oikos-push/Cargo.toml` (`reqwest`, `jsonwebtoken`, `tokio`, `futures-util`; dev: `axum`, `tokio` with `macros`)
- Test: `crates/oikos-push/tests/fcm.rs` (a local axum server stands in for both Google endpoints)

**Interfaces:**
- Consumes: `Priority` (Task 1).
- Produces:
  - `pub struct ServiceAccount { project_id, client_email, private_key, token_uri }`, loaded with `pub fn load(path: &Path) -> Result<ServiceAccount, FcmError>`. Its `Debug` hides `private_key`.
  - `pub enum Outcome { Sent, Invalid, Message(String), Transient(String) }`.
  - `pub fn classify(status: u16, body: &str) -> Outcome`:
    - 2xx → `Sent`.
    - `details[].errorCode == "UNREGISTERED"` → `Invalid`.
    - `INVALID_ARGUMENT`, in either `error.status` or `errorCode`:
      - `Invalid` when `error.message` lowercased contains `"registration token"`;
      - otherwise `Message(error.message)`.
    - Anything else → `Transient`.
  - `pub trait FcmTransport: Send + Sync { fn send<'a>(&'a self, token: &'a str, data: &'a BTreeMap<String, String>, priority: Priority, ttl: u32) -> futures_util::future::BoxFuture<'a, Outcome>; }`.
  - `pub struct HttpFcm`, built with `HttpFcm::new(account: ServiceAccount, fcm_base: Url)`. The real base is `https://fcm.googleapis.com/`; tests pass the local server. It implements `FcmTransport`.
  - `pub struct SendResult { pub sent: usize, pub invalid: Vec<String>, pub transient: Vec<String>, pub message_errors: usize, pub last_message_error: String }`.
  - `pub async fn send_data(t: &dyn FcmTransport, data: &BTreeMap<String,String>, tokens: &[String], priority: Priority, ttl: u32) -> SendResult`.

- [ ] **Step 1: Write the failing tests:**
  - `classify_unregistered_is_invalid`: 404 with `{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}`.
  - `classify_invalid_argument_about_token_is_invalid`: message `"The registration token is not a valid FCM registration token"`.
  - `classify_invalid_argument_about_message_is_message`: message `"Message is too big"`.
  - `classify_server_error_is_transient`: 503.
  - `send_data_splits_tokens_by_outcome`: a fake transport mapping `ok → Sent`, `dead → Invalid`, `big → Message`, `flaky → Transient` gives `sent == 1`, `invalid == ["dead"]`, `message_errors == 1`, `transient == ["flaky"]`.
  - `http_fcm_posts_data_only_message`: the local server receives `POST /v1/projects/p/messages:send` with `Authorization: Bearer at-1`, and a body whose `message.token`, `message.data` and `message.android.priority == "high"`, `message.android.ttl == "43200s"` match, with no `notification` key. The token endpoint returns `{"access_token":"at-1","expires_in":3600}`.
  - `access_token_is_cached_until_near_expiry`: two sends make one token request.
  - `unauthorized_drops_the_cached_token_and_is_transient`: FCM answers 401 once; the outcome is `Transient` and the next send requests a new access token.
  - `oauth_failure_is_transient`: the token endpoint answers 500, so the outcome is `Transient`.
  - The test service account uses an RSA key generated for the tests and committed under `crates/oikos-push/tests/fixtures/test-sa.json`. It is clearly a test key and never a real one.
- [ ] **Step 2: Run `cargo test -p oikos-push --test fcm`.** Expected: FAIL.
- [ ] **Step 3: Implement `fcm.rs`.**
  - The JWT has claims `iss = client_email`, `scope = "https://www.googleapis.com/auth/firebase.messaging"`, `aud = token_uri`, `iat = now` and `exp = now + 3600`, signed RS256 with `private_key`.
  - Exchange it at `token_uri` with `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer`.
  - Cache the access token behind a `tokio::sync::Mutex` until `expires_in - 60` s.
  - The reqwest timeout is 10 s. `send_data` sends to the tokens concurrently (`join_all`).
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(push): FCM HTTP v1 client with service-account OAuth`.

### Task 4: In-memory retry queue

**Files:**
- Create: `crates/oikos-push/src/queue.rs`
- Test: in-file `mod tests`

**Interfaces:**
- Consumes: `Priority`.
- Produces:
  - `pub struct Pending { pub id: String, pub data: BTreeMap<String,String>, pub tokens: Option<Vec<String>>, pub priority: Priority, pub ttl: u32, pub queued_at: u64, pub attempts: u32, pub next_at: u64 }`. `tokens: None` means "resolve at send time".
  - `pub struct RetryQueue`, with these methods:
    - `new(retry_interval: u64, max_retries: u32)`.
    - `add(&mut self, p: Pending)`.
    - `take_due(&mut self, now: u64) -> Vec<Pending>`: drops items past `queued_at + ttl` or at `max_retries`.
    - `put_back(&mut self, p: Pending, now: u64)`: increments `attempts` and sets `next_at = now + retry_interval`.
    - `len(&self) -> usize`.

- [ ] **Step 1: Write the failing tests.** Port `tests/test_notification_queue.py`, minus its file-persistence tests. Keep the TTL-expiry and max-retries cases with their values.
- [ ] **Step 2: Run `cargo test -p oikos-push queue`.** Expected: FAIL.
- [ ] **Step 3: Implement `queue.rs`** from `notification_queue.py`'s `add_notification`, `get_notifications_to_retry`, `mark_success` and `mark_failure`, using the injected `now`.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(push): in-memory retry queue`.

### Task 5: `PushService`

**Files:**
- Create: `crates/oikos-push/src/service.rs`
- Modify: `crates/oikos-push/src/lib.rs` (re-exports), `crates/oikos-push/Cargo.toml` (`oikos-core` for `command::Publish`, `tracing`)
- Test: `crates/oikos-push/tests/service.rs`, using `#[tokio::test(start_paused = true)]`, a fake `FcmTransport` that records calls and returns scripted outcomes, and an `mpsc::Receiver<Publish>` to inspect what was published

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  - `pub struct PushSettings { pub prefix: String, pub legacy_prefix: String, pub retry_interval: u64, pub max_retries: u32, pub history_size: usize, pub history_days: u64 }`.
  - `pub enum PushState { Off, Syncing, Ready }` and `pub struct PushStatus { pub state: PushState, pub installs: usize, pub queued: usize }`. Both are `Serialize` with snake_case values.
  - `pub struct PushService`, constructed with `PushService::new(settings: PushSettings, publisher: mpsc::Sender<Publish>, transport: Option<Arc<dyn FcmTransport>>, clock: fn() -> u64) -> Arc<PushService>`. Its methods:
    - `filters(&self) -> Vec<String>`: `<p>/notification/+`, `<p>/tokens/+`, `<p>/history`, plus the three legacy ones when `legacy_prefix` is not empty.
    - `wants(&self, topic: &str) -> bool`.
    - `on_message(self: &Arc<Self>, topic: &str, payload: &[u8], retain: bool)`.
    - `on_connected(&self)` and `on_subscribed(self: &Arc<Self>)`.
    - `submit(self: &Arc<Self>, source: &str, payload: &serde_json::Value) -> Result<String, Rejected>`: returns the alert id.
    - `history_topic(&self) -> String` and `responses_topic(&self) -> String`.
    - `status(&self) -> PushStatus`.
    - `spawn_retry_worker(self: &Arc<Self>) -> JoinHandle<()>`.

**Rules this task pins:**
- **Starting state.** The service begins not loaded and not synced.
  - `on_connected` sets synced to false, bumps a connection epoch and clears the tokens. Once loaded, it also trims the history and publishes it retained.
  - `on_subscribed` does nothing when synced or when a settle timer is already pending. Otherwise it spawns a 2 s settle timer tagged with the epoch. A timer whose epoch is stale does nothing.
- **Retained history before the first load.** While not loaded, a retained `<p>/history` is kept as the candidate, and a retained `<legacy>/history` as the seed. A non-retained `<p>/history` (the service's own echo) is always ignored.
- **The first settle:**
  1. Adopt the candidate, or else the seed, or else start empty.
  2. Mark it loaded and trim it.
  3. Apply the held alerts in arrival order.
  4. Publish `<p>/history` retained.
  5. Mark synced, then send the held alerts.
- **Notifications before the first load** are only held. Nothing is published or sent.
- **Notifications once loaded** are applied to the history and published. If synced, they are sent; with no tokens, a log line says "kept in history only". If not synced, they are queued with `tokens: None`.
- **Without a transport,** alerts are applied and published but never sent or queued, and the state is `Off`.
- **FCM results:**
  - An invalid token gets an empty retained `Publish` on every topic in `topics_for_token`, and those entries are removed.
  - Transient tokens are re-queued, with only the failing tokens.
  - A message error is logged once and dropped.
- **The retry worker** ticks every `retry_interval` while synced. Queued alerts are sent only to tokens that are still live.

- [ ] **Step 1: Write the failing tests.** Port the behavioural cases of `tests/test_service.py`, minus the MQTT-handler and persistence ones, with its payloads. Add:
  - `first_boot_never_publishes_an_empty_history`: `on_connected`, then `submit("t", {"v":1,"id":"a","title":"A"})`, then a retained `<p>/history` holding `[{"id":"old",…}]`. Nothing is published until the 2 s settle. After it, the history publish holds `["a","old"]`.
  - `legacy_history_seeds_once_when_new_is_empty`: only a retained `oikos-push/history` arrives, and after the settle the `<p>/history` publish equals it. After a second `on_connected` + settle with the legacy topic replayed, nothing is re-imported.
  - `own_history_echo_is_ignored`.
  - `stale_settle_timer_is_ignored`: `on_connected`, `on_subscribed`, advance 1 s, `on_connected`, advance 1.5 s, and the state is still `Syncing`.
  - `same_token_on_legacy_and_new_topic_is_sent_once_and_cleared_on_both`: the fake transport is called once for `t1`. It answers `Invalid`, so empty retained publishes go to both token topics.
  - `no_transport_keeps_history_and_reports_off`.
  - `transient_failure_is_retried_for_failed_tokens_only`.
  - `submit_rejects_invalid_payload`: `{"v":1}` gives `Err`.
- [ ] **Step 2: Run `cargo test -p oikos-push --test service`.** Expected: FAIL.
- [ ] **Step 3: Implement `service.rs`.** State lives in a `std::sync::Mutex`; never hold it across an `.await`. Sends run in `tokio::spawn`ed tasks.
- [ ] **Step 4: Run the tests.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(push): push service with first-sync history adoption`.

### Task 6: Hub wiring for push

**Files:**
- Modify:
  - `crates/oikos-os/Cargo.toml` (`oikos-push`).
  - `crates/oikos-os/src/config.rs`.
  - `crates/oikos-core/src/registry/mod.rs:41-47` (add `"push"` to `default_reserved`).
  - `crates/oikos-os/src/mqtt.rs`: `static_filters` takes the push filters; `poll_loop` calls `hub.on_connected()` on ConnAck and `hub.on_suback()` on `Packet::SubAck`; drop the `PushConfig` parameter of `start` and `outbound`.
  - `crates/oikos-os/src/hub.rs`.
  - `crates/oikos-os/src/main.rs`.
  - `crates/oikos-os/src/http/api.rs` (`GET /push`).
  - `oikos.example.toml`, `README.md`.
- Test: `crates/oikos-os/tests/hub.rs`, `crates/oikos-os/tests/config.rs`, `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Consumes: `PushService` and its API (Task 5); `HttpFcm` and `ServiceAccount` (Task 3).
- Produces:
  - `PushConfig { prefix: "oikos/push", legacy_prefix: "oikos-push", credentials: Option<PathBuf>, retry_interval: 60, max_retries: 10, history_size: 100, history_days: 7 }` with `#[serde(default, deny_unknown_fields)]`, so `history_topic` / `responses_topic` fail at load with serde's "unknown field" error.
  - In `STRING_ENV_KEYS`, `push.history_topic` / `push.responses_topic` become `push.prefix`, `push.legacy_prefix` and `push.credentials`.
  - On the hub:
    - `Hub::push(&self) -> &Arc<PushService>`: the hub always builds one; the transport is `None` without credentials.
    - `Hub::on_connected(&self)` and `Hub::on_suback(&self)`.
    - `Hub::notify(&self, source: &str, payload: &serde_json::Value) -> Result<String, oikos_push::Rejected>`.
  - `GET /api/v1/push` answers `200` with `PushStatus` JSON.
  - `main.rs` loads `ServiceAccount` when `credentials` is set. On error it logs a `warn` naming the path and runs without a transport. It then calls `hub.push().spawn_retry_worker()` and aborts that handle on shutdown.

- [ ] **Step 1: Write the failing tests:**
  - `config.rs`: `push_defaults_are_new_topics` and `old_push_keys_are_rejected`. The second loads a TOML holding `[push] history_topic = "x"`, gets `Err`, and checks the message contains `history_topic`.
  - `hub.rs`:
    - `push_history_topic_feeds_alert_store`: the existing history tests now use `oikos/push/history`.
    - `push_topics_are_not_discovery`: ingesting `oikos/push/tokens/a` adds no ignored entry.
    - `notify_publishes_history_after_sync`: `on_connected`, `on_suback`, advance 2 s with paused time, `notify("flow", {"v":1,"id":"x","title":"T"})`, and the publisher receives a retained `oikos/push/history` whose JSON has id `x`.
    - `alert_action_replies_on_new_responses_topic`.
  - `http.rs`: `push_status_reports_off_without_credentials` expects `{"state":"off",…}`.
- [ ] **Step 2: Run `cargo test -p oikos-os`.** Expected: FAIL.
- [ ] **Step 3: Implement the wiring.**
  - `ingest_message` calls `push.on_message` for every topic where `push.wants(topic)` is true, then falls through.
  - `ingest` compares `push.history_topic()` where it compared `history_topic` before.
  - The `history_topic` / `responses_topic` fields of `Hub` are replaced by the push service's getters.
  - Update `oikos.example.toml`'s `[push]` section and the README's push section, including the §8 switch-over with the ACL checklist from spec §8 step 0. Delete the stale `oikos/history` wording.
- [ ] **Step 4: Run the gate** (Global Constraints). Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(os): hub owns push — new topics, FCM service, /api/v1/push`.

### Task 7: `oikos-frigate` bridge

**Files:**
- Create: `crates/oikos-frigate/Cargo.toml`, `crates/oikos-frigate/src/lib.rs`
- Modify: `crates/oikos-os/src/config.rs` (`FrigateConfig` gains these fields with these defaults: `media_url: Option<String>`, which falls back to `url`; `bridge: false`; `labels: Vec<String>`, `cameras: Vec<String>` and `zones: Vec<String>`, all empty meaning all/any; `cooldown: 0`; `severity: "warning"`; `ttl: 43200`; `topic_prefix: "frigate"`. Its `Debug` keeps masking the password), `crates/oikos-os/src/hub.rs`, `crates/oikos-os/src/mqtt.rs` (subscribe `<topic_prefix>/events` when the bridge is on), `oikos.example.toml`, `README.md`
- Test: in-crate `mod tests`, and `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Produces:
  - `pub struct FrigateSettings { pub media_url: String, pub labels, pub cameras, pub zones: BTreeSet<String>, pub cooldown: u64, pub severity: String, pub ttl: u32 }`.
  - `pub struct FrigateBridge`, built with `FrigateBridge::new(settings)`, and `fn on_event(&mut self, payload: &[u8], now: u64) -> Option<serde_json::Value>`. It returns a v1 alert payload.
- Hub: when `[frigate] bridge` and a URL are both set, `ingest_message` on `<topic_prefix>/events` runs the bridge (held in a `Mutex`), and any payload goes to `hub.notify("frigate", &v)`.

- [ ] **Step 1: Write the failing tests.** Port `tests/test_frigate_bridge.py` (16) one for one, including the `loc` (`frigate.object`) cases. Add a hub test, `frigate_event_becomes_history_alert_when_bridge_on`, and its mirror, `frigate_events_ignored_when_bridge_off`.
- [ ] **Step 2: Run `cargo test -p oikos-frigate -p oikos-os`.** Expected: FAIL.
- [ ] **Step 3: Implement `lib.rs`** as a port of `frigate_bridge.py`'s `Bridge.qualifies`, `alert`, `handle` and `_cooling_down`, with the clock passed in. Then do the hub wiring.
- [ ] **Step 4: Run the gate.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(frigate): Frigate event bridge in the hub`.

### Task 8: `oikos-watchdog` crate

**Files:**
- Create: `crates/oikos-watchdog/Cargo.toml`, `crates/oikos-watchdog/src/lib.rs`, `crates/oikos-watchdog/src/gateway.rs`, `crates/oikos-watchdog/src/acl.rs`, `crates/oikos-watchdog/src/tail.rs`
- Modify: `crates/oikos-os/src/config.rs` (`WatchdogConfig`, with `gateway: { enabled: false, prefix: "encryption-gateway/health" }` and `acl: { enabled: false, source: Topic | File (default Topic), log_topic: "$SYS/broker/log/#", log_file: Option<PathBuf>, cooldown: 300 }`; an ACL watchdog enabled with `source = "file"` but no `log_file` is a config error), `crates/oikos-os/src/hub.rs`, `crates/oikos-os/src/mqtt.rs` (subscribe `<prefix>/+` and `log_topic` when enabled), `crates/oikos-os/src/main.rs` (spawn the tail task for the file source), `oikos.example.toml`, `README.md`
- Test: in-crate `mod tests`; `crates/oikos-watchdog/tests/tail.rs`

**Interfaces:**
- Produces:
  - `pub struct GatewayWatchdog`, built with `new(prefix: String)`, and `fn on_message(&mut self, topic: &str, payload: &[u8], now: u64) -> Option<serde_json::Value>`.
  - `pub struct AclWatchdog`, built with `new(cooldown: u64)`, and `fn on_line(&mut self, line: &str, now: u64) -> Option<serde_json::Value>`.
  - `pub fn tail(path: PathBuf, lines: mpsc::Sender<String>) -> JoinHandle<()>`. It starts at the end of the file, polls every 1 s, and reopens from the start when the file shrinks (rotation).
- Hub: watchdog payloads go to `hub.notify("system", &v)`.

- [ ] **Step 1: Write the failing tests.**
  - Port `tests/test_gateway_health_watchdog.py` (15) and `tests/test_broker_acl_watchdog.py` (17), the latter covering both Mosquitto line shapes, the cooldown and the `(+N more in last Xs)` suffix.
  - `tail.rs`:
    - `tail_emits_appended_lines_only`: lines written before start are skipped, and two appended lines arrive.
    - `tail_follows_truncation`.
- [ ] **Step 2: Run `cargo test -p oikos-watchdog`.** Expected: FAIL.
- [ ] **Step 3: Implement `gateway.rs` and `acl.rs`** from the two Python bridges, plus `tail.rs`. Then wire the hub: the topic source routes `$SYS/broker/log/#` payloads, one line per message, to `AclWatchdog::on_line`; the file source feeds lines from `tail` through a task that locks the watchdog.
- [ ] **Step 4: Run the gate.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(watchdog): gateway health and broker ACL watchdogs in the hub`.

### Task 9: Flow `notify` node

**Files:**
- Modify:
  - Core: `crates/oikos-core/src/flow/model.rs` (`NodeKind::Notify(NotifyParams)` tagged `"notify"`; `type_name`; not a trigger, no ports) and `crates/oikos-core/src/flow/validate.rs`.
  - Engine: `crates/oikos-os/src/flows/engine.rs`.
  - Web: `crates/oikos-web/src/flow_picker.rs`, `flow_edit.rs`, `flow_text.rs` (role `Action`), `flows.rs`, `icons.rs` and `screens/flow_canvas/settings.rs`. Mirror every `NodeKind::FlowEnable` arm in these files.
  - `README.md` (Flows).
- Test: `model.rs` and `validate.rs` in-file tests; `crates/oikos-os/tests/flows.rs`; the web `flow_text.rs` tests

**Interfaces:**
- Consumes: `Hub::notify` (Task 6).
- Produces: `pub struct NotifyParams { pub title: String, pub body: Option<String>, pub channel: Option<String>, pub severity: Option<String>, pub entity_id: Option<String>, pub image_url: Option<String>, pub silent: Option<bool>, pub id: Option<String>, #[serde(flatten)] pub extra: BTreeMap<String, Value> }`.

- [ ] **Step 1: Write the failing tests:**
  - `notify_node_round_trips`: `{"id":"n","type":"notify","params":{"title":"Door open"}}`.
  - `validate_rejects_empty_notify_title`.
  - `validate_rejects_unknown_notify_channel`: `"pager"` is not one of the seven channels; the same check applies to severity.
  - In `tests/flows.rs`:
    - `notify_step_submits_v1_alert`: the history publish holds an alert with `source == "flow"`, `id == "flow-<flow>-<run>"` and the given title.
    - `notify_with_fixed_id_replaces_previous`.
  - Web: `notify_text` renders `Notify "Door open"`.
- [ ] **Step 2: Run `cargo test --workspace`.** Expected: FAIL.
- [ ] **Step 3: Implement.** The engine builds `{"v":1, "id", "title", …}` from the params. The defaults are `channel` `misc`, `severity` `info` and id `flow-<flow id>-<run id>`. It calls `hub.notify("flow", &v)`. `Ok(id)` gives `(StepOutcome::Ok, format!("sent {id}"), None)`; `Err(r)` gives `failed(r.reason)`. Web: the ＋Add picker entry is "Notify", in the actions group. The parameter editor has fields for title, body, channel (select), severity (select) and silent (checkbox).
- [ ] **Step 4: Run the gate, and also `trunk build` in `crates/oikos-web`.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(flows): notify node`.

### Task 10: MCP `send_notification` and `list_alerts`

**Files:**
- Modify: `crates/oikos-os/src/mcp/params.rs`, `crates/oikos-os/src/mcp/tools.rs`, `crates/oikos-os/src/mcp/mod.rs` (the server instructions: add both tools and the `notify` node to the flow node list), `README.md` (MCP tools table)
- Test: `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: `Hub::notify`, `Hub::alerts()`.
- Produces:
  - `SendNotificationParams`, with the same fields as `NotifyParams` minus `extra`.
  - `ListAlertsParams { limit: Option<usize> (default 20), channel: Option<String> }`.
  - The `send_notification` tool returns `{ "id": … }` or a tool error carrying the rejection reason. `list_alerts` returns `Vec<AlertView>`, newest first.

- [ ] **Step 1: Write the failing tests:**
  - `send_notification_tool_returns_id`.
  - `send_notification_rejects_missing_title`.
  - `list_alerts_filters_by_channel_and_limits`.
  - Extend the instructions test (whatever `tests/mcp.rs` already asserts about the tool list) to include both names.
- [ ] **Step 2: Run `cargo test -p oikos-os --test mcp`.** Expected: FAIL.
- [ ] **Step 3: Implement** both tools in the style of the existing ones (`#[tool(name = …)]`, with `read_only_hint` on `list_alerts`).
- [ ] **Step 4: Run the gate.** Expected: PASS.
- [ ] **Step 5: Commit.** Message: `feat(mcp): send_notification and list_alerts`.
