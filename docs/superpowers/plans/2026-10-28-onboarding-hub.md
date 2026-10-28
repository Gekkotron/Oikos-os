# Onboarding — Hub Implementation Plan (plan 1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The hub boots from `/data/hub.json` plus a TimescaleDB `settings` table instead of `oikos.toml`. It imports an existing `oikos.toml` once, runs a setup mode (HTTP only) until MQTT and the database are configured, and exposes the setup and settings API that the web wizard (plan 2) will call.

**Architecture:** A new `oikos_os::setup` module turns `hub.json` and the settings rows (or their cache file) into today's `Config`. The rest of `main.rs` keeps starting the hub exactly as it does now. Any change that needs new settings re-executes the process (`exec` of its own binary), so `Hub` stays start-once. The setup and settings routes have their own state (`Arc<SetupCtx>`) and are merged into the router in both modes.

**Tech Stack:** Rust, axum, sqlx (Postgres/TimescaleDB), figment (env overlay and import only), rmcp, `if-addrs` (new, for the banner).

**Spec:** `docs/superpowers/specs/2026-10-28-onboarding-design.md`

Plans 2 (web wizard and Settings pages) and 3 (compose, `init-bundled`, `update.sh`, docs) follow this one. Plan 3 writes `./data/bundled.json` in the shape fixed in Task 7.

## Global Constraints

- `hub.json`, `settings-cache.json` and (plan 3) `bundled.json` are written with mode `0600`, atomically (write `*.tmp`, then `rename`).
- A secret is **write-only** over HTTP and MCP. Reads return `{"set": bool}`, never the value. `Debug` impls redact them, as `MqttConfig` already does.
- In a `PUT`, a secret field that's absent keeps the stored value, `null` clears it, and a string sets it.
- A `PUT` that changes a service's URL or host without sending its secret is refused: `422 {"error":"Enter the password for the new address"}`.
- `OIKOS_*` env overrides apply only to bootstrap keys: `mqtt.*`, `history.url`, `http.*`, `log.*`.
- A database that's down never stops the house: Normal mode boots from `settings-cache.json`.
- Log line after the import copy finishes, exactly: `oikos.toml imported into the database; it is no longer read and can be deleted`.
- Banner, exactly: `Open http://<ip>:<port> to set up your home` (setup not finished) / `Open http://<ip>:<port> to open your home` (finished).
- Never name the maintainer's own machines, IPs, rooms or family in tracked files (tests use `192.0.2.x` / `example.com`).
- Commits use `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`.

## Review Focus

1. **A corrupt or half-written `hub.json`** must refuse to start, naming the file. Falling back to setup mode would look like the house was wiped. Owned by Task 1.
2. **The database is down at the first start after the upgrade.** The hub must boot Normal from the imported values and finish the copy later. Owned by Task 6.
3. **Wrong MQTT credentials sent to `PUT /setup/mqtt` on a running hub** must not be saved, or the live house would restart into a broker it can't reach. Owned by Task 7.
4. **A secret leaks through a GET, MCP, the cache path in a log, or `Debug`.** Owned by Tasks 2, 8 and 9.
5. **A second `PUT` that arrives while a restart is pending** (for example, a double click) must not corrupt `hub.json`. The writes are serialized through `SetupCtx`'s mutex and the file is written atomically. Owned by Task 7.

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/oikos-api/src/setup.rs` (new) | Wire types shared with the web: `Location`, `SetupStep`, `SetupStatus`, `SecretView`, `TestResult`, `GeoPlace`, the section views and inputs. |
| `crates/oikos-os/src/setup/mod.rs` (new) | `SetupCtx`, `Mode`, re-exports. |
| `crates/oikos-os/src/setup/hub_file.rs` (new) | `HubFile`: load, save, env overlay, completeness. |
| `crates/oikos-os/src/setup/sections.rs` (new) | `Sections` ⇄ settings rows; `assemble(&HubFile, &Sections) -> Config`. |
| `crates/oikos-os/src/setup/secret.rs` (new) | `SecretInput` (keep/clear/set) and the new-address rule. |
| `crates/oikos-os/src/setup/store.rs` (new) | The `settings` table and `settings-cache.json`. |
| `crates/oikos-os/src/setup/import.rs` (new) | `oikos.toml` → `(HubFile, Sections)`. |
| `crates/oikos-os/src/setup/boot.rs` (new) | Decides Setup or Run at start; the background import copy. |
| `crates/oikos-os/src/setup/restart.rs` (new) | `schedule_restart()`: re-exec self. |
| `crates/oikos-os/src/setup/probe.rs` (new) | Connection tests: MQTT, database, Solcast, Frigate. |
| `crates/oikos-os/src/setup/routes.rs` (new) | `/api/v1/setup/*` and `/api/v1/settings/*`. |
| `crates/oikos-os/src/setup/banner.rs` (new) | LAN addresses and the banner lines. |
| `crates/oikos-os/migrations/0004_settings.sql` (new) | The `settings` table. |
| `crates/oikos-os/src/config.rs` | `SolcastConfig.api_key`, `PushConfig.service_account`, `WeatherConfig.latitude/longitude`, `Config::with_bootstrap_env`. |
| `crates/oikos-os/src/weather/{open_meteo,poller}.rs` | Geocoding returns the time zone; the poller uses the configured coordinates. |
| `crates/oikos-push/src/fcm.rs` | `ServiceAccount::from_json`. |
| `crates/oikos-os/src/{main.rs,cli.rs,hub.rs,http/mod.rs,mcp/tools.rs}` | Wiring. |
| `Dockerfile`, `compose.yml` | `OIKOS_DATA_DIR=/data` and the `./data:/data` volume (the TOML mount stays until plan 3). |

---

### Task 1: `HubFile`, the bootstrap file

**Files:**
- Create: `crates/oikos-api/src/setup.rs` (start it with `Location`, `SetupStep`), `crates/oikos-os/src/setup/mod.rs`, `crates/oikos-os/src/setup/hub_file.rs`
- Modify: `crates/oikos-api/src/lib.rs`, `crates/oikos-os/src/lib.rs` (`pub mod setup;`), `crates/oikos-os/src/config.rs` (add `Config::with_bootstrap_env`)
- Test: inline `#[cfg(test)]` in `hub_file.rs`

**Interfaces:**
- Produces (`oikos_api::setup`):
  - `pub struct Location { pub city: String, pub country: Option<String>, pub latitude: f64, pub longitude: f64, pub timezone: String }` (Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)
  - `pub enum SetupStep { Welcome, Mqtt, Database, Devices, Services, Done }`, serde `snake_case`, `Default = Welcome`
- Produces (`oikos_os::setup::hub_file`):
  - `pub struct HubFile { pub mqtt: Option<MqttConfig>, pub z2m_base: Option<String>, pub database_url: Option<String>, pub location: Option<Location>, pub pending_city: Option<(String, Option<String>)>, pub http: HttpConfig, pub log: LogConfig, pub step: SetupStep, pub finished: bool, pub settings_imported: bool }`. `pending_city` is a city (and country) to geocode into `location`, set only by the import., with `#[serde(default)]` and a `Debug` that masks the password in `database_url` (reuse the masking from `HistoryConfig`'s `Debug`)
  - `pub const HUB_FILE: &str = "hub.json";`
  - `pub fn load(dir: &Path) -> Result<Option<HubFile>, HubFileError>`: `Ok(None)` when missing; `Err` (naming the path) when unreadable or invalid JSON
  - `pub fn save(&self, dir: &Path) -> std::io::Result<()>`: atomic, `0600`, creates `dir`
  - `pub fn is_complete(&self) -> bool`: `mqtt.is_some() && database_url.is_some()`
  - `pub fn with_bootstrap_env(self) -> Result<HubFile, figment::Error>`
- Produces (`config.rs`): `pub const BOOTSTRAP_ENV_PREFIXES: &[&str] = &["mqtt.", "history.url", "http.", "log."];`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn missing_file_is_none() { assert!(HubFile::load(tmp.path()).unwrap().is_none()); }
#[test] fn save_then_load_round_trips() { /* full HubFile with mqtt+location; load == saved */ }
#[cfg(unix)] #[test] fn saved_file_is_0600() { assert_eq!(meta.permissions().mode() & 0o777, 0o600); }
#[test] fn invalid_json_is_an_error_naming_the_file() {
    std::fs::write(dir.join("hub.json"), "{\"mqtt\":").unwrap();
    let err = HubFile::load(dir).unwrap_err().to_string();
    assert!(err.contains("hub.json"));
}
#[test] fn complete_needs_mqtt_and_database() { /* mqtt only → false; both → true */ }
#[test] fn debug_hides_passwords() {
    // mqtt.password = "s3cret", database_url = "postgres://u:dbpass@h/db"
    let s = format!("{file:?}");
    assert!(!s.contains("s3cret") && !s.contains("dbpass"));
}
#[test] fn env_overrides_bootstrap_keys_only() {
    figment::Jail::expect_with(|jail| {
        jail.set_env("OIKOS_MQTT__HOST", "192.0.2.5");
        jail.set_env("OIKOS_HISTORY__URL", "postgres://u:p@192.0.2.6/oikos");
        jail.set_env("OIKOS_WEATHER__ENABLED", "true");   // not bootstrap: ignored
        let f = HubFile::default().with_bootstrap_env().unwrap();
        assert_eq!(f.mqtt.unwrap().host, "192.0.2.5");
        assert_eq!(f.database_url.as_deref(), Some("postgres://u:p@192.0.2.6/oikos"));
        Ok(())
    });
}
```

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::hub_file`. Expected: compile errors (module missing).
- [ ] **Step 3: Implement.** `with_bootstrap_env` only fills `mqtt` when at least one `OIKOS_MQTT__*` variable is set, starting from `MqttConfig::default()`. It maps `OIKOS_HISTORY__URL` → `database_url`, and reuses the `STRING_ENV_KEYS` raw-string rule so passwords like `123456` stay strings.
- [ ] **Step 4: Run them and check they pass.** Run `cargo test -p oikos-os setup::hub_file`. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(setup): hub.json bootstrap file`.

### Task 2: `Sections` and assembling a `Config`

**Files:**
- Create: `crates/oikos-os/src/setup/sections.rs`
- Modify: `crates/oikos-os/src/config.rs`, `crates/oikos-push/src/fcm.rs`, `crates/oikos-os/src/hub.rs` (`push_transport`), `crates/oikos-os/src/solar/poller.rs`
- Test: inline in `sections.rs`; existing `tests/config.rs` and `tests/solar_poller.rs` keep passing

**Interfaces:**
- Consumes: `HubFile`, `Location` (Task 1).
- Produces (`config.rs` changes):
  - `SolcastConfig.api_key: Option<String>`; `read_key()` returns it when set, otherwise reads the file as today. `SolcastConfig` gets a hand-written `Debug` that masks it.
  - `PushConfig.service_account: Option<String>` (the key JSON); `push_transport` uses `ServiceAccount::from_json` when it's set, otherwise `credentials` as today. A hand-written `Debug` masks it.
  - `WeatherConfig.latitude: Option<f64>`, `WeatherConfig.longitude: Option<f64>` (used in Task 4).
- Produces (`oikos_push::fcm`): `pub fn from_json(raw: &str) -> Result<ServiceAccount, serde_json::Error>`.
- Produces (`oikos_os::setup::sections`):
  - `pub struct WeatherSection { pub enabled: bool, pub refresh_minutes: u64 }` (default `false`, `15`)
  - `pub struct SolcastSection { pub enabled: bool, pub sites: Vec<String>, pub daily_limit: u32, pub api_key: Option<String> }` (defaults as `SolcastConfig`)
  - `pub struct PushSection { pub service_account: Option<String>, pub prefix: String, pub legacy_prefix: String, pub retry_interval: u64, pub max_retries: u32, pub history_size: usize, pub history_days: u64 }`
  - `pub struct AdvancedSection { pub topics: TopicsConfig, pub commands: CommandsConfig, pub automation: AutomationConfig, pub watchdog: WatchdogConfig, pub groups: GroupsConfig, pub alarm: AlarmConfig, pub history_buffer_rows: usize }`
  - `pub struct Sections { pub weather: WeatherSection, pub solcast: SolcastSection, pub frigate: FrigateConfig, pub push: PushSection, pub climate: ClimateConfig, pub advanced: AdvancedSection }` (all `#[serde(default)]`, Clone, PartialEq)
  - `pub const SECTION_KEYS: [&str; 6] = ["weather", "solcast", "frigate", "push", "climate", "advanced"];`
  - `pub fn to_rows(&self) -> Vec<(&'static str, serde_json::Value)>`
  - `pub fn from_rows(rows: impl IntoIterator<Item = (String, serde_json::Value)>) -> Sections`: an unknown key is ignored; a row that doesn't parse falls back to that section's default, with `tracing::warn!` naming the key
  - `pub fn assemble(file: &HubFile, s: &Sections) -> Config`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn rows_round_trip() { let s = sample(); assert_eq!(Sections::from_rows(owned(s.to_rows())), s); }
#[test] fn missing_rows_are_defaults() { assert_eq!(Sections::from_rows(vec![]), Sections::default()); }
#[test] fn bad_row_falls_back_to_that_section_only() { /* weather row = json!(5), solcast valid → weather default, solcast kept */ }
#[test] fn assemble_maps_location_and_bootstrap() {
    let cfg = assemble(&file_with(Location { city: "Lyon".into(), country: Some("FR".into()),
        latitude: 45.76, longitude: 4.84, timezone: "Europe/Paris".into() }), &Sections::default());
    assert_eq!(cfg.flows.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!((cfg.flows.latitude, cfg.flows.longitude), (Some(45.76), Some(4.84)));
    assert_eq!(cfg.weather.city, "Lyon");
    assert_eq!((cfg.weather.latitude, cfg.weather.longitude), (Some(45.76), Some(4.84)));
    assert_eq!(cfg.history.url, file.database_url.unwrap());
    assert_eq!(cfg.topics.z2m_base, "zigbee2mqtt"); // file.z2m_base None → advanced.topics default
}
#[test] fn assemble_z2m_base_from_file_wins() { /* file.z2m_base = Some("z2m") → cfg.topics.z2m_base == "z2m" */ }
#[test] fn solcast_api_key_beats_the_file() { /* api_key Some("k") + api_key_file nonexistent → read_key() == Ok("k") */ }
#[test] fn debug_hides_service_account_and_api_key() { /* format!("{cfg:?}") has neither value */ }
```

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::sections`. Expected: compile errors.
- [ ] **Step 3: Implement** `from_json` and the config fields, then `assemble`. `assemble` copies `HubFile.mqtt` (or the default), `http`, `log`, `database_url` → `history.url`, `advanced.history_buffer_rows` → `history.buffer_rows`, and location → `flows.{timezone,latitude,longitude}` + `weather.{city,country,latitude,longitude}`. It leaves `credentials` / `api_key_file` as `None`.
- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test -p oikos-os setup::sections && cargo test -p oikos-os --test config && cargo test -p oikos-push`. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(setup): settings sections assemble the hub config`.

### Task 3: The `settings` table and its cache

**Files:**
- Create: `crates/oikos-os/migrations/0004_settings.sql`, `crates/oikos-os/src/setup/store.rs`, `crates/oikos-os/tests/settings_db.rs`, `crates/oikos-os/tests/common/db.rs`
- Modify: `crates/oikos-os/src/history/db.rs` (add `pub fn pool(&self) -> &PgPool`), `crates/oikos-os/tests/history_db.rs` (use `common::db::TestDb`, moved from there unchanged)

**Interfaces:**
- Consumes: `Sections` (Task 2), `Db` (existing).
- Produces:
  - SQL: `CREATE TABLE settings (key text PRIMARY KEY, value jsonb NOT NULL, updated_at timestamptz NOT NULL DEFAULT now());`
  - `pub async fn load_sections(db: &Db) -> Result<Sections, sqlx::Error>`
  - `pub async fn save_sections(db: &Db, s: &Sections) -> Result<(), sqlx::Error>`: every row upserted in one transaction, `updated_at = now()`
  - `pub const CACHE_FILE: &str = "settings-cache.json";`
  - `pub fn read_cache(dir: &Path) -> Option<Sections>`: `None` when missing or unparsable (with a warning)
  - `pub fn write_cache(dir: &Path, s: &Sections) -> std::io::Result<()>`: atomic, `0600`

- [ ] **Step 1: Write the failing tests.** In `tests/settings_db.rs`, these skip when `TEST_DATABASE_URL` is unset, like `history_db.rs`:

```rust
#[tokio::test] async fn empty_table_loads_defaults() { assert_eq!(load_sections(&db).await.unwrap(), Sections::default()); }
#[tokio::test] async fn save_then_load_round_trips() { /* sample with solcast api_key + frigate password */ }
#[tokio::test] async fn save_twice_overwrites() { /* second save with weather.enabled flipped wins */ }
```

  Inline in `store.rs`: `cache_round_trips`, `cache_is_0600`, `corrupt_cache_is_none`.

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::store` and `TEST_DATABASE_URL=… cargo test -p oikos-os --test settings_db`. Expected: compile errors.
- [ ] **Step 3: Implement** the migration and the functions above. Use one `INSERT … ON CONFLICT (key) DO UPDATE` per section inside the transaction.
- [ ] **Step 4: Run the tests and check they pass.** Run the same commands, plus `cargo test -p oikos-os --test history_db`. Expected: PASS (DB tests skip without the variable; CI runs them).
- [ ] **Step 5: Commit** with message `feat(setup): settings table and cache file`.

### Task 4: Weather from the configured location; geocoding returns the time zone

**Files:**
- Modify: `crates/oikos-os/src/weather/open_meteo.rs`, `crates/oikos-os/src/weather/poller.rs`
- Test: `crates/oikos-os/tests/weather_client.rs`, `crates/oikos-os/tests/weather_poller.rs` (existing patterns, mock endpoints)

**Interfaces:**
- Produces: `Place.timezone: Option<String>` (from Open-Meteo's `timezone` result field); `pub async fn geocode_many(client, ep, city, country, count: u8) -> Result<Vec<Place>, FetchError>` (`geocode` becomes `geocode_many(..,1)` + first).
- Poller: when `WeatherConfig.latitude` and `longitude` are both `Some`, it builds the `Place` from them and `city`/`country` and makes **no** geocoding request; otherwise it behaves as today.

- [ ] **Step 1: Write the failing tests.** `geocode_reads_the_timezone` (fixture result with `"timezone":"Europe/Paris"`). Then `poller_with_coordinates_skips_geocoding`: the mock geocoding endpoint counts 0 calls, the forecast request carries `latitude=45.76&longitude=4.84`.
- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os --test weather_client --test weather_poller`. Expected: FAIL.
- [ ] **Step 3: Implement** both changes.
- [ ] **Step 4: Run the tests and check they pass.** Same command. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(weather): use the configured location; geocoding returns the time zone`.

### Task 5: Import `oikos.toml`

**Files:**
- Create: `crates/oikos-os/src/setup/import.rs`
- Test: inline, with `figment::Jail` and temp dirs

**Interfaces:**
- Consumes: `Config` (existing loader and validators), `HubFile`, `Sections`.
- Produces:
  - `pub fn import_toml(path: &Path) -> anyhow::Result<(HubFile, Sections)>`. It runs `Config::load(Some(path))`, then the same `validate()` calls `main.rs` makes today (move them into `pub fn Config::validate_all(&self) -> Result<(), String>` and call that from both places). Then it maps everything:
    - `HubFile`: `mqtt`, `z2m_base = Some(topics.z2m_base)`, `database_url` (`None` when `history.url` is empty), `http`, `log`, `finished = true` only when `database_url` is `Some`, `step = Done` (or `Database` when the URL is missing), `settings_imported = false`
    - `location = Some(..)` only when `[flows]` has both coordinates: `city`/`country` from `[weather]` (empty city → `""`), `timezone` from `[flows] timezone` or the system zone (`iana_time_zone`). Without coordinates but with a non-empty `[weather] city`, `pending_city = Some((city, country))` instead.
    - `Sections`: every section is copied, including `push.service_account` = the content of `cfg.push.credentials` when that file is readable, and `solcast.api_key` = `cfg.solcast.read_key().ok()`
  - `pub fn toml_candidate(data_dir: &Path, flag: Option<&Path>) -> Option<PathBuf>`: the `--config` path if that file exists, else `<data_dir>/oikos.toml` if it exists, else `None`. A missing file never imports, so the Dockerfile's default `--config` stays harmless.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn imports_mqtt_history_and_location() { /* TOML with [mqtt] host 192.0.2.1, [history] url, [flows] lat/lon/tz, [weather] city → fields mapped, finished == true, step == Done */ }
#[test] fn no_history_url_means_database_step() { /* database_url None, finished false, step Database */ }
#[test] fn no_coordinates_means_pending_city() { /* location None, pending_city == Some(("Lyon", Some("FR"))) */ }
#[test] fn reads_the_key_files() { /* secrets/firebase.json + secrets/solcast.key next to the toml → service_account, api_key set */ }
#[test] fn invalid_toml_is_refused_with_todays_message() { /* [climate] enabled + prefix "#" → Err containing "[climate] prefix" */ }
#[test] fn candidate_ignores_missing_files() { /* flag points nowhere, no data/oikos.toml → None */ }
```

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::import`. Expected: compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run them and check they pass.** Same command. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(setup): import oikos.toml into hub.json and sections`.

### Task 6: Boot, the background import copy, re-exec, `--data`

**Files:**
- Create: `crates/oikos-os/src/setup/boot.rs`, `crates/oikos-os/src/setup/restart.rs`
- Modify: `crates/oikos-os/src/setup/mod.rs`, `crates/oikos-os/src/cli.rs`, `crates/oikos-os/src/main.rs`, `Dockerfile` (`ENV OIKOS_DATA_DIR=/data`), `compose.yml` (add `- ./data:/data` to `hub.volumes`; keep the TOML and secrets mounts until plan 3), `.gitignore` (`/data/`)
- Test: inline in `boot.rs` and `cli.rs`; `tests/settings_db.rs` for the DB paths

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
  - `cli.rs`: `Cli::Run { config: Option<PathBuf>, data_dir: PathBuf }`; `--data <DIR>` global, `env = "OIKOS_DATA_DIR"`, default `./data`.
  - `pub enum Mode { Setup, Run }`
  - `pub struct SetupCtx { pub data_dir: PathBuf, pub mode: Mode, pub file: tokio::sync::Mutex<HubFile>, pub db: Option<Db>, pub sections: tokio::sync::Mutex<Sections>, pub restarting: AtomicBool, pub restart: RestartHook }` (in `setup/mod.rs`)
  - `pub enum Boot { Setup(Arc<SetupCtx>), Run { cfg: Config, ctx: Arc<SetupCtx> } }`
  - `pub async fn resolve(data_dir: &Path, config_flag: Option<&Path>) -> anyhow::Result<Boot>`. The algorithm:
    1. `HubFile::load`. If it's `None` and `toml_candidate` finds a file: `import_toml`, `save` the file, `write_cache` the sections.
    2. `with_bootstrap_env()`. If `!is_complete()` → `Boot::Setup`.
    3. `db = Db::connect_lazy(url)`. If `!settings_imported`: sections come from the cache (or defaults), and `spawn_import_copy` runs. Otherwise, try `migrate` + `load_sections` within **5 s**. On success, `write_cache`. On failure or timeout, use `read_cache` with `warn!("settings: database unreachable, starting from the cached copy")`, or defaults with a second warning when there's no cache.
    4. `Boot::Run { cfg: assemble(&file, &sections), .. }`.
  - `pub fn spawn_import_copy(ctx: Arc<SetupCtx>) -> JoinHandle<()>`: retries `migrate` + `save_sections` with the history writer's backoff. On success it sets `settings_imported = true`, saves the file, and logs the exact import line.
  - `pub fn spawn_location_lookup(ctx: Arc<SetupCtx>) -> JoinHandle<()>`: spawned in Run mode when `pending_city` is set. It geocodes the city with Task 4's `geocode_many` and the weather poller's backoff. On a match it stores `location` (the time zone is the result's, or the `[flows]` one when the result has none), clears `pending_city`, saves, and calls `schedule_restart`. When nothing matches, it clears `pending_city` and logs a warning. Until then the old Paris fallback applies, as today.
  - `restart.rs`: `pub enum RestartHook { Exec, Record(Arc<AtomicUsize>) }` (a `SetupCtx` field, `Exec` in `main.rs`, `Record` in tests). `pub fn schedule_restart(ctx: &SetupCtx, after: Duration)`. Only the first call while `restarting` is false does anything. With `Record`, it only increments the count. It sleeps for `after`, then `exec`s `std::env::current_exe()` with `std::env::args_os().skip(1)` (`std::os::unix::process::CommandExt::exec`). If `exec` returns, it logs an error and `std::process::exit(1)`.
  - `main.rs`: `Cli::Run` calls `resolve`. `Boot::Run` keeps today's start sequence (with `cfg.validate_all()` replacing the five `validate` calls) and stores `ctx` for Task 7. `Boot::Setup` initialises logging from `ctx.file.http/log`, serves only the setup router (Task 7) and waits for Ctrl-C. `Cli::McpStdio` reads the log level from `<OIKOS_DATA_DIR or ./data>/hub.json` when present, and uses defaults otherwise.

- [ ] **Step 1: Write the failing tests** (inline `boot.rs`, temp dirs; the DB cases go in `tests/settings_db.rs` behind `TEST_DATABASE_URL`):

```rust
#[tokio::test] async fn empty_dir_is_setup_mode() { assert!(matches!(resolve(tmp, None).await?, Boot::Setup(_))); }
#[tokio::test] async fn toml_without_history_is_setup_mode_with_mqtt_kept() { /* Boot::Setup, ctx.file.mqtt is Some */ }
#[tokio::test] async fn import_with_unreachable_db_boots_run_from_the_toml() {
    // toml with history.url = postgres://u:p@127.0.0.1:1/oikos (nothing listens), [weather] enabled
    let Boot::Run { cfg, .. } = resolve(tmp, Some(&toml)).await? else { panic!() };
    assert!(cfg.weather.enabled);
    assert!(tmp.join("hub.json").exists() && tmp.join("settings-cache.json").exists());
}
#[tokio::test] async fn complete_file_and_db_down_uses_the_cache() { /* settings_imported=true, cache has weather enabled → cfg.weather.enabled */ }
#[tokio::test] async fn corrupt_hub_file_is_an_error() { assert!(resolve(tmp, None).await.is_err()); }
#[tokio::test] async fn pending_city_is_geocoded_then_restarts() { /* mock geocoding endpoint; spawn_location_lookup → location set, pending_city None, restart count 1 */ }
// tests/settings_db.rs
#[tokio::test] async fn import_copy_fills_the_table_and_sets_the_flag() { /* after spawn_import_copy: load_sections == imported; HubFile.settings_imported */ }
```

  In `cli.rs`: `data_defaults_to_dot_data`, `data_flag_and_env`.

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::boot cli`. Expected: FAIL / compile errors.
- [ ] **Step 3: Implement** `boot.rs`, `restart.rs`, the CLI and `main.rs` wiring, and the Docker and compose lines.
- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test -p oikos-os`. Expected: PASS. Run `cargo run -p oikos-os -- --data /tmp/oikos-empty`; expected: the hub stays up, logs `http listening`, and doesn't connect to MQTT.
- [ ] **Step 5: Commit** with message `feat(setup): boot from hub.json, setup mode, background import copy`.

### Task 7: The setup API (location, MQTT, database, progress)

**Files:**
- Create: `crates/oikos-os/src/setup/secret.rs`, `crates/oikos-os/src/setup/probe.rs` (MQTT and database probes), `crates/oikos-os/src/setup/routes.rs`, `crates/oikos-os/src/setup/bundled.rs`, `crates/oikos-os/tests/setup_http.rs`
- Modify: `crates/oikos-api/src/setup.rs`, `crates/oikos-os/src/http/mod.rs` (`router` takes `ctx: Arc<SetupCtx>` and merges `setup::routes::router(ctx)`; new `pub fn setup_router(ctx) -> Router`, which is the setup routes, `/health` and the static UI fallback), `crates/oikos-os/src/main.rs`

**Interfaces:**
- Consumes: `SetupCtx`, `schedule_restart`, `geocode_many` (Task 4).
- Produces (`oikos_api::setup`):
  - `pub struct SecretView { pub set: bool }`
  - `pub struct TestResult { pub ok: bool, pub message: String, pub detail: Option<serde_json::Value> }`
  - `pub struct GeoPlace { pub name: String, pub admin: Option<String>, pub country: Option<String>, pub latitude: f64, pub longitude: f64, pub timezone: Option<String> }`
  - `pub struct MqttInput { pub host: String, pub port: u16, pub user: Option<String>, pub password: Secret, pub client_id: Option<String>, pub z2m_base: Option<String> }`
  - `pub struct MqttView { pub host, pub port, pub user, pub password: SecretView, pub client_id, pub z2m_base }`
  - `pub struct DatabaseInput { pub host: String, pub port: u16, pub database: String, pub user: String, pub password: Secret }`, plus `DatabaseView` (same fields, `password: SecretView`)
  - `pub struct SetupStatus { pub mode: String /* "setup"|"running" */, pub step: SetupStep, pub finished: bool, pub bundled: bool, pub location: Option<Location>, pub mqtt: Option<MqttView>, pub database: Option<DatabaseView>, pub restarting: bool }`
  - `pub enum Secret { Keep, Clear, Set(String) }`, with serde: absent → `Keep` (`#[serde(default)]`), `null` → `Clear`, string → `Set`. Write `deserialize_with` by hand; no new dependency.
- Produces (`oikos_os::setup`):
  - `secret::pub fn require_for_new_address(old: Option<&str>, new: &str, secret: &Secret) -> Result<(), &'static str>`: `Err("Enter the password for the new address")` when `old.is_some() && old != Some(new)` and `secret` is `Keep`.
  - `secret::pub fn apply(stored: Option<String>, input: Secret) -> Option<String>`
  - `bundled::pub struct Bundled { pub mqtt_user: String, pub mqtt_password: String, pub db_user: String, pub db_password: String, pub db_name: String }`, and `pub fn load(dir: &Path) -> Option<Bundled>` (`bundled.json`). Plan 3 writes this file.
  - `probe::pub async fn mqtt(cfg: &MqttConfig) -> TestResult`: connects with a 5 s timeout and a client id of `<client_id>-probe`. It waits for ConnAck and reports `refused`, `bad credentials`, `timed out` or `connected`.
  - `probe::pub async fn database(url: &str) -> TestResult`: connects (5 s). It checks `SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb'` (no row → `ok:false`, "TimescaleDB extension is not available"), runs `Db::migrate`, and puts `detail = {"history_since": <min(time) of entity_state or null>}`.
  - Routes (all JSON, under `/api/v1`):

| Route | Behaviour |
| --- | --- |
| `GET /setup` | `SetupStatus`; `bundled` = `bundled::load(dir).is_some()`; when the file has no mqtt or database yet, views are pre-filled from `Bundled` with `password.set = true`. |
| `GET /setup/geocode?city=&country=` | `Vec<GeoPlace>`, at most 5. |
| `PUT /setup/location` | Body `Location`. Saves; `step = max(step, Mqtt)`. In Run mode: `schedule_restart(500 ms)`. |
| `POST /setup/test/mqtt` | Body `MqttInput`; the secret resolves against the stored one (or `Bundled`); runs `probe::mqtt`. |
| `PUT /setup/mqtt` | Applies the new-address rule (`422`), then probes. A failed probe → `422 TestResult` and the file is **unchanged**. Otherwise saves `mqtt` + `z2m_base`, `step = max(step, Database)`; restarts when in Run mode. |
| `POST /setup/test/database`, `PUT /setup/database` | Same pattern with `probe::database`. Builds the URL with percent-encoded user and password. Saves `database_url`. Restarts when the file becomes complete (leaving setup mode) or when in Run mode. |
| `PUT /setup/step` | Body `{"step": SetupStep}`; saves it (never moves `finished`). |
| `POST /setup/finish` | `finished = true`, `step = Done`. |

  Every write takes `ctx.file.lock()`, edits a clone, `save`s it, then replaces the locked value. While `ctx.restarting` is true, writes answer `503 {"error":"restarting"}`.

- [ ] **Step 1: Write the failing tests.** Inline `secret.rs`: `absent_is_keep`, `null_is_clear`, `string_is_set`, `new_host_without_password_is_refused`, `same_host_keeps_password`. In `tests/setup_http.rs`, serve `setup_router` on `127.0.0.1:0` over a temp dir, following `tests/http.rs`:

```rust
#[tokio::test] async fn status_in_empty_dir() { /* mode "setup", step "welcome", bundled false, mqtt null */ }
#[tokio::test] async fn bundled_prefills_views_without_leaking() {
    // write bundled.json; GET /setup → mqtt.user == "oikos", mqtt.password == {"set":true}; body has no bundled password text
}
#[tokio::test] async fn location_put_advances_to_mqtt() { /* PUT /setup/location → GET step == "mqtt"; hub.json has the location */ }
#[tokio::test] async fn failed_mqtt_probe_saves_nothing() {
    // PUT /setup/mqtt host 127.0.0.1 port <closed port> → 422, ok false; hub.json unchanged
}
#[tokio::test] async fn mqtt_new_host_without_password_is_422() { /* stored mqtt host A with password; PUT host B, no password field → 422 "Enter the password for the new address" */ }
#[tokio::test] async fn step_put_never_finishes() { /* PUT /setup/step {"step":"done"} → finished stays false */ }
```

  In `tests/settings_db.rs` (DB-gated): `database_probe_reports_timescale_and_history_since`.
- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os --test setup_http setup::secret`. Expected: compile errors.
- [ ] **Step 3: Implement** the types, probes, routes, and the router merge. In `main.rs`, `Boot::Setup` serves `http::setup_router(ctx)` and `Boot::Run` serves `http::router(hub, ctx)`.
- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test -p oikos-os`. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(setup): setup API for location, MQTT and the database`.

### Task 8: The settings API for services

**Files:**
- Modify: `crates/oikos-api/src/setup.rs`, `crates/oikos-os/src/setup/routes.rs`, `crates/oikos-os/src/setup/probe.rs` (Solcast and Frigate probes)
- Test: `crates/oikos-os/tests/setup_http.rs`, `crates/oikos-os/tests/settings_db.rs`

**Interfaces:**
- Consumes: `Sections`, `save_sections`, `write_cache`, `assemble`, `Config::validate_all`, `secret::*`, `schedule_restart`.
- Produces (`oikos_api::setup`): views and inputs per section. Views replace each secret with `SecretView`; inputs replace it with `Secret`; other fields are as in `Sections`:
  - `SolcastView` / `SolcastInput` (`api_key`)
  - `FrigateView` / `FrigateInput` (`password`; `url`, `user`, `media_url`, `bridge`, `labels`, `cameras`, `zones`, `cooldown`, `severity`, `ttl`, `topic_prefix`, `live`)
  - `PushView` (`service_account: SecretView`, `project_id: Option<String>` read from the stored key JSON's `project_id`) / `PushInput` (`service_account: Secret`, other fields as `PushSection`)
  - `weather`, `climate`, `advanced` have no secrets: the section JSON as is
- Routes:

| Route | Behaviour |
| --- | --- |
| `GET /settings/{section}` | `section` ∈ `SECTION_KEYS`, else `404`. Works in both modes (setup mode answers defaults or the cache). |
| `PUT /settings/{section}` | Run mode only (`409 {"error":"finish MQTT and database setup first"}` in setup mode). Merges secrets with `secret::apply`. Frigate: `require_for_new_address(old.url, new.url, &password)`. Builds the candidate `Sections`, `assemble`s it with the current file, runs `validate_all` (`422 {"error": <message>}`), and requires `weather.enabled` ⇒ location is set (`422 "Set the home location first"`). Then `save_sections` (DB unreachable → `503 {"error":"database unreachable"}`, nothing saved), then `write_cache`, then `schedule_restart(500 ms)`. Answers `200 {"restarting": true}`. |
| `POST /settings/test/solcast` | Body `SolcastInput`. Makes one sites request with the key (resolved against the stored one) and reports `ok`, plus the sites' names in `detail`. |
| `POST /settings/test/frigate` | Body `FrigateInput`. Fetches `<url>/api/config` with the login (5 s) and returns `detail = {"cameras": [names]}`. |

- [ ] **Step 1: Write the failing tests** (`setup_http.rs`; PUT cases DB-gated in `settings_db.rs`, serving `router` with a test `Hub` + `SetupCtx` in Run mode):

```rust
#[tokio::test] async fn get_never_returns_secrets() { /* sections with frigate.password, solcast.api_key, push.service_account; GET each → no secret text; {"set":true} */ }
#[tokio::test] async fn unknown_section_is_404() {}
#[tokio::test] async fn put_in_setup_mode_is_409() {}
#[tokio::test] async fn frigate_new_url_without_password_is_422() {}
#[tokio::test] async fn invalid_solcast_is_422_and_nothing_saved() { /* enabled, sites [] → 422 contains "[solcast] sites"; load_sections unchanged */ }
#[tokio::test] async fn weather_without_location_is_422() {}
#[tokio::test] async fn put_saves_db_and_cache_and_keeps_omitted_secret() { /* PUT solcast without api_key → stored key kept in DB and cache */ }
#[tokio::test] async fn push_view_shows_project_id() {}
```

  Restart in tests: build `SetupCtx` with `RestartHook::Record` (Task 6) and assert the count is `1`.
- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os --test setup_http`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test -p oikos-os` (and with `TEST_DATABASE_URL`). Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(setup): settings API for weather, solar, cameras, push, climate`.

### Task 9: MCP tools

**Files:**
- Modify: `crates/oikos-os/src/mcp/tools.rs`, `crates/oikos-os/src/mcp/params.rs`, `crates/oikos-os/src/hub.rs` (`pub(crate) setup: OnceLock<Arc<SetupCtx>>`, set in `main.rs`), `docs/reference` MCP tool list if one exists (`grep -rl get_home_settings docs/`)
- Test: `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: the GET/PUT logic from Task 8. Refactor it into `pub async fn read_section(ctx, key) -> Result<Value, ApiError>` and `pub async fn write_section(ctx, key, body: Value) -> Result<Value, ApiError>` in `routes.rs`, so HTTP and MCP share one path.
- Produces: `get_settings { section }` (read_only) and `set_settings { section, value }` (idempotent), in the style of `get_home_settings` / `set_home_settings`.

- [ ] **Step 1: Write the failing tests.** `mcp_get_settings_hides_secrets` and `mcp_set_settings_refuses_new_frigate_url_without_password`.
- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os --test mcp`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run them and check they pass.** Same command. Expected: PASS.
- [ ] **Step 5: Commit** with message `feat(mcp): get_settings and set_settings`.

### Task 10: The URL banner

**Files:**
- Create: `crates/oikos-os/src/setup/banner.rs`
- Modify: `crates/oikos-os/Cargo.toml` (`if-addrs = "0.13"`), `crates/oikos-os/src/main.rs` (log after `http listening`, in both modes)

**Interfaces:**
- Produces:
  - `pub fn lan_addresses(ifaces: &[(String, std::net::IpAddr)]) -> Vec<std::net::Ipv4Addr>`: IPv4 only; skips loopback, link-local `169.254/16`, and names starting with `docker`, `br-`, `veth`, `cni`, `flannel`, `virbr`; sorted and deduplicated.
  - `pub fn banner_lines(addrs: &[Ipv4Addr], port: u16, finished: bool) -> Vec<String>`. When `addrs` is empty, it returns one line with `localhost`.
  - `pub fn log_banner(bind: &str, finished: bool)`: reads the interfaces with `if_addrs::get_if_addrs()` and logs each line at `info`.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn skips_loopback_docker_and_ipv6() {
    let got = lan_addresses(&[("lo".into(), "127.0.0.1".parse().unwrap()),
        ("docker0".into(), "172.17.0.1".parse().unwrap()), ("eth0".into(), "192.0.2.20".parse().unwrap()),
        ("eth0".into(), "fe80::1".parse().unwrap())]);
    assert_eq!(got, vec!["192.0.2.20".parse::<Ipv4Addr>().unwrap()]);
}
#[test] fn lines_say_set_up_until_finished() {
    assert_eq!(banner_lines(&[ip("192.0.2.20")], 8081, false), vec!["Open http://192.0.2.20:8081 to set up your home"]);
    assert_eq!(banner_lines(&[ip("192.0.2.20")], 8081, true), vec!["Open http://192.0.2.20:8081 to open your home"]);
}
#[test] fn no_address_falls_back_to_localhost() {}
```

- [ ] **Step 2: Run them and check they fail.** Run `cargo test -p oikos-os setup::banner`. Expected: compile errors.
- [ ] **Step 3: Implement**, and wire it into `main.rs`.
- [ ] **Step 4: Run the tests and check they pass.** Run `cargo test -p oikos-os && cargo clippy -p oikos-os --all-targets -- -D warnings && cargo fmt --check`. Expected: PASS, no warnings.
- [ ] **Step 5: Commit** with message `feat(setup): print the URL to open at startup`.

---

## After the last task

- Run the full workspace check: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` (check `~/dev-scratch/cargo-target` size first; `cargo clean` above 5 GB).
- Manual check: run against a copy of a real `oikos.toml` placed in a temp `--data` dir with a reachable test database. The hub boots Normal, `hub.json` + `settings-cache.json` appear, the import line is logged once, and a second start doesn't log it again.
- The live hub isn't switched by this plan alone. Plan 3 moves `oikos.toml` and `secrets/` into `./data/` and drops the old mounts.
