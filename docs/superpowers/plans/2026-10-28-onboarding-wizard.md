# Onboarding — Web Wizard Implementation Plan (plan 2 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The browser setup wizard at `/setup` (Welcome → MQTT → Database → Devices → Services → Done) and the matching Settings pages, built on the setup and settings API from plan 1.

**Architecture:** `main.rs` mounts a new `Root` that asks `GET /api/v1/setup` once. In setup mode it renders a small `SetupApp`: only `/setup`, with no socket and no shell. In running mode it renders today's `App` with a `/setup` route added. Each step is one form component, and its Settings page reuses that same component. Any save that makes the hub re-exec shows an "Applying…" overlay. The overlay polls `GET /api/v1/setup` until the hub answers again, then reloads the page at a target path. Reloading is simpler than patching live state, and after a reload `/setup` resumes from the step the hub stored.

**Tech Stack:** Rust, Leptos (CSR), leptos_router, gloo-net, wasm-bindgen-test (headless Chrome via `wasm-pack test`), the existing `FetchStub` test helper.

**Spec:** `docs/superpowers/specs/2026-10-28-onboarding-design.md` (sections "Hub states", "The wizard", "Settings afterwards", "Secrets"). Mockup: `docs/superpowers/specs/2026-10-28-onboarding-mockup.html`.

**Depends on:** plan 1 (PR #82) merged into `main`. Branch from `origin/main` after that merge (`feat/onboarding-wizard`). Do not stack on `feat/onboarding`.

## Global Constraints

- Every user-facing word goes through the FR/EN table: the new `src/i18n/setup.rs` (`SetupText`, `setup_text(lang, k)`). `cargo test -p oikos-web --test bare_text` must stay green, and no file may be added to `bare_text_allow.txt`.
- Exact copy fixed by the spec (EN / FR):
  - `Applying…` / `Application…`
  - `Saved, enter a new one to replace it` / `Enregistré, saisissez-en un nouveau pour le remplacer`
  - `No devices yet. Check that Zigbee2MQTT publishes to this broker under {base}` / `Aucun appareil pour l'instant. Vérifiez que Zigbee2MQTT publie sur ce broker sous {base}`
  - `Run setup again` / `Relancer la configuration`
  - Mockup titles: `Welcome to Oikos`, `Where is your home?`, `Connect to your MQTT broker`, `Connect to the database`, `Your devices`, `Services`, `Your home is ready`, button `Open my house`. Translate the FR side in the tone of the existing tables.
- A password field is always sent as a **string**. An empty field sends `""`, which the hub reads as "keep". The web never sends `null` for a secret, so nothing in this plan can clear one.
- `PUT /api/v1/settings/{section}` replaces the whole section. A form starts from the section's GET reply as a `serde_json::Value` and overwrites only the fields it shows. Fields it doesn't show travel back unchanged, and `{"set":true}` secret views read as "keep".
- The hub's error text (`{"error": …}` or a `TestResult.message`) is shown as received, never translated.
- Look: reuse the existing tokens and classes (`sheet__panel`, `field`, `select`, `chip`, `override__field`). New CSS goes in `styles/setup.css`, linked from `index.html` after `picker.css`. Both themes (Night Glass and Ivoire) must read correctly.
- Docs pages are plan 3's job. This plan touches no `docs/users/` page.
- Never name the maintainer's machines, IPs, rooms or family in tracked files. Tests use `192.0.2.x`, `example.com`, and city `Bordeaux`.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`.
- Verification commands (from the repo root):
  - native: `cargo test -p oikos-web --lib` and `cargo test -p oikos-web --test bare_text`
  - wasm: `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo clippy -p oikos-web --target wasm32-unknown-unknown -- -D warnings`
  - browser: `cd crates/oikos-web && wasm-pack test --headless --chrome --test <file>`

## Review Focus

1. **The hub disappears for a few seconds after the Database step**, because it re-execs from setup mode into running mode. The page must show "Applying…", wait, and land on Devices inside the full app (socket connected). It must not show a network-error toast or stay stuck in `SetupApp`. Owned by Task 2 (`wait_for_hub`) and Task 4 (the DB step sets the target).
2. **A field edited after a successful Test.** Next must turn off again until the new values pass a test, or the hub refuses (422) and the user doesn't understand why. Owned by Task 4.
3. **A stored secret with an empty password field.** The body must carry `"password": ""`, never `null` and never a missing key that a later refactor could turn into `null`. Owned by Tasks 4 and 8.
4. **Fields a form doesn't show** (Frigate `zones`, `cooldown`, `severity`, `ttl`, `topic_prefix`; Push `prefix`, `legacy_prefix`, `retry_interval`, `max_retries`, `history_size`, `history_days`; Climate `prefix`) must be sent back exactly as the GET returned them. Owned by Tasks 6 and 8.
5. **The geocoder is down (502) or finds nothing.** The user must still be able to type latitude, longitude and time zone and continue. An unknown time zone keeps Next off and names the field. Owned by Task 3.

---

## File structure

| File | Responsibility |
| --- | --- |
| `src/api/setup.rs` (new) | REST calls for `/api/v1/setup/*` and `/api/v1/settings/*`; `SaveError`. |
| `src/i18n/setup.rs` (new) | `SetupText` and `setup_text`. |
| `src/setup/mod.rs` (new) | `SetupState`, `Applying`, `wait_for_hub`, the redirect rules, `SetupApp`. |
| `src/setup/wizard.rs` (new) | `SetupScreen`: the step frame, progress, Back/Next plumbing. |
| `src/setup/welcome.rs` (new) | `LocationForm`, `WelcomeStep`. |
| `src/setup/connection.rs` (new) | `MqttForm`, `DatabaseForm`, the test gate. |
| `src/setup/devices.rs` (new) | `DevicesStep`. |
| `src/setup/services/mod.rs` (new) | `ServicesStep`, `ServiceCard`, `CardState`, skipped-list storage, section load/save helpers. |
| `src/setup/services/basic.rs` (new) | Weather, Climate and Alarm forms. |
| `src/setup/services/energy.rs` (new) | `EnergyForm` (role pickers + off-peak). |
| `src/setup/services/keys.rs` (new) | Solcast, Frigate and Push forms. |
| `src/setup/done.rs` (new) | `DoneStep`. |
| `src/app.rs`, `src/main.rs`, `src/shell/mod.rs` | `Root`, the `/setup` route, the landing redirect, a frame without chrome on `/setup`. |
| `src/screens/settings.rs`, `src/screens/home_settings_panel.rs` | New Settings pages, the editable Broker page, the Energy panel on Home, "Run setup again". |
| `styles/setup.css` (new), `index.html` | Wizard styles. |
| `tests/setup_*.rs` (new) | Browser tests, using `#[path = "support/fetch_stub.rs"] mod fetch_stub;`. |

---

### Task 1: API client, setup state and text table

**Files:**
- Create: `src/api/setup.rs`, `src/setup/mod.rs`, `src/i18n/setup.rs`
- Modify: `src/api/mod.rs` (`pub mod setup;`), `src/lib.rs` (`pub mod setup;`), `src/i18n/mod.rs` (`pub mod setup;`)
- Test: inline `#[cfg(test)]` in `src/api/setup.rs` and `src/setup/mod.rs`

**Interfaces:**
- Produces (`crate::api::setup`), all using `check`/`read_json` from `api::core`:
  - `pub async fn get_setup() -> Result<SetupStatus, ApiError>`
  - `pub async fn geocode(city: &str, country: Option<&str>) -> Result<Vec<GeoPlace>, ApiError>`: `GET /api/v1/setup/geocode?city=…&country=…`, with values percent-encoded through `encode_id`.
  - `pub async fn put_location(loc: &Location) -> Result<SetupStatus, SaveError>`
  - `pub async fn test_mqtt(body: &Value) -> Result<TestResult, ApiError>` / `pub async fn put_mqtt(body: &Value) -> Result<SetupStatus, SaveError>`
  - `pub async fn test_database(body: &Value) -> Result<TestResult, ApiError>` / `pub async fn put_database(body: &Value) -> Result<SetupStatus, SaveError>`
  - `pub async fn put_step(step: SetupStep) -> Result<SetupStatus, SaveError>` (body `{"step": "<snake_case>"}`)
  - `pub async fn finish() -> Result<SetupStatus, SaveError>` (`POST /api/v1/setup/finish`)
  - `pub async fn get_section(section: &str) -> Result<Value, ApiError>` / `pub async fn put_section(section: &str, body: &Value) -> Result<(), SaveError>` (the reply `{"restarting":true}` is dropped; every successful section PUT restarts the hub)
  - `pub async fn test_service(kind: &str, body: &Value) -> Result<TestResult, ApiError>` (`POST /api/v1/setup/test/{solcast|frigate}`)
  - `pub enum SaveError { Failed(TestResult), Refused(String), Restarting, Api(ApiError) }`, `pub fn decode_save_error(status: u16, body: &str) -> SaveError`, `impl SaveError { pub fn message(&self, lang: Lang) -> String }`
- Produces (`crate::setup`):
  - `#[derive(Clone, Copy)] pub struct SetupState(pub RwSignal<Option<SetupStatus>>)`. `None` means the hub has no setup API (older hub) or the call failed; nothing redirects then.
  - `pub fn redirects_to_setup(status: &SetupStatus, path: &str) -> bool`
  - `pub fn opening_step(status: &SetupStatus, query_step: Option<SetupStep>) -> SetupStep`: `query_step` (from `/setup?step=<snake_case>`) wins when given; else `Welcome` when finished, else the stored step. Either way it's clamped to `Database` in setup mode.
  - `pub fn setup_href(step: SetupStep) -> String`: `/setup?step=<snake_case>`, the Applying target a wizard save uses so a restart resumes at the next step.
- Produces (`crate::i18n::setup`): `pub enum SetupText`, `pub fn setup_text(lang: Lang, k: SetupText) -> &'static str`, `pub fn use_setup_text() -> impl Fn(SetupText) -> &'static str + Copy + 'static` (shaped like `use_device_text`). Later tasks add their variants here.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn save_errors_decode_by_status_and_body() {
    assert!(matches!(decode_save_error(422, r#"{"ok":false,"message":"bad credentials","detail":null}"#),
        SaveError::Failed(r) if r.message == "bad credentials"));
    assert!(matches!(decode_save_error(422, r#"{"error":"Enter the password for the new address"}"#),
        SaveError::Refused(m) if m == "Enter the password for the new address"));
    assert!(matches!(decode_save_error(503, r#"{"error":"restarting"}"#), SaveError::Restarting));
    assert!(matches!(decode_save_error(500, "oops"), SaveError::Api(_)));
}

#[test]
fn setup_mode_sends_every_path_to_setup() {
    let s = status("setup", false, SetupStep::Welcome);
    assert!(redirects_to_setup(&s, "/"));
    assert!(redirects_to_setup(&s, "/dashboards/house"));
    assert!(!redirects_to_setup(&s, "/setup"));
}

#[test]
fn a_running_hub_redirects_only_landing_until_finished() {
    assert!(redirects_to_setup(&status("running", false, SetupStep::Devices), "/"));
    assert!(!redirects_to_setup(&status("running", false, SetupStep::Devices), "/rooms/x"));
    assert!(!redirects_to_setup(&status("running", true, SetupStep::Done), "/"));
}

#[test]
fn opening_step_resumes_but_never_past_database_in_setup_mode() {
    assert_eq!(opening_step(&status("running", true, SetupStep::Done), None), SetupStep::Welcome);
    assert_eq!(opening_step(&status("setup", false, SetupStep::Devices), None), SetupStep::Database);
    assert_eq!(opening_step(&status("running", false, SetupStep::Services), None), SetupStep::Services);
    // Run setup again on a finished hub: a save restarts it, and the reload must resume, not loop.
    assert_eq!(opening_step(&status("running", true, SetupStep::Done), Some(SetupStep::Mqtt)), SetupStep::Mqtt);
    assert_eq!(opening_step(&status("setup", false, SetupStep::Welcome), Some(SetupStep::Services)), SetupStep::Database);
    assert_eq!(setup_href(SetupStep::Mqtt), "/setup?step=mqtt");
}
```

(`status(mode, finished, step)` is a test helper that builds a `SetupStatus` with every other field `None`/`false`.)

- [ ] **Step 2: Run them, expect compile failures** — `cargo test -p oikos-web --lib setup`
- [ ] **Step 3: Implement the interfaces above.** A 422 body is a `TestResult` when it parses as one, else `{"error"}`. Anything that parses as neither falls back to `Api(WebError::Http(status))`, like `decode_error`. `Restarting.message` is `setup_text(lang, SetupText::Applying)`.
- [ ] **Step 4: Run the tests and `cargo check -p oikos-web --target wasm32-unknown-unknown`, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): setup API client and state`

---

### Task 2: Root, setup-mode app, `/setup` route and the Applying overlay

**Files:**
- Modify: `src/main.rs` (mount `oikos_web::app::Root`), `src/app.rs` (`Root`; the `/setup` route; `HouseRedirect` consults `SetupState`), `src/shell/mod.rs` (no sidebar or header while `pathname` starts with `/setup`), `src/setup/mod.rs`
- Create: `src/setup/wizard.rs` (frame only: progress and an empty body per step), `styles/setup.css`; link it in `index.html`
- Test: `tests/setup_boot.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `#[component] pub fn Root() -> impl IntoView` in `app.rs`. It provides `Applying` and `SetupState`, calls `get_setup()` once, and renders nothing until the reply. `mode == "setup"` → `<SetupApp/>`; any other reply, **or an error** → `<App/>`, which has the route `/setup` → `SetupScreen` and keeps every existing route. `<ApplyingOverlay/>` renders above both.
  - `#[component] pub fn SetupApp() -> impl IntoView` (`setup/mod.rs`): provides lang (`install_lang`), theme (`install_theme`) and `Toasts`; no `House`, no `/ws`. `Router` with `/setup` → `SetupScreen`; any other path navigates to `/setup` with `replace: true`.
  - `#[derive(Clone, Copy)] pub struct Applying { pub target: RwSignal<Option<String>>, pub failed: RwSignal<bool>, reload: fn(&str) }` with `Applying::new(reload: fn(&str)) -> Applying` and `pub fn start(&self, target: &str)`. `Root` passes `reload_to`, which does `window.location().assign(target)`.
  - `pub async fn wait_for_hub(interval_ms: u32, attempts: u32) -> Option<SetupStatus>`: first sleeps `interval_ms`, so a hub about to restart isn't caught still answering. Then it polls `get_setup()` every `interval_ms` and returns the first reply with `restarting == false`, ignoring errors. It gives `None` after `attempts`. The overlay uses `(1000, 90)`.
  - `#[component] pub fn ApplyingOverlay() -> impl IntoView`: while `target` is `Some`, a full-screen layer reading `Applying…`. When `wait_for_hub` returns, it calls `reload(target)`. On `None` it sets `failed` and shows `The hub did not come back. Check its log.` with a Retry button that runs the wait again.
  - `#[component] pub fn SetupScreen() -> impl IntoView` (`wizard.rs`): `step: RwSignal<SetupStep>` starts at `opening_step(status, <the `step` query parameter, parsed>)`, where `status` is `SetupState` or a fresh `get_setup()` when that's `None`. It shows a six-dot progress line and a `match step` that later tasks fill in. It also provides `pub struct WizardNav { pub step: RwSignal<SetupStep> }` as context.
  - `HouseRedirect`: when `SetupState` holds a status and `redirects_to_setup(status, "/")` is true, it navigates to `/setup`; otherwise it does what it does today.

- [ ] **Step 1: Write the failing browser tests** in `tests/setup_boot.rs`:
  - `setup_mode_sends_any_path_to_setup`: stub `GET /api/v1/setup` → 200 `{"mode":"setup","step":"welcome","finished":false,"bundled":false,"location":null,"mqtt":null,"database":null,"restarting":false}`; `history.replaceState` to `/dashboards/house`; mount `Root`; after a few ticks, `location.pathname == "/setup"`, and no request to `/api/v1/dashboards` appears in the stub log.
  - `wait_for_hub_returns_once_the_restart_is_over`: stub `GET /api/v1/setup` with `restarting: true`, call `wait_for_hub(10, 50)` in `spawn_local`, change the stub to `restarting: false` after 30 ms, and assert it resolves to `Some`.
  - `overlay_reloads_to_its_target`: `Applying::new(record)` (`record` pushes into a `thread_local!` `Vec<String>`), `start("/setup")`, stub `restarting:false`; within 1 s `Applying…` has shown and the record holds `["/setup"]`. Mount the overlay with that `Applying` provided; the overlay reads the interval from a `const` that the test can't change, so the test waits up to 2 s.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome --test setup_boot`, **expect FAIL**
- [ ] **Step 3: Implement.** Keep `App`'s body as it is, apart from the route and `HouseRedirect`. `main.rs` still removes `#boot` after mounting.
- [ ] **Step 4: Run the browser test, `cargo test -p oikos-web --lib`, the wasm check and clippy, expect PASS.** Run the other browser suites (`wasm-pack test --headless --chrome`) once to confirm the new `Root` broke nothing that mounts `App` directly.
- [ ] **Step 5: Commit** — `feat(web): boot into the setup app or the house`

---

### Task 3: Welcome step and Settings › Location

**Files:**
- Create: `src/setup/welcome.rs`
- Modify: `src/setup/wizard.rs` (Welcome body), `src/screens/settings.rs` (`SettingsPage::Location`, slug `location`, listed second after Home), `src/i18n/setup.rs`, `src/i18n/settings.rs` (hint)
- Test: inline unit tests; `tests/setup_welcome.rs`

**Interfaces:**
- Produces:
  - `pub fn place_label(p: &GeoPlace) -> String`: name, admin and country joined by `, `, skipping `None`s. Example: `Bordeaux, Nouvelle-Aquitaine, FR`.
  - `pub fn coords_label(lat: f64, lon: f64, tz: &str) -> String`: example `44.84° N, 0.58° W · Europe/Paris`, two decimals, with N/S and E/W from the sign.
  - `pub fn location_problem(city: &str, country: &str, lat: f64, lon: f64, tz: &str) -> Option<SetupText>`. In order: blank city → `CityRequired`; a non-empty country that isn't two ASCII letters → `CountryTwoLetters`; lat outside ±90 → `LatitudeRange`; lon outside ±180 → `LongitudeRange`; `tz.parse::<chrono_tz::Tz>()` fails → `TimeZoneUnknown`.
  - `#[component] pub fn LocationForm(initial: Option<Location>, applying_target: String, #[prop(into)] on_saved: Callback<SetupStatus>) -> impl IntoView`. It has City and Country (placeholder `FR`) fields, a Search button that calls `geocode` and lists up to 5 results as buttons, and a "Correct" disclosure with Latitude, Longitude and Time zone. Picking a result fills them all; a result without a time zone keeps the current one. A geocode error or empty list shows `NoPlaceFound` and opens the disclosure. The Save/Next button is disabled while `location_problem` is `Some`, and the problem text shows under the form. On save it calls `put_location`. On `Ok(s)`: if `s.restarting`, it calls `Applying::start(&applying_target)`; otherwise `on_saved(s)`. Errors show `SaveError::message`.
  - `#[component] pub fn WelcomeStep() -> impl IntoView`: the title and lead copy, language chips (English / Français) that set the `RwSignal<Lang>` context, then `LocationForm` with `applying_target = setup_href(SetupStep::Mqtt)` and `on_saved` → `WizardNav.step.set(SetupStep::Mqtt)`.

- [ ] **Step 1: Write the failing tests**
  - Unit: `place_label` example above, and one with `admin: None`. `coords_label(44.8378, -0.5792, "Europe/Paris") == "44.84° N, 0.58° W · Europe/Paris"` and `coords_label(-33.87, 151.21, "Australia/Sydney") == "33.87° S, 151.21° E · Australia/Sydney"`. `location_problem` returns each variant for its input and `None` for `("Bordeaux","FR",44.84,-0.58,"Europe/Paris")` and for an empty country.
  - Browser `picking_a_place_saves_its_coordinates`: stub `GET /api/v1/setup/geocode?city=Bordeaux&country=FR` → one place `{name:"Bordeaux",admin:"Nouvelle-Aquitaine",country:"FR",latitude:44.84,longitude:-0.58,timezone:"Europe/Paris"}` and `PUT /api/v1/setup/location` → a status with `step:"mqtt"`. Type, Search, pick, Next. The recorded PUT body is `{"city":"Bordeaux","country":"FR","latitude":44.84,"longitude":-0.58,"timezone":"Europe/Paris"}`.
  - Browser `geocoder_down_still_lets_coordinates_through`: geocode → 502 `{"error":"the city lookup failed"}`. The disclosure opens, typing `Europe/Paris` and coordinates enables Next, and typing `Europe/Nowhere` disables it with the time-zone text.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**, plus the Settings page: `SettingsPage::Location` renders `LocationForm` with `initial` from `get_setup().location` and `applying_target = "/settings/location"`. The `on_saved` callback is unused there, because a running hub always restarts on a location change.
- [ ] **Step 4: Run unit and browser tests, bare_text, the wasm check and clippy, expect PASS.** Also update `every_page_round_trips_its_slug` if it lists the pages by hand.
- [ ] **Step 5: Commit** — `feat(web): setup Welcome step and Location settings`

---

### Task 4: MQTT and Database steps, Settings › Broker and Settings › Database

**Files:**
- Create: `src/setup/connection.rs`
- Modify: `src/setup/wizard.rs`, `src/screens/settings.rs` (`BrokerPage` gains `MqttForm` under its status panel; new `SettingsPage::Database`, slug `database`, after Broker), `src/i18n/setup.rs`, `src/i18n/settings.rs`
- Test: inline unit tests; `tests/setup_connection.rs`

**Interfaces:**
- Produces:
  - `pub struct MqttDraft { pub host: String, pub port: String, pub user: String, pub password: String, pub client_id: String, pub z2m_base: String }` with `pub fn from_view(v: Option<&MqttView>) -> MqttDraft` (no view → `localhost`, `1883`, the rest empty) and `pub fn body(&self) -> Result<Value, SetupText>` (the port must parse as `u16` ≥ 1, else `PortInvalid`; blank `user`, `client_id` and `z2m_base` are left out; `password` is always present as a string).
  - `pub struct DatabaseDraft { host, port, database, user, password: String }` with `from_view(Option<&DatabaseView>)` (no view → `localhost`, `5432`, `oikos`, `oikos`, empty) and `body(&self) -> Result<Value, SetupText>` (the same port rule; blank database or user → `FieldRequired`).
  - `pub fn test_passed(last_ok: Option<&str>, body: &Value) -> bool`: true when the last successful test was run on exactly this body (`body.to_string()`).
  - `#[component] pub fn MqttForm(initial: Option<MqttView>, applying_target: String, #[prop(into)] on_saved: Callback<SetupStatus>, #[prop(optional)] on_back: Option<Callback<()>>) -> impl IntoView`. It has Host, Port, User, Password (`type="password"`; when `initial.password.set`, the placeholder is `SecretSaved`), and an `<details>` "Advanced" with Client id and Zigbee2MQTT base topic. **Test connection** calls `test_mqtt` and shows `TestResult.message` (in a success or failure style). Next/Save is enabled only when `test_passed`. Save calls `put_mqtt`: `Failed(r)` shows `r.message`, `Refused(m)` shows `m`, `Restarting` and `Ok(s)` with `s.restarting` start Applying at `applying_target`: `setup_href(Database)` from the wizard, `/settings/broker` from Settings. Any other `Ok(s)` calls `on_saved(s)`.
  - `#[component] pub fn DatabaseForm(...)`, the same shape. On a successful test whose `detail.history_since` is a string, it shows `HistorySince` with the date formatted by the existing `crate::format` date helper. No Skip.
  - In the wizard: MQTT `on_saved` → step `Database`. Database (`applying_target = setup_href(Devices)`, Settings: `/settings/database`) `on_saved` → step `Devices`. In setup mode, saving the database makes the hub restart (`status.restarting == true`), so the Applying overlay reloads `/setup?step=devices` in running mode.

- [ ] **Step 1: Write the failing tests**
  - Unit `an_empty_password_is_sent_as_an_empty_string`: `MqttDraft { password: "".into(), .. }.body()` has `"password": ""`, and `body.get("password") != Some(&Value::Null)`. The same for `DatabaseDraft`.
  - Unit `a_changed_field_needs_a_new_test`: `test_passed(Some(&b1.to_string()), &b1)` is true. With `b2` = the same draft but port `1884`, it's false. `test_passed(None, &b1)` is false.
  - Unit: `port: "abc"` → `Err(PortInvalid)`; `port: "0"` → `Err(PortInvalid)`.
  - Browser `next_waits_for_a_passing_test_of_these_values`: Next disabled. Test → stub 200 `{"ok":true,"message":"Connected","detail":null}`, Next enabled. Edit Host, Next disabled again.
  - Browser `a_refused_save_shows_the_brokers_reason`: test ok; `PUT /api/v1/setup/mqtt` → 422 `{"ok":false,"message":"bad user name or password","detail":null}`; that text shows and the step doesn't move.
  - Browser `bundled_credentials_prefill_and_keep_the_password`: initial view `{host:"localhost",port:1883,user:"oikos",password:{set:true},client_id:null,z2m_base:null}`. User shows `oikos`, the password placeholder is `Saved, enter a new one to replace it`, and the recorded test body has `"password":""`.
  - Browser `saving_the_database_in_setup_mode_starts_applying`: test ok, PUT → 200 status with `restarting:true`. `Applying.target == Some("/setup?step=devices")`, using an `Applying::new(record)` as in Task 2.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run unit and browser tests, bare_text, the wasm check and clippy, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): MQTT and database steps, editable Broker and Database settings`

---

### Task 5: Devices step

**Files:**
- Create: `src/setup/devices.rs`
- Modify: `src/setup/wizard.rs`, `src/screens/devices.rs` (make `device_rooms` `pub(crate)`), `src/i18n/setup.rs`
- Test: inline unit tests; `tests/setup_devices.rs`

**Interfaces:**
- Consumes: `House` (`devices`, `entities`, `areas`; only present in running mode, which is always the case from this step on), `crate::api::set_override`, `OverridePatch` (set only `area`, the way `override_editor::dirty_patch` builds a one-field patch).
- Produces:
  - `pub fn group_by_room(devices: &[DeviceView], rooms_of: impl Fn(&DeviceView) -> Vec<String>) -> Vec<(Option<String>, Vec<DeviceView>)>`. A device goes under its first room, rooms are sorted by name, and the `None` group ("No room") comes last. Devices inside a group are sorted by name.
  - `#[component] pub fn DevicesStep(z2m_base: String, #[prop(default = 5000)] empty_after_ms: u32) -> impl IntoView`. It shows the `{n} devices found` count, the groups, and one row per device with name, model and a room `<select>` (the hub's areas plus "No room"). Changing the room sends `set_override(entity_id, area patch)` for **each** entity whose `device_id` is this device's. With zero devices after `empty_after_ms`, it shows the `NoDevicesYet` copy with `{base}` = `z2m_base` (default `zigbee2mqtt` when empty) and a "Back to MQTT settings" button → step `Mqtt`. Next and Skip both call `put_step(Services)` → step `Services`.
  - Inline rename is **left out** of this plan. The spec calls it optional, and the device sheet already renames. Flagged for the reviewer.

- [ ] **Step 1: Write the failing tests**
  - Unit `rooms_sort_with_no_room_last`: devices in "Kitchen", "Living room" and none come out as Kitchen, Living room, None, and a device with two rooms sits under the first.
  - Browser `no_devices_points_back_to_the_broker`: an empty `House`, `empty_after_ms = 10`. After 50 ms the text contains `under zigbee2mqtt`, and clicking the button sets `WizardNav.step` to `Mqtt`.
  - Browser `choosing_a_room_moves_every_entity_of_the_device`: a device with two entities. Choosing `kitchen` records two `PUT` override requests, each with `"area":"kitchen"`. Use the same endpoint and body shape `api::set_override` sends today.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run unit and browser tests, bare_text, the wasm check and clippy, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): setup Devices step`

---

### Task 6: Services step, with Weather, Climate and Alarm

**Files:**
- Create: `src/setup/services/mod.rs`, `src/setup/services/basic.rs`
- Modify: `src/setup/wizard.rs`, `src/screens/settings.rs` (`SettingsPage::Weather`, slug `weather`), `src/i18n/setup.rs`, `src/i18n/settings.rs`
- Test: inline unit tests; `tests/setup_services.rs`

**Interfaces:**
- Produces (`services/mod.rs`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum CardState { NotSetUp, SetUp, Skipped }`, `pub fn card_state(set_up: bool, skipped: bool) -> CardState` (set up wins).
  - `pub const SERVICE_IDS: [&str; 7] = ["weather", "solar", "energy", "cameras", "push", "climate", "alarm"];` in the mockup's order.
  - `pub fn skipped() -> BTreeSet<String>` / `pub fn set_skipped(id: &str, on: bool)`: `localStorage["oikos.setup.skipped"]` as a JSON array, wrapped so that any storage failure reads as empty. Skipped is per browser (a deliberate choice: the hub has no field for it, and it only labels a card).
  - `pub fn with_fields(mut base: Value, fields: &[(&str, Value)]) -> Value`: overwrites the named top-level keys of the section's GET reply and keeps everything else.
  - `pub async fn save_section(section: &str, body: Value, applying_target: &str) -> Result<(), String>`: `put_section`, then `Applying::start(applying_target)` on success or `Restarting`. Every service form takes an `applying_target: String` prop: `setup_href(Services)` from the wizard, its own `/settings/<slug>` from Settings. On failure it returns the message.
  - `#[component] pub fn ServiceCard(id: &'static str, icon: &'static str, title: SetupText, desc: Signal<String>, state: Signal<CardState>, children: ChildrenFn) -> impl IntoView`: a collapsed row with a state pill. Opening it shows `children` and a Skip link that marks it skipped and closes it.
  - `#[component] pub fn ServicesStep() -> impl IntoView`: the seven cards, Back → `Devices`, Next → `put_step(Done)` → `Done`. The Energy, Solar, Cameras and Push card bodies are placeholders until Tasks 7 and 8.
- Produces (`services/basic.rs`), each loading its own section on mount:
  - `WeatherForm`: `enabled` switch and `refresh_minutes` (integer ≥ 1, else `MinutesInvalid`). Its line `Uses your home location: {place}` takes the place from `SetupState`. Set up = `enabled`.
  - `ClimateForm`: `enabled` switch plus the beta warning `Beta: it drives your heating. Check your schedules before turning it on.` Set up = `enabled`.
  - `AlarmForm`: `enabled` switch through `api::alarm::get_alarm` → `settings.enabled = x` → `put_alarm_settings` (no restart, no Applying), plus a link to `/settings/alarm`. Set up = `enabled`.
  - Settings › Weather renders `WeatherForm`.

- [ ] **Step 1: Write the failing tests**
  - Unit: `card_state(true, true) == SetUp`, `card_state(false, true) == Skipped`, `card_state(false, false) == NotSetUp`. `with_fields(json!({"enabled":false,"prefix":"oikos/climate"}), &[("enabled", json!(true))]) == json!({"enabled":true,"prefix":"oikos/climate"})`.
  - Browser `climate_keeps_its_prefix`: `GET /api/v1/settings/climate` → `{"enabled":false,"prefix":"house/climate"}`. Switch on and save; the recorded PUT body is `{"enabled":true,"prefix":"house/climate"}` and Applying starts.
  - Browser `alarm_switch_needs_no_restart`: stub `GET /api/v1/alarm` and its settings PUT (the same paths `api::alarm` uses). Switching on sends `enabled: true` and Applying stays `None`.
  - Browser `skip_marks_the_card`: Skip on Weather; the pill reads `Skipped`, and survives remounting the step.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run unit and browser tests, bare_text, the wasm check and clippy, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): setup Services step with weather, climate and alarm`

---

### Task 7: Energy card and the Home page's Energy panel

**Files:**
- Create: `src/setup/services/energy.rs`
- Modify: `src/screens/home_settings_panel.rs` (pull the off-peak list editor out into `pub fn OffPeakEditor` and use it in place; add an **Energy** panel holding `EnergyRolePicker` ×3 above it), `src/setup/services/mod.rs` (Energy card body), `src/i18n/setup.rs`
- Test: inline unit tests; `tests/setup_energy.rs`; the existing `tests/home_settings_panel.rs` must stay green unchanged

**Interfaces:**
- Consumes: `House.entities`, `oikos_core::energy::EnergyRole` (`Grid` = bought, `GridExport` = sold, `Solar` = production), `api::set_override` with an `energy_role` patch (`"energyRole"`; see `override_editor` line ~50), and `api::home::{get_home_settings, put_home_settings}`.
- Produces:
  - `pub fn role_candidates(entities: &[EntityView]) -> Vec<EntityView>`: sensors whose `device_class` is `energy` or `power`, sorted by name.
  - `pub fn holder(entities: &[EntityView], role: EnergyRole) -> Option<String>`: the entity whose override `energy_role` parses to `role`.
  - `#[component] pub fn EnergyRolePicker(role: EnergyRole) -> impl IntoView`: a `<select>` with "None" plus the candidates. Choosing one sets its `energyRole` and clears it (`null`) on the previous holder. Choosing None clears the holder. No hub restart.
  - `#[component] pub fn OffPeakEditor(...)`: the existing off-peak window editor exactly as it behaves today, with the props it needs to stay inside `HomeSettingsPanel`'s draft/save flow. The Energy card mounts `HomeSettingsPanel`'s off-peak part through it, saved with `put_home_settings` (whole document from `get_home_settings`, only `off_peak` changed).
  - Energy card: set up = `holder(Grid).is_some()`.

- [ ] **Step 1: Write the failing tests**
  - Unit: `role_candidates` keeps an `energy` sensor and a `power` sensor and drops a `temperature` sensor. `holder` finds the entity whose override is `"grid"`.
  - Browser `moving_a_role_clears_the_old_holder`: entity A holds `grid`, and choosing B for Grid bought records two override PUTs: A with `"energyRole":null`, B with `"energyRole":"grid"`.
  - Browser `off_peak_from_the_card_keeps_the_rest_of_home_settings`: `GET /api/v1/home/settings` returns a document with other fields set. Adding a window and saving records a PUT whose non-`off_peak` fields equal the GET.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement** (extract `OffPeakEditor` first and rerun `tests/home_settings_panel.rs` before adding anything)
- [ ] **Step 4: Run unit and browser tests (including `home_settings_panel`), bare_text, the wasm check and clippy, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): energy roles in setup and on the Home settings page`

---

### Task 8: Solar forecast, Cameras and Push cards, plus their Settings pages

**Files:**
- Create: `src/setup/services/keys.rs`
- Modify: `src/setup/services/mod.rs`, `src/screens/settings.rs` (`SettingsPage::SolarForecast` `solar-forecast`, `Cameras` `cameras`, `Push` `push-notifications`, after Weather), `src/i18n/setup.rs`, `src/i18n/settings.rs`, `Cargo.toml` (web-sys features `File`, `FileList` if missing)
- Test: inline unit tests; `tests/setup_keys.rs`

**Interfaces:**
- Consumes: `SolcastView`, `FrigateView` and `PushView` from `oikos_api` (deserialize the GET with them for display; build the PUT with `with_fields` on the raw `Value`), plus `test_service` and `save_section` from Task 6.
- Produces:
  - `pub fn parse_list(s: &str) -> Vec<String>`: splits on commas and whitespace, drops empty parts, keeps the first of each duplicate.
  - `pub fn project_id_of(json: &str) -> Option<String>`: the `project_id` string of a service-account JSON, `None` when it isn't JSON or has no such field.
  - `SolcastForm`: `enabled`, API key (secret; placeholder `SecretSaved` when set), Rooftop site ids (`parse_list`), Calls per day (integer ≥ 1). **Test (uses 1 call)** shows the message plus `detail.sites[].name`. Set up = `enabled && api_key.set`.
  - `FrigateForm`: URL, User, Password (secret), Media URL for the phone, Alerts (`bridge`), Alert on chips (`person`, `car`, `dog`, `cat`, plus any stored label not in that list), Live view (`live`). **Test** shows the message and `detail.cameras` as chips; toggling a chip edits `cameras` (empty = all). Set up = `url.is_some()`. The hub's 422 `Enter the password for the new address` shows as received.
  - `PushForm`: a file input (`accept=".json"`) read with `Blob::text()`. A file without a `project_id` shows `NotAServiceAccount` and isn't sent. Otherwise it sends `with_fields(view, [("service_account", text)])`. The card desc shows `project_id` once it's stored. Set up = `service_account.set`.
  - The three Settings pages render the same forms.

- [ ] **Step 1: Write the failing tests**
  - Unit: `parse_list("a, b  c,,a") == ["a","b","c"]`. `project_id_of(r#"{"project_id":"demo-123","type":"service_account"}"#) == Some("demo-123")`; `project_id_of("nope") == None`.
  - Browser `frigate_save_keeps_the_fields_it_does_not_show`: GET returns `zones:["porch"]`, `cooldown:90`, `severity:"alert"`, `ttl:7`, `topic_prefix:"frigate"`, `password:{set:true}`. Change the URL to `http://192.0.2.5:5000` and type a password. The recorded PUT has those five fields unchanged, `"password":"<typed>"` and the new URL.
  - Browser `an_empty_secret_field_keeps_the_stored_key`: Solcast GET with `api_key:{set:true}`. Toggle `enabled` and save; the PUT body has no `api_key` value other than `{"set":true}` or `""`, never `null`.
  - Browser `push_upload_sends_the_file_text`: set the file input's files through a `DataTransfer` holding `{"project_id":"demo-123"}` and save. The PUT `service_account` equals that text and keeps `prefix` from the GET.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run unit and browser tests, bare_text, the wasm check and clippy, expect PASS**
- [ ] **Step 5: Commit** — `feat(web): solar forecast, cameras and push in setup and Settings`

---

### Task 9: Done step and "Run setup again"

**Files:**
- Create: `src/setup/done.rs`
- Modify: `src/setup/wizard.rs`, `src/screens/settings.rs` (a `Run setup again` link to `/setup` under the Settings list), `src/i18n/setup.rs`
- Test: `tests/setup_done.rs`

**Interfaces:**
- Consumes: `SetupState`, `House.devices`, the card states from Task 6 (expose `pub fn service_states() -> Signal<Vec<(&'static str, CardState)>>` from `services/mod.rs`, read from the same sources the cards use).
- Produces: `#[component] pub fn DoneStep() -> impl IntoView`. Summary rows, each with a ✓ or a muted mark:
  - Location: `place, country · tz`
  - MQTT broker: `host:port`
  - Database: `host:port / database`
  - Devices: `{n} found, {m} in rooms`
  - one row per service: `Set up`, `Skipped` or `Not set up`

  **Open my house** calls `finish()`, sets `SetupState` to the reply, then navigates to `app::landing_path()`. A `finish` error shows its message.

- [ ] **Step 1: Write the failing tests**
  - Browser `open_my_house_finishes_and_leaves`: stub `POST /api/v1/setup/finish` → status with `finished:true`. Click; the POST is recorded, and the path becomes `/dashboards/house`.
  - Browser `summary_shows_skipped_services`: mark `cameras` skipped, mount; the Cameras row reads `Skipped`.
  - Browser `run_setup_again_is_listed`: mount `SettingsScreen`; a link with `href="/setup"` exists.
- [ ] **Step 2: Run, expect FAIL**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run the whole web suite (`cargo test -p oikos-web`, `wasm-pack test --headless --chrome`), the wasm check and clippy, expect PASS**
- [ ] **Step 5: Manual pass** on a local hub: `cargo run -p oikos-os -- --data ~/dev-scratch/oikos-setup-demo` with `docker compose up -d mosquitto timescaledb` (or any reachable broker and TimescaleDB). Go through Welcome → Done in both themes and both languages, at 1440 px and at 390 px. Note anything off in the PR description.
- [ ] **Step 6: Commit** — `feat(web): setup Done step and Run setup again`
