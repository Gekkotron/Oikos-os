# Off-peak device Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Off-peak can come from a device as well as (or instead of) the typed windows; flows get a `trigger.off_peak` and an `if` with `op: is_off_peak`; the Energy day chart splits by the device's recorded history.

**Architecture:** A pure resolver in `oikos_core::energy::tariff` turns a device reading into `Option<bool>` (None = not usable) and falls back to the windows. The hub's flow engine keeps one resolved baseline and one boundary timer and fans changes out to `trigger.off_peak` nodes. The grid day reads the device's value and availability rows for the day, reduces them to spans, and `build_grid_day` consults the spans before the windows.

**Tech Stack:** Rust workspace (`oikos-core`, `oikos-api`, `oikos-os` with tokio/axum/sqlx/TimescaleDB, `oikos-web` Leptos/wasm).

**Spec:** `docs/superpowers/specs/2026-10-30-off-peak-device-design.md`

## Global Constraints

- Generic: no vendor, meter model or country name in code, strings, tests or docs. Test fixtures use neutral ids (`sensor.tariff`) and neutral codes (`"OFF"`, `"PEAK"`, `"HC.."` only as a generic example in docs is fine).
- A house with no `off_peak_device` behaves exactly as today: same settings JSON (field omitted when `None`), same grid day, same flows.
- Windows and device are both optional; no code path requires a window.
- Never fire `trigger.off_peak` at boot, on reconnect replay, on a flow save, or on a home settings change.
- en + fr strings for every new UI text.
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`), message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only.
- Every task ends with `cargo check --workspace --all-targets` green (adding a `NodeKind` variant or `Op` variant breaks exhaustive matches in `oikos-web` and `oikos-os`; the task that adds it fixes them).

## Review Focus

1. **Reconnect replay with a device set**: the broker replays the device's retained state after a link drop; it must seed, never fire (pinned in Task 3: `reconnect_replay_seeds_without_firing`).
2. **Device reports a value that is neither in `values` nor a "not usable" word** (e.g. a new code): it means peak, not fallback (Task 1: `unknown_code_is_peak`).
3. **On/off and numeric devices**: `Bool(true)` must match `"on"`, a number `1.0` must match `"1"` (Task 1: `bool_and_number_match_their_text_form`).
4. **Device without windows going offline during off-peak**: resolves to peak and fires `end` once, not repeatedly (Task 3: `fallback_without_windows_ends_once`).
5. **Grid day for a past day before the device had any readings**: identical to today's windows-only result (Task 4: `day_without_device_readings_matches_windows_only`).

---

### Task 1: Core resolver and the settings field

**Files:**
- Modify: `crates/oikos-core/src/energy/tariff.rs`
- Modify: `crates/oikos-api/src/home.rs` (field + re-export), `crates/oikos-api/src/lib.rs:45` (re-export `OffPeakDevice`)
- Modify: `crates/oikos-os/src/home.rs` (`validate`)
- Test: in-file `#[cfg(test)]` modules of those three files

**Interfaces:**
- Produces (in `oikos_core::energy`):
  - `pub struct OffPeakDevice { pub entity: String, pub values: Vec<String> }` — `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`, `schemars` behind the feature like `OffPeakWindow`.
  - `pub fn reading_text(value: &Scalar) -> String` — `Bool(true)`→`"on"`, `Bool(false)`→`"off"`, `Number(n)`→`n.to_string()` (so `1.0`→`"1"`), `Text(t)`→`t`.
  - `pub fn device_off_peak(device: &OffPeakDevice, value: &StateValue, availability: Availability) -> Option<bool>` — `None` when `availability == Offline`, `Scalar::of_state(value)` is `None`, or the text is `""`, `"unknown"` or `"unavailable"`; else `Some(device.values.contains(&reading_text(..)))`.
  - `pub fn resolve_off_peak(device: Option<bool>, windows: &[OffPeakWindow], minute: u16) -> bool` — `device.unwrap_or_else(|| is_off_peak(windows, minute))`.
  - `pub fn next_boundary(windows: &[OffPeakWindow], minute: u16) -> Option<u16>` — minutes from `minute` (1..=1440) to the next window start or end; `None` for no windows.
- Produces (api): `HomeSettings.off_peak_device: Option<OffPeakDevice>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, doc comment "The entity whose state says off-peak, and the values that mean it; the windows are the backup."

- [ ] **Step 1: Write the failing tests** in `tariff.rs` tests:
  - `device_value_in_list_is_off_peak`: `Text("OFF")`, values `["OFF"]`, `Online` → `Some(true)`.
  - `unknown_code_is_peak`: `Text("NEW")` → `Some(false)`.
  - `offline_unknown_and_empty_are_not_usable`: `Offline` with `Text("OFF")`; `StateValue::Unknown`; `Text("")`; `Text("unknown")`; `Text("unavailable")` → all `None`. `Availability::Unknown` with `Text("OFF")` → `Some(true)` (most entities never announce availability).
  - `bool_and_number_match_their_text_form`: `OnOff(true)` with `["on"]` → `Some(true)`; `Number(1.0)` with `["1"]` → `Some(true)`.
  - `resolve_prefers_device_then_windows`: `resolve_off_peak(Some(false), &[w("22:00","06:00")], 23*60) == false`; `resolve_off_peak(None, same, 23*60) == true`; `resolve_off_peak(None, &[], 0) == false`.
  - `next_boundary_wraps`: windows `22:00–06:00`: from `21*60` → `Some(60)`; from `23*60` → `Some(7*60)`; from `6*60` → `Some(16*60)`; `&[]` → `None`.
- [ ] **Step 2:** `cargo test -p oikos-core energy::tariff` → FAIL (unresolved names).
- [ ] **Step 3:** Implement the five items above in `tariff.rs` (core already depends on `flow::Scalar` and `model::{StateValue, Availability}` in-crate).
- [ ] **Step 4:** `cargo test -p oikos-core energy::tariff` → PASS.
- [ ] **Step 5: Settings field tests** in `oikos-api/src/home.rs`: `off_peak_device_round_trips_and_is_omitted_when_none` (JSON without the key decodes to `None`; `None` encodes without the key; `{"entity":"sensor.tariff","values":["OFF"]}` round-trips).
- [ ] **Step 6: Validation tests** in `oikos-os/src/home.rs` tests: `off_peak_device_must_name_an_entity_and_values` — unknown id → `Err` containing `"off_peak_device"`; empty `entity` → same; empty `values` → same; a known sensor with `["OFF"]` → `Ok`; device with empty windows → `Ok`.
- [ ] **Step 7:** Add the field, the re-export, and the `validate` branch (any entity kind is accepted; messages start with `off_peak_device`). Fix any `HomeSettings { .. }` literals the compiler flags.
- [ ] **Step 8:** `cargo test -p oikos-api home && cargo test -p oikos-os home::` → PASS; `cargo check --workspace --all-targets` → OK.
- [ ] **Step 9: Commit** `feat(energy): off-peak device setting and resolver`.

### Task 2: Flow model — `trigger.off_peak` and `op: is_off_peak`

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs`, `crates/oikos-core/src/flow/validate.rs`, `crates/oikos-core/src/flow/scalar.rs:88`, `crates/oikos-core/src/flow/mod.rs` (re-exports)
- Modify (exhaustive-match fixes only, real UI comes in Task 6): `crates/oikos-web/src/{icons.rs,flow_text.rs,flows.rs,flow_list.rs,flow_edit.rs,if_value.rs}`, `crates/oikos-web/src/screens/flow_canvas/{settings.rs,graph.rs}`, `crates/oikos-os/src/flows/{engine.rs,hub.rs}`

**Interfaces:**
- Produces:
  - `NodeKind::TriggerOffPeak(TriggerOffPeakParams)`, wire `"trigger.off_peak"`, `type_name()` returns it, `is_trigger()` includes it.
  - `pub enum OffPeakEdge { Start, End }` (`snake_case`, no default), `pub struct TriggerOffPeakParams { pub edge: OffPeakEdge, #[serde(flatten)] pub extra: BTreeMap<String, Value> }`.
  - `Op::IsOffPeak`, wire `"is_off_peak"`.
- Consumes: nothing from Task 1.

- [ ] **Step 1: Failing tests** in `model.rs`: `trigger_off_peak_round_trips` (`{"id":"t","type":"trigger.off_peak","params":{"edge":"start"}}` decodes, re-encodes identically; `"edge":"middle"` fails to decode); `if_is_off_peak_round_trips` (like `if_is_dark_round_trips_without_value_entity_or_missing`).
  In `validate.rs`: `op_is_off_peak_rejects_entity_value_missing_average_and_offset` (each of `value`, `entity`, `missing`, `average`, `offset_minutes` → `BadOperand` with reason `"<field> is not allowed with op: is_off_peak"`); `if_is_off_peak_is_valid_after_a_schedule` (schedule → `if is_off_peak` → command validates, i.e. the valueless-trigger check exempts it like `is_dark`); `if_after_trigger_off_peak_may_read_the_trigger_value` (`trigger.off_peak` → `if eq "start"` with no entity validates).
- [ ] **Step 2:** `cargo test -p oikos-core flow::` → FAIL.
- [ ] **Step 3:** Implement: variant, params, `Op::IsOffPeak` + `wire_name`; in `validate.rs` an `If(p) if p.op == Op::IsOffPeak` arm rejecting the five fields; extend the `offset_minutes` guard message only for non-`is_dark`; `check_ifs_after_valueless_triggers` exempts `IsOffPeak`; `scalar.rs` `compare` gets `Op::IsOffPeak => unreachable!("is_off_peak is handled in the engine before compare")`.
- [ ] **Step 4:** `cargo test -p oikos-core flow::` → PASS.
- [ ] **Step 5:** `cargo check --workspace --all-targets`; fix each non-exhaustive match with the minimal arm (engine: `TriggerOffPeak` is not armed by schedules/suns, `run_node` treats it like other triggers; `If` with `IsOffPeak` temporarily returns `failed("is_off_peak is not wired yet")` — replaced in Task 3; web arms fall through to the generic/`If` presentation — replaced in Task 6).
- [ ] **Step 6:** `cargo test -p oikos-core && cargo check --workspace --all-targets` → OK.
- [ ] **Step 7: Commit** `feat(flows): trigger.off_peak and if is_off_peak in the model`.

### Task 3: Hub — resolve now, fire the trigger, answer the check

**Files:**
- Modify: `crates/oikos-os/src/home.rs` (new `Hub::off_peak_reading`)
- Modify: `crates/oikos-os/src/flows/engine.rs`
- Test: `engine.rs` `#[cfg(test)]` (reuse `engine_at`, `add_sensor` style helpers)

**Interfaces:**
- Consumes: Task 1 `device_off_peak`, `resolve_off_peak`, `next_boundary`, `HomeSettings.off_peak_device`; Task 2 node/op.
- Produces:
  - `impl Hub { pub fn off_peak_now(&self, now: Micros) -> OffPeakNow }` with `pub struct OffPeakNow { pub off_peak: bool, pub device: Option<String>, pub next_boundary: Option<Micros> }` — `device` is the entity id when the device reading was usable (used for `TriggerInfo.entity`), `next_boundary` is the next window start/end as an absolute time (from `next_boundary` on the windows of `house_today()`), `None` when there are no windows.
  - Engine fields `off_peak_base: Option<bool>` (None = unseeded), `off_peak_timer: Option<chrono::DateTime<Tz>>`, `off_peak_settle: Option<chrono::DateTime<Tz>>`.

Rules (the spec's, made exact):
- **Seed silently** (set base, no fire): `reload()` while online and no device is set; `WsEvent::HomeSettingsChanged`; the device's first `StateChanged` / `state_seen` (current epoch) while unseeded; the settle timer.
- **Unseed**: `ConnectionChanged(!Online)` and events `Lagged` (same places that clear `known`). On `ConnectionChanged(Online)` arm `off_peak_settle = now + 60 s`; when it passes and the base is still `None`, seed (covers a device that never replays).
- **Fire** (base is `Some(prev)`, new value differs): on `StateChanged` of the device entity and when `off_peak_timer` passes. Fire every enabled flow with a `trigger.off_peak` whose `edge` matches (`Start` when new is `true`), one fire per flow, via `fire()`; offline → `add_skipped_run_because(.., "the broker link is offline")` like `tick`. `TriggerInfo { entity: now.device, value: "start"|"end", previous: "end"|"start" }`.
- After every resolve, re-arm `off_peak_timer` to `next_boundary`; `next_deadline()` includes both timers; the deadline branch calls a new `tick_off_peak()`.
- The base is kept whether or not any flow has a `trigger.off_peak` (cheap); only matching flows fire.
- `run_node` `If` with `Op::IsOffPeak`: `hub.off_peak_now(now()).off_peak`, detail `"is_off_peak → {b}"`, port `true`/`false` (replace the Task 2 placeholder).

- [ ] **Step 1: Failing tests** (engine tests, a fake clock via `engine.now`):
  - `device_change_fires_start_then_end`: device `sensor.tariff` values `["OFF"]`, flows `on` (edge start) and `off` (edge end); seed with `"PEAK"`, change to `"OFF"` → one `on` run; back to `"PEAK"` → one `off` run.
  - `reconnect_replay_seeds_without_firing`: seeded `"PEAK"`; `ConnectionChanged(Offline)`; `ConnectionChanged(Online)`; `StateChanged` `"OFF"` → no run; next change to `"PEAK"` → `off` runs.
  - `settings_change_never_fires`: seeded; `HomeSettingsChanged` whose new windows flip the answer → no run.
  - `flow_save_never_fires`: seeded; `reload()` → no run.
  - `window_boundary_fires_without_device`: windows `22:00–06:00`, clock 21:59, seeded; advance to 22:00 and `tick_off_peak()` → one `on` run; `TriggerInfo.entity == None`.
  - `fallback_without_windows_ends_once`: device set, no windows, seeded `"OFF"` (true); `StateChanged` with availability `Offline` → one `off` run; a second offline report → no run.
  - `is_off_peak_reads_the_resolver`: `run_node` on `{"op":"is_off_peak"}` with device `"OFF"` → port `["true"]`, detail `"is_off_peak → true"`.
- [ ] **Step 2:** `cargo test -p oikos-os flows::engine` → FAIL.
- [ ] **Step 3:** Implement `Hub::off_peak_now` (registry `state(id)` → `device_off_peak` → `resolve_off_peak` at `local_minute` of `now` with `windows_for(&self.off_peak_history(), today)`), then the engine rules above.
- [ ] **Step 4:** `cargo test -p oikos-os flows::` → PASS.
- [ ] **Step 5:** `cargo clippy -p oikos-os --all-targets -- -D warnings` → clean.
- [ ] **Step 6: Commit** `feat(flows): off-peak trigger and is_off_peak in the engine`.

### Task 4: Energy day chart follows the device history

**Files:**
- Modify: `crates/oikos-os/src/history/db.rs` (two reads), `crates/oikos-os/src/history/query.rs`, `crates/oikos-os/src/http/api.rs:1087-1105`, `crates/oikos-os/src/mcp/tools.rs:471-490`
- Test: `query.rs` tests; `crates/oikos-os/tests/history_db.rs`

**Interfaces:**
- Consumes: Task 1 `OffPeakDevice`, `device_off_peak`.
- Produces:
  - `Db::states_between(&self, entity: &str, from: Micros, to: Micros) -> Result<(Option<StateValue>, Vec<(Micros, StateValue)>), sqlx::Error>` — the last value row before `from` (any `num`, decoded from `value` jsonb) and the rows in `[from, to)` in `(time, seq)` order; undecodable rows skipped.
  - `Db::availability_between(&self, entity: &str, from: Micros, to: Micros) -> Result<(Option<Availability>, Vec<(Micros, Availability)>), sqlx::Error>` — same shape over `entity_availability` (use the existing `parse_availability`).
  - `pub struct DeviceSpan { pub from: Micros, pub to: Micros, pub off_peak: Option<bool> }` and `pub fn device_spans(device: &OffPeakDevice, day_start: Micros, until: Micros, values: (Option<StateValue>, Vec<(Micros, StateValue)>), avail: (Option<Availability>, Vec<(Micros, Availability)>)) -> Vec<DeviceSpan>` in `query.rs` — merges both streams; each span's `off_peak` is `device_off_peak` of the value and availability in force; no carried value → `None`; spans cover `[day_start, until)` only.
  - `DayInput.device: Option<&'a [DeviceSpan]>`; `grid_day(.., device: Option<&OffPeakDevice>, ..)` gains the parameter after `history`.

- [ ] **Step 1: Failing pure tests** in `query.rs`:
  - `device_span_splits_a_slot`: windows none, device spans `00:00–00:10 PEAK`, `00:10–… OFF`; rises at 00:05 (0.2) and 00:20 (0.3) → slot 00:00 `peak 0.2, off_peak 0.3`; `tariffs == true`; one band starting 00:10.
  - `device_gap_falls_back_to_windows`: windows `00:00–06:00`, span `None` over 01:00–02:00 between `PEAK` spans → a rise at 01:30 counts off-peak.
  - `rest_of_today_uses_windows`: `now` = 12:00, spans end at 12:00, windows `22:00–06:00` → a band from 22:00 to day end.
  - `day_without_device_readings_matches_windows_only`: `device: Some(&[])` (a device set, no readings that day) → `GridDay` equal to the same input with `device: None` except `tariffs`, which is equal too when windows exist.
  - `device_spans_merge_value_and_availability`: carry `Text("OFF")`, Offline at 03:00, Online at 04:00 → spans `[0,3h) Some(true)`, `[3h,4h) None`, `[4h,until) Some(true)`.
- [ ] **Step 2:** `cargo test -p oikos-os history::query` → FAIL.
- [ ] **Step 3:** Implement `device_spans`; in `build_grid_day` the `off_peak_at` closure checks the span containing `at` first (`Some(b)` → `b`), else the windows; `tariffs = !windows.is_empty() || input.device.is_some()`; the band loop runs when `tariffs`.
- [ ] **Step 4:** `cargo test -p oikos-os history::query` → PASS.
- [ ] **Step 5: DB test** `grid_day_reads_text_device_rows_db` in `tests/history_db.rs` (same harness as `grid_day_totals_match_counter_increase_db`): insert text value rows and one offline row for `sensor.tariff`, call `states_between`/`availability_between`, assert decoded values and order. Run `cargo test -p oikos-os --test history_db grid_day_reads_text_device_rows_db` → PASS (skips the same way the others do without a DB).
- [ ] **Step 6:** Implement the two `Db` reads and wire `grid_day`: when `hub.home_settings().off_peak_device` is `Some`, read both streams over `[day_start, until)` and pass the spans. Update both callers (`http/api.rs`, `mcp/tools.rs`).
- [ ] **Step 7:** `cargo test -p oikos-os` → PASS; `cargo clippy -p oikos-os --all-targets -- -D warnings` → clean.
- [ ] **Step 8: Commit** `feat(history): grid day splits by the off-peak device's history`.

### Task 5: Web settings — device row in the off-peak editor

**Files:**
- Modify: `crates/oikos-web/src/screens/home_settings_panel.rs` (`Draft`, `OffPeakEditor`, the panel's save), `crates/oikos-web/src/setup/services/energy.rs` (`OffPeakCard` save), `crates/oikos-web/src/i18n/home.rs`
- Test: `crates/oikos-web/tests/setup_energy.rs`; in-file tests of `home_settings_panel.rs`

**Interfaces:**
- Consumes: Task 1 `OffPeakDevice`, `HomeSettings.off_peak_device`.
- Produces: `Draft.off_peak_device: Option<(String, Vec<String>)>`; `pub fn off_peak_device(d: &Draft) -> Option<OffPeakDevice>` (None when entity empty; values deduplicated, order kept).
- New `HomeText` keys (en / fr):
  - `OffPeakDevice` — "Follow a device" / "Suivre un appareil"
  - `OffPeakDeviceHint` — "Pick the entity that reports the tariff, then the values that mean off-peak." / "Choisissez l'entité qui indique le tarif, puis les valeurs qui signifient heures creuses."
  - `OffPeakValues` — "Off-peak values" / "Valeurs heures creuses"
  - `OffPeakAddValue` — "Add a value" / "Ajouter une valeur"
  - `OffPeakBackup` — "Backup windows — used when the device has no reading" / "Plages de secours — utilisées quand l'appareil ne donne rien"
  - `OffPeakClearDevice` — "Remove device" / "Retirer l'appareil"

- [ ] **Step 1: Failing tests**: in `setup_energy.rs`, `off_peak_card_saves_the_device_and_keeps_the_rest` (mirror `off_peak_from_the_card_keeps_the_rest_of_home_settings`: the PUT body carries `off_peak_device` and unchanged other fields; with no windows the body has `"off_peak": []`). In-file: `draft_device_round_trips` (`Draft::new` from settings with a device, then `off_peak_device(&draft)` equals it; empty entity → `None`; duplicate values collapse).
- [ ] **Step 2:** `cargo test -p oikos-web --test setup_energy` → FAIL.
- [ ] **Step 3:** Implement: the device row inside `OffPeakEditor` (so both the panel and the card get it) using the existing entity picker; value chips from `oikos_core::vocabulary` for the picked entity (as `if_value.rs` does) plus the entity's current value; a text input + `OffPeakAddValue` for free values; when an on/off entity is picked and values are empty, preselect `["on"]`; heading switches to `OffPeakBackup` when a device is set; no window row is added by default. Both saves put back `off_peak` and `off_peak_device`.
- [ ] **Step 4:** `cargo test -p oikos-web` → PASS; `cargo check -p oikos-web --target wasm32-unknown-unknown` → OK.
- [ ] **Step 5: Commit** `feat(web): pick an off-peak device in settings`.

### Task 6: Web flow editor — Off-peak trigger and Is off-peak check

**Files:**
- Modify: `crates/oikos-web/src/flow_picker.rs` (two library entries), `icons.rs`, `flow_text.rs`, `flows.rs`, `flow_list.rs` (`TriggerKind`), `flow_edit.rs`, `screens/flow_canvas/settings.rs` (edge sheet; op list), `screens/flow_canvas/graph.rs` if needed, `i18n/flows.rs`
- Test: in-file tests of `flow_text.rs`, `i18n/flows.rs`, `screens/flow_canvas/settings.rs`

**Interfaces:**
- Consumes: Task 2 `NodeKind::TriggerOffPeak`, `OffPeakEdge`, `Op::IsOffPeak`.
- New `FlowText` keys (en / fr): `NodeOffPeak` "Off-peak" / "Heures creuses"; `OffPeakSub` "When off-peak starts or ends" / "Quand les heures creuses commencent ou finissent"; `OffPeakStarts` "Off-peak starts" / "Les heures creuses commencent"; `OffPeakEnds` "Off-peak ends" / "Les heures creuses finissent"; `IsOffPeak` "Is it off-peak?" / "Est-ce les heures creuses ?"; `IsOffPeakSub` "Off-peak hours right now" / "Heures creuses en ce moment"; `OpIsOffPeak` "is off-peak" / "heures creuses".

- [ ] **Step 1: Failing tests**: `off_peak_cards_read_as_sentences` (trigger start → "Off-peak starts", end → "Off-peak ends", `if is_off_peak` → "Is it off-peak?", fr too); `flipping_to_is_off_peak_and_back_keeps_the_operands` (mirror the `is_dark` test in `settings.rs`; `switch_op` treats `IsOffPeak` like `IsDark` for stashing operands); `op_from("is_off_peak") == Some(Op::IsOffPeak)`.
- [ ] **Step 2:** `cargo test -p oikos-web` → FAIL.
- [ ] **Step 3:** Implement: library entries `Group::When` `TriggerOffPeak { edge: Start }` (`NodeOffPeak`, `OffPeakSub`) after the sun entry, and `Group::And` `If { op: IsOffPeak, .. }` (`IsOffPeak`, `IsOffPeakSub`) after the dark entry; edge sheet with two chips Starts/Ends; `is_off_peak` in the Compare list; an icon (reuse an existing energy/bolt icon; no new asset); replace every Task 2 placeholder arm.
- [ ] **Step 4:** `cargo test -p oikos-web` → PASS; `cargo check -p oikos-web --target wasm32-unknown-unknown` → OK.
- [ ] **Step 5: Commit** `feat(web): off-peak trigger and check in the flow editor`.

### Task 7: MCP instructions and docs

**Files:**
- Modify: the oikos MCP server instructions string (find with `grep -rn "trigger.sun" crates/oikos-os/src/mcp`), `docs/reference/flow-nodes.md`, `docs/users/flows.md`, `docs/users/web-ui.md` (Off-peak card), `docs/users/history.md` (chart split), `docs/reference/http-api.md` (`off_peak_device`), `docs/reference/mcp-tools.md` if it lists home settings fields

- [ ] **Step 1:** Add `trigger.off_peak` (`edge`: `start`|`end`, trigger value `"start"`/`"end"`) and `op: is_off_peak` beside `trigger.sun` / `is_dark` in the MCP instructions and `flow-nodes.md`; the four set-ups table from the spec in `web-ui.md`; one paragraph in `history.md`; the field in `http-api.md`. Generic wording only.
- [ ] **Step 2:** `cargo test -p oikos-os mcp` → PASS (instruction snapshot tests, if any, updated).
- [ ] **Step 3:** `grep -rniE "linky|zlinky|enedis|tempo" crates docs --include='*.rs' --include='*.md' | grep -v superpowers` → no new hits.
- [ ] **Step 4: Commit** `docs: off-peak device, trigger and check`.

### Final verification

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` → all green (check `df -h` and the cargo target size first).
