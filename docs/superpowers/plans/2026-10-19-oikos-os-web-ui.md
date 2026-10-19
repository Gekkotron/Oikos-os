# Oikos-os Web UI (Leptos CSR) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Leptos 0.8 client-side WebAssembly UI, built with Trunk and embedded in the `oikos-os` binary, that renders the hub's `/api/v1/snapshot` + `/ws` into a beautiful dashboard (Home + Entity sheet + Devices + Settings), with optimistic commands and a live override editor.

**Architecture:** `oikos-web` becomes a `wasm32` binary crate: `src/main.rs` mounts a Leptos app. A pure, native-testable `HouseState` applies `WsEvent`s to in-memory `BTreeMap`s; a thin `House` wraps that state behind Leptos `RwSignal`s. A single WebSocket client with exponential-backoff reconnect feeds `House`. REST calls (`POST /command`, `PATCH /override`, `DELETE /override`, `PUT /areas`) go through `src/api.rs`. Styling is hand-written CSS tokens on `:root`, Night Glass by default, Warm Minimal under `[data-theme="light"]` / `prefers-color-scheme: light`. The hub replaces its placeholder `static_ui.rs` with a `rust-embed` service rooted at `crates/oikos-web/dist/`, with an SPA fallback that answers `index.html` for any unknown non-`/api`, non-`/ws`, non-`/mcp` path. No Node, no JavaScript toolchain, no npm.

**Tech Stack:** Rust 2024 (stable, wasm32-unknown-unknown target), Leptos 0.8 (CSR), leptos_router 0.8, Trunk 0.21, gloo-net 0.6 (WebSocket + fetch), gloo-timers 0.3, web-sys (Storage, Window, matchMedia), wasm-bindgen 0.2, wasm-bindgen-futures 0.4, console_error_panic_hook 0.1, serde-wasm-bindgen 0.6, rust-embed 8 (hub side), wasm-bindgen-test 0.3 (one smoke test).

**Spec:** `docs/superpowers/specs/2026-10-18-oikos-os-design.md` (§4.5 UI, §7 tokens, §4.3 wire types, §4.6 error handling)

**Seams carried over:** `docs/superpowers/plans/2026-10-18-oikos-os-hub-deferred.md` → "Seams for the UI plan".

## Global Constraints

- Rust 2024 edition, stable toolchain. The web crate is `wasm32-unknown-unknown`; `cargo check -p oikos-web --target wasm32-unknown-unknown` is clean.
- No `unsafe`. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` are clean before every commit.
- No Node, no npm, no JavaScript toolchain, no Google Fonts: Inter and Fraunces are shipped as local WOFF2 under `crates/oikos-web/assets/fonts/`, declared with `@font-face`.
- Workspace `authors = ["Gekkotron <60887050+Gekkotron@users.noreply.github.com>"]`, `license = "Apache-2.0"`. The user's real name appears nowhere.
- Every commit uses `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …` and ends with the trailer `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`. Below, `$GITC` stands for that `git -c … -c …` prefix.
- Wire types come from `oikos-api`: `Snapshot`, `WsEvent`, `EntityView`, `DeviceView`, `AreaView`, `BridgeView`, `IgnoredView`, `ConnectionStatus`, `CommandResponse`, `OverridePatch`, `ApiError`. Commands and overrides use `oikos_core::model::{Command, UiOverride, AreasDocument}`.
- Design tokens are the exact hex and dimensions from spec §7. Light theme switches under `[data-theme="light"]` on `<html>` or under `prefers-color-scheme: light` when the user has not pinned a theme.
- Dev workflow is two-process: `cargo run -p oikos-os -- run` on 8080, `trunk serve` in `crates/oikos-web/` proxying `/api`, `/ws`, `/mcp` to 8080. Release workflow is: `cd crates/oikos-web && trunk build --release`, then `cargo build --release -p oikos-os`; the dist is embedded.
- The UI must honour the hub seams:
  - `state_changed` fires only on a real change (value, attributes or availability); the UI's "last update" label comes from `EntityState::last_update`.
  - An `asLight` switch arrives with `kind: Light`, `capabilities.brightness: false`, `state.value` typed `OnOff` — the light card must render it without a brightness slider and read `OnOff` for its on/off state.
  - `AreaView::entity_count` is the total; `AreaView::primary_count` is what chip badges and card group counts display.
  - A `part_of` child's state belongs to its parent's card when relevant (e.g. a cover's `moving` sensor); it is not shown separately on Home.
  - `oikos_core::model::UNASSIGNED_AREA` ("Unassigned") is an area id; `Snapshot::areas` is the chip order (do not re-sort).
  - A sensor whose last message was `unknown` or `unavailable` arrives as `StateValue::Text` carrying that literal; the UI renders a dash, never the word.
- `send_ha_birth` is not an UI concern; the UI never speaks MQTT.

## Review Focus

1. A WebSocket reconnect must apply the fresh snapshot as a replacement, not merge it over the stale cache; any pre-reconnect queued events older than the snapshot are stale. Pinned in Task 3.
2. A `PATCH /override` 400 (unknown field, out-of-range value) must surface the server's `ApiError::error` text to the user and leave the editor unsubmitted with the pending values intact. Pinned in Task 13.
3. A command that is rejected (`CommandResponse::accepted = false`, 422) or whose publish fails (503) must revert the optimistic UI and show the reason, not silently settle. Pinned in Task 8.
4. A sensor whose state is `StateValue::Text("unknown")` or `StateValue::Text("unavailable")` renders as a dash, never as the literal word. Pinned in Task 6 and reused by Task 10.
5. Reading or writing `localStorage` from a private window or a storage-blocked browser must not panic the UI; theme falls back silently to `prefers-color-scheme`. Pinned in Task 4.

---

### Task 1: Trunk + Leptos CSR scaffold

**Files:**
- Create: `crates/oikos-web/src/main.rs`, `crates/oikos-web/src/app.rs`, `crates/oikos-web/index.html`, `crates/oikos-web/Trunk.toml`, `crates/oikos-web/styles/main.css` (empty), `crates/oikos-web/.gitignore` (`/dist`)
- Modify: `crates/oikos-web/Cargo.toml` (keep `[lib]` as a plain `rlib` holding the modules, add a `[[bin]] name = "oikos-web"` that depends on the lib, add deps), `crates/oikos-web/src/lib.rs` (becomes `pub mod app;` for Task 16's smoke test), `Cargo.toml` workspace (add `leptos`, `leptos_router`, `gloo-net`, `gloo-timers`, `web-sys`, `wasm-bindgen`, `wasm-bindgen-futures`, `console_error_panic_hook`, `serde-wasm-bindgen`, `wasm-bindgen-test` to `[workspace.dependencies]`)

**Interfaces:**
- Produces: a browser-mountable Leptos app that renders the string "Oikos-os" and nothing else. Entry point `pub fn main()`; root component `pub fn App() -> impl IntoView` lives in `src/app.rs`. `index.html` is Trunk's entry.

- [ ] **Step 1: Write workspace deps.** Add to `[workspace.dependencies]`: `leptos = { version = "0.8", features = ["csr"] }`, `leptos_router = { version = "0.8", features = ["csr"] }`, `gloo-net = { version = "0.6", features = ["websocket", "http"] }`, `gloo-timers = { version = "0.3", features = ["futures"] }`, `web-sys = { version = "0.3", features = ["Window", "Storage", "MediaQueryList", "MediaQueryListEvent", "Location"] }`, `wasm-bindgen = "0.2"`, `wasm-bindgen-futures = "0.4"`, `console_error_panic_hook = "0.1"`, `serde-wasm-bindgen = "0.6"`, `wasm-bindgen-test = "0.3"`.
- [ ] **Step 2: Rewrite `crates/oikos-web/Cargo.toml`.** Replace the `crate-type = ["cdylib", "rlib"]` with a plain `[lib] path = "src/lib.rs"` and add a `[[bin]] name = "oikos-web" path = "src/main.rs"`. Depend on `leptos`, `leptos_router`, `gloo-net`, `gloo-timers`, `web-sys`, `wasm-bindgen`, `wasm-bindgen-futures`, `console_error_panic_hook`, `serde-wasm-bindgen`, `serde`, `serde_json`, and `oikos-api = { path = "../oikos-api" }`, `oikos-core = { path = "../oikos-core" }`. Keep `edition.workspace`, `authors.workspace`, `license.workspace`, `repository.workspace`.
- [ ] **Step 3: Write `src/lib.rs`** `#![forbid(unsafe_code)] pub mod app;` (every later task adds its own `pub mod …;` here).
- [ ] **Step 4: Write `src/main.rs`.** `fn main() { console_error_panic_hook::set_once(); leptos::mount::mount_to_body(oikos_web::app::App); }`.
- [ ] **Step 5: Write `src/app.rs`.** Export `#[component] pub fn App() -> impl IntoView { view! { <main>"Oikos-os"</main> } }`.
- [ ] **Step 6: Write `index.html`.** `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Oikos-os</title><link data-trunk rel="rust" data-wasm-opt="s"/><link data-trunk rel="css" href="styles/main.css"/></head><body></body></html>`.
- [ ] **Step 7: Write `Trunk.toml`.** `[build] target = "index.html" dist = "dist"`. `[[proxy]] backend = "http://127.0.0.1:8080/api/v1/"`. `[[proxy]] backend = "ws://127.0.0.1:8080/ws" ws = true`. `[[proxy]] backend = "http://127.0.0.1:8080/mcp/"`.
- [ ] **Step 8: Verify.** `cd crates/oikos-web && trunk build` succeeds and emits `dist/index.html` plus a hashed `oikos-web-*.wasm`. `cargo check -p oikos-web --target wasm32-unknown-unknown` is clean. `cargo clippy -p oikos-web --target wasm32-unknown-unknown -- -D warnings` is clean.
- [ ] **Step 9: Commit.**

```bash
git add Cargo.toml Cargo.lock crates/oikos-web
$GITC commit -m "feat(web): leptos csr scaffold with trunk" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2: Hub serves the embedded dist

**Files:**
- Create: `crates/oikos-web/dist/.gitkeep`
- Modify: `crates/oikos-os/Cargo.toml` (add `rust-embed = { version = "8", features = ["debug-embed"] }` and `mime_guess = "2"`), `crates/oikos-os/src/http/static_ui.rs` (replace the placeholder), `crates/oikos-os/src/http/mod.rs` (nothing structural; the `fallback(static_ui::index)` route still points at the module), `.gitignore` (add `crates/oikos-web/dist/` is already present per Task 1 — keep `.gitkeep` untracked by removing it from ignore)

**Interfaces:**
- Consumes: `crates/oikos-web/dist/` as a `rust-embed` folder.
- Produces: `pub(super) async fn index(uri: Uri) -> Response` in `src/http/static_ui.rs`. For `/`, returns `dist/index.html` with content type `text/html`. For any other path, strips the leading `/` and looks it up in the embed: a hit returns the file with a content type from `mime_guess`; a miss returns `dist/index.html` (SPA fallback). When `dist` has no `index.html` at compile time (greenfield checkout before `trunk build`), returns a 503 HTML page saying "The Oikos-os web UI has not been built. Run `trunk build --release` in `crates/oikos-web`."
- The function sets `Cache-Control: no-cache` on `index.html` and `Cache-Control: public, max-age=31536000, immutable` on any file whose name contains a `-` followed by a hex hash (Trunk's hashed assets).

- [ ] **Step 1: Write the failing test.** Add `crates/oikos-os/tests/http_static_ui.rs`:

```rust
#[tokio::test]
async fn root_returns_the_embedded_index_or_a_missing_page() {
    // Build the router with no hub; call `/` with hyper::Request; assert status 200
    // and body contains either "<!doctype html>" (embedded) or "has not been built".
}

#[tokio::test]
async fn unknown_path_falls_back_to_index() {
    // GET /entity/0xabc; assert status 200 and the body contains "<!doctype html>".
}

#[tokio::test]
async fn api_fallback_is_not_html() {
    // GET /api/v1/does-not-exist; assert status 404 and content type is JSON.
}
```

- [ ] **Step 2: Run the tests and watch them fail.** `cargo test -p oikos-os http_static_ui`; expected FAIL on all three.
- [ ] **Step 3: Implement `static_ui.rs`.** Replace the module with a `rust-embed` struct `#[derive(RustEmbed)] #[folder = "../oikos-web/dist/"] struct Dist;` and a handler `pub(super) async fn index(uri: Uri) -> Response` with the behaviour above. Keep the handler's name `index` so `http/mod.rs` compiles untouched.
- [ ] **Step 4: Create `crates/oikos-web/dist/.gitkeep`** (empty) so `rust-embed` can resolve the folder before `trunk build` has ever run; replace the single `/dist` entry in `crates/oikos-web/.gitignore` with two lines: `/dist/*` on the first, `!/dist/.gitkeep` on the second.
- [ ] **Step 5: Run `trunk build` in `crates/oikos-web`,** then run `cargo test -p oikos-os http_static_ui -- --nocapture`; the first and second tests see the real `<!doctype html>`, the third sees the JSON 404.
- [ ] **Step 6: Delete the old placeholder.** Remove `crates/oikos-os/assets/index.html` once the new `static_ui.rs` no longer includes it. Confirm `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-os/Cargo.toml crates/oikos-os/src/http/static_ui.rs crates/oikos-os/tests/http_static_ui.rs crates/oikos-web/.gitignore crates/oikos-web/dist/.gitkeep
git rm crates/oikos-os/assets/index.html
$GITC commit -m "feat(os): serve the embedded web dist with an SPA fallback" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 3: House state + WebSocket client with reconnect

**Files:**
- Create: `crates/oikos-web/src/house_state.rs`, `crates/oikos-web/src/house.rs`, `crates/oikos-web/src/ws.rs`

**Interfaces:**
- Consumes: `oikos_api::{Snapshot, WsEvent, EntityView, DeviceView, AreaView, BridgeView, IgnoredView, ConnectionStatus}`.
- Produces:
  - `pub struct HouseState { pub connection: ConnectionStatus, pub bridge: BridgeView, pub areas: Vec<AreaView>, pub devices: Vec<DeviceView>, pub entities: BTreeMap<String, EntityView>, pub ignored: Vec<IgnoredView>, pub generated_at: u64 }` in `house_state.rs`, with `impl HouseState { pub fn new() -> Self; pub fn from_snapshot(s: Snapshot) -> Self; pub fn apply(&mut self, event: WsEvent); }`. `apply` on `Snapshot` **replaces** every field; on `EntityUpserted`, `EntityRemoved`, `StateChanged`, `AreasChanged`, `DevicesChanged`, `BridgeChanged`, `ConnectionChanged`, patches only that field; on `CommandFailed`, is a no-op (toasts are a UI-only concern, surfaced in Task 8).
  - `#[derive(Clone, Copy)] pub struct House { pub connection: RwSignal<ConnectionStatus>, pub bridge: RwSignal<BridgeView>, pub areas: RwSignal<Vec<AreaView>>, pub devices: RwSignal<Vec<DeviceView>>, pub entities: RwSignal<BTreeMap<String, EntityView>>, pub ignored: RwSignal<Vec<IgnoredView>>, pub command_failed: RwSignal<Option<(String, String)>> }` in `house.rs`, with `impl House { pub fn new() -> Self; pub fn apply(&self, event: WsEvent); }`. `House::apply` fans the event out to the same signals `HouseState::apply` would.
  - `pub fn connect(url: &str, house: House)` in `ws.rs`: spawns a `wasm_bindgen_futures::spawn_local` loop that opens a `gloo_net::websocket::futures::WebSocket`, reads frames, decodes `WsEvent` with `serde_json`, calls `house.apply(event)`, and on close/error waits 1 s / 2 s / 4 s / 8 s (capped at 10 s) before reconnecting; writes `ConnectionStatus::Connecting` before each attempt and `Offline { reason }` on error (back-off reset on the first `Snapshot` event received).

- [ ] **Step 1: Write failing tests** `crates/oikos-web/src/house_state.rs` (unit tests at the bottom of the file, native-compiled because the module uses no wasm or Leptos types):

```rust
#[cfg(test)]
mod tests {
    // snapshot_replaces_the_cache: build HouseState with one entity, apply a Snapshot with a different entity, assert the old one is gone.
    // entity_upserted_patches_one_entity: apply EntityUpserted and assert the map holds it.
    // state_changed_updates_only_that_entity_state: assert attributes, availability and last_update move.
    // areas_changed_replaces_the_chip_list: new order wins.
    // devices_changed_replaces_the_device_list.
    // bridge_changed_updates_bridge_only.
    // connection_changed_updates_connection_only.
    // command_failed_is_a_noop_on_state.
    // entity_removed_drops_the_entry.
    // from_snapshot_populates_every_field.
    // Review Focus 1: snapshot_after_lagging_replaces_stale_entries — apply a Snapshot that omits an id that was present; assert it is gone.
}
```

- [ ] **Step 2: Run the tests and watch them fail.** `cargo test -p oikos-web --lib house_state`; expected FAIL (module not found).
- [ ] **Step 3: Implement `HouseState`.** `apply(Snapshot)` replaces every field. For `StateChanged`, mutate `entities.get_mut(&id).map(|e| e.state = state)`. For `EntityUpserted`, `entities.insert(view.id.clone(), view)`. For `EntityRemoved`, `entities.remove(&id)`. For `AreasChanged`, `DevicesChanged`, `BridgeChanged`, `ConnectionChanged`, assign the given value. `CommandFailed` leaves `self` untouched.
- [ ] **Step 4: Run the tests to verify they pass.** `cargo test -p oikos-web --lib house_state`.
- [ ] **Step 5: Implement `House`.** Each `RwSignal` is created in `House::new()` with the default of the inner type; `apply` dispatches on the `WsEvent` variant and calls `.set(…)` / `.update(…)` on the matching signal. `Snapshot` writes every signal (replace semantics, see Review Focus 1).
- [ ] **Step 6: Implement `connect`.** Build the WS URL from the current `window.location`: scheme `ws`/`wss` from `location.protocol`, host from `location.host`, path `/ws`. Loop: set `house.connection` to `Connecting`; try to open; on each incoming text frame, parse to `WsEvent` and call `house.apply`; on close or error, set `house.connection` to `Offline { reason }` and `gloo_timers::future::TimeoutFuture::new(backoff).await`, then double the backoff capped at 10 000 ms; reset the backoff on the next successful `Snapshot` message.
- [ ] **Step 7: Verify.** `cargo check -p oikos-web --target wasm32-unknown-unknown` is clean; `cargo clippy -p oikos-web --target wasm32-unknown-unknown -- -D warnings` is clean. `trunk build` succeeds (even though App does not use `House` yet).
- [ ] **Step 8: Commit.**

```bash
git add crates/oikos-web/src/house_state.rs crates/oikos-web/src/house.rs crates/oikos-web/src/ws.rs
$GITC commit -m "feat(web): house state, reactive signals and websocket client" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 4: Theme + design tokens + bundled fonts

**Files:**
- Create: `crates/oikos-web/src/theme.rs`, `crates/oikos-web/styles/tokens.css`, `crates/oikos-web/styles/fonts.css`, `crates/oikos-web/assets/fonts/Inter-Medium.woff2`, `crates/oikos-web/assets/fonts/Inter-SemiBold.woff2`, `crates/oikos-web/assets/fonts/Inter-Bold.woff2`, `crates/oikos-web/assets/fonts/Fraunces-Medium.woff2`
- Modify: `crates/oikos-web/styles/main.css` (import `tokens.css` and `fonts.css`), `crates/oikos-web/index.html` (add `<link data-trunk rel="copy-dir" href="assets"/>`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq)] pub enum Theme { Night, Warm, Auto }` in `theme.rs`.
  - `pub fn install_theme() -> RwSignal<Theme>`: reads `localStorage.getItem("oikos.theme")` inside `try { … } catch (_) {}` (any error, missing or disabled storage reads as `Theme::Auto`), initialises an `RwSignal<Theme>` with the result, creates an effect that writes `data-theme="light"` on `<html>` for `Warm`, `data-theme="dark"` for `Night`, removes the attribute for `Auto`, and persists to `localStorage` with another silent `try`.
- `tokens.css` declares every spec §7 value as a CSS custom property on `:root`; the light theme overrides them under both `:root[data-theme="light"]` and `@media (prefers-color-scheme: light) { :root:not([data-theme="dark"]) { … } }`. `fonts.css` declares `@font-face` for Inter 500/600/700 and Fraunces 500, each pointing at `/assets/fonts/<file>.woff2`.

- [ ] **Step 1: Write the failing test.** Add `crates/oikos-web/src/theme.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::Theme;
    use super::parse;
    #[test]
    fn parses_known_values() {
        assert_eq!(parse(Some("night")), Theme::Night);
        assert_eq!(parse(Some("warm")), Theme::Warm);
        assert_eq!(parse(Some("auto")), Theme::Auto);
        assert_eq!(parse(None), Theme::Auto);
        assert_eq!(parse(Some("gibberish")), Theme::Auto);
    }
    #[test]
    fn serialises_known_values() {
        assert_eq!(Theme::Night.as_str(), "night");
        assert_eq!(Theme::Warm.as_str(), "warm");
        assert_eq!(Theme::Auto.as_str(), "auto");
    }
}
```

- [ ] **Step 2: Run the tests and watch them fail.** `cargo test -p oikos-web --lib theme`.
- [ ] **Step 3: Implement `theme.rs`.** `parse(Option<&str>) -> Theme` and `impl Theme { pub fn as_str(self) -> &'static str }` cover the tests. `install_theme()` builds the signal: localStorage access through `web_sys::window().and_then(|w| w.local_storage().ok().flatten())` with a surrounding `Result::ok` chain so any error path yields `Theme::Auto`; an effect sets or removes the `data-theme` attribute on `document.document_element()`.
- [ ] **Step 4: Download the fonts.** Fetch Inter 500/600/700 from `https://github.com/rsms/inter/releases` (WOFF2 subset is fine) and Fraunces 500 from `https://github.com/undercase/Fraunces/releases`; copy the four files under `crates/oikos-web/assets/fonts/`.
- [ ] **Step 5: Write `tokens.css`.** All spec §7 values on `:root`, with the light overrides under both `:root[data-theme="light"]` and the `prefers-color-scheme` media query.
- [ ] **Step 6: Write `fonts.css`** with four `@font-face` entries pointing at `/assets/fonts/*.woff2` and `font-display: swap`.
- [ ] **Step 7: Update `index.html` and `styles/main.css`.** Add the `data-trunk copy-dir="assets"` link; `main.css` becomes `@import "./tokens.css"; @import "./fonts.css";` plus a baseline `html, body { font-family: Inter, system-ui, sans-serif; background: var(--bg); color: var(--ink); }`.
- [ ] **Step 8: Run the tests to verify they pass.** `cargo test -p oikos-web --lib theme`.
- [ ] **Step 9: Verify in a browser.** `trunk serve`; open `http://127.0.0.1:8080`; the page is Night Glass by default; toggling `data-theme="light"` in devtools shows Warm Minimal.
- [ ] **Step 10: Commit.**

```bash
git add crates/oikos-web/src/theme.rs crates/oikos-web/styles crates/oikos-web/assets crates/oikos-web/index.html
$GITC commit -m "feat(web): design tokens, bundled fonts, theme signal" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 5: App shell + routing + screen stubs

**Files:**
- Modify: `crates/oikos-web/src/main.rs`, `crates/oikos-web/src/app.rs`
- Create: `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/src/screens/home.rs`, `crates/oikos-web/src/screens/entity_sheet.rs`, `crates/oikos-web/src/screens/devices.rs`, `crates/oikos-web/src/screens/settings.rs`, `crates/oikos-web/src/components/mod.rs`, `crates/oikos-web/src/components/status_pill.rs`

**Interfaces:**
- Consumes: `House` from Task 3, `install_theme` from Task 4.
- Produces: `App` provides `House` as a Leptos context, calls `ws::connect`, and renders the shell:
  - Top bar: product name, status pill, theme toggle, four nav links (`/`, `/devices`, `/settings`).
  - A `<Router>` with routes `path!("/")` → `HomeScreen`, `path!("/entity/:id")` → `EntitySheetScreen`, `path!("/devices")` → `DevicesScreen`, `path!("/settings")` → `SettingsScreen`, fallback → `HomeScreen`.
  - Each screen component is a `#[component] pub fn <Name>Screen() -> impl IntoView` stub that renders its name.
  - `StatusPill(connection)`: text and background colour follow `ConnectionStatus` (Connecting → amber, Online → mint, Offline → rose, with the reason as `title`).

- [ ] **Step 1: Wire `App`.** In `src/app.rs`: `let house = House::new();` `provide_context(house);` `let _theme = install_theme();` `ws::connect("/ws", house);`. Render the shell with `<Router><Routes>…</Routes></Router>` using `leptos_router::components::{Router, Routes, Route}` and `leptos_router::path!`.
- [ ] **Step 2: Implement `StatusPill`.** `#[component] pub fn StatusPill() -> impl IntoView` reads `House` from context and derives three states, each with a CSS class (`.pill--connecting`, `.pill--online`, `.pill--offline`). Add the pill styles to `styles/main.css`.
- [ ] **Step 3: Implement the four screen stubs** that just render `<h1>Home</h1>`, `<h1>Entity</h1>` with the `:id` from `use_params_map`, `<h1>Devices</h1>`, `<h1>Settings</h1>`.
- [ ] **Step 4: Verify.** `trunk serve`; the four URLs render; the status pill is "Reconnecting…" at first, then "Online" once the hub is reachable.
- [ ] **Step 5: Commit.**

```bash
git add crates/oikos-web/src/{main.rs,app.rs,screens,components} crates/oikos-web/styles/main.css
$GITC commit -m "feat(web): app shell, router and status pill" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 6: Icons + card frame + state formatter

**Files:**
- Create: `crates/oikos-web/src/icons.rs`, `crates/oikos-web/src/format.rs`, `crates/oikos-web/src/components/card_frame.rs`

**Interfaces:**
- Produces:
  - `pub fn kind_icon(kind: Kind, device_class: Option<&str>) -> &'static str` returning an inline SVG string. One SVG per `Kind` plus overrides for common `device_class` values (`door`, `window`, `motion`, `smoke`, `power`, `temperature`, `humidity`, `battery`); unknowns fall back to the kind icon.
  - `pub fn display_value(state: &EntityState, unit: Option<&str>) -> String` in `format.rs`. For `StateValue::Text(s)` where `s.eq_ignore_ascii_case("unknown") || s.eq_ignore_ascii_case("unavailable")`, returns `"—"`. For `StateValue::Number(n)` with `unit = Some("°C")`, returns `"21.5 °C"` (one decimal when the value is not integer). For `StateValue::OnOff(true)` returns `"On"`. For `StateValue::Unknown` returns `"—"`.
  - `#[component] pub fn CardFrame(entity: EntityView, on_click: Callback<()>, children: Children) -> impl IntoView`: a card with the entity's name, kind icon, availability dot, pending dot, and a tap target that fires `on_click`; `children` is the card's body (control cluster).

- [ ] **Step 1: Write failing tests.** `crates/oikos-web/src/format.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::display_value;
    use oikos_core::model::{EntityState, StateValue};
    #[test] fn unknown_text_renders_as_dash() {
        let s = EntityState { value: StateValue::Text("unknown".into()), ..Default::default() };
        assert_eq!(display_value(&s, None), "—");
    }
    #[test] fn unavailable_text_renders_as_dash() {
        let s = EntityState { value: StateValue::Text("UnAvAiLaBlE".into()), ..Default::default() };
        assert_eq!(display_value(&s, None), "—");
    }
    #[test] fn integer_number_omits_decimal() {
        let s = EntityState { value: StateValue::Number(21.0), ..Default::default() };
        assert_eq!(display_value(&s, Some("°C")), "21 °C");
    }
    #[test] fn fractional_number_keeps_one_decimal() {
        let s = EntityState { value: StateValue::Number(21.47), ..Default::default() };
        assert_eq!(display_value(&s, Some("°C")), "21.5 °C");
    }
    #[test] fn on_off_renders_on_and_off() {
        let s_on = EntityState { value: StateValue::OnOff(true), ..Default::default() };
        let s_off = EntityState { value: StateValue::OnOff(false), ..Default::default() };
        assert_eq!(display_value(&s_on, None), "On");
        assert_eq!(display_value(&s_off, None), "Off");
    }
    #[test] fn unknown_variant_renders_as_dash() {
        let s = EntityState { value: StateValue::Unknown, ..Default::default() };
        assert_eq!(display_value(&s, None), "—");
    }
}
```

- [ ] **Step 2: Run them and watch them fail.** `cargo test -p oikos-web --lib format`.
- [ ] **Step 3: Implement `format::display_value`,** honouring the five Review Focus seams (dash for `unknown`/`unavailable`, dash for `StateValue::Unknown`).
- [ ] **Step 4: Implement `icons::kind_icon`** with inline SVG strings. Reuse the Oikos app's icon geometry where it exists (see `docs/superpowers/specs/2026-09-24-oikos-design.md` §4 if present; otherwise draft from scratch).
- [ ] **Step 5: Implement `CardFrame`** using Leptos signals; add `.card`, `.card--pending`, `.card--offline` styles to `styles/main.css` using the tokens.
- [ ] **Step 6: Verify.** `cargo test -p oikos-web --lib`; `cargo check -p oikos-web --target wasm32-unknown-unknown` clean.
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-web/src/{icons.rs,format.rs,components} crates/oikos-web/styles/main.css
$GITC commit -m "feat(web): icons, card frame and state formatter" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 7: REST client + optimistic command helper + Home screen

**Files:**
- Create: `crates/oikos-web/src/api.rs`, `crates/oikos-web/src/commands.rs`, `crates/oikos-web/src/components/area_chip.rs`, `crates/oikos-web/src/components/toast.rs`
- Modify: `crates/oikos-web/src/screens/home.rs`, `crates/oikos-web/src/components/mod.rs`, `crates/oikos-web/src/app.rs` (install the toast queue in the shell)

**Interfaces:**
- Produces:
  - `pub async fn send_command(id: &str, cmd: Command) -> Result<CommandResponse, ApiError>` in `api.rs`, using `gloo_net::http::Request::post(&format!("/api/v1/entities/{id}/command")).json(&cmd)?.send().await?`; non-2xx bodies decode as `ApiError`; on transport failure returns `ApiError { error: format!("…") }`.
  - `pub async fn set_override(id: &str, patch: OverridePatch) -> Result<EntityView, ApiError>`
  - `pub async fn clear_override(id: &str) -> Result<(), ApiError>`
  - `pub async fn set_areas(doc: AreasDocument) -> Result<Vec<AreaView>, ApiError>`
  - `pub fn fire_command(house: House, id: String, cmd: Command, toasts: Toasts)` in `commands.rs`: spawns `send_command`, on `CommandResponse { accepted: true, state: Some(s), .. }` writes the echoed state back into the signal (the WS also will — this is just immediate feedback), on `accepted: false` or on `Err(ApiError)` enqueues a toast and the WS `CommandFailed` event (if any) also enqueues one through `House::command_failed`.
  - `pub struct Toasts(pub RwSignal<Vec<Toast>>)` + `#[component] pub fn ToastQueue()` in `components/toast.rs`.
  - `#[component] pub fn AreaChip(area: AreaView, selected: bool, on_click: Callback<String>) -> impl IntoView` showing `area.name` and `area.primary_count` as a badge (see Review Focus seam 3).
  - `HomeScreen` reads `House` from context, derives a signal for the chip order (`Snapshot.areas` as returned), the primary entities per area (`entities.values().filter(|e| e.is_primary())`), and renders a grid of cards grouped by effective area; a `"Favorites"` chip appears when any entity has `ui.favorite = true` and only shows the favourite primaries.
- Consumes: `House`, `CardFrame`, `kind_icon`, `display_value`.

- [ ] **Step 1: Write the REST client.** Four async functions. Each tests transport error (`ApiError { error: "network error: ..." }`) and non-2xx (`serde_json::from_slice::<ApiError>(&bytes)` and surface the server message).
- [ ] **Step 2: Write `commands::fire_command`.** Watch Review Focus 3 — rejection reverts and toasts. Implementation plan: before firing, snapshot the current `EntityState`; on `accepted: false` or any `Err`, write the snapshot back and push a toast; on 503 (returned as `ApiError`), the toast reads "Broker offline — command refused" and the state is restored.
- [ ] **Step 3: Add a toast queue to the shell.** `ToastQueue` reads `Toasts` from context; also auto-dismiss each toast after 4 s via `gloo_timers`.
- [ ] **Step 4: Implement `AreaChip`** with `.chip`, `.chip--selected`, `.chip-badge` styles.
- [ ] **Step 5: Implement `HomeScreen`.** State: a signal `selected: RwSignal<String>` initialised to the first chip. Honour the seams: do not re-sort `Snapshot.areas`; show `"Unassigned"` last if present in the list; add a `"Favorites"` virtual chip at the front when any entity has `ui.favorite`. Fold `part_of` children: they are not shown on Home.
- [ ] **Step 6: Verify** `trunk serve` connects to the running hub; the real house's 11 areas appear as chips in the broker's order; selecting a chip shows that room's primary entities as cards with their `kind_icon` and `display_value`.
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-web/src/{api.rs,commands.rs,screens/home.rs,components,app.rs} crates/oikos-web/styles/main.css
$GITC commit -m "feat(web): home screen, area chips, rest client, optimistic commands" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 8: Light and Switch cards (optimistic, with reversion)

**Files:**
- Create: `crates/oikos-web/src/cards/mod.rs`, `crates/oikos-web/src/cards/light.rs`, `crates/oikos-web/src/cards/switch.rs`
- Modify: `crates/oikos-web/src/screens/home.rs` (dispatch by `kind` to a card component)

**Interfaces:**
- Consumes: `EntityView`, `fire_command`, `House`.
- Produces:
  - `#[component] pub fn LightCard(entity: EntityView) -> impl IntoView` — shows an on/off toggle; shows a brightness slider when `entity.capabilities.brightness` is true (seam: `asLight` switch has `brightness: false`, so no slider); shows an HS wheel entry when any `color_mode` is `hs`; the slider dispatches `Command::TurnOn { brightness: Some(…), color: None, effect: None }` on release.
  - `#[component] pub fn SwitchCard(entity: EntityView) -> impl IntoView` — on/off toggle. For `asLight` switches, Task 7's dispatcher routes them to `LightCard` (seam: `entity.kind == Kind::Light`).
  - Both cards read `entity.state.value` according to kind:
    - `LightCard` reads `StateValue::Light(LightState { on, .. })` for native lights.
    - `LightCard` reads `StateValue::OnOff(on)` when `entity.protocol_kind == Kind::Switch` and `entity.kind == Kind::Light` (`asLight`).
    - `SwitchCard` reads `StateValue::OnOff(on)`.

- [ ] **Step 1: Write a `cards/switch.rs` test** compiled natively that proves an `asLight` switch's `on` flag reads from `StateValue::OnOff`, not `StateValue::Light`:

```rust
#[cfg(test)]
mod tests {
    use super::read_on;
    use oikos_core::model::{EntityState, StateValue, LightState};
    #[test]
    fn as_light_switch_reads_on_off() {
        let s = EntityState { value: StateValue::OnOff(true), ..Default::default() };
        assert_eq!(read_on(&s), Some(true));
    }
    #[test]
    fn native_light_reads_light_state() {
        let s = EntityState { value: StateValue::Light(LightState { on: true, ..Default::default() }), ..Default::default() };
        assert_eq!(read_on(&s), Some(true));
    }
    #[test]
    fn unknown_reads_none() {
        let s = EntityState { value: StateValue::Unknown, ..Default::default() };
        assert_eq!(read_on(&s), None);
    }
}
```

- [ ] **Step 2: Run and watch them fail.** `cargo test -p oikos-web --lib cards`.
- [ ] **Step 3: Implement `cards::light::read_on`** that accepts either `StateValue::OnOff` or `StateValue::Light` and returns an `Option<bool>`; `cards::switch` re-exports it.
- [ ] **Step 4: Build the card views.** Toggle sends `Command::TurnOn { .. }` or `Command::TurnOff` through `fire_command`; brightness slider sends `TurnOn { brightness: Some(v), .. }`. Watch Review Focus 3: `fire_command` already reverts and toasts.
- [ ] **Step 5: Add a kind dispatcher** in `screens/home.rs`: `fn card_for(entity: EntityView) -> AnyView` matching on `entity.kind`.
- [ ] **Step 6: Verify in the browser.** Toggle a real light; it responds within the confirm timeout. Toggle an alarm-arming entity (whose publish is refused when the broker is down) to see the toast + revert.
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-web/src/{cards,screens/home.rs}
$GITC commit -m "feat(web): light and switch cards with optimistic revert" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 9: Cover and Climate cards

**Files:**
- Create: `crates/oikos-web/src/cards/cover.rs`, `crates/oikos-web/src/cards/climate.rs`
- Modify: `crates/oikos-web/src/cards/mod.rs`, `crates/oikos-web/src/screens/home.rs` (dispatch), `crates/oikos-web/src/house.rs` (expose a `fn part_of_children(&self, parent_id: &str) -> Vec<EntityView>` helper, native-tested — see Review Focus seam about `part_of`)

**Interfaces:**
- Produces:
  - `#[component] pub fn CoverCard(entity: EntityView) -> impl IntoView` — Open/Close/Stop buttons dispatching `Command::Open`, `Close`, `Stop`; when `capabilities.position` is true, a position slider dispatching `SetPosition { position }`. If a `part_of` child whose `entity.name` contains "moving" has `StateValue::OnOff(true)` or `StateValue::Text(s)` where `s` equals "moving"/"opening"/"closing", the card shows a "Moving" indicator.
  - `#[component] pub fn ClimateCard(entity: EntityView) -> impl IntoView` — shows current and target, buttons `−`/`+` to step by `temp_step` (default 0.5), a mode dropdown bound to `capabilities.hvac_modes` dispatching `SetHvacMode`, a fan dropdown bound to `capabilities.fan_modes` dispatching `SetFanMode`.
- `House::part_of_children(&self, parent_id: &str) -> Vec<EntityView>` returns all entities whose `part_of == Some(parent_id)`.

- [ ] **Step 1: Write the `part_of` helper test** in `house_state.rs`:

```rust
#[test]
fn part_of_children_returns_folded_entities() {
    let mut state = HouseState::new();
    state.apply(WsEvent::EntityUpserted(parent("cover.hall")));
    state.apply(WsEvent::EntityUpserted(child("cover.hall_moving", "cover.hall")));
    assert_eq!(state.part_of_children("cover.hall").len(), 1);
    assert_eq!(state.part_of_children("cover.hall")[0].id, "cover.hall_moving");
}
```

- [ ] **Step 2: Run and watch it fail.** `cargo test -p oikos-web --lib house_state`.
- [ ] **Step 3: Implement `HouseState::part_of_children`** and mirror it on `House` (which clones from the `entities` signal).
- [ ] **Step 4: Implement `CoverCard` and `ClimateCard`.** Watch the seam: the cover reads `moving` from its folded child, not from its own topic.
- [ ] **Step 5: Dispatch in Home.**
- [ ] **Step 6: Verify in the browser against the house's covers and climate entities.**
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-web/src/{cards/cover.rs,cards/climate.rs,cards/mod.rs,screens/home.rs,house_state.rs,house.rs}
$GITC commit -m "feat(web): cover and climate cards with part_of folding" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 10: Sensor, binary sensor, number, select, button cards

**Files:**
- Create: `crates/oikos-web/src/cards/sensor.rs`, `crates/oikos-web/src/cards/binary_sensor.rs`, `crates/oikos-web/src/cards/number.rs`, `crates/oikos-web/src/cards/select.rs`, `crates/oikos-web/src/cards/button.rs`
- Modify: `crates/oikos-web/src/cards/mod.rs`, `crates/oikos-web/src/screens/home.rs`

**Interfaces:**
- Produces:
  - `SensorCard`: read-only, `display_value(&entity.state, entity.unit.as_deref())`. Watch Review Focus 4: `StateValue::Text("unknown"|"unavailable")` already renders as `—` through `display_value`.
  - `BinarySensorCard`: read-only, with the device-class icon (door, window, motion, smoke) in its on/off state.
  - `NumberCard`: a slider from `capabilities.min` to `capabilities.max` stepping by `capabilities.step`, dispatching `Command::SetNumber`.
  - `SelectCard`: a `<select>` whose options come from `capabilities.options`, dispatching `Command::SelectOption`.
  - `ButtonCard`: a single press button dispatching `Command::Press`; shows a brief pending dot while awaiting the hub.

- [ ] **Step 1: Write a sensor test** at the bottom of `cards/sensor.rs` compiled natively:

```rust
#[cfg(test)]
mod tests {
    use super::sensor_text;
    use oikos_core::model::{EntityState, StateValue};
    #[test] fn unknown_sensor_text() {
        let s = EntityState { value: StateValue::Text("unknown".into()), ..Default::default() };
        assert_eq!(sensor_text(&s, Some("W")), "—");
    }
    #[test] fn numeric_sensor_text() {
        let s = EntityState { value: StateValue::Number(42.0), ..Default::default() };
        assert_eq!(sensor_text(&s, Some("W")), "42 W");
    }
}
```

- [ ] **Step 2: Run and watch it fail.** `cargo test -p oikos-web --lib cards`.
- [ ] **Step 3: Implement `sensor_text` as a thin wrapper around `format::display_value`** so the Review Focus seam has an owning test in this task.
- [ ] **Step 4: Implement the five cards.** Each dispatches commands through `fire_command`.
- [ ] **Step 5: Dispatch in Home.**
- [ ] **Step 6: Verify in the browser** against real sensors in the house.
- [ ] **Step 7: Commit.**

```bash
git add crates/oikos-web/src/{cards,screens/home.rs}
$GITC commit -m "feat(web): sensor, binary sensor, number, select and button cards" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 11: Alarm, vacuum, water heater cards

**Files:**
- Create: `crates/oikos-web/src/cards/alarm.rs`, `crates/oikos-web/src/cards/vacuum.rs`, `crates/oikos-web/src/cards/water_heater.rs`
- Modify: `crates/oikos-web/src/cards/mod.rs`, `crates/oikos-web/src/screens/home.rs`

**Interfaces:**
- Produces:
  - `AlarmPanelCard`: shows the current state; one button per `capabilities.alarm_modes` dispatching `Command::Arm { mode, code }` (prompts for a code when `capabilities.code_arm_required` is true, with `code_format` selecting the input type: numeric vs text); a Disarm button dispatching `Command::Disarm`.
  - `VacuumCard`: one button per `capabilities.vacuum_actions` dispatching `Command::VacuumAction`; a fan speed select from `capabilities.fan_speeds` dispatching `Command::SetFanSpeed`.
  - `WaterHeaterCard`: current and target with step buttons dispatching `SetTemperature`; a mode select from `capabilities.hvac_modes` (the hub reuses the HVAC list for water heater) dispatching `SetHvacMode`.

- [ ] **Step 1: Implement the three cards.** No new behaviour beyond pattern repetition; each one builds on `fire_command`.
- [ ] **Step 2: Dispatch in Home.**
- [ ] **Step 3: Verify in the browser** against the house's alarm panel and (if available) vacuum / water heater entities.
- [ ] **Step 4: Commit.**

```bash
git add crates/oikos-web/src/{cards,screens/home.rs}
$GITC commit -m "feat(web): alarm, vacuum and water heater cards" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 12: Entity sheet (detail screen)

**Files:**
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs`

**Interfaces:**
- Consumes: `House`, every card component, `kind_icon`, `display_value`.
- Produces: `EntitySheetScreen` reads the `:id` from the route, pulls the matching `EntityView` from `House::entities`, and renders:
  - Header: `kind_icon` + `entity.name` + area + availability.
  - A full-control cluster delegating to the card for the kind (brightness slider, HS wheel, effects, position slider, HVAC + fan modes, options, temperature steps).
  - Device info block when `entity.device_id` is `Some`: pulls the matching `DeviceView` and shows model, vendor, ieee, availability, battery (`u8`), linkquality (`u32`).
  - Raw state block: `entity.state.value` serialised with `serde_json::to_string_pretty`, `entity.state.attributes` dumped as JSON, `last_update` formatted as ISO 8601 UTC from the unix seconds (helper `fn format_timestamp(seconds: u64) -> String`).
  - A "Back" link to `/`.
  - A placeholder container for the Override editor (filled in Task 13).

- [ ] **Step 1: Write a `format_timestamp` test** in `format.rs`:

```rust
#[cfg(test)]
mod ts_tests {
    use super::format_timestamp;
    #[test] fn unix_epoch_formats_as_iso() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
    }
}
```

- [ ] **Step 2: Run and watch it fail.** `cargo test -p oikos-web --lib format`.
- [ ] **Step 3: Implement `format_timestamp`** without pulling `chrono`: use `time::OffsetDateTime::from_unix_timestamp(seconds as i64)` from the `time = "0.3"` crate (add to workspace deps and oikos-web deps). Format as `format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z")`.
- [ ] **Step 4: Implement `EntitySheetScreen`.** The full-control cluster reuses the card components with a `detailed: bool` prop toggled on; add `.sheet` styles.
- [ ] **Step 5: Verify in the browser.** Click a card on Home; `EntitySheetScreen` opens; the device info block appears for Zigbee entities; the raw state block updates live.
- [ ] **Step 6: Commit.**

```bash
git add crates/oikos-web/src/{screens/entity_sheet.rs,format.rs,Cargo.toml} Cargo.toml
$GITC commit -m "feat(web): entity detail sheet with device info and raw state" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 13: Override editor

**Files:**
- Create: `crates/oikos-web/src/override_editor.rs`
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs` (mount the editor)

**Interfaces:**
- Consumes: `EntityView`, `api::{set_override, clear_override}`, `Toasts`.
- Produces: `#[component] pub fn OverrideEditor(entity: EntityView) -> impl IntoView`. Fields render as one of five input kinds matching the hub's `FIELD_TYPES` table (text, flag, int, brightness, energyRole, lightOnPreset). The editor builds a `serde_json::Map<String, Value>` of dirty fields and submits it as the body of `PATCH /override`. On success it updates the entity in `House` (the WS also pushes `EntityUpserted`); on failure (`ApiError { error }`) it toasts the server's message and leaves the form unsubmitted (Review Focus 2). A "Clear override" button calls `DELETE /override`.

- [ ] **Step 1: Write a dirty-fields test** at the bottom of `override_editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::dirty_patch;
    use oikos_core::model::UiOverride;
    use serde_json::json;
    #[test]
    fn unchanged_fields_are_absent() {
        let before = UiOverride::default();
        let mut after = UiOverride::default();
        after.favorite = true;
        let patch = dirty_patch(&before, &after);
        assert_eq!(patch, json!({"favorite": true}).as_object().cloned().unwrap());
    }
    #[test]
    fn cleared_text_sends_null() {
        let mut before = UiOverride::default();
        before.name = Some("foo".into());
        let after = UiOverride::default();
        let patch = dirty_patch(&before, &after);
        assert_eq!(patch, json!({"name": null}).as_object().cloned().unwrap());
    }
}
```

- [ ] **Step 2: Run and watch them fail.** `cargo test -p oikos-web --lib override_editor`.
- [ ] **Step 3: Implement `dirty_patch(&UiOverride, &UiOverride) -> OverridePatch`** that walks the app's 19 fields and emits only differences (a cleared `Option` → `Value::Null`, a flipped flag → the new bool, etc.).
- [ ] **Step 4: Implement `OverrideEditor`.** Watch Review Focus 2: show `ApiError::error` in a visible status line and keep the inputs populated.
- [ ] **Step 5: Verify in the browser.** Change a device's name through the editor; the Home card re-renders. Try an invalid value (`lightOnBrightness: 1000`) and confirm the server's rejection text appears.
- [ ] **Step 6: Commit.**

```bash
git add crates/oikos-web/src/{override_editor.rs,screens/entity_sheet.rs}
$GITC commit -m "feat(web): override editor with per-field inputs and server-side validation" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 14: Devices screen (read-only administration view)

**Files:**
- Modify: `crates/oikos-web/src/screens/devices.rs`

**Interfaces:**
- Consumes: `House`.
- Produces: `DevicesScreen` renders a table of Z2M devices (`House::devices`): friendly name, model + vendor, availability, interview state, power source, battery, linkquality, number of entities. Clicking a row expands it to list the entity ids (links to each entity sheet). Below the table, an "Ignored discovery topics" section lists `House::ignored` with each topic and reason.

- [ ] **Step 1: Implement `DevicesScreen`.** No new state; just view code and styles (`.devices-table`, `.ignored-list`).
- [ ] **Step 2: Verify in the browser** that the 66 devices and 26 ignored topics listed in the hub verification doc appear.
- [ ] **Step 3: Commit.**

```bash
git add crates/oikos-web/src/screens/devices.rs crates/oikos-web/styles/main.css
$GITC commit -m "feat(web): devices administration view" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 15: Settings / status screen

**Files:**
- Modify: `crates/oikos-web/src/screens/settings.rs`

**Interfaces:**
- Consumes: `House`, theme signal.
- Produces: `SettingsScreen` with four blocks:
  - Broker connection: `connection` status with the latest reason text when offline.
  - Z2M bridge: `bridge.version`, `bridge.coordinator`, `bridge.permit_join`, `bridge.state`.
  - Counts: devices, entities (plus `primary_count` across all areas), areas, ignored.
  - Theme: a tri-switch (Auto / Night / Warm) bound to the theme signal.

- [ ] **Step 1: Implement `SettingsScreen`.**
- [ ] **Step 2: Verify in the browser.** All four blocks render; the theme switch persists through a reload.
- [ ] **Step 3: Commit.**

```bash
git add crates/oikos-web/src/screens/settings.rs crates/oikos-web/styles/main.css
$GITC commit -m "feat(web): settings and status screen with theme switch" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 16: Wasm smoke test, README update, final verification

**Files:**
- Create: `crates/oikos-web/tests/smoke.rs`
- Modify: `README.md` (add a "Web UI" section), `crates/oikos-web/Cargo.toml` (dev-dep `wasm-bindgen-test`)

**Interfaces:**
- Produces: one `wasm-bindgen-test` test that mounts `App` into a scratch node and asserts the shell renders the product name and the status pill.

- [ ] **Step 1: Write the smoke test.**

```rust
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);
#[wasm_bindgen_test]
fn app_renders() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    leptos::mount::mount_to(host.clone().unchecked_into(), oikos_web::app::App);
    assert!(host.inner_html().contains("Oikos-os"));
}
```

(The test imports `oikos_web::app::App` from the lib target set up in Task 1.)

- [ ] **Step 2: Document the dev and release workflow in `README.md`.** Dev: two terminals, `cargo run -p oikos-os -- run` and `trunk serve` in `crates/oikos-web/`. Release: `cd crates/oikos-web && trunk build --release` then `cargo build --release -p oikos-os`.
- [ ] **Step 3: Verify everything.**
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
  - `cargo check -p oikos-core --target wasm32-unknown-unknown`
  - `cargo check -p oikos-api --target wasm32-unknown-unknown`
  - `cargo check -p oikos-web --target wasm32-unknown-unknown`
  - `cd crates/oikos-web && wasm-pack test --headless --firefox` (or Chrome) runs the smoke test
  - `cd crates/oikos-web && trunk build --release` emits a hashed wasm/js under `dist/`
  - `cargo build --release -p oikos-os`; run `./target/release/oikos-os run`, open `http://127.0.0.1:8080`, confirm every real device of the house renders, every control works, and the override editor round-trips.
- [ ] **Step 4: Commit.**

```bash
git add crates/oikos-web/tests/smoke.rs crates/oikos-web/Cargo.toml README.md
$GITC commit -m "feat(web): wasm smoke test and documented build pipeline" -m "Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```
