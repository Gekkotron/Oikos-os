# Home header, hub side — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The hub fetches outdoor weather from Open-Meteo and publishes it as `weather_*` entities and `GET /api/v1/weather`. It stores a per-house home settings document. Virtual entities gain a `select` kind.

**Architecture:** Wire types go in `oikos-api` (new `weather.rs` and `home.rs`, plus two `WsEvent` variants). The hub side is a new `oikos-os/src/weather/` module, made of an Open-Meteo client with pure view building, pure entity builders and a poller task. A new `oikos-os/src/home.rs` holds home settings. Both are wired into `Hub` the way dashboards, virtual entities and climate already are. Weather entities follow the virtual-entity pattern: retained discovery configs, plus retained state topics under a reserved `weather` sub-tree.

**Tech Stack:** Rust 2024 workspace, tokio, axum 0.8, reqwest 0.13, rmcp, serde/schemars, MQTT via the hub's `Publish` queue.

**Spec:** `docs/superpowers/specs/2026-10-23-home-header-weather-hub-design.md`

## Global Constraints

- No changes under `crates/oikos-web` except the smallest compile fix for new `WsEvent` variants (Task 1). No behaviour change there.
- `oikos-api` and `oikos-core` must still build for `wasm32-unknown-unknown`: no I/O, no clock, no new native-only dependency in them.
- Generic project: no machine names, IPs, or rooms of a real house in tracked files. Examples use public places (Paris, Lyon, Saint-Étienne).
- Units are metric only: °C, km/h, mm, hPa, %.
- Nothing leaves the LAN while `[weather] enabled = false`: no geocoding, no forecast request.
- Weather failures never stop the hub or break other routes.
- Errors follow the existing `HubError` → HTTP mapping: `Rejected` → 422, `Publish` → 503. "Unavailable" uses `HubError::Publish("weather unavailable")`, as climate does with `"climate disabled"`.
- Wire field names are snake_case, like `HouseSettings` and `Dashboard`.
- Every commit runs `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` clean. Commits use `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com` and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Docs: update the `docs/` pages named in each task. Never update the README.

## Decisions made while planning (refining the spec)

These follow the existing code and do not change what the user sees. The spec is updated to match in Task 6.
- **Weather state:** one retained topic per key, `P/weather/<key>`, with a plain payload (number, enum text, or `ON`/`OFF`). The spec described one JSON document read through templates. Strict templates would fail on a missing value, whereas a missing value can simply leave its topic unpublished.
- **Home settings topic:** `<ui_prefix>/_home` (default `oikos/ui/_home`), next to `_areas` and `_types`, intercepted by the hub before the registry. The spec put it at `P/home/settings`. This way no new reserved sub-tree is needed.
- **MCP `set_home_settings`:** takes the whole document, like `set_climate_settings`, rather than a partial patch.
- **Validation errors:** answer 422 (`Rejected`), not 400.
- **Field types:** `WeatherView` numbers are `Option<f64>`, because Open-Meteo can return `null`. `fetched_at` is Unix seconds (`u64`), like `Snapshot.generated_at`.
- **Home settings over WS:** no `Snapshot` field (adding one would break a web test literal). The web fetches `GET /api/v1/home/settings` at boot and listens for `HomeSettingsChanged`.

## Review Focus

1. **`null` values from Open-Meteo**, for example `minutely_15` outside high-resolution coverage, or a missing `uv_index`. Expected: that field is `None`, its entity is not republished and keeps its last value, and nothing panics. Test: `build_view_tolerates_nulls` (Task 3).
2. **City names with spaces or accents** ("Saint-Étienne", "New York"). Expected: the city is sent URL-encoded and resolves. Test: `geocode_sends_the_city_url_encoded` (Task 3).
3. **Broker offline when a fetch succeeds.** Expected: the view is still served by `GET /api/v1/weather`, and the entity values that could not be published are retried on the next successful fetch, not lost. Test: `weather_publish_failure_is_retried_next_time` (Task 4).
4. **`tonight_min` and `days` across midnight**: they use the place's local time from Open-Meteo (`timezone=auto`), not the hub's zone or UTC. Test: `tonight_min_near_midnight_uses_local_time` (Task 3).
5. **A foreign discovery config under node id `weather`** (another integration on the same prefix) while weather is disabled. Expected: only configs whose `device.identifiers` contain `oikos_weather` are cleared. Test: `disabled_weather_clears_only_its_own_configs` (Task 4).

---

### Task 1: Wire types — weather view, home settings, WS events

**Files:**
- Create: `crates/oikos-api/src/weather.rs`
- Create: `crates/oikos-api/src/home.rs`
- Modify: `crates/oikos-api/src/lib.rs` (modules and re-exports)
- Modify: `crates/oikos-api/src/events.rs` (two `WsEvent` variants, plus a serde test)
- Modify: `crates/oikos-web/src/house.rs:~154` and `crates/oikos-web/src/house_state.rs:~77` (add no-op arms only)

**Interfaces:**
- Produces (`oikos_api`):
  - `enum WeatherCondition { Clear, PartlyCloudy, Cloudy, Fog, Drizzle, Rain, FreezingRain, Snow, Showers, Thunderstorm }`, serde `snake_case`, with `fn try_from_wmo(code: u16) -> Option<WeatherCondition>` (the §4 table), `fn from_wmo(code: u16) -> WeatherCondition` (`try_from_wmo(code).unwrap_or(Cloudy)`), `fn as_str(self) -> &'static str` (the snake_case name) and `const ALL: [WeatherCondition; 10]`.
  - `struct WeatherPlace { name: String, admin: Option<String>, country: Option<String> }`
  - `struct WeatherCurrent { temperature, apparent_temperature, humidity, cloud_cover, precipitation, wind_speed, wind_gusts, wind_direction, pressure, uv_index: Option<f64>, condition: Option<WeatherCondition>, daylight: Option<bool> }`
  - `struct WeatherToday { min, max, precipitation, precipitation_probability, wind_gusts_max: Option<f64> }`
  - `struct WeatherDay { date: String /* YYYY-MM-DD local */, condition: Option<WeatherCondition>, min, max, precipitation_probability: Option<f64> }`
  - `struct WeatherView { place: WeatherPlace, fetched_at: u64, stale: bool, current: WeatherCurrent, today: WeatherToday, rain_next_hour: Option<f64>, tonight_min: Option<f64>, days: Vec<WeatherDay> }`
  - `struct HomeSettings { eco_setpoint: Option<f64>, mode_entity: Option<String>, indoor_entity: Option<String> }`, `#[serde(default)]`, `Default` all `None`.
  - `WsEvent::WeatherChanged` (unit variant) and `WsEvent::HomeSettingsChanged { settings: HomeSettings }`.
  - All of these derive `Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema`.

- [ ] **Step 1: Write the failing tests** (in `weather.rs` and `home.rs` `#[cfg(test)]`, and `events.rs`)

```rust
#[test]
fn wmo_codes_map_to_conditions() {
    use WeatherCondition::*;
    let cases = [(0, Clear), (1, PartlyCloudy), (2, PartlyCloudy), (3, Cloudy), (45, Fog), (48, Fog),
        (51, Drizzle), (55, Drizzle), (56, FreezingRain), (57, FreezingRain), (66, FreezingRain),
        (67, FreezingRain), (61, Rain), (65, Rain), (71, Snow), (77, Snow), (80, Showers),
        (82, Showers), (85, Snow), (86, Snow), (95, Thunderstorm), (99, Thunderstorm), (42, Cloudy)];
    for (code, want) in cases { assert_eq!(WeatherCondition::from_wmo(code), want, "code {code}"); }
}

#[test]
fn condition_serializes_snake_case() {
    assert_eq!(serde_json::to_value(WeatherCondition::PartlyCloudy).unwrap(), json!("partly_cloudy"));
    assert_eq!(WeatherCondition::FreezingRain.as_str(), "freezing_rain");
}

#[test]
fn home_settings_default_to_null_and_accept_partial_json() {
    let s: HomeSettings = serde_json::from_value(json!({"eco_setpoint": 15.0})).unwrap();
    assert_eq!(s, HomeSettings { eco_setpoint: Some(15.0), ..HomeSettings::default() });
    assert_eq!(serde_json::to_value(HomeSettings::default()).unwrap(),
        json!({"eco_setpoint": null, "mode_entity": null, "indoor_entity": null}));
}

#[test]
fn new_events_serialize() {
    assert_eq!(serde_json::to_value(WsEvent::WeatherChanged).unwrap(), json!({"type": "weather_changed"}));
    let e = WsEvent::HomeSettingsChanged { settings: HomeSettings::default() };
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["type"], "home_settings_changed");
    assert_eq!(serde_json::from_value::<WsEvent>(v).unwrap(), e);
}
```

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p oikos-api`
Expected: compile errors (the types don't exist yet).

- [ ] **Step 3: Implement the types**

`from_wmo` uses the table from spec §4: unknown codes → `Cloudy`. Logging is not allowed in `oikos-api` (no I/O), so the `debug` log for an unknown code goes at the call site in Task 3, which uses `try_from_wmo`.

- [ ] **Step 4: Add the no-op arms in the web**

In `house.rs`, the `apply` match gets `WsEvent::WeatherChanged | WsEvent::HomeSettingsChanged { .. } => {}`, with a one-line comment saying the home header (a later project) will use them. Do the same in `house_state.rs`.

- [ ] **Step 5: Verify**

Run: `cargo test -p oikos-api && cargo check -p oikos-api --target wasm32-unknown-unknown && cargo check -p oikos-web --target wasm32-unknown-unknown && cargo test --workspace --no-run`
Expected: tests pass and everything compiles.

- [ ] **Step 6: Commit** — `feat(api): weather view, home settings and their WS events`

---

### Task 2: `select` virtual entities

**Files:**
- Modify: `crates/oikos-api/src/virtual_entities.rs` (add `VirtualKind::Select` and the `options` field)
- Modify: `crates/oikos-os/src/virtual_entities.rs` (`component`, `is_config_topic` kinds, `select_config`, `build_config`, `initial_state`, tests)
- Modify: `crates/oikos-os/src/hub.rs:1317-1355` (`set_virtual`: reset the state when it falls outside new options)
- Modify: every `VirtualSpec { … }` literal (add `options: None`): `crates/oikos-os/tests/hub.rs:834,901,1013`, `crates/oikos-os/tests/flows.rs:1261`, `crates/oikos-os/src/virtual_entities.rs:188`
- Modify: `crates/oikos-os/src/mcp/tools.rs` (the `set_virtual` doc comment mentions `select` and `options`)
- Test: `crates/oikos-os/src/virtual_entities.rs` (unit) and `crates/oikos-os/tests/hub.rs` (integration)
- Docs: `docs/users/groups-and-virtual-entities.md` (the select kind and a house-mode example: `virtual_house_mode` with `home`/`away`/`night`, set by the user's flows), `docs/reference/http-api.md` (`PUT /api/v1/virtual/{id}` fields), `docs/reference/mcp-tools.md` (`set_virtual`), `docs/reference/mqtt-topics.md` (`<switch|number|button|select>/virtual/<id>/config`)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `VirtualKind::Select` and `VirtualSpec.options: Option<Vec<String>>` (`#[serde(default)]`). In `oikos-os`, `virtual_entities::select_config(prefix: &str, id: &str, spec: &VirtualSpec) -> Value`.

- [ ] **Step 1: Write the failing unit tests** in `virtual_entities.rs`

```rust
#[test]
fn select_config_lists_options_and_echoes() {
    let mut s = spec(VirtualKind::Select);
    s.options = Some(vec!["home".into(), "away".into(), "night".into()]);
    let c = select_config("oikos", "house_mode", &s);
    assert_eq!(c["command_topic"], "oikos/virtual/house_mode");
    assert_eq!(c["state_topic"], c["command_topic"]);
    assert_eq!(c["options"], json!(["home", "away", "night"]));
    assert_eq!(c["retain"], true);
    assert_eq!(config_topic("oikos", VirtualKind::Select, "house_mode"), "oikos/select/virtual/house_mode/config");
    assert!(is_config_topic("oikos", "house_mode", "oikos/select/virtual/house_mode/config"));
}

#[test]
fn select_initial_state_rules() {
    let mut s = spec(VirtualKind::Select);
    s.options = Some(vec!["home".into(), "away".into()]);
    assert_eq!(initial_state(&s), Ok(Some("home".to_owned())));
    s.initial = Some(json!("away"));
    assert_eq!(initial_state(&s), Ok(Some("away".to_owned())));
    s.initial = Some(json!("party"));
    assert!(initial_state(&s).is_err());
}

#[test]
fn select_options_are_validated() {
    let mut s = spec(VirtualKind::Select);
    for bad in [None, Some(vec![]), Some(vec!["a".into(), "a".into()]), Some(vec!["  ".into()]),
                Some(vec!["x".repeat(65)]), Some((0..33).map(|i| i.to_string()).collect())] {
        s.options = bad.clone();
        assert!(initial_state(&s).is_err(), "{bad:?}");
    }
    let mut sw = spec(VirtualKind::Switch);
    sw.options = Some(vec!["a".into()]);
    assert!(initial_state(&sw).is_err(), "options refused on a switch");
}
```

- [ ] **Step 2: Write the failing integration tests** in `tests/hub.rs`

- `select_virtual_is_created_and_commanded`: `set_virtual("house_mode", select spec with home/away/night)`. The view's `kind == Kind::Select`, its state is `"home"`, and two retained publishes go out: config, then state `home`. Then `send_command("virtual_house_mode", Command { … select option "away" … })` publishes `away` on `oikos/virtual/house_mode`. Then `send_command` with option `"party"` returns `Err(HubError::Command(_))` and publishes nothing. Use the select command shape the existing select tests use (search for `Kind::Select` in `oikos-core/src/command/kinds.rs`).
- `select_options_change_resets_a_missing_state`: create with `home`/`away`, ingest `away` on the state topic, then redefine with options `home`/`night`. Expect a config publish followed by a retained state publish `home`, and the view's state is `home`. Redefining again with `home`/`night`, with the state still `home`, publishes the config only.

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p oikos-os virtual_entities && cargo test -p oikos-os --test hub select_`
Expected: they fail to compile (`Select` and `options` don't exist yet).

- [ ] **Step 4: Implement**

`initial_state` validates the options: required for `Select`, 1 to 32 entries, each 1 to 64 characters after trimming, no duplicates, and refused (when `Some`) for other kinds. The initial state is `initial` (a string among the options) or the first option. In `set_virtual`, when the entity already exists, its kind is `Select`, and its current state value is not among the new options, publish the first option retained to the state topic and ingest it.

- [ ] **Step 5: Verify**

Run: `cargo test -p oikos-os && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 6: Docs** — update the four pages listed above.

- [ ] **Step 7: Commit** — `feat(hub): select virtual entities`

---

### Task 3: Open-Meteo client and view building

**Files:**
- Create: `crates/oikos-os/src/weather/mod.rs` (`pub mod open_meteo;`, and later the other submodules)
- Create: `crates/oikos-os/src/weather/open_meteo.rs`
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod weather;`)
- Create: `crates/oikos-os/tests/fixtures/weather/geocoding_paris.json`, `forecast_paris.json`, `forecast_nulls.json`, `forecast_late_evening.json`
- Test: `crates/oikos-os/tests/weather_client.rs`

**Interfaces:**
- Consumes: `oikos_api::{WeatherView, WeatherPlace, WeatherCurrent, WeatherToday, WeatherDay, WeatherCondition}` (Task 1).
- Produces (`oikos_os::weather::open_meteo`):
  - `pub struct Endpoints { pub geocoding: String, pub forecast: String }`, where `impl Default` gives `https://geocoding-api.open-meteo.com/v1/search` and `https://api.open-meteo.com/v1/forecast`.
  - `pub struct Place { pub name: String, pub admin: Option<String>, pub country: Option<String>, pub latitude: f64, pub longitude: f64 }`, with `fn view(&self) -> WeatherPlace`.
  - `pub fn client() -> reqwest::Client`: connect timeout 5 s, total 15 s, `User-Agent: oikos-os/<CARGO_PKG_VERSION>`.
  - `pub async fn geocode(client: &reqwest::Client, ep: &Endpoints, city: &str, country: Option<&str>) -> Result<Option<Place>, FetchError>`. `Ok(None)` means the city was not found.
  - `pub async fn forecast(client: &reqwest::Client, ep: &Endpoints, place: &Place) -> Result<Forecast, FetchError>`
  - `pub struct Forecast`: the deserialized response (arrays as `Vec<Option<f64>>`, times as `Vec<String>`).
  - `pub fn build_view(forecast: &Forecast, place: &Place, fetched_at: u64) -> WeatherView` (`stale: false`, which the hub sets on read).
  - `#[derive(Debug, thiserror::Error)] pub enum FetchError { Http(String), Decode(String) }`

- [ ] **Step 1: Write the fixtures**

These are realistic Open-Meteo responses for a public place.
- `geocoding_paris.json`: `{"results":[{"name":"Paris","latitude":48.85341,"longitude":2.3488,"country_code":"FR","admin1":"Île-de-France"}]}`.
- `forecast_paris.json`: `utc_offset_seconds` 7200, `current.time` `"2026-10-23T14:15"`, and every requested variable. `minutely_15` covers 14:15 onward. `hourly` covers 2026-10-23T00:00 for 48 h. `daily` has 6 days starting 2026-10-23.
- `forecast_nulls.json`: same, but `minutely_15.precipitation` all `null` and `hourly.uv_index` `null`.
- `forecast_late_evening.json`: `current.time` `"2026-10-23T23:30"`, with a known hourly minimum at `2026-10-24T06:00` and a lower value at `2026-10-24T10:00`, which lies outside the window.

- [ ] **Step 2: Write the failing tests** in `tests/weather_client.rs`

The stub server is an axum `Router` on `127.0.0.1:0`, like `oikos-push/tests/fcm.rs:118`. It records the query strings it receives into a shared `Arc<Mutex<Vec<String>>>`.

```rust
#[tokio::test] async fn geocode_picks_the_first_result_and_sends_country() // Place{name:"Paris", country:Some("FR"), lat 48.85341}, query has name=Paris, countryCode=FR, count=10, format=json
#[tokio::test] async fn geocode_without_results_is_none()                   // stub answers {"generationtime_ms":0.1} -> Ok(None)
#[tokio::test] async fn geocode_sends_the_city_url_encoded()                // city "Saint-Étienne": stub's decoded `name` param == "Saint-Étienne"
#[tokio::test] async fn http_error_is_fetch_error()                         // stub 500 -> Err(FetchError::Http(_))
#[tokio::test] async fn forecast_requests_the_needed_variables()            // query has latitude, longitude, timezone=auto, forecast_days=6, and current= contains temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,is_day,cloud_cover,precipitation,wind_speed_10m,wind_gusts_10m,wind_direction_10m,pressure_msl ; hourly= temperature_2m,uv_index ; minutely_15=precipitation ; daily= weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,precipitation_probability_max,wind_gusts_10m_max
```

Pure tests on `build_view` (no server; deserialize the fixture into `Forecast`):

```rust
#[test] fn build_view_maps_current_today_and_days() // current.* from fixture; condition from weather_code; daylight from is_day==1; today = daily[0]; days.len()==5 and days[0].date=="2026-10-24"; fetched_at passed through; stale==false
#[test] fn rain_next_hour_sums_four_slots_from_now() // the 4 minutely_15 slots with time >= current.time (14:15,14:30,14:45,15:00), summed
#[test] fn uv_index_comes_from_the_current_hour()     // hourly slot "2026-10-23T14:00"
#[test] fn build_view_tolerates_nulls()               // forecast_nulls: rain_next_hour None, current.uv_index None, other fields Some
#[test] fn tonight_min_near_midnight_uses_local_time()// forecast_late_evening: min over hourly slots with now<=t<first 09:00 after now == the 06:00 value (10:00 lower value ignored)
```

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p oikos-os --test weather_client`
Expected: compile errors.

- [ ] **Step 4: Implement `open_meteo.rs`**

Build queries with `reqwest`'s `.query(&[...])`, so encoding is handled for you. Parse local times with `chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M")`, and compare naive local times only.
- `tonight_min` is the minimum of `hourly.temperature_2m` over the slots `t` with `now_hour <= t < first 09:00 strictly after now`, where `now_hour` is `current.time` truncated to the hour.
- `rain_next_hour` is the sum of the non-null values among the first 4 `minutely_15` slots with time ≥ `current.time`. If all are null, it is `None`.
- An unknown WMO code (`try_from_wmo` is `None`) logs at `debug` here (`tracing::debug!(code, "unknown WMO weather code")`), then maps to `Cloudy`.

- [ ] **Step 5: Verify** — Run: `cargo test -p oikos-os --test weather_client`. Expected: PASS.

- [ ] **Step 6: Commit** — `feat(weather): Open-Meteo client and weather view`

---

### Task 4: Weather entities and hub state

**Files:**
- Create: `crates/oikos-os/src/weather/entities.rs`
- Modify: `crates/oikos-os/src/weather/mod.rs` (`pub mod entities;`)
- Modify: `crates/oikos-core/src/registry/mod.rs:39-49` (`default_reserved` adds `"weather"`, and its doc comment)
- Modify: `crates/oikos-os/src/hub.rs` (new fields and methods below; intercept in `ingest_message` while disabled)
- Modify: `crates/oikos-os/src/config.rs` (add `WeatherConfig` and `Config.weather`; validation comes in Task 5)
- Create: `crates/oikos-os/src/weather/hub.rs` (the `impl Hub` block below)
- Test: `crates/oikos-os/src/weather/entities.rs` (unit) and `crates/oikos-os/tests/weather_hub.rs`

**Interfaces:**
- Consumes: `WeatherView` (Task 1).
- Produces:
  - `weather::entities::KEYS: [&str; 19]`, in spec §4 order: `temperature, apparent_temperature, humidity, condition, daylight, cloud_cover, precipitation, wind_speed, wind_gusts, wind_direction, pressure, uv_index, today_min, today_max, today_precipitation, today_precipitation_probability, today_wind_gusts_max, rain_next_hour, tonight_min`.
  - `pub fn config_topic(prefix: &str, key: &str) -> String`: `P/<sensor|binary_sensor>/weather/<key>/config`.
  - `pub fn state_topic(prefix: &str, key: &str) -> String`: `P/weather/<key>`.
  - `pub fn availability_topic(prefix: &str) -> String`: `P/weather/availability`.
  - `pub fn configs(prefix: &str) -> Vec<(String, serde_json::Value)>`: 19 (topic, config) pairs.
  - `pub fn states(view: &WeatherView) -> Vec<(&'static str, String)>`: one pair per key whose value is `Some`. Numbers use Rust's `f64` `to_string()`, `condition` uses `as_str()`, `daylight` is `ON`/`OFF`.
  - `pub fn is_own_config(payload: &[u8]) -> bool`: true when `device.identifiers` contains `"oikos_weather"`.
  - `pub struct WeatherConfig { pub enabled: bool, pub city: String, pub country: Option<String>, pub refresh_minutes: u64 }` in `config.rs`, with `#[serde(default)]`, defaults `false`/`""`/`None`/`15`, and `Config.weather` (validation comes in Task 5).
  - Hub, in `oikos-os/src/weather/hub.rs` as an `impl Hub` block, like `climate/hub.rs`:
    - `pub fn weather_enabled(&self) -> bool`
    - `pub fn weather_view(&self) -> Result<WeatherView, HubError>`: `Publish("weather unavailable")` when disabled or before the first success; `stale` is computed at read time.
    - `pub(crate) fn apply_weather(&self, view: WeatherView)`: stores the view, publishes availability `online` and every key whose payload differs from the last *successfully published* payload, then broadcasts `WsEvent::WeatherChanged` if the view's data changed (ignoring `fetched_at`).
    - `pub(crate) fn set_weather_offline(&self)`: publishes availability `offline` retained (idempotent: once per transition).
    - `pub(crate) fn publish_weather_configs(&self)`: all 19 configs retained, plus availability `offline`.
    - `pub(crate) fn weather_refresh_minutes(&self) -> u64`
  - Pure freshness helpers in `weather/mod.rs`: `pub fn is_stale(fetched_at: u64, now: u64, refresh_minutes: u64) -> bool` (true when `now - fetched_at > 2 * refresh_minutes * 60`) and `pub fn is_expired(fetched_at: u64, now: u64) -> bool` (true when `> 3600`).

**Entity configs** (spec §4 table). Each has `name` (English, for example "Outdoor temperature", "Feels like", "Today's minimum"), `unique_id` `weather_<key>`, `state_topic`, `availability_topic`, and `device: {"identifiers": ["oikos_weather"], "name": "Weather"}`. Units and device classes are as in the table, and `state_class: "measurement"` is set on the "Now" rows. `condition` has `device_class: "enum"` and `options` set to `WeatherCondition::ALL` names. `daylight` is a `binary_sensor` with `payload_on: "ON"` and `payload_off: "OFF"`.

- [ ] **Step 1: Write the failing unit tests** in `entities.rs`

```rust
#[test] fn there_are_nineteen_configs_with_own_device() // configs("oikos").len()==19; every config device.identifiers==["oikos_weather"]; unique_id=="weather_"+key; availability_topic=="oikos/weather/availability"
#[test] fn config_topics_use_the_component()              // config_topic("oikos","daylight")=="oikos/binary_sensor/weather/daylight/config"; ("oikos","temperature")=="oikos/sensor/weather/temperature/config"
#[test] fn condition_is_an_enum_sensor()                   // device_class=="enum", options == ALL names (10)
#[test] fn states_skip_missing_values()                    // view with uv_index None -> no "uv_index" pair; daylight Some(true) -> ("daylight","ON"); condition Rain -> "rain"; temperature 13.4 -> "13.4"
#[test] fn own_config_is_recognised()                      // is_own_config(config json) true; foreign {"device":{"identifiers":["0x1234"]}} false; garbage false
#[test] fn freshness_rules()                               // is_stale(0, 1800, 15)==false, is_stale(0, 1801, 15)==true; is_expired(0,3600)==false, is_expired(0,3601)==true
```

- [ ] **Step 2: Write the failing hub tests** in `tests/weather_hub.rs`

Use the `hub()` and `drain()` helpers copied from `tests/hub.rs:33-53,844-850`, with `cfg.weather.enabled = true` and `cfg.weather.city = "Paris"`. Use a `sample_view()` with every field `Some`, and a `paris()` `Place`.
- `weather_unavailable_before_first_fetch`: `weather_view()` is `Err(HubError::Publish(_))`.
- `publish_configs_then_apply_publishes_states_and_online`: call `publish_weather_configs()` and expect 19 configs plus availability `offline`, all retained. Then call `apply_weather(view)` and expect availability `online` plus 19 retained state publishes. `weather_view()` is `Ok` with `stale == false`. After ingesting the drained publishes back into the hub (the broker echo), `entity_view("weather_temperature")` has kind `Sensor` and the state value from the view.
- `apply_weather_publishes_only_changes`: applying the same view twice publishes nothing the second time, apart from availability if it was offline, and broadcasts `WeatherChanged` only once. Changing only `temperature` publishes one state.
- `weather_publish_failure_is_retried_next_time` (Review Focus 3): `set_connection(Offline)`, then `apply_weather(view)` publishes nothing but `weather_view()` is `Ok`. Then `set_connection(Online)`, apply the same view again, and expect all 19 states published.
- `set_weather_offline_is_once_per_transition`: two calls produce one `offline` publish.
- `disabled_weather_clears_only_its_own_configs` (Review Focus 5): use a hub with `weather.enabled = false`. `ingest_message("oikos/sensor/weather/temperature/config", <own config>, true)` produces an empty retained publish on that topic, and the entity is not created. `ingest_message("oikos/weather/temperature", b"13.4", true)` produces an empty retained publish. A foreign config on `oikos/sensor/weather/foo/config` whose identifiers are `["0xabc"]` is not cleared and is applied normally.
- `weather_state_topic_is_not_discovery`: `oikos/weather/temperature` never appears under `ignored()`.

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p oikos-os weather:: && cargo test -p oikos-os --test weather_hub`
Expected: compile errors.

- [ ] **Step 4: Implement**

The `Hub` gets these fields: `weather_enabled: bool`, `weather_refresh_minutes: u64`, `weather: Mutex<WeatherSlot>`. `WeatherSlot` is private to `weather/hub.rs`: `{ view: Option<WeatherView>, published: BTreeMap<&'static str, String>, online: Option<bool> }`. `published` is updated only when `publish` returns `Ok`.

Clearing while disabled: at the top of `ingest_message`, when `!weather_enabled` and a non-empty retained payload arrives, act on two kinds of topic. For `P/weather/...`, publish an empty retained payload and return. For a discovery config topic with node id `weather` whose payload passes `is_own_config`, publish an empty retained payload and return without applying. Ignore publish errors: the next connect replays the retained message and retries.

- [ ] **Step 5: Verify** — Run: `cargo test -p oikos-os && cargo test -p oikos-core`. Expected: PASS.

- [ ] **Step 6: Docs** — in `docs/reference/mqtt-topics.md`: rows for `P/<sensor|binary_sensor>/weather/<key>/config`, `P/weather/<key>` and `P/weather/availability`, plus `weather` in the reserved sub-trees sentence.

- [ ] **Step 7: Commit** — `feat(weather): weather entities and hub state`

---

### Task 5: Weather poller, config validation, REST, MCP and the user docs

**Files:**
- Create: `crates/oikos-os/src/weather/poller.rs`
- Modify: `crates/oikos-os/src/config.rs` (`WeatherConfig::validate`, string env keys `"weather.city"` and `"weather.country"`, tests)
- Modify: `crates/oikos-os/src/main.rs` (validate `cfg.weather`; `let weather = hub.start_weather();` after `mqtt::start`; abort it on shutdown)
- Modify: `crates/oikos-os/src/flows/engine.rs:729-735` (make `random_roll` `pub(crate)` for the jitter)
- Modify: `crates/oikos-os/src/http/api.rs` (route `.route("/weather", get(weather))`)
- Modify: `crates/oikos-os/src/mcp/tools.rs` (`get_weather` tool; the tool count in docs goes up by 1)
- Test: `crates/oikos-os/tests/weather_poller.rs`, `crates/oikos-os/tests/config.rs` (validation cases)
- Docs: `docs/reference/configuration.md` (`[weather]`), `docs/reference/http-api.md` (`GET /api/v1/weather`, `weather_changed` event), `docs/reference/mcp-tools.md` (`get_weather`, count), `docs/users/weather.md` (new; see spec §8 for its contents, including the Open-Meteo CC BY 4.0 credit and the privacy note), `docs/users/flows.md` (a pointer to the weather entities). Check `docs/users/_category_.json` and the `sidebar_position` of neighbouring pages so the new page appears in the docs site.

**Interfaces:**
- Consumes: `open_meteo::{Endpoints, client, geocode, forecast, build_view, Place}` (Task 3); `Hub::{apply_weather, set_weather_offline, publish_weather_configs, weather_refresh_minutes, weather_enabled}` and `is_expired` (Task 4).
- Produces:
  - `impl WeatherConfig { pub fn validate(&self) -> Result<(), String> }`. When enabled, it rejects an empty or whitespace `city`, a `country` that is not exactly two ASCII letters, and `refresh_minutes < 10`. Disabled always validates. Messages start with `[weather]`.
  - `impl Hub { pub fn start_weather(self: &Arc<Self>) -> Option<JoinHandle<()>> }`: `None` when disabled. Otherwise it spawns `poller::run(hub, Endpoints::default())`.
  - `pub async fn poller::run(hub: Arc<Hub>, endpoints: Endpoints)` and `pub fn poller::spawn_with(hub: &Arc<Hub>, endpoints: Endpoints) -> JoinHandle<()>`. The second exists for tests and takes the city, country and refresh from the hub's config.
  - The HTTP handler `async fn weather(State(hub)) -> Result<Json<WeatherView>, HttpError>`.
  - The MCP tool `get_weather` (read-only), returning `WeatherView`.

**Poller behaviour** (spec §3):
1. `publish_weather_configs()`. If the broker is not online yet, wait for `hub.connection()` to become `Online` first.
2. Geocode, retrying network and HTTP errors with backoff of 30 s, doubling, capped at 15 min. On `Ok(None)`, log `error!(city, "weather: city not found")` and return; weather stays unavailable. On `Ok(Some(place))`, log `info!` with the name, admin area, country and coordinates.
3. Loop: forecast. On `Ok`, call `build_view(…, clock::now())` then `apply_weather`. On `Err`, log `warn!` once per outage (re-armed by the next success), and when `is_expired(last_success, now)` call `set_weather_offline()`. Then sleep `refresh_minutes` ± 10 %, using `random_roll()`.

Tests use the real `tokio` clock, and there are no `#[cfg(test)]` hooks: tests go through `spawn_with` and the hub config. `refresh_minutes ≥ 10` is enforced only by `WeatherConfig::validate` at start. The poller takes whatever the hub holds, with a 1 s minimum sleep, so tests set `refresh_minutes = 0` and run fast. State that floor in a comment.

- [ ] **Step 1: Write the failing config tests** in `tests/config.rs`

```rust
#[test] fn weather_disabled_by_default_and_valid()          // Config::default().weather.enabled==false, refresh_minutes==15, validate()==Ok
#[test] fn weather_enabled_needs_a_city()                    // enabled, city "" -> Err containing "city"; city "  " -> Err
#[test] fn weather_country_is_two_letters()                  // "FR" ok, "fr" ok, "FRA" err, "F1" err
#[test] fn weather_refresh_has_a_floor()                     // 9 err, 10 ok
#[test] fn weather_from_toml_and_env()                       // TOML "[weather]\nenabled=true\ncity=\"Lyon\"\ncountry=\"FR\"" parses; OIKOS_WEATHER__CITY="1234" stays a string (string env key)
```

- [ ] **Step 2: Write the failing poller and route tests** in `tests/weather_poller.rs`

The stub serves the Task 3 fixtures from both paths and counts requests. The hub is online, with `weather.enabled = true`, `city = "Paris"` and `refresh_minutes = 0`. The test forwards publishes into the hub (`hub.ingest`) so entities exist.
- `poller_publishes_offline_first_then_online_and_states`: the first publish is the configs plus availability `offline`. Within 5 s, availability goes `online`, `weather_temperature` exists with the fixture value, and `GET /api/v1/weather` (served with `http::router` on an ephemeral port, as in `tests/http_flows_config.rs`) returns 200 with `place.name == "Paris"` and `days.len() == 5`.
- `unknown_city_stays_unavailable`: the geocoding stub returns no results. After 2 s, `GET /api/v1/weather` is 503 and the forecast endpoint was never called.
- `disabled_weather_makes_no_request`: with `enabled = false`, `start_weather()` is `None`, and a stub that is never called stays at 0 requests after 1 s. `GET /api/v1/weather` is 503.
- `fetch_failure_keeps_last_view_and_goes_stale`: the stub serves one good forecast, then 500s. The view is still served. With a forged `fetched_at` far in the past, applied via `apply_weather`, `weather_view().stale == true`.
- `get_weather_mcp_tool_matches_rest`: follow the pattern in `tests/mcp.rs`. The `get_weather` result equals the REST body.

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p oikos-os --test config weather_ && cargo test -p oikos-os --test weather_poller`
Expected: compile errors.

- [ ] **Step 4: Implement** the config validation, `poller.rs`, `start_weather`, the main wiring, the route and the MCP tool.

- [ ] **Step 5: Verify**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 6: Docs** — the pages listed under Files.

- [ ] **Step 7: Commit** — `feat(weather): Open-Meteo poller, GET /api/v1/weather and get_weather`

---

### Task 6: Home settings document, REST, WS and MCP

**Files:**
- Create: `crates/oikos-os/src/home.rs` (pure validation and topic helpers, plus an `impl Hub` block)
- Modify: `crates/oikos-os/src/lib.rs` (`pub mod home;`)
- Modify: `crates/oikos-os/src/hub.rs` (field `home_settings: Mutex<HomeSettings>`; in `ingest`, intercept `<ui_prefix>/_home` before dashboards, like `dashboard_id_of`)
- Modify: `crates/oikos-os/src/http/api.rs` (`.route("/home/settings", get(home_settings).put(put_home_settings))`)
- Modify: `crates/oikos-os/src/mcp/tools.rs` and `crates/oikos-os/src/mcp/params.rs` (`get_home_settings`, `set_home_settings { settings: HomeSettings }`)
- Modify: `docs/superpowers/specs/2026-10-23-home-header-weather-hub-design.md` (fold in the "Decisions made while planning" section: per-key weather topics, `<ui_prefix>/_home`, whole-document MCP, 422, `Option` fields, `fetched_at` as Unix seconds)
- Test: `crates/oikos-os/src/home.rs` (unit) and `crates/oikos-os/tests/home_settings.rs`
- Docs: `docs/reference/http-api.md` (both routes, `home_settings_changed`), `docs/reference/mcp-tools.md` (both tools, count +2), `docs/reference/mqtt-topics.md` (`<ui_prefix>/_home`), `docs/users/web-ui.md` (a short note that the Settings fields arrive with the home header, only if that page already lists hub-side settings; otherwise skip)

**Interfaces:**
- Consumes: `oikos_api::HomeSettings` and `WsEvent::HomeSettingsChanged` (Task 1); `Kind::Select` entities (Task 2, for the tests).
- Produces:
  - `pub fn home::validate(settings: &HomeSettings, reg: &Registry) -> Result<(), String>`: `eco_setpoint` must be finite and in `5.0..=30.0`; `mode_entity` must exist with `Kind::Select`; `indoor_entity` must exist with `Kind::Sensor` and `device_class == Some("temperature")`. Messages name the field (`"eco_setpoint …"`).
  - `pub fn home::topic(ui_prefix: &str) -> String`: `<ui_prefix>/_home`.
  - `impl Hub { pub fn home_settings(&self) -> HomeSettings; pub fn set_home_settings(&self, s: HomeSettings) -> Result<HomeSettings, HubError>; pub(crate) fn ingest_home(&self, payload: &[u8]) }`
    - `set_home_settings` validates (`Rejected` on failure), publishes retained, applies, broadcasts `HomeSettingsChanged`, and returns the stored document.
    - `ingest_home` decodes. An empty payload means defaults. An undecodable payload logs `warn!` and keeps the previous document. A change broadcasts `HomeSettingsChanged`.
  - HTTP: `GET` → 200 with the document; `PUT` → 200 with the stored document, 422 on validation errors, 503 while the broker is offline.

- [ ] **Step 1: Write the failing unit tests** in `home.rs`

```rust
#[test] fn eco_setpoint_bounds()          // 5.0 ok, 30.0 ok, 4.9 err, 30.1 err, NaN err, None ok; err text contains "eco_setpoint"
#[test] fn mode_entity_must_be_a_select() // registry with a select (ingest a select discovery fixture from oikos-core/tests/fixtures/discovery, or a virtual select config) ok; a switch id err; unknown id err; err text contains "mode_entity"
#[test] fn indoor_entity_must_be_a_temperature_sensor() // temperature sensor fixture ok; humidity sensor err; unknown err; err contains "indoor_entity"
#[test] fn topic_sits_beside_areas()      // topic("oikos/ui")=="oikos/ui/_home"
```

- [ ] **Step 2: Write the failing integration tests** in `tests/home_settings.rs`
- `defaults_when_nothing_retained`: `home_settings() == HomeSettings::default()`.
- `set_publishes_retained_applies_and_broadcasts`: subscribe to events. `set_home_settings({eco_setpoint: 15})` makes one retained publish on `oikos/ui/_home` whose payload JSON equals the document, `home_settings()` returns it, and the event received is `HomeSettingsChanged` carrying it.
- `retained_document_is_applied_on_ingest`: `hub.ingest("oikos/ui/_home", br#"{"eco_setpoint":16.5}"#)` gives `home_settings().eco_setpoint == Some(16.5)`, and no override for `_home` appears (`entity_view("_home")` is `None`, and `ignored()` is unchanged).
- `invalid_retained_document_keeps_previous`: ingest a valid document, then `b"not json"`. The settings are unchanged.
- `rest_get_put_roundtrip_and_422`: over `http::router` on an ephemeral port. `PUT {"eco_setpoint": 15}` → 200 with the body echoed. `GET` → the same. `PUT {"eco_setpoint": 50}` → 422 with an `error` containing `eco_setpoint`. `PUT {"mode_entity": "virtual_house_mode"}` after `set_virtual` creates a select → 200.
- `mcp_set_and_get_home_settings`: follow `tests/mcp.rs`.

- [ ] **Step 3: Run them to make sure they fail**

Run: `cargo test -p oikos-os home:: && cargo test -p oikos-os --test home_settings`
Expected: compile errors.

- [ ] **Step 4: Implement** `home.rs`, the hub field and the intercept, the routes and the MCP tools.

- [ ] **Step 5: Verify**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS.

- [ ] **Step 6: Docs and spec** — the pages listed above, and the spec fold-in.

- [ ] **Step 7: Commit** — `feat(hub): home settings document (eco setpoint, mode entity, indoor sensor)`

---

## Execution order

Task 1 comes first. Tasks 2, 3 and 6 then depend only on Task 1 (Task 6's tests use a select from Task 2, so run Task 2 before Task 6). Task 4 needs Tasks 1 and 3, and Task 5 needs Tasks 3 and 4. Suggested sequence: 1 → 2 → 3 → 4 → 5 → 6. If you run work in parallel (at most 2 at a time), pair 2 with 3, then 4 with 6.
