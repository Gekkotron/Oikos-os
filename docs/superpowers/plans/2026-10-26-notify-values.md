# Notify Values Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `{…}` placeholders in a flow notify card's title and body (entity values with units, `{trigger}`, `:N` rounding, spaced maths, EN/FR translation), filled by the hub at run time, with a **＋ Value** picker and a live preview in the web editor.

**Architecture:** One pure template engine in `oikos-core::flow::template` (parse, evaluate, format, translate) shared by the hub (validation, run-time render) and the web app (preview, canvas title). Values reach it through a caller-supplied lookup `&dyn Fn(&str) -> Option<Reading>`, so the engine knows nothing of the registry or the web `House`.

**Tech Stack:** Rust 2024 workspace, Leptos (wasm) web, Tokio hub, serde/schemars. No new dependency.

**Spec:** `docs/superpowers/specs/2026-10-26-notify-values-design.md`

## Global Constraints

- No new crate dependency (`minijinja` is not the user-facing syntax).
- Tracked files stay generic: no user hostnames, IPs, rooms or family names (example ids: `weather_*`, `sensor.kitchen_temperature`).
- All new UI text exists in EN and FR through the existing i18n tables.
- Unit after a number: U+00A0 non-breaking space (`10.1 °C`); `%` sticks in EN (`77%`), takes U+00A0 in FR (`77 %`).
- Decimal mark: `.` for `en`, `,` for `fr`. Templates always use `.` in literals.
- `:N` allows N in 0..=6, rounds half away from zero. No `:N`: at most 2 decimals, trailing zeros dropped, `-0` prints `0`. No thousands separator.
- `?` marks every run-time miss; a push is never skipped because of a value.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only (another session may share the worktree).
- Branch `feat/notify-values` (from `origin/main`, its upstream wrongly points at `origin/main`): push with `git push -u origin feat/notify-values`, never to `main`.
- Disk is tight: check `df -h ~` and `du -sh ~/dev-scratch/cargo-target` before building; over 5 GB run `cargo clean`; run per-crate tests (`-p`) rather than `--workspace` when free space is under 5 GB.
- CI gates (run before the PR): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p oikos-core --target wasm32-unknown-unknown`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, `cargo test -p oikos-core -p oikos-os -p oikos-web`.

## Review Focus

1. Entity ids with `-`, `,`, accents (`double_take_rémi`, `0x…_c,28-0a-…_zigbee2mqtt`): one id, not maths; inserted by the picker and rendered. → Task 1 tests.
2. A caret after an accented character: JS `selectionStart` counts UTF-16 units, Rust strings are UTF-8; inserting must not split a character or land off by one. → Task 6 `insert_at` tests.
3. A flow run with no trigger (manual **Run now**, schedule, sun): `{trigger}` prints `?` and the push still goes. → Task 4 test.
4. A text containing literal braces typed by hand (`Temp {ok}` where `ok` is not an entity): refused at save with a readable reason; `{{ok}}` saves and sends `{ok}`. → Task 1 + Task 4 tests.
5. Placeholder on an entity that disappears after save: the run prints `?` and still sends (no failed step). → Task 4 test.

---

### Task 1: Template parser

**Files:**
- Create: `crates/oikos-core/src/flow/template/mod.rs`, `crates/oikos-core/src/flow/template/parse.rs`
- Modify: `crates/oikos-core/src/flow/mod.rs` (add `pub mod template;` and re-export `Lang`, `Reading`, `Template`, `TemplateError`)

**Interfaces:**
- Produces (in `template/mod.rs`):
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] #[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))] pub enum Lang { #[default] En, Fr }`
  - `pub struct Template` (opaque; holds `Vec<Part>`, `Part = Text(String) | Value { expr: Expr, decimals: Option<u8> }`, `Expr = Number(f64) | Entity(String) | Trigger | Bin(Box<Expr>, BinOp, Box<Expr>)`, `BinOp = Add | Sub | Mul | Div`)
  - `#[derive(Clone, Debug, PartialEq, thiserror::Error)] #[error("{reason}")] pub struct TemplateError { pub reason: String }`
  - `pub fn parse(text: &str) -> Result<Template, TemplateError>`
  - `impl Template { pub fn entities(&self) -> Vec<&str> }` — distinct ids in first-seen order, `trigger` excluded.
  - `impl Template { pub fn is_plain(&self) -> bool }` — true when there is no placeholder.

Grammar (spec "Syntax"): outside braces `{{` → `{`, `}}` → `}`, a lone `}` is an error. Inside `{…}`: split on whitespace; `(` and `)` are tokens of their own even when glued; a token exactly `+`, `-`, `*`, `/` is an operator; a token matching `-?\d+(\.\d+)?` is a number; `trigger` is the trigger; anything else is an entity id. A trailing `:N` is cut from the last token (`x:1` → `x` + 1; a lone `:1` token after `)` also works). Precedence `* /` over `+ -`, left-associative.

Error reasons (exact text, tests assert `contains`): `unclosed {`, `unexpected }`, `empty {}`, `missing operator between \`a\` and \`b\``, `missing value after \`-\``, `unbalanced parentheses`, `decimals must be 0 to 6`.

- [ ] **Step 1: Write the failing tests** in `parse.rs` `#[cfg(test)] mod tests`:

```rust
#[test] fn plain_text_has_no_entities() { let t = parse("Door open").unwrap(); assert!(t.is_plain()); assert!(t.entities().is_empty()); }
#[test] fn ids_with_dashes_commas_and_accents_are_one_entity() {
    assert_eq!(parse("{double_take_rémi}").unwrap().entities(), vec!["double_take_rémi"]);
    assert_eq!(parse("{0x00_c,28-0a-18_5_zigbee2mqtt}").unwrap().entities(), vec!["0x00_c,28-0a-18_5_zigbee2mqtt"]);
    assert_eq!(parse("{a-b}").unwrap().entities(), vec!["a-b"]);
    assert_eq!(parse("{a - b}").unwrap().entities(), vec!["a", "b"]);
}
#[test] fn trigger_numbers_and_decimals() {
    let t = parse("{trigger} {(weather_today_max - weather_today_min):1} {x * -2}").unwrap();
    assert_eq!(t.entities(), vec!["weather_today_max", "weather_today_min", "x"]);
    assert!(parse("{x:6}").is_ok());
    assert!(parse("{x:7}").unwrap_err().reason.contains("decimals must be 0 to 6"));
}
#[test] fn escaped_braces_are_text() { assert!(parse("{{ok}}").unwrap().is_plain()); }
#[test] fn bad_templates_say_why() {
    for (text, why) in [("Temp {x", "unclosed {"), ("a } b", "unexpected }"), ("{ }", "empty {}"),
        ("{a b}", "missing operator between `a` and `b`"), ("{a -b}", "missing operator between `a` and `-b`"),
        ("{a -}", "missing value after `-`"), ("{(a + b}", "unbalanced parentheses"), ("{a + b)}", "unbalanced parentheses")] {
        assert!(parse(text).unwrap_err().reason.contains(why), "{text}");
    }
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-core flow::template` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `parse` as a hand-written tokenizer plus recursive descent (`expr := term (('+'|'-') term)*`, `term := atom (('*'|'/') atom)*`, `atom := number | trigger | id | '(' expr ')'`).
- [ ] **Step 4: Run** `cargo test -p oikos-core flow::template` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): parse notify templates` (stage the three files).

### Task 2: Evaluate, format and translate

**Files:**
- Create: `crates/oikos-core/src/flow/template/render.rs`, `crates/oikos-core/src/flow/template/words.rs`
- Modify: `crates/oikos-core/src/flow/template/mod.rs`

**Interfaces:**
- Consumes: Task 1 `Template`, `Lang`, `parse`; existing `Scalar::of_state`, `Scalar::of_generic`, `crate::types::type_def`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct Reading { pub value: Scalar, pub unit: Option<String>, pub device_class: Option<String> }`
  - `impl Reading { pub fn of(component: &str, device_class: Option<&str>, unit: Option<&str>, state: &StateValue) -> Option<Reading> }` — `Scalar::of_state`, and for `StateValue::Generic` `type_def(component)` + `Scalar::of_generic`; `None` when there is no value.
  - `impl Template { pub fn render(&self, lang: Lang, lookup: &dyn Fn(&str) -> Option<Reading>, trigger: Option<&Reading>) -> String }`
  - `impl Template { pub fn label(&self, name: &dyn Fn(&str) -> Option<String>) -> String }` — the template with each entity id replaced by `name(id)` (id when `None`), operators re-spaced, `:N` kept, `{trigger}` kept, literal braces re-escaped. For the canvas title.
  - `pub fn render(text: &str, lang: Lang, lookup: &dyn Fn(&str) -> Option<Reading>, trigger: Option<&Reading>) -> Result<String, TemplateError>` (free fn: parse + render).
  - `words.rs`: `pub fn on_off_word(on: bool, device_class: Option<&str>, lang: Lang) -> &'static str`, `pub fn text_word(raw: &str, lang: Lang) -> String`, `pub fn format_number(n: f64, decimals: Option<u8>, lang: Lang) -> String`, `pub fn with_unit(number: String, unit: Option<&str>, lang: Lang) -> String`.

Evaluation rules (spec "Syntax"): a bare entity/trigger whose value is `Bool` → `on_off_word`, `Text` → `text_word`, `Number` → `format_number` + `with_unit`. Inside maths only numbers count; text/bool/missing/÷0/non-finite → `?` for the whole placeholder. Units: `+`/`-` keep the unit when both sides share it or one side has none; `*`/`/` keep it when the other side has none; else drop. `:N` on a non-number is ignored.

Word tables (exact):
- `on_off_word`: device class `door|window|opening|garage_door` → Open/Closed, Ouvert/Fermé; `motion|occupancy|presence` → Detected/Clear, Détecté/Rien; else On/Off, Allumé/Éteint.
- `text_word` table (EN / FR): clear Clear/Dégagé, partly_cloudy Partly cloudy/Éclaircies, cloudy Cloudy/Nuageux, fog Fog/Brouillard, drizzle Drizzle/Bruine, rain Rain/Pluie, freezing_rain Freezing rain/Pluie verglaçante, snow Snow/Neige, showers Showers/Averses, thunderstorm Thunderstorm/Orage; open Open/Ouvert, closed Closed/Fermé, opening Opening/Ouverture, closing Closing/Fermeture, stopped Stopped/Arrêté; disarmed Disarmed/Désarmée, armed_home Armed (home)/Armée (présence), armed_away Armed (away)/Armée (absence), armed_night Armed (night)/Armée (nuit), arming Arming/Armement, pending Pending/En attente, triggered Triggered/Déclenchée; off Off/Arrêt, heat Heat/Chauffage, cool Cool/Climatisation, auto Auto/Auto, dry Dry/Déshumidification, fan_only Fan/Ventilation, heat_cool Heat/cool/Chaud/froid, on On/Allumé. Anything else: `_` → space, first letter upper-cased, same in both languages.

- [ ] **Step 1: Write the failing tests** in `render.rs`:

```rust
fn r(v: Scalar, unit: Option<&str>, dc: Option<&str>) -> Reading { Reading { value: v, unit: unit.map(Into::into), device_class: dc.map(Into::into) } }
fn house(id: &str) -> Option<Reading> { match id {
    "t" => Some(r(Scalar::Number(10.1), Some("°C"), None)), "max" => Some(r(Scalar::Number(12.0), Some("°C"), None)),
    "min" => Some(r(Scalar::Number(6.6), Some("°C"), None)), "rain" => Some(r(Scalar::Number(0.0), Some("mm"), None)),
    "hum" => Some(r(Scalar::Number(77.0), Some("%"), None)), "cond" => Some(r(Scalar::Text("partly_cloudy".into()), None, None)),
    "door" => Some(r(Scalar::Bool(true), None, Some("door"))), "mode" => Some(r(Scalar::Text("eco_mode".into()), None, None)),
    _ => None } }
fn en(text: &str) -> String { render(text, Lang::En, &house, None).unwrap() }
fn fr(text: &str) -> String { render(text, Lang::Fr, &house, None).unwrap() }
#[test] fn values_carry_units_and_decimal_marks() {
    assert_eq!(en("Now {t}"), "Now 10.1\u{a0}°C"); assert_eq!(fr("Now {t}"), "Now 10,1\u{a0}°C");
    assert_eq!(en("{t:0}"), "10\u{a0}°C"); assert_eq!(en("{hum}"), "77%"); assert_eq!(fr("{hum}"), "77\u{a0}%");
    assert_eq!(en("{rain}"), "0\u{a0}mm");
}
#[test] fn maths_rounds_and_keeps_shared_units() {
    assert_eq!(en("{max - min:1}"), "5.4\u{a0}°C"); assert_eq!(en("{max - min}"), "5.4\u{a0}°C");
    assert_eq!(en("{rain * 2}"), "0\u{a0}mm"); assert_eq!(en("{t * hum}"), "777.7");
    assert_eq!(en("{(max + min) / 2:1}"), "9.3\u{a0}°C"); assert_eq!(en("{1 / 3}"), "0.33"); assert_eq!(en("{2.5:0}"), "3");
}
#[test] fn misses_print_a_question_mark() {
    assert_eq!(en("{gone}"), "?"); assert_eq!(en("{t / 0}"), "?"); assert_eq!(en("{cond + 1}"), "?"); assert_eq!(en("{trigger}"), "?");
}
#[test] fn text_and_on_off_are_translated() {
    assert_eq!(en("{cond}"), "Partly cloudy"); assert_eq!(fr("{cond}"), "Éclaircies");
    assert_eq!(fr("{door}"), "Ouvert"); assert_eq!(en("{mode}"), "Eco mode");
}
#[test] fn trigger_uses_the_given_reading() {
    let trig = r(Scalar::Number(3.0), Some("mm"), None);
    assert_eq!(render("{trigger}", Lang::Fr, &house, Some(&trig)).unwrap(), "3\u{a0}mm");
}
#[test] fn escaped_braces_render_literally() { assert_eq!(en("{{t}} {t:0}"), "{t} 10\u{a0}°C"); }
#[test] fn label_names_entities() {
    let name = |id: &str| (id == "max").then(|| "Today's maximum".to_owned());
    assert_eq!(parse("Spread {max - min:1}").unwrap().label(&name), "Spread {Today's maximum - min:1}");
}
#[test] fn reading_of_a_number_state() {
    let got = Reading::of("sensor", Some("temperature"), Some("°C"), &StateValue::Number(10.1)).unwrap();
    assert_eq!(got.value, Scalar::Number(10.1)); assert_eq!(got.unit.as_deref(), Some("°C"));
    assert!(Reading::of("sensor", None, None, &StateValue::Unknown).is_none());
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-core flow::template` — Expected: FAIL (`render`/`Reading` missing).
- [ ] **Step 3: Implement** `render.rs` and `words.rs` per the rules above; round with `(n * 10^N).round() / 10^N` (`f64::round` is half away from zero).
- [ ] **Step 4: Run** `cargo test -p oikos-core flow::template` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): render notify templates with units, rounding and EN/FR words`.

### Task 3: `lang` on notify + syntax validation

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (`NotifyParams`), `crates/oikos-core/src/flow/validate.rs`

**Interfaces:**
- Consumes: Task 1 `parse`, `Lang`.
- Produces: `NotifyParams { …, #[serde(default, skip_serializing_if = "Option::is_none")] pub lang: Option<Lang> }` (doc: "The language the text values are written in: `en` (the default) or `fr`."). The `title`/`body` doc comments gain one line: "Takes `{entity_id}` placeholders filled at run time (see the flows docs)." `validate` refuses a title or body that does not parse: `BadNotify { node, reason: "title: <reason>" }` / `"body: <reason>"`.

- [ ] **Step 1: Write the failing tests** in `validate.rs` tests (reuse `notify_flow`):

```rust
#[test] fn validate_rejects_bad_notify_templates() {
    let err = validate(&notify_flow(r#"{"title":"Temp {x"}"#)).unwrap_err();
    assert!(matches!(&err, FlowError::BadNotify { reason, .. } if reason.starts_with("title:") && reason.contains("unclosed {")), "{err:?}");
    let err = validate(&notify_flow(r#"{"title":"T","body":"{a b}"}"#)).unwrap_err();
    assert!(matches!(&err, FlowError::BadNotify { reason, .. } if reason.starts_with("body:")), "{err:?}");
    assert!(validate(&notify_flow(r#"{"title":"{weather_temperature:0}","body":"{{ok}}","lang":"fr"}"#)).is_ok());
}
```
and in `model.rs` tests: `{"title":"T","lang":"de"}` fails to decode as a notify node; `"lang":"fr"` round-trips to `Some(Lang::Fr)`.

- [ ] **Step 2: Run** `cargo test -p oikos-core flow::` — Expected: FAIL.
- [ ] **Step 3: Implement** the field and the two checks after the empty-title check.
- [ ] **Step 4: Run** `cargo test -p oikos-core` — Expected: PASS (whole crate; schema snapshot tests, if any, updated).
- [ ] **Step 5: Commit** `feat(core): notify lang and template validation`.

### Task 4: Hub — check entities, render at run time

**Files:**
- Modify: `crates/oikos-os/src/flows/hub.rs` (`check_house`, new `reading` method), `crates/oikos-os/src/flows/engine.rs` (`NodeKind::Notify` arm)
- Test: `crates/oikos-os/tests/flows.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces: `impl Hub { pub fn reading(&self, id: &str) -> Option<Reading> }` (registry entity + state → `Reading::of(&e.component, e.device_class.as_deref(), e.unit.as_deref(), &state.value)`). `check_house` refuses `unknown entity \`<id>\` in node \`<node>\`` for any id in the notify title/body templates. The engine renders title and body with `params.lang.unwrap_or_default()`, lookup `|id| hub.reading(id)`, trigger = `TriggerInfo` → `Scalar::from_json(&t.value)` with the unit/device class of `hub.reading(&t.entity)` (value replaced by the trigger's). Step detail becomes `sent <alert id>: <rendered title>`.

- [ ] **Step 1: Write the failing tests** in `tests/flows.rs`. Add a fixture helper that ingests `discovery/sensor__0x00124b0008fd6568__temperature_1.json` on `oikos/sensor/0x00124b0008fd6568/temperature_1/config` and a state `{"temperature_1": 10.14}` on `zigbee2mqtt/greenhouse/soil_sensors`; id `0x00124b0008fd6568_temperature_1_zigbee2mqtt` (const `TEMP_ID`).

```rust
#[tokio::test(start_paused = true)]
async fn notify_renders_values_in_the_flow_language() {
    // engine_hub + temperature fixture + push_ready; MOTION_STATE_TOPIC ← {"occupancy": false}
    let list = run_notify(&hub, &mut publisher, "temp", json!({
        "title": format!("Temp {{{TEMP_ID}:1}}"),
        "body": format!("{{{MOTION_ID}}} {{trigger}}"),
        "lang": "fr" })).await;
    assert_eq!(list[0]["title"], "Temp 10,1\u{a0}°C");
    assert_eq!(list[0]["body"], "Rien ?"); // manual run: {trigger} is "?"
    let step = &hub.flow("temp").unwrap().runs[0].steps.last().unwrap().detail;
    assert!(step.ends_with(": Temp 10,1\u{a0}°C"), "{step}");
}
#[tokio::test]
async fn set_flow_rejects_unknown_entity_in_notify_text() {
    // hub_with_entities; flow with one notify node {"title":"T {nope}"} → InvalidFlow containing "nope"; nothing published
}
#[tokio::test(start_paused = true)]
async fn notify_with_a_vanished_entity_still_sends() {
    // save a notify naming TEMP_ID, then ingest an empty payload on its config topic, run: step Ok, title "T ?"
}
```

- [ ] **Step 2: Run** `cargo test -p oikos-os --test flows notify` — Expected: FAIL.
- [ ] **Step 3: Implement** `Hub::reading`, the `check_house` pass for notify templates (titles/bodies already validated, so `parse` cannot fail there; ignore an `Err`), and the engine render.
- [ ] **Step 4: Run** `cargo test -p oikos-os --test flows` — Expected: PASS (existing `notify_step_submits_v1_alert` still green).
- [ ] **Step 5: Commit** `feat(hub): fill notify placeholders at run time`.

### Task 5: Assistant instructions and user docs

**Files:**
- Modify: `crates/oikos-os/src/mcp/mod.rs` (the `notify` sentence in the server instructions), `docs/users/flows.md`

**Interfaces:**
- Consumes: Tasks 1–4 behaviour.

- [ ] **Step 1: Replace** in `mcp/mod.rs` the text `` `notify` (a phone push: `title`, optional `body`, `channel`, `severity`, `entity_id`, `image_url`, `silent`, `id`). `` with:

```text
`notify` (a phone push: `title`, optional `body`, `channel`, `severity`, `entity_id`, `image_url`, `silent`, `id`, `lang`). Its `title` and `body` take placeholders filled at run time: `{entity_id}` is the entity's value with its unit (`{weather_temperature}` → `10.1 °C`), `{trigger}` the value that started the run, `:N` rounds to N decimals (`{weather_temperature:0}`), and `+ - * /` with spaces around them plus parentheses do maths (`{weather_today_max - weather_today_min:1}`). Text and on/off values are written in `lang` (`en`, the default, or `fr`: pass the language the user writes in). `{{` and `}}` are literal braces; a value that is unknown at run time prints `?`.
```
- [ ] **Step 2: Add** to `docs/users/flows.md`, after the paragraph describing the notify card's fields, a `### Values in a notification` section: one sentence on **＋ Value**, the syntax table from the spec (EN column and FR column), the `?` rule, the spaces-around-operators rule (with the `a-b` reason), the language rule (the web app's language when the flow is saved), and the preview line. Generic example ids only (`weather_*`).
- [ ] **Step 3: Run** `cargo test -p oikos-os mcp` — Expected: PASS (instruction-length or snapshot tests, if any).
- [ ] **Step 4: Commit** `docs: notify placeholders for users and the assistant`.

### Task 6: Web — ＋ Value insertion and language stamp

**Files:**
- Create: `crates/oikos-web/src/screens/flow_canvas/template_field.rs`
- Modify: `crates/oikos-web/src/screens/flow_canvas/mod.rs` (add the module), `crates/oikos-web/src/screens/flow_canvas/settings.rs` (`notify_fields` uses `TemplateField` for title and body), `crates/oikos-web/src/screens/flow_canvas/toolbar.rs` (save stamps `lang`), `crates/oikos-web/src/flow_edit.rs` (`stamp_notify_lang`), `crates/oikos-web/src/i18n/mod.rs` (`Lang::core`), `crates/oikos-web/src/i18n/flows.rs` (`FlowText::AddValue` "＋ Value"/"＋ Valeur", `FlowText::TriggerValueRow` "Trigger value"/"Valeur du déclencheur"; add both to the ALL list)

**Interfaces:**
- Consumes: Task 3 `NotifyParams.lang`, `oikos_core::flow::template::Lang`; existing `EntityPicker` (`Mode::Single`, `pinned: Some(Pinned { text })`, `pool: Some(keep(|e| !is_signal(e)))`), `TextField` props.
- Produces:
  - `pub fn insert_at(text: &str, caret_utf16: Option<u32>, piece: &str) -> (String, u32)` — new text and the caret after `piece`, both in UTF-16 units; `None` or past-the-end appends.
  - `#[component] pub fn TemplateField(label: Signal<String>, initial: String, placeholder: Signal<String>, optional: bool, write: Callback<Option<String>>) -> impl IntoView` — `TextField` plus a `NodeRef<Input>`, the caret remembered on `input`/`keyup`/`click`/`select` from `selection_start()`, a **＋ Value** button opening the picker; a pick inserts `{id}` (pinned row → `{trigger}`), writes, refocuses and `set_selection_range`s the caret.
  - `impl Lang { pub fn core(self) -> oikos_core::flow::template::Lang }`
  - `pub fn stamp_notify_lang(flow: &mut Flow, lang: oikos_core::flow::template::Lang)` — sets `lang: Some(lang)` on every notify node.

- [ ] **Step 1: Write the failing tests** (native `cargo test -p oikos-web`):

```rust
#[test] fn insert_at_counts_utf16_units() {
    assert_eq!(insert_at("Temp ", Some(5), "{t}"), ("Temp {t}".into(), 8));
    assert_eq!(insert_at("é!", Some(1), "{t}"), ("é{t}!".into(), 4));
    assert_eq!(insert_at("🌧 x", Some(2), "{r}"), ("🌧{r} x".into(), 5));
    assert_eq!(insert_at("ab", None, "{t}"), ("ab{t}".into(), 5));
    assert_eq!(insert_at("ab", Some(99), "{t}"), ("ab{t}".into(), 5));
}
#[test] fn stamp_sets_lang_on_every_notify() {
    // flow with two notify nodes (one lang en) and a delay → both Some(Fr), delay untouched
}
```
- [ ] **Step 2: Run** `cargo test -p oikos-web insert_at stamp_` — Expected: FAIL.
- [ ] **Step 3: Implement** `insert_at`, `stamp_notify_lang`, `Lang::core`, `TemplateField`, and call `stamp_notify_lang(&mut snapshot, lang_untracked.core())` in `toolbar.rs` `save` before `put_flow`.
- [ ] **Step 4: Run** `cargo test -p oikos-web` and `cargo check -p oikos-web --target wasm32-unknown-unknown` — Expected: PASS, no warnings.
- [ ] **Step 5: Commit** `feat(web): insert values into notify text with the entity picker`.

### Task 7: Web — preview line and named card title

**Files:**
- Modify: `crates/oikos-web/src/screens/flow_canvas/template_field.rs` (`NotifyPreview`), `crates/oikos-web/src/screens/flow_canvas/settings.rs` (render it under the body), `crates/oikos-web/src/flow_text.rs` (notify title via `Template::label`), `crates/oikos-web/src/i18n/flows.rs` (`FlowText::Preview` "Preview"/"Aperçu"), `crates/oikos-web/src/i18n/weather.rs` (agreement test), `crates/oikos-web/styles/main.css` (`.node-editor__preview`, `.node-editor__preview--error` using existing tokens)

**Interfaces:**
- Consumes: Task 2 `render`, `Reading::of`, `Template::label`; `House.entities`; the draft node's title/body.
- Produces: `#[component] pub fn NotifyPreview(title: Signal<String>, body: Signal<Option<String>>, trigger_entity: Signal<Option<String>>) -> impl IntoView` — "Preview: <title> — <body>" rendered with the web language and a lookup over `house.entities` (`Reading::of(&e.component, e.device_class.as_deref(), e.unit.as_deref(), &e.state.value)`); `trigger_entity` = the draft's first `trigger.state` entity, whose current reading stands in for `{trigger}`. A `TemplateError` shows its reason with the error class instead.

- [ ] **Step 1: Write the failing tests:**

```rust
// flow_text.rs tests
#[test] fn notify_title_names_placeholder_entities() {
    // ctx with entity "weather_today_max" named "Today's maximum": title "Max {weather_today_max:0}"
    assert_eq!(text(n), "Notify \"Max {Today's maximum:0}\"");
}
// i18n/weather.rs tests
#[test] fn condition_labels_match_the_notify_words() {
    for c in WeatherCondition::ALL { for lang in [Lang::En, Lang::Fr] {
        assert_eq!(condition_label(lang, c), oikos_core::flow::template::text_word(c.as_str(), lang.core()));
    } }
}
```
(Use the actual name of `WeatherCondition`'s snake-case string accessor in `crates/oikos-api/src/weather.rs`; `text_word` must be re-exported from `template`.)
- [ ] **Step 2: Run** `cargo test -p oikos-web notify_title condition_labels` — Expected: FAIL on the title test.
- [ ] **Step 3: Implement** `NotifyPreview`, mount it in `notify_fields`, and switch `t::notify` input to `parse(&p.title).map(|t| t.label(&name)).unwrap_or(p.title.clone())`, `name` being the entity display-name helper the other card sentences use.
- [ ] **Step 4: Run** `cargo test -p oikos-web` and `cargo check -p oikos-web --target wasm32-unknown-unknown` — Expected: PASS.
- [ ] **Step 5: Check on screen**: build the web app, open a flow, add a notify card, insert `weather_temperature` with **＋ Value**, see the preview fill in, in EN and FR (headless Chrome over CDP per the docs screenshot recipe; width ≥ 500 px).
- [ ] **Step 6: Commit** `feat(web): notify preview and named placeholders on the card`.

### Finish

- [ ] Run every CI gate from Global Constraints; all green.
- [ ] `git log origin/main..HEAD --oneline` shows only this branch's commits; `git push -u origin feat/notify-values`; open a PR against `main` with the spec link, a summary and `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
