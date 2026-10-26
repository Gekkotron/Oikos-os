# Camera and Lock Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Offer Lock and Camera widgets on dashboards. The Lock widget shows the state, has one button, and asks before unlocking. The Camera widget shows 1–4 live Frigate cameras per tile.

**Architecture:**
- **Part A (PR 1)** adds a `LockCard` (`cards/lock.rs`) driven by a pure `lock_face()` function. It is used on Home, by the dashboard widget and from a new library section.
- **Part B (PR 2)** adds:
  - a `WidgetConfig::Cameras` variant;
  - a `camera` field on alerts;
  - three hub routes (`/api/v1/cameras`, `/cameras/{id}/latest.jpg`, `/cameras/{id}/stream`) behind `[frigate] live`, served through a new `MediaProxy::fetch_camera`;
  - a web `CameraWidget`. Tiles poll `latest.jpg` about once per second, the full-screen view plays MJPEG, and a pure `camera_layout()` picks the layout.

**Tech Stack:** Rust workspace: axum + reqwest hub (`oikos-os`), Leptos 0.8 wasm web (`oikos-web`), shared `oikos-core` / `oikos-api`, `oikos-frigate`, `oikos-push`.

**Spec:** `docs/superpowers/specs/2026-10-26-camera-lock-widgets-design.md`

## Global Constraints

- The project is generic. No real camera, room, host or IP names in tracked files. Use `front_door`, `garden` and "Front Door".
- Every user-visible string goes through the i18n modules in English and French. The `crates/oikos-web/tests/bare_text.rs` check must stay green.
- `[frigate] live` defaults to `false`. The frame routes answer 403 while it is off.
- `media::is_event_media` and `MediaProxy::fetch` stay unchanged. Camera frames use a separate method.
- Frigate paths the hub may fetch for cameras: `/api/config` (JSON, never passed through), `/api/{id}/latest.jpg` and `/api/{id}` (MJPEG). Nothing else.
- The camera list is cached about 60 s. A missing id triggers at most one refresh per 10 s.
- `h` is clamped to `1..=1080` on the hub. The web asks for the displayed height × `devicePixelRatio`, capped at 720.
- Commits use `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …` and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Before a build, check `df -h` and the size of `~/dev-scratch/cargo-target`. If the target is over 5 GB, run `cargo clean -p <crate>` for this project's crates. When free space is under 5 GB, run per-crate tests only.
- CI runs `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `wasm-pack test --headless --chrome` on `crates/oikos-web`. Run clippy before every push.
- **PR 1** (Part A) is the current branch `feat/camera-lock-widgets`, which already holds the spec and this plan. **PR 2** (Part B) goes on a new branch `feat/camera-widget` from `origin/main` after PR 1 merges, targeting `main`. Never stack the two PRs.

## Review Focus

1. **Frigate down or unreachable.** `GET /cameras` answers 200 with an empty list, never a 5xx. The frame routes answer 502. A tile keeps its last frame and shows an "unavailable" note instead of a broken image. Tested in B3 (routes) and B4 (`next_poll_delay` backs off on `Failed`).
2. **A camera removed from Frigate but still in a widget.** The frame route answers 404. The tile shows "Camera not found" and stops polling that camera. Tested in B3 (route), B4 (`next_poll_delay`) and B5 (`missing_camera_shows_not_found`).
3. **A camera id with characters Frigate never uses** (`../config`, a space, `%2F`). The hub answers 404 without any request reaching Frigate. Tested in B3 by checking the fake's request log.
4. **The lock changes state while "Unlock?" is open** (someone locks or unlocks it by hand). Confirming sends nothing unless the lock still reads Locked, and the question closes. Tested in A1 with `unlock_still_wanted`.
5. **A frame fetch that keeps failing.** The tile backs off: 1 s, 2 s, 5 s, then every 10 s. It never retries in a tight loop. Tested in B4 with `next_poll_delay`.

---

## Part A — Lock widget (PR 1, branch `feat/camera-lock-widgets`)

### Task A1: Lock face logic and text

**Files:**
- Create: `crates/oikos-web/src/cards/lock.rs` (pure part only in this task)
- Modify: `crates/oikos-web/src/cards/mod.rs` (add `pub mod lock;`)
- Modify: `crates/oikos-web/src/i18n/security.rs` (new keys)

**Interfaces:**
- Produces (in `cards::lock`):
  - `pub enum LockWord { Locked, Unlocked, Locking, Unlocking, Jammed, Open, Opening, Offline, Unknown }`
  - `pub enum LockAction { Lock, Unlock }`
  - `pub struct LockFace { pub word: LockWord, pub action: Option<LockAction>, pub warn: bool }`. `action: None` means the button is shown disabled.
  - `pub fn lock_face(state: &EntityState) -> LockFace`. It reads `StateValue::Generic`, field `"state"`, `GenericValue::Text` holding the type table's values (`locked`, `unlocked`, `locking`, `unlocking`, `jammed`, `open`, `opening`). `Availability::Offline` wins over any value. A missing or unrecognised value gives `Unknown`.
  - `pub fn lock_command(action: LockAction) -> Command`, which builds `Command::Control { control: "locked".into(), value: Some(Value::Bool(action == LockAction::Lock)) }`.
  - `pub fn unlock_still_wanted(state: &EntityState) -> bool`, which is `lock_face(state).action == Some(LockAction::Unlock)`.
- Produces (in `i18n::security`):
  - `SecurityText::VerbLock` ("Lock" / "Verrouiller") and `VerbUnlock` ("Unlock" / "Déverrouiller");
  - one key per `LockWord`: `LockLocked` … `LockUnknown`. EN: "Locked", "Unlocked", "Locking…", "Unlocking…", "Jammed", "Open", "Opening…", "Offline", "Unknown". FR: "Verrouillé", "Déverrouillé", "Verrouillage…", "Déverrouillage…", "Bloqué", "Ouvert", "Ouverture…", "Hors ligne", "Inconnu";
  - `pub fn lock_word(lang: Lang, w: LockWord) -> &'static str`.

- [ ] **Step 1: Write the failing tests** in `cards/lock.rs` `mod tests`:

```rust
#[test]
fn face_per_state() {
    use LockAction::*; use LockWord::*;
    for (raw, word, action, warn) in [
        ("locked", Locked, Some(Unlock), false),
        ("unlocked", Unlocked, Some(Lock), true),
        ("jammed", Jammed, Some(Lock), true),
        ("open", Open, Some(Lock), true),
        ("opening", Opening, None, true),
        ("locking", Locking, None, false),
        ("unlocking", Unlocking, None, false),
        ("weird", Unknown, None, false),
    ] {
        assert_eq!(lock_face(&state(raw, Availability::Online)), LockFace { word, action, warn }, "{raw}");
    }
}

#[test]
fn offline_wins_and_disables() {
    assert_eq!(lock_face(&state("locked", Availability::Offline)),
               LockFace { word: LockWord::Offline, action: None, warn: false });
}

#[test]
fn no_state_is_unknown() {
    assert_eq!(lock_face(&EntityState::default()).word, LockWord::Unknown);
}

#[test]
fn unlock_is_wanted_only_while_locked() {
    assert!(unlock_still_wanted(&state("locked", Availability::Online)));
    assert!(!unlock_still_wanted(&state("unlocked", Availability::Online)));
    assert!(!unlock_still_wanted(&state("locked", Availability::Offline)));
}

#[test]
fn commands_use_the_locked_control() {
    assert_eq!(lock_command(LockAction::Lock),
               Command::Control { control: "locked".into(), value: Some(serde_json::Value::Bool(true)) });
    assert_eq!(lock_command(LockAction::Unlock),
               Command::Control { control: "locked".into(), value: Some(serde_json::Value::Bool(false)) });
}
```

  `state(raw, availability)` is a test helper. It builds an `EntityState` with `StateValue::Generic` holding `"state" → Text(raw)` and the given availability. Add one i18n test that `lock_word(Lang::Fr, LockWord::Jammed) == "Bloqué"`.

- [ ] **Step 2: Run** `cargo test -p oikos-web --lib cards::lock`. Expected: FAIL (the items aren't defined yet).
- [ ] **Step 3: Implement** the items listed under Interfaces. Before implementing, read the choice field's decode in `oikos-core` (`types` / generic state) to confirm the stored text is the table `value` (`locked`), not the raw payload (`LOCKED`). If it is the raw payload, match on the table's `default` words instead, and change the test helper to match.
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib cards::lock i18n::security`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(web): lock card state logic and words`.

### Task A2: `LockCard` and Home dispatch

**Files:**
- Modify: `crates/oikos-web/src/cards/lock.rs` (the component)
- Modify: `crates/oikos-web/src/screens/home.rs:93` (`card_for`: inside `Kind::Generic`, when `entity.component == "lock"`, render `LockCard` before the `generic_placement` match)
- Modify: `crates/oikos-web/src/styles/cards.css` (a `.card--lock-warn` modifier using the existing warning token)
- Test: `crates/oikos-web/tests/lock_card.rs` (new, wasm browser test; copy the setup from `tests/generic_card.rs`)

**Interfaces:**
- Consumes: A1's `lock_face`, `lock_command`, `unlock_still_wanted`, `lock_word`, `SecurityText::{VerbLock, VerbUnlock}`.
- Produces: `#[component] pub fn LockCard(entity: EntityView) -> impl IntoView`.

Model the component on `cards/door.rs`:
- `CardFrame` with `open_entity`;
- `Confirming` + `ConfirmBar` with the question `action_question(lang, SecurityText::VerbUnlock, &entity.name)`;
- the state word in a `card__note` span;
- one `btn`: enabled with the `VerbLock`/`VerbUnlock` text when `action` is `Some`, otherwise `disabled` and showing the state word.

Lock sends straight away. Unlock calls `confirming.ask(&id)`. `on_yes` re-reads the entity from `House::entities` (untracked). It sends `lock_command(Unlock)` only if `unlock_still_wanted`. In every case it calls `confirming.clear()`. An `Effect` clears `confirming` for this id when the face stops offering Unlock.

- [ ] **Step 1: Write the failing browser tests** in `tests/lock_card.rs`:
  - `locked_lock_asks_before_unlocking`: render `card_for` on a locked `lock.x` entity. Click the button and assert that `.confirm__question` reads "Unlock Thing?" and nothing has been sent (`Pending` is empty). Click the confirm's first button and assert that the `lock.x` command is pending.
  - `unlocked_lock_locks_at_once`: click the button and assert that the command is pending with no `.confirm` present.
  - `offline_lock_button_is_disabled`: the button has the `disabled` attribute.
  - `question_closes_when_the_lock_unlocks_by_hand`: open the question, set the entity's state to `unlocked` in `House::entities`, tick, and assert that no `.confirm` is present.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome crates/oikos-web --test lock_card`. Expected: FAIL. If chromedriver isn't available locally, note it, and rely on CI for this step and Step 4.
- [ ] **Step 3: Implement** `LockCard` and the `card_for` branch.
- [ ] **Step 4: Run** the same command. Expected: PASS. Also run `cargo test -p oikos-web --test bare_text`.
- [ ] **Step 5: Commit**: `feat(web): lock card with a question before unlocking`.

### Task A3: Lock widget in the dashboard and the library

**Files:**
- Modify: `crates/oikos-web/src/dashboards/widget.rs:25-37` (add `WidgetKind::Lock => view! { <LockCard entity=entity /> }`; the placeholder arm then serves only `Camera`, until B5)
- Modify: `crates/oikos-web/src/dashboards/library.rs` (add `Section::Locks` between `WaterHeaters` and `Rooms`, with suffix `"locks"`; add `DashText::SectionLocks` "Locks" / "Serrures"; entity rows: `Kind::Generic if e.component == "lock" => (Section::Locks, WidgetKind::Lock)`)
- Modify: `crates/oikos-web/src/i18n/dashboards.rs` (the `SectionLocks` text)
- Modify: `crates/oikos-web/src/styles/widgets.css` (the `dashboard-library__header--locks` / `__item--locks` colour, matching the sibling sections)
- Modify: `docs/users/web-ui.md` § Dashboards (one paragraph on the Lock widget)

**Interfaces:**
- Consumes: A2's `LockCard`.

- [ ] **Step 1: Write the failing tests** in `library.rs` `mod tests`:

```rust
#[test]
fn locks_get_their_own_section() {
    let mut lock = ent("lock.front", "Front door", Kind::Generic);
    lock.component = "lock".into();
    let mut siren = ent("siren.x", "Siren", Kind::Generic);
    siren.component = "siren".into();
    let got = run("", &house(vec![lock, siren]));
    let locks = of(&got, Section::Locks);
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].kind, WidgetKind::Lock);
    assert!(matches!(&locks[0].config, WidgetConfig::Entity { entity_id, .. } if entity_id == "lock.front"));
}

#[test]
fn no_lock_no_lock_section() {
    let got = run("", &house(vec![ent("l1", "Lamp", Kind::Light)]));
    assert!(of(&got, Section::Locks).is_empty());
}
```

  Extend `sections_come_in_display_order` so that it includes a lock entity.
- [ ] **Step 2: Run** `cargo test -p oikos-web --lib dashboards::library`. Expected: FAIL (`Section::Locks` doesn't exist yet).
- [ ] **Step 3: Implement** the changes in the files listed above.
- [ ] **Step 4: Run** `cargo test -p oikos-web --lib`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS, and no warnings.
- [ ] **Step 5: Commit**: `feat(web): lock widget in the dashboard library`. Then push and open PR 1 against `main`, titled "Dashboard Lock widget". The body links the spec and plan, and notes that the Camera widget follows in its own PR.

---

## Part B — Camera widget (PR 2, branch `feat/camera-widget` from `origin/main` after PR 1 merges)

### Task B1: `WidgetConfig::Cameras` and its validation

**Files:**
- Modify: `crates/oikos-core/src/dashboard/model.rs` (the variant and its tests)
- Modify: `crates/oikos-os/src/dashboards/store.rs:160-180` (`validate_widget`)
- Modify: `crates/oikos-web/src/dashboards/config_sheet.rs` (`title_of` and `set_title` arms, so it compiles)

**Interfaces:**
- Produces: `WidgetConfig::Cameras { cameras: Vec<String>, #[serde(default, skip_serializing_if = "Option::is_none")] title: Option<String> }`, placed after `Energy`. `pub const MAX_CAMERAS: usize = 4;` in `dashboard::model`.

- [ ] **Step 1: Write the failing tests.**
  - In `model.rs`: `widget_config_cameras_round_trips`. Decode `{"cameras":["front_door","garden"],"title":"Outside"}` to `Cameras`, serialise it back, and check the JSON is the same.
  - In `store.rs` `mod tests`: `camera_widget_rules`. Assert `validate_widget` returns
    - `Ok` for 1 and 4 cameras;
    - `Err` for 0 cameras, 5 cameras, `["a","a"]`, `[""]`, and a Camera widget with `Entity` config;
    - `Err` for a Lock widget with a `Cameras` config.

    Replace the existing line 275 assertion (Camera with `entity_config()` is ok) with the opposite.
- [ ] **Step 2: Run** `cargo test -p oikos-core dashboard` and `cargo test -p oikos-os --lib dashboards::store`. Expected: FAIL.
- [ ] **Step 3: Implement.** `WidgetKind::Camera` takes `Cameras` with `1..=MAX_CAMERAS` distinct, non-empty ids. Remove it from the `Entity` group. The error messages follow the existing `format!` style.
- [ ] **Step 4: Run** the same commands, plus `cargo check -p oikos-web --target wasm32-unknown-unknown`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(dashboards): camera widget config holds 1 to 4 cameras`.

### Task B2: Alerts carry the Frigate camera id

**Files:**
- Modify: `crates/oikos-frigate/src/lib.rs` (`payload.insert("camera", raw id)`, when present; make `pretty` a `pub fn`)
- Modify: `crates/oikos-push/src/normalize.rs:41-58,285-310` (`Alert.camera: Option<String>`, read from the v1 payload with `opt(payload.get("camera"))`; the legacy path leaves it `None`)
- Modify: `crates/oikos-push/src/history.rs:26-33` (add `("camera", &alert.camera)` to the optional keys)
- Modify: `crates/oikos-api/src/alerts.rs:92-170` (`AlertView.camera: Option<String>`, parsed with `text(object.get("camera"))`)
- Modify: every `AlertView { … }` literal (`oikos-web` `alerts.rs`, `house_state.rs`, `components/alert_row.rs`, `tests/security.rs`) to add `camera: None`

**Interfaces:**
- Produces: `AlertView.camera: Option<String>` and `pub fn oikos_frigate::pretty(name: &str) -> String`.

- [ ] **Step 1: Write the failing tests.**
  - The frigate test at `lib.rs:288` expects `"camera": "front_door"` in the payload.
  - `history.rs`: in the round-trip test at line 283, add `"camera": "front_door"` and include `"camera"` in the optional-key list at line 299.
  - `alerts.rs`: `parse_history` reads `camera`, and an entry without it gives `None`.
- [ ] **Step 2: Run** `cargo test -p oikos-frigate -p oikos-push -p oikos-api`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command, plus `cargo check --workspace --all-targets`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(alerts): frigate alerts carry their camera id`.

### Task B3: Hub camera routes behind `[frigate] live`

**Files:**
- Create: `crates/oikos-api/src/cameras.rs` (export it from `lib.rs`)
- Modify: `crates/oikos-os/src/config.rs:198-240` (`FrigateConfig.live: bool`, default `false`)
- Modify: `crates/oikos-os/src/media.rs` (camera list cache and `fetch_camera`; move the 401-retry loop into a private `get(&self, url: &str, range: Option<&str>)` shared with `fetch`)
- Modify: `crates/oikos-os/src/http/api.rs` (three routes and handlers next to `media`)
- Test: `crates/oikos-os/tests/http.rs` (extend `fake_frigate`, add the tests below), `crates/oikos-os/tests/config.rs` (`live` defaults to false and parses `live = true`)
- Docs: `oikos.example.toml` `[frigate]` (`# live = false  # live frames through the hub: readable by every device on the LAN`), `docs/users/cameras.md` (a "Live view on dashboards" section), `docs/users/security.md:20` (live frames when `live = true`)

**Interfaces:**
- Produces (in `oikos_api::cameras`):
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)] pub struct CamerasView { pub live: bool, pub cameras: Vec<CameraInfo> }`
  - `pub struct CameraInfo { pub id: String, pub name: String }`
- Produces (in `media`):
  - `pub enum CameraFile { Latest, Stream }`
  - `pub fn is_camera_id(id: &str) -> bool`: non-empty, at most 64 characters, `[A-Za-z0-9_-]` only.
  - `impl MediaProxy { pub async fn camera_ids(&self) -> Result<Vec<String>, MediaError>; pub async fn has_camera(&self, id: &str) -> Result<bool, MediaError>; pub async fn fetch_camera(&self, id: &str, file: CameraFile, height: u32) -> Result<Upstream, MediaError> }`
    - `camera_ids` reads `{base}/api/config`, keeps the keys of `cameras` whose `enabled` isn't `false`, and caches them for 60 s.
    - `has_camera` refreshes once on a miss, at most every 10 s.
    - `fetch_camera` builds `{base}/api/{id}/latest.jpg?h={h}&height={h}` or `{base}/api/{id}?h={h}&height={h}&fps=5`. It sends both height spellings because Frigate versions differ. It returns `Forbidden` for an id that fails `is_camera_id`.
- Produces (HTTP):
  - `GET /api/v1/cameras` → `CamerasView`. Names come from `oikos_frigate::pretty`. It returns 200 with an empty list when there is no proxy or Frigate fails.
  - `GET /api/v1/cameras/{id}/latest.jpg?h=` and `GET /api/v1/cameras/{id}/stream?h=` answer as follows, checked in this order:
    1. 403 when `live` is off or there is no Frigate;
    2. 404 when `!is_camera_id(id)` or `has_camera` returns `Ok(false)`;
    3. 502 when `has_camera` returns an error or the frame fetch fails;
    4. otherwise Frigate's status, `content-type` and streamed body, with `cache-control: no-store`.

    `h` defaults to 360 and is clamped to `1..=1080`.

- [ ] **Step 1: Extend `fake_frigate`.**
  - `/api/config` returns `{"cameras":{"front_door":{"enabled":true},"garden":{},"old":{"enabled":false}}}` and counts its reads.
  - `/api/front_door/latest.jpg` and `/api/garden/latest.jpg` require the cookie (reuse the `snapshot` handler) and record the query string.
  - `/api/front_door` returns `content-type: multipart/x-mixed-replace; boundary=frame` with one `--frame` part.
  - A request log (`Vec<String>` of paths) is used by Review Focus 3.

  Then write the failing tests:
  - `cameras_lists_enabled_frigate_cameras`: with `live = true`, the response is `{"live":true,"cameras":[{"id":"front_door","name":"Front Door"},{"id":"garden","name":"Garden"}]}`. A second call doesn't re-read `/api/config`.
  - `cameras_without_frigate_is_empty`: the response is `{"live":false,"cameras":[]}`.
  - `camera_frames_need_live`: with `live = false`, `latest.jpg` answers 403 and the fake saw no frame request.
  - `camera_latest_passes_through_with_login`: the response is 200, `image/jpeg`, body `JPEG`, and `cache-control: no-store`. The fake saw `h=720&height=720` for `?h=720`, and `h=1080` for `?h=5000`.
  - `camera_stream_is_multipart`: the response is 200 with a `content-type` that starts with `multipart/x-mixed-replace`.
  - `unknown_or_odd_camera_ids_never_reach_frigate`: `nope`, `..%2Fconfig`, `a%20b` and `old` all answer 404. The request log holds no path besides `/api/login` and `/api/config`.
  - `frigate_down_lists_nothing_and_frames_fail`: point `url` at a closed port. `/cameras` answers 200 with an empty list, and `latest.jpg` answers 502.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test http camera` and `cargo test -p oikos-os --test config`. Expected: FAIL.
- [ ] **Step 3: Implement** the config field, `media.rs` additions, routes and docs.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test http`, which covers the existing `media_proxy_*` tests (they must stay green), and `cargo test -p oikos-os --lib media`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(hub): live camera frames behind [frigate] live`.

### Task B4: Web camera data and pure helpers

**Files:**
- Create: `crates/oikos-web/src/cameras.rs` (export it from `lib.rs`)
- Modify: `crates/oikos-web/src/api/core.rs` (`pub async fn cameras() -> Result<CamerasView, ApiError>`, a GET of `/api/v1/cameras` following the existing `check` + `read_json` pattern)
- Modify: `crates/oikos-web/src/app.rs:84` (`provide_context(CameraList::default())` and `load_cameras` at start-up)

**Interfaces:**
- Produces (in `cameras`):
  - `#[derive(Clone, Copy, Default)] pub struct CameraList(pub RwSignal<CamerasView>)`
  - `pub fn load_cameras(list: CameraList)`, which spawns the fetch and keeps the old value on error.
  - `pub enum CameraLayout { Single, Carousel, Stack, Grid }`
  - `pub fn camera_layout(span: Span, count: usize) -> CameraLayout`. The rules: `count <= 1` → Single; `2x2` with 2 → Stack; `2x2` with 3–4 → Grid; any other span with ≥ 2 → Carousel.
  - `pub fn latest_alert_for<'a>(alerts: &'a [AlertView], camera: &str, now: u64, within: u64) -> Option<&'a AlertView>`. It returns the newest alert whose `camera == Some(camera)` (or, when `camera` is `None`, whose `title == oikos_frigate::pretty(camera)` and whose channel is camera), with `sent_at + within >= now`.
    - Check that `oikos-web` may depend on `oikos-frigate` (it's wasm-safe: serde_json only). If it isn't, copy `pretty` into `cameras.rs` with a comment pointing to the source.
    - Callers pass `within = 86_400` for the badge and `u64::MAX` for the live-off snapshot.
  - `pub enum FrameOutcome { Loaded, Failed, NotFound }`
  - `pub fn next_poll_delay(outcome: FrameOutcome, failures_in_a_row: u32) -> Option<u32>` (milliseconds). `Loaded` → `Some(1000)`. `Failed` → `Some` of 1000, 2000, 5000 for the 1st, 2nd and 3rd failure, then 10 000. `NotFound` → `None` (stop).
  - `pub fn frame_height(css_px: f64, dpr: f64) -> u32`, which computes `(css_px * dpr).round()` clamped to `1..=720`.

- [ ] **Step 1: Write the failing tests** in `cameras.rs`. Cover:
  - every row of the spec's layout table, plus the `1x1` fallback (Single or Carousel);
  - `latest_alert_for`: the camera field match, the title fallback, the 24 h cutoff at the exact boundary, newest-first, and other cameras ignored;
  - the `next_poll_delay` sequence, including `failures_in_a_row = 7` → `Some(10000)`;
  - `frame_height(180.0, 2.0) == 360` and `frame_height(600.0, 3.0) == 720`.
- [ ] **Step 2: Run** `cargo test -p oikos-web --lib cameras`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(web): camera list and camera tile helpers`.

### Task B5: `CameraWidget` and the full-screen viewer

**Files:**
- Create: `crates/oikos-web/src/dashboards/camera_widget.rs` (the tile, one `CameraFrame` per camera, the carousel, the badge)
- Create: `crates/oikos-web/src/components/camera_viewer.rs` (full screen)
- Modify: `crates/oikos-web/src/dashboards/widget.rs` (a `(WidgetKind::Camera, WidgetConfig::Cameras { cameras, title })` arm, and removal of the now-unused placeholder arm)
- Modify: `crates/oikos-web/src/dashboards/mod.rs`, `components/mod.rs`, `styles/widgets.css`
- Modify: `crates/oikos-web/src/i18n/dashboards.rs`: `WidgetText::Live` ("Live" / "En direct"), `LiveOff` ("Live view off" / "Direct désactivé"), `NoEvents` ("No events yet" / "Aucun événement"), `CameraNotFound` ("Camera not found" / "Caméra introuvable"), `CameraUnavailable` ("Camera unavailable" / "Caméra indisponible"), `PrevCamera` / `NextCamera` (aria labels "Previous camera" / "Caméra précédente", "Next camera" / "Caméra suivante"), `Close` (reuse `DashText::Close` if present)
- Test: `crates/oikos-web/tests/camera_widget.rs` (new, wasm)

**Interfaces:**
- Consumes: B4's `CameraList`, `camera_layout`, `latest_alert_for`, `next_poll_delay`, `frame_height`, `FrameOutcome`; B1's `WidgetConfig::Cameras`; `House::alerts`; `i18n::format::ago`; the `IntersectionObserver` pattern in `components/sparkline.rs:93`; and the page-visibility listener in `screens/energy.rs:346-410` (reuse it, don't add a second listener per tile).
- Produces:
  - `#[component] pub fn CameraWidget(cameras: Vec<String>, span: Span) -> impl IntoView`
  - `#[component] pub fn CameraViewer(cameras: Vec<String>, start: usize, on_close: Callback<()>) -> impl IntoView`

**Behaviour:**
- **Frame source.** While `CameraList.live` is true, a frame is `/api/v1/cameras/{id}/latest.jpg?h={frame_height}&t={counter}`.
- **Polling.** The next request is scheduled only after the previous `load` or `error`, using `next_poll_delay`.
- **Swapping frames.** The new frame is swapped in only once it has loaded. Load it in a detached `HtmlImageElement` (add the web-sys feature if it's missing), then set the visible `src`.
- **Errors.** An `<img>` `error` event carries no status. On an error, issue one `gloo_net` GET of the same URL: a 404 maps to `NotFound`, anything else to `Failed`.
- **Pausing.** Polling stops while the tile isn't intersecting the viewport or the page is hidden, and resumes on show.
- **Live off.** When `live` is false, show `latest_alert_for(.., u64::MAX)`'s `image_url` with the `LiveOff` note. With no such alert, show the camera icon and `NoEvents`.
- **Badge.** It reads `"{body} · {ago}"` and links to `/alerts/{id}`. Clicking it stops propagation.
- **Carousel.** Arrows (buttons with aria labels) and swipe (pointer down/up with dx > 40 px) move through the cameras, and dots show the position. Only the visible camera polls.
- **Full screen.** A click on a picture outside edit mode opens `CameraViewer`. Use the existing sheet or overlay pattern if one fits; otherwise a fixed overlay. It shows `<img src="/api/v1/cameras/{id}/stream?h={frame_height}">` when live is on, the snapshot when it's off, arrows to step through cameras, and close on a button, Escape or Back. Closing sets the `src` to `""` before unmounting.
- **Edit mode.** A click does what the other widgets do (the grid's overlay already catches it; check `grid.rs`).

- [ ] **Step 1: Write the failing browser tests** in `tests/camera_widget.rs`. Provide `CameraList` with `live: true` and two cameras, and `House` with one alert for `front_door` 10 minutes old.
  - `wide_tile_shows_one_camera_with_arrows`: `2x1` with two cameras shows one `img` and two arrow buttons. Clicking next changes the caption to "Garden".
  - `big_tile_with_three_cameras_is_a_grid_with_an_empty_square`: `2x2` with three cameras shows four grid cells, one of them empty.
  - `badge_links_to_the_alert`: the badge `href` is `/alerts/<id>`.
  - `live_off_shows_the_alert_snapshot`: with `live: false`, the `img` `src` is the alert's `image_url` and the note reads "Live view off".
  - `missing_camera_shows_not_found`: the test runner has no hub, so frame requests 404. After the first error the tile shows "Camera not found" and schedules no further frame request.
  - `viewer_streams_and_closes_on_escape`: a click opens `.camera-viewer` with an `img` `src` containing `/stream?h=`. Escape removes it.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome crates/oikos-web --test camera_widget`. Expected: FAIL. If chromedriver isn't available locally, rely on CI and say so in the task report.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the same command, plus `cargo test -p oikos-web --test bare_text` and `cargo clippy -p oikos-web --target wasm32-unknown-unknown -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(web): live camera widget and full-screen viewer`.

### Task B6: Cameras in the library and the config sheet

**Files:**
- Modify: `crates/oikos-web/src/dashboards/library.rs`:
  - `Section::Cameras`, after `Locks`, with suffix `"cameras"`;
  - `entries()` gains a `cameras: &[CameraInfo]` parameter;
  - one row per camera (`WidgetKind::Camera`, `Cameras { cameras: vec![id], title: None }`);
  - an "All cameras" row (the first `MAX_CAMERAS` ids) when there are ≥ 2 cameras;
  - `make_widget_with_id`'s span comes from the new `initial_span`;
  - `WidgetLibrary` calls `load_cameras` on mount.
- Modify: `crates/oikos-web/src/dashboards/config_sheet.rs`: for a `Cameras` config, show an ordered list. Each row has the name, up and down buttons (reuse `shifted_order`) and a remove button, disabled when it's the last one. Below it, an "Add camera" `select` lists the cameras from `CameraList` not yet in the list, hidden at `MAX_CAMERAS`. `title_of` and `set_title` already handle `Cameras` (B1).
- Modify: `crates/oikos-web/src/i18n/dashboards.rs`: `DashText::SectionCameras` ("Cameras" / "Caméras"), `AllCameras` ("All cameras" / "Toutes les caméras"), `AddCamera` ("Add camera" / "Ajouter une caméra"), `CamerasLabel` ("Cameras" / "Caméras"), `MoveUp` / `MoveDown` / `Remove` (reuse the existing keys if the dashboard sheet already has them).
- Modify: `crates/oikos-web/src/styles/widgets.css` (the cameras section colour)
- Modify: `docs/users/web-ui.md` § Dashboards (the Camera widget: layouts by size, badge, full screen, the `live` switch link)
- Modify: `TODOS.md` (remove the `Camera` / `Lock` P1 line, and the now-empty "P1" heading under Dashboards)

**Interfaces:**
- Consumes: B1's `MAX_CAMERAS`; B3's `CameraInfo`; B4's `CameraList`, `load_cameras`.
- Produces: `fn initial_span(kind: WidgetKind, config: &WidgetConfig) -> Span`. A `Cameras` config with ≥ 2 ids gives `Span::X2x2`. Otherwise it's the first allowed span. Existing callers of `entries(...)` and the tests' `run(...)` pass `&[]`.

- [ ] **Step 1: Write the failing tests** in `library.rs` `mod tests`:
  - `cameras_section_lists_each_camera_and_all`: two cameras produce three rows under `Section::Cameras`. The "All cameras" row's config holds both ids.
  - `one_camera_has_no_all_row`.
  - `no_cameras_no_section`.
  - `all_cameras_keeps_the_first_four`: five cameras give four ids.
  - `initial_span_is_big_for_several_cameras`: `initial_span(Camera, Cameras{2 ids}) == X2x2`, `initial_span(Camera, Cameras{1 id}) == X2x1`, and `initial_span(Light, Entity) == X1x1`.

  In `config_sheet.rs` tests, check that moving a camera down swaps it via `shifted_order`, and that a removal is refused when only one camera is left. Put the removal check in a pure `fn remove_camera(ids: &mut Vec<String>, id: &str) -> bool`.
- [ ] **Step 2: Run** `cargo test -p oikos-web --lib dashboards`. Expected: FAIL.
- [ ] **Step 3: Implement** the changes in the files listed above.
- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and the two wasm test files from B5 and A2. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(web): cameras in the widget library and config sheet`. Then push and open PR 2 against `main`, titled "Dashboard Camera widget (live Frigate cameras)". The body says that `[frigate] live = true` is needed for live frames, and that the hub and web must be rebuilt on the live host.
