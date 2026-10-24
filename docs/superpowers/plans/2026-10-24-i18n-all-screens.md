# Every web screen in FR/EN — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every word the web UI writes itself follows the FR/EN toggle, with language-aware numbers and dates, and a guard test that stops English from creeping back.

**Architecture:** keep project 1's plain-Rust string table (enum → `(en, fr)` pair, `fn …(lang: Lang, …) -> String` helpers, `use_text()` / `use_lang()` in components) and split `src/i18n.rs` into a module with one file per area. Pure functions take an explicit `lang: Lang`; components read the context signal. A native test scans `src/` for bare user-facing literals, with a ratchet allowlist that each area task shrinks.

**Tech Stack:** Rust 2024, Leptos (CSR, wasm32), chrono, `wasm-bindgen-test` browser tests via `wasm-pack test --headless --chrome`.

**Spec:** `docs/superpowers/specs/2026-10-24-i18n-all-screens-design.md` (approved 2026-10-24). Read it first; its glossary and its "Decisions taken for the builder" are requirements.

## Global Constraints

- Scope is `crates/oikos-web` only. No change to the hub, `oikos-api`, `oikos-core` (`SwitchType::label` stays English there, the web stops calling it), the Android app or the docs.
- No external i18n crate, no runtime loading. Every entry holds both languages, so a missing French text does not compile.
- Names the user typed are never translated. Hub-written text (`ApiError.error`, `CommandResponse.reason`, `Problem.reason`, alert `title`/`body`, flow step logs) is shown as received, inside a translated lead where there is one. Do **not** render an alert's `loc` hint.
- Wording follows the spec glossary exactly; a new French term not in it is listed in the task's commit message body (`Terms: …`).
- French typography: `U+00A0` before `:` `;` `?` `!` and between a number and its unit (`°C` included, both languages); `…` is one character; sentence case for titles.
- Numbers: FR decimal `,` and thousands `U+202F`; EN decimal `.` and thousands `U+2009` (today's). 24-hour clock in both languages.
- Text updates without a reload when the toggle changes: views read the language reactively; handlers and closures read the signal at call time (`get_untracked()`), never a value captured at mount.
- Existing tests keep running in English unless they set a language.
- Defaults the web writes into stored data ("New flow", "(copy)", …) use the language active at creation and are never re-translated.
- Commits as `Gekkotron <60887050+Gekkotron@users.noreply.github.com>` (`git -c user.name=… -c user.email=…`), stage explicit paths only, end each message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Another session may share the worktree: check `git log origin/main..HEAD` before any push.
- Disk is tight (about 7 GB free). Run `df -h /` before a build; under 5 GB run per-crate commands only. Never delete a target directory you did not create.

## Commands

- Native tests: `cargo test -p oikos-web` (unit tests in `src/` plus the guard test).
- Wasm check: `cargo check -p oikos-web --target wasm32-unknown-unknown`.
- Lint: `cargo fmt --all --check && cargo clippy -p oikos-web --all-targets -- -D warnings`.
- Browser tests, one file: `cd crates/oikos-web && wasm-pack test --headless --chrome --test <name>` (all files: drop `--test`).

## PR split

- **PR 1** = Tasks 1–4, on `feat/i18n-all-screens` (already holds the spec, `dbe8a55`). Push and open the PR after Task 4.
- **PR 2…** = one PR per area task (5–13), each on its own branch from `origin/main` after PR 1 merges (`feat/i18n-<area>`). Task 5 (flows) goes first and alone: its PR lists the new French terms and the user corrects them before Tasks 6–13 start. Tasks 6–13 are independent of each other.
- **Last PR** = Task 14.

## Review Focus

1. **Toggle on a mounted screen**: a memo or a closure captured the language at mount, so the text stays in the old language until a reload. Pinned by the toggle test in Task 2 and by each area test that flips the signal.
2. **Search after translation**: typing the English kind ("cover") on the French UI, or "volet" on the English UI, finds nothing. Pinned in Task 3 (`search_words`) and Task 5 (flow card library).
3. **French decimals in inputs**: a number typed as "21,5" into a French field (setpoints, delays, brightness) is rejected or read as 21. Every number input the area tasks touch parses both `,` and `.`, pinned by `parse_decimal` in Task 2.
4. **Long French labels**: French runs about 20 % longer and breaks buttons, chips and card titles (cards already break mid-word, see TODOS). Task 14's screenshot pass at 390 px and 1280 px checks this; any overflow gets `overflow-wrap: anywhere` or `text-overflow: ellipsis` in the owning CSS file.
5. **Plural and agreement at 0 and 1**: "0 lumières allumées", "1 automatisations". Pinned by the `count` helper in Task 2 and one 0/1/2 test per area that shows a count.

---

### Task 1: Split `i18n.rs` into a module, no behaviour change

**Files:**
- Delete: `crates/oikos-web/src/i18n.rs`
- Create: `crates/oikos-web/src/i18n/mod.rs`: `Lang`, `parse`, `from_browser`, storage, `install_lang`, `use_lang`, `use_text`, `Key` + `text`, `unseen_alerts`, `flow_started`, `pub use` of every child item
- Create: `src/i18n/home.rs` (`home_date`, `home_time`, `home_number`, greeting, mood, detail, chips, `HomeText`, category and climate sentences, `house_mode_label`, `weather_link_label`)
- Create: `src/i18n/weather.rs` (`WeatherText`, `condition_label`, `compass_word`, `uv_word`, `feels_like`, `today_range`, `card_sunset`, `data_from`, rain, wind, sunrise, `short_weekday`, `updated_at`)
- Create: `src/i18n/solar.rs` (`SolarText` and every `solar_*`)
- Create: `src/i18n/ambiances.rs` (`ambiance_*`, `flow_sentence`)
- Create: `src/i18n/dashboards.rs` (`WidgetText`, `widget_text`, `GroupState`, `group_state`)
- The tests in the current `mod tests` move with the functions they test.

**Interfaces:**
- Produces: the same public paths as today (`crate::i18n::home_date`, `crate::i18n::Key`, …) through `pub use`, so no caller changes. New area files in later tasks are added as `pub mod <area>;` in `i18n/mod.rs` and are reached as `crate::i18n::<area>::…`; they are **not** re-exported flat.

- [ ] **Step 1:** Move the code as listed. Use `git mv crates/oikos-web/src/i18n.rs crates/oikos-web/src/i18n/mod.rs` first so history follows the shell part.
- [ ] **Step 2:** Add the table test the spec asks for to the tables that exist today (`Key`, `HomeText`, `WeatherText`, `SolarText`, `WidgetText`): a `const ALL` per enum, then for each variant and each language the text is non-empty, and `en != fr` unless the variant is in a short `SAME` list (`Beta`-like words that are equal in both, e.g. "Solcast", "Zigbee2MQTT", "MQTT"). This is the shape every area table copies (recipe step 5).
- [ ] **Step 3:** Run `cargo test -p oikos-web` and `cargo check -p oikos-web --target wasm32-unknown-unknown`. Expected: the old tests plus the new table tests all PASS, and no caller outside `src/i18n/` is edited. If the table test finds an entry with identical EN and FR text that should differ, fix the French.
- [ ] **Step 4:** Run the lint command. Expected: clean.
- [ ] **Step 5:** Commit: `refactor(web): split the i18n table into one file per area`.

### Task 2: Language-aware formatting primitives

**Files:**
- Create: `crates/oikos-web/src/i18n/format.rs`
- Modify: `src/i18n/home.rs` (`home_number` and `home_time` call the new helpers)
- Modify: `src/format.rs` (the private `number` and `date_time` go; `with_unit` moves)
- Modify: `src/ambiances/report.rs` (`local_time` moves out)
- Test: unit tests in `src/i18n/format.rs`; browser test `crates/oikos-web/tests/lang_toggle.rs`

**Interfaces:**
- Produces, all in `crate::i18n::format`:
  - `pub fn number(lang: Lang, n: f64) -> String`: today's `format::number` rule (integers without decimals, one decimal otherwise, no decimals and grouped thousands from 1000 up, `—` when not finite) with the language's separators.
  - `pub fn decimal(lang: Lang, v: f64, decimals: usize) -> String`: fixed places, language separator. `home_number` becomes a call to it.
  - `pub fn with_unit(value: String, unit: Option<&str>) -> String`: `U+00A0` between value and unit (today it is a plain space, so fix the existing `format.rs` tests).
  - `pub fn parse_decimal(s: &str) -> Option<f64>`: trims, accepts `,` or `.` as the decimal separator, ignores `U+202F`/`U+2009`/`U+00A0`/space thousands separators.
  - `pub fn time(t: NaiveTime) -> String`: "14:05" in both languages.
  - `pub fn weekday_short(lang: Lang, d: Weekday) -> &'static str`: FR `Lun.`…`Dim.` (as `short_weekday` today), EN `Mon`…`Sun`. `short_weekday` calls it.
  - `pub fn weekday_long(lang: Lang, d: Weekday) -> &'static str` and `pub fn month_long(lang: Lang, month: u32) -> &'static str` (the tables `home_date` holds today; `home_date` calls them).
  - `pub fn month_short(lang: Lang, month: u32) -> &'static str`: FR `janv. févr. mars avr. mai juin juil. août sept. oct. nov. déc.`, EN `Jan`…`Dec`.
  - `pub fn date_short(lang: Lang, d: NaiveDate) -> String`: FR "24 oct.", EN "Oct 24".
  - `pub fn date_time(lang: Lang, t: NaiveDateTime) -> String`: FR "24 oct. 2026, 14:05", EN "Oct 24 2026, 14:05".
  - `pub fn ago(lang: Lang, secs: u64) -> String`: under 60 s FR "à l'instant" / EN "just now"; under 1 h "il y a 5 min" / "5 min ago"; under 24 h "il y a 3 h" / "3 h ago"; under 48 h "hier" / "yesterday"; else "il y a 4 j" / "4 d ago".
  - `pub fn count(lang: Lang, n: usize, one: (&str, &str), other: (&str, &str)) -> String`: "{n} {form}". FR uses `one` for 0 and 1, EN only for 1. Each pair is `(en, fr)`.
  - `pub fn local_date_time(unix: u64) -> NaiveDateTime`: the browser's clock on wasm, UTC off the browser (generalizes `ambiances/report.rs::local_time`, which then calls it).
- Changes: `home_time(Lang::En, …)` becomes 24 h ("21:02"), per spec decision 7. `home_time(Lang::Fr, …)` keeps "21 h 02".

- [ ] **Step 1: Write the failing unit tests** in `src/i18n/format.rs`:

```rust
#[test]
fn numbers_follow_the_language() {
    assert_eq!(number(Lang::Fr, 21.46), "21,5");
    assert_eq!(number(Lang::En, 21.46), "21.5");
    assert_eq!(number(Lang::Fr, 1250.0), "1\u{202F}250");
    assert_eq!(number(Lang::En, 1250.0), "1\u{2009}250");
    assert_eq!(number(Lang::Fr, -1250.0), "-1\u{202F}250");
    assert_eq!(number(Lang::Fr, 20.0), "20");
    assert_eq!(number(Lang::Fr, f64::NAN), "—");
    assert_eq!(decimal(Lang::Fr, 3.0, 2), "3,00");
    assert_eq!(with_unit(number(Lang::Fr, 9.5), Some("°C")), "9,5\u{A0}°C");
}

#[test]
fn decimals_parse_with_either_separator() {
    assert_eq!(parse_decimal("21,5"), Some(21.5));
    assert_eq!(parse_decimal(" 21.5 "), Some(21.5));
    assert_eq!(parse_decimal("1\u{202F}250,5"), Some(1250.5));
    assert_eq!(parse_decimal("abc"), None);
    assert_eq!(parse_decimal(""), None);
}

#[test]
fn dates_and_times_follow_the_language() {
    let d = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
    let t = NaiveTime::from_hms_opt(14, 5, 0).unwrap();
    assert_eq!(time(t), "14:05");
    assert_eq!(date_short(Lang::Fr, d), "24 oct.");
    assert_eq!(date_short(Lang::En, d), "Oct 24");
    assert_eq!(date_time(Lang::Fr, d.and_time(t)), "24 oct. 2026, 14:05");
    assert_eq!(date_time(Lang::En, d.and_time(t)), "Oct 24 2026, 14:05");
    assert_eq!(month_short(Lang::Fr, 2), "févr.");
    assert_eq!(weekday_short(Lang::Fr, Weekday::Sat), "Sam.");
    assert_eq!(crate::i18n::home_time(Lang::En, NaiveTime::from_hms_opt(21, 2, 0).unwrap()), "21:02");
}

#[test]
fn relative_times() {
    assert_eq!(ago(Lang::Fr, 30), "à l'instant");
    assert_eq!(ago(Lang::Fr, 300), "il y a 5\u{A0}min");
    assert_eq!(ago(Lang::En, 300), "5\u{A0}min ago");
    assert_eq!(ago(Lang::Fr, 3 * 3600), "il y a 3\u{A0}h");
    assert_eq!(ago(Lang::En, 30 * 3600), "yesterday");
    assert_eq!(ago(Lang::Fr, 4 * 86_400), "il y a 4\u{A0}j");
}

#[test]
fn counts_agree_at_zero_and_one() {
    let one = ("light on", "lumière allumée");
    let other = ("lights on", "lumières allumées");
    assert_eq!(count(Lang::Fr, 0, one, other), "0 lumière allumée");
    assert_eq!(count(Lang::Fr, 1, one, other), "1 lumière allumée");
    assert_eq!(count(Lang::Fr, 2, one, other), "2 lumières allumées");
    assert_eq!(count(Lang::En, 0, one, other), "0 lights on");
    assert_eq!(count(Lang::En, 1, one, other), "1 light on");
}
```

- [ ] **Step 2:** Run `cargo test -p oikos-web i18n::format`. Expected: FAIL (module missing).
- [ ] **Step 3:** Implement the functions listed in Interfaces. Point `home_number`, `home_date`, `short_weekday` and `home_time` at them; delete the duplicated tables.
- [ ] **Step 4:** Run `cargo test -p oikos-web`. Expected: PASS. Update the existing assertions that expected am/pm or a plain space before a unit (home header, weather `updated_at`, `format.rs`), and list them in the commit body.
- [ ] **Step 5: Write the toggle browser test** `tests/lang_toggle.rs` (`#![cfg(target_arch = "wasm32")]`, mount like `tests/shell.rs::mount`): mount the shell in EN, assert the sidebar contains "Automations", set the `RwSignal<Lang>` to `Lang::Fr` **without remounting**, `tick().await`, assert it contains "Automatisations" and not "Automations". Run `wasm-pack test --headless --chrome --test lang_toggle`. Expected: PASS. Later tasks copy this pattern for their screen.
- [ ] **Step 6:** Commit: `feat(web): language-aware numbers, dates, relative times and counts`.

### Task 3: State words, kind words and entity labels

**Files:**
- Create: `crates/oikos-web/src/i18n/states.rs`
- Modify: `src/format.rs` (`display_value`, `degrees`, `kind_word`, `type_word`, `title_suffix`, `entity_title`, `list_label`, `word`, `choice_word` and their callers: about 140 call sites, listed by `grep -rn -E '\b(display_value|degrees|kind_word|type_word|entity_title|list_label|title_suffix|choice_word)\(' crates/oikos-web/src`)
- Modify: `src/components/history_chart/format.rs` (`state_axis_labels` "off"/"on")
- Test: unit tests in `src/i18n/states.rs` and `src/format.rs`

**Interfaces:**
- Consumes: `crate::i18n::format::{number, with_unit}` (Task 2).
- Produces, in `crate::i18n::states`:
  - `pub fn kind_word(lang: Lang, kind: Kind) -> &'static str`: one entry for every `Kind` variant (Light → Lumière, Switch → Interrupteur, Cover → Volet, Climate → Thermostat, Sensor → Capteur, BinarySensor → Détecteur, AlarmPanel → Alarme, Number → Nombre, Select → Liste, Button → Bouton, Vacuum → Aspirateur, WaterHeater → Chauffe-eau, Generic → Appareil; EN as today's `word` output).
  - `pub fn switch_type_word(lang: Lang, t: SwitchType) -> &'static str`: Switch → Interrupteur, Light → Lumière, Door → Gâche de porte, Siren → Sirène, Chime → Carillon, Plug → Prise; EN = `SwitchType::label`.
  - `pub fn device_class_word(lang: Lang, class: &str) -> Option<&'static str>`: the binary-sensor and sensor device classes the hub exposes (door, window, garage_door, motion, occupancy, presence, moisture, smoke, gas, battery, temperature, humidity, illuminance, power, energy, voltage, current, pressure, co2, pm25). `None` otherwise.
  - `pub fn state_word(lang: Lang, ctx: StateCtx, raw: &str) -> String`: known vocabulary translated (cover open/closed/opening/closing/stopped; lock locked/unlocked/locking/unlocking/jammed; alarm disarmed/armed_home/armed_away/armed_night/arming/pending/triggered; HVAC and water-heater off/heat/cool/auto/heat_cool/dry/fan_only/eco; vacuum docked/cleaning/returning/paused/idle/error; presence home/not_home; unavailable/unknown). Anything else is `format::word(raw)` unchanged.
  - `pub enum StateCtx { Cover, Lock, Alarm, Hvac, Vacuum, Presence, BinarySensor(Option<String>), Other }` (the binary-sensor variant carries the device class, for agreement: door `on` → "Ouverte", window `on` → "Ouverte", motion `on` → "Mouvement", moisture `on` → "Fuite").
  - `pub fn on_off(lang: Lang, light: bool, on: bool) -> &'static str`: light → `Key::LightOn/LightOff` ("Allumé"/"Éteint"); otherwise `Key::SwitchOn/SwitchOff` ("Activé"/"Désactivé").
  - `pub fn search_words(kind: Kind, switch_type: Option<SwitchType>) -> [&'static str; 2]`: the EN and FR words, lower case, for search matching.
- Changes in `crate::format`: every function above gains `lang: Lang` as its **first** parameter; `kind_word` and `type_word` delegate to `states`; `word` stays language-free (raw prettifier); `choice_word(lang, s)` calls `state_word(lang, StateCtx::Presence, s)`.

- [ ] **Step 1: Write failing tests** in `src/i18n/states.rs`:

```rust
#[test]
fn agreement_follows_the_device_class() {
    let door = StateCtx::BinarySensor(Some("door".into()));
    assert_eq!(state_word(Lang::Fr, door.clone(), "on"), "Ouverte");
    assert_eq!(state_word(Lang::Fr, door, "off"), "Fermée");
    assert_eq!(state_word(Lang::Fr, StateCtx::Cover, "open"), "Ouvert");
    assert_eq!(state_word(Lang::En, StateCtx::Cover, "open"), "Open");
    assert_eq!(state_word(Lang::Fr, StateCtx::Hvac, "heat"), "Chauffage");
    assert_eq!(state_word(Lang::Fr, StateCtx::Alarm, "armed_away"), "Activée (absent)");
    assert_eq!(state_word(Lang::Fr, StateCtx::Presence, "not_home"), "Absent");
}

#[test]
fn unknown_values_keep_the_raw_prettifier() {
    assert_eq!(state_word(Lang::Fr, StateCtx::Other, "power_on_behavior"), "Power on behavior");
    assert_eq!(state_word(Lang::Fr, StateCtx::Cover, "half_way"), "Half way");
}

#[test]
fn every_kind_has_two_words() {
    for kind in ALL_KINDS {
        let [en, fr] = search_words(kind, None);
        assert!(!en.is_empty() && !fr.is_empty(), "{kind:?}");
    }
    assert_eq!(search_words(Kind::Cover, None), ["cover", "volet"]);
    assert_eq!(search_words(Kind::Switch, Some(SwitchType::Plug)), ["plug", "prise"]);
}
```

`ALL_KINDS` is a `const` slice of every `Kind` variant, kept in `states.rs` (add a match on `Kind` in a test helper so a new variant fails to compile).

And in `src/format.rs`, update the existing `display_value` tests to pass `Lang::En` (assertions unchanged except the `U+00A0`) and add:

```rust
#[test]
fn french_card_values() {
    assert_eq!(display_value(Lang::Fr, &on_off(true), None), "Activé");
    assert_eq!(display_value(Lang::Fr, &light(true, Some(204)), None), "Allumé · 80\u{A0}%");
    assert_eq!(display_value(Lang::Fr, &number_state(21.46), Some("°C")), "21,5\u{A0}°C");
    assert_eq!(display_value(Lang::Fr, &cover(Some("open"), Some(40)), None), "Ouvert · 40\u{A0}%");
}
```

(`on_off`, `light`, `number_state` and `cover` are whatever state builders the existing tests in `format.rs` use; reuse them.)

- [ ] **Step 2:** Run `cargo test -p oikos-web states format`. Expected: FAIL.
- [ ] **Step 3:** Implement `states.rs`, rewire `format.rs` and update every caller. Callers in components pass `use_lang()()` inside the reactive closure that renders the value; pure callers take a `lang` from their own caller. Where a caller has no language at hand and is not a component, add a `lang: Lang` parameter to it, not a context read.
- [ ] **Step 4:** Make every search over entities match `search_words` as well as the name and id: `flow_picker.rs` (card library), `screens/devices.rs` (device search), and the entity pickers in `climate/entity_field.rs` and `dashboards/library.rs`. Add to `flow_picker.rs` tests: on `Lang::Fr`, the query "cover" finds a cover entity whose name holds neither word; on `Lang::En`, "volet" finds it.
- [ ] **Step 5:** Run `cargo test -p oikos-web`, the wasm check, the lint command and `wasm-pack test --headless --chrome`. Expected: all PASS (existing browser tests run in EN and keep their assertions).
- [ ] **Step 6:** Commit: `feat(web): state, kind and device-class words in FR/EN, bilingual search`.

### Task 4: Guard test in ratchet mode

**Files:**
- Create: `crates/oikos-web/tests/bare_text.rs` (native only: `#![cfg(not(target_arch = "wasm32"))]`)
- Create: `crates/oikos-web/tests/bare_text_allow.txt`

**Interfaces:**
- Produces: the allowlist format every area task edits:
  - `file: src/<path>.rs  # not translated yet` (whole file skipped; area tasks delete these lines),
  - `text: <exact literal>  # <reason>` (a literal allowed anywhere: "kWh", "Zigbee2MQTT", "MQTT", "Solcast", "Wi-Fi", "—", "·").
- The test fails on an entry that is never used (a `file:` whose file has no flagged literal anymore, or a `text:` nothing matches), so the list only shrinks.

**Detection rule** (a line-based scan is enough; `syn` is allowed if it stays under ~150 lines):
- Scan every `src/**/*.rs` except `src/i18n/**`. Skip from a `#[cfg(test)]` line to the end of that module, and skip `//` comments.
- A literal is flagged when it holds a run of two or more ASCII letters **and** (it contains a space **or** it starts with an upper-case letter) **and** the line does not match any exclusion: `class=`, `class:`, `id=`, `href=`, `style`, `src=`, `type=`, `key=`, `data-`, `for=`, `role=`, `inputmode=`, `autocomplete=`, `log::`, `web_sys::console`, `expect(`, `panic!`, `unreachable!`, `assert`, `#[`, `"/api`, `concat!`, `include_`, `.query_selector`, `serde`, `rename`.
- Also flag a literal that is the value of `aria-label=`, `title=`, `placeholder=` or `alt=`, even when lower case.

- [ ] **Step 1: Write the scanner's own tests** in `tests/bare_text.rs` against inline snippets:

```rust
#[test]
fn flags_view_text_and_attributes() {
    assert_eq!(flagged(r#"<button>"Save"</button>"#), ["Save"]);
    assert_eq!(flagged(r#"<input placeholder="search"/>"#), ["search"]);
    assert_eq!(flagged(r#"toasts.push("Flow saved");"#), ["Flow saved"]);
}

#[test]
fn ignores_technical_literals() {
    assert!(flagged(r#"<div class="card card--wide">"#).is_empty());
    assert!(flagged(r#"let id = "group_cover_living";"#).is_empty());
    assert!(flagged(r#"#[serde(rename = "Light")]"#).is_empty());
    assert!(flagged(r#"// "Not this""#).is_empty());
    assert!(flagged("#[cfg(test)]\nmod tests { const X: &str = \"Some text\"; }").is_empty());
}
```

- [ ] **Step 2:** Run `cargo test -p oikos-web --test bare_text`. Expected: FAIL (no `flagged`).
- [ ] **Step 3:** Implement `fn flagged(source: &str) -> Vec<String>` and the main test `no_bare_text_outside_the_table`, which runs it on every file, removes allowlisted hits, fails listing `path:line "literal"` for the rest, and fails on unused allowlist entries. With `BARE_TEXT_BLESS=1` set, it rewrites the allowlist: one `file:` line per file that still has hits, keeping the existing `text:` lines.
- [ ] **Step 4:** Run `BARE_TEXT_BLESS=1 cargo test -p oikos-web --test bare_text`, then add the `text:` exceptions by hand for real non-words, then run without the variable. Expected: PASS. The `file:` lines are now the to-do list for Tasks 5–14; there are no `file:` lines for files Tasks 1–3 finished.
- [ ] **Step 5:** Full check: native tests, wasm check, lint, browser tests. Expected: PASS.
- [ ] **Step 6:** Commit `test(web): guard against untranslated text, ratchet allowlist`. Push `feat/i18n-all-screens` (`git push -u origin feat/i18n-all-screens`) and open PR 1: "Every web screen in FR/EN, part 1: table split, formatting, state words, guard". The body lists the French terms Tasks 2–3 introduced that are not in the glossary.

---

## Area tasks (5–13): the common recipe

Each area task applies the same recipe to its files. Each one is its own PR from `origin/main` (after PR 1 merges), on branch `feat/i18n-<area>`.

1. **Create `src/i18n/<area>.rs`** with one `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum <Area>Text` for fixed words and a `pub fn <area>_text(lang: Lang, k: <Area>Text) -> &'static str` holding the `(en, fr)` pairs. Use a `pub fn …(lang: Lang, …) -> String` helper for each sentence with values, with counts through `i18n::format::count`. Register it with `pub mod <area>;` in `i18n/mod.rs`. Reuse `Key` entries that already exist (Save, Delete, Edit…) instead of duplicating them.
2. **Route every flagged literal in the area's files through it**, including `aria-label`, `title`, `placeholder`, toasts, confirmation questions and error leads. Numbers, dates and times go through `i18n::format`, state and kind words through `i18n::states`. A number `<input>` parses with `parse_decimal`.
3. **Delete the area's `file:` lines** from `tests/bare_text_allow.txt`. Run `cargo test -p oikos-web --test bare_text`. Expected: PASS with no hit in the area's files.
   The guard cannot tell a lone lower-case word from an id outside `view!` (colour names in `flow_text.rs`, "sunrise"/"sunset", "min", chart "on"/"off"). Also run `grep -n -E '(=>|^\s*)"[a-z]+",?$' <area files>` and translate each hit that people read; leave ids alone.
4. **Add the browser test** named in the task. It mounts the area's main screen with `Lang::Fr`, asserts the listed French strings are present and the listed English ones absent, then sets the signal to `Lang::En` without remounting and asserts the reverse. Keep existing EN tests as they are.
5. **Table test**: in `src/i18n/<area>.rs`, a unit test iterates every `<Area>Text` variant (list them in a `const ALL: &[<Area>Text]`) and checks that `en` and `fr` are non-empty, and that `en != fr` unless the variant is in a short `SAME` list.
6. Run native tests, wasm check, lint and the area's browser test files. Expected: PASS.
7. Commit `feat(web): <area> in FR/EN` with a `Terms:` line listing each French term not in the glossary. Push and open the PR.

### Task 5: Flows (first, alone, glossary checkpoint)

**Files:** `flow_list.rs`, `flows.rs`, `flow_picker.rs`, `flow_text.rs`, `flow_edit.rs`, `flow_layout.rs`, `schedule.rs`, `screens/flows/{mod,bar,card,row,room_menu}.rs`, `screens/flow_canvas/{mod,graph,library,settings,sun,toolbar,viewport,drawer}.rs`, `screens/flow_schedule.rs`, `components/flow_switch.rs`, `dashboards/flow_button.rs`. Area module: `i18n/flows.rs`, `FlowText`.

**Specifics:**
- `flow_text.rs` and `schedule.rs` are pure: their public functions gain `lang: Lang` first. Their unit tests get FR twins: "Turn on Lumière table" → "Allumer Lumière table"; "When Lumière table changes" → "Quand Lumière table change"; "Lumière table is on?" → "Lumière table est allumée ?"; a daily 07:00 cron → EN "Every day at 07:00", FR "Tous les jours à 07:00"; weekdays only → "Du lundi au vendredi à 07:00".
- Colour and effect names in card titles come from the table.
- Run times and step times (`format_timestamp` callers in `screens/flows/{card,row}.rs` and `screens/flow_canvas/drawer.rs`) show `i18n::format::ago` as text and `i18n::format::date_time(lang, local_date_time(secs))` as the `title`. `format::format_timestamp` is removed once these callers and `screens/alert_detail.rs` (Task 8) stop using it; in this task keep it for Task 8.
- Defaults written to stored flows ("New flow", the `notify` title default, "(copy)") use the creation-time language (spec decision 1).
- Hub validation errors on save stay as received, after a translated lead: FR "Enregistrement impossible : <hub text>".
- Browser test: extend `tests/flows_list.rs` and `tests/flow_canvas.rs` with a French case: list title "Automatisations", the "New flow" button reads "Nouvelle automatisation", the card library sections read "Déclencheurs", "Conditions", "Actions"; the English words "When", "Turn on" and "New flow" are absent.
- After the PR is open, **stop**. The user reviews the `Terms:` list before Tasks 6–13 start.

### Task 6: Climate

**Files:** `climate/{mod,entity_field,proposal,settings_sheet,timeline,zone_card}.rs`, `screens/climate.rs`, `cards/climate.rs`, `cards/water_heater.rs`, `components/dial.rs`. Area module: `i18n/climate.rs`, `ClimateText`.
**Specifics:** setpoint inputs parse with `parse_decimal`; timeline day chips use `weekday_short`; mode names come from `state_word(lang, StateCtx::Hvac, …)`. Browser test: extend `tests/climate_timeline.rs`: FR shows "Consigne" and "Lun.", not "Setpoint" or "Mon"; a typed "19,5" is saved as 19.5.

### Task 7: Energy and solar leftovers

**Files:** `energy.rs`, `energy_day.rs`, `screens/energy.rs`, `screens/energy_day.rs`, `dashboards/energy_stat.rs`, plus any `solar/*` file still listed in the allowlist. Area module: `i18n/energy.rs`, `EnergyText`.
**Specifics:** the day chart's axis hours use `time`, the ‹ › day label uses `date_short`, and the off-peak/peak legend comes from the table. Browser test: extend `tests/energy.rs`: FR shows "Heures creuses" and "Consommation", not "Off-peak" or "Consumption"; a 1 250 W value reads "1 250 W" with `U+202F`.

### Task 8: Security and alerts

**Files:** `security.rs`, `screens/security.rs`, `alerts.rs`, `screens/alerts.rs`, `screens/alert_detail.rs`, `components/alert_row.rs`, `components/alarm_hero.rs`, `cards/alarm.rs`, `cards/siren.rs`, `cards/door.rs`, `cards/binary_sensor.rs`. Area module: `i18n/security.rs`, `SecurityText`.
**Specifics:** the two `to_locale_date_string("default", …)` calls are replaced by `date_short` / `date_time` with the toggle's language; "Today" / "Yesterday" headings come from the table. Alert title and body stay as the hub sent them (no `loc` rendering). Remove `format::format_timestamp` after replacing its call here. Browser test: extend `tests/security.rs`: FR shows "Aujourd'hui" and "Désactivée" for a disarmed alarm, not "Today" or "Disarmed".

### Task 9: Devices, entity sheet and overrides

**Files:** `override_editor.rs`, `screens/devices.rs`, `screens/entity_sheet.rs`, `screens/generic_panel.rs`, `settings_rules.rs`, `cards/{generic,number,select,button,text,sensor,switch,light,cover,vacuum,chime}.rs`, `components/{card_frame,card_place,robot,hs_wheel,sparkline,room_title}.rs`. Area module: `i18n/devices.rs`, `DeviceText`.
**Specifics:** the "Show as" choices use `switch_type_word`; card fields use `display_value(lang, …)`. Card texts still read once at build through `i18n::current_lang()` (card titles in `components/card_frame.rs`, light/switch/climate names, siren and generic values, water-heater target/inlet, climate-group target, `dashboards/history_widget.rs` title) become reactive here: `cards::live_value` shows the pattern; a `String` prop of `Stepper`/`PowerToggle`/`ModeSelect` becomes a `Signal<String>`. Browser tests: extend `tests/sheet.rs` and `tests/generic_card.rs`: FR shows "Afficher comme", "Prise" and "Entité", not "Show as" or "Plug".

### Task 10: Groups

**Files:** `groups.rs`, `screens/groups.rs`, `screens/group_sheet_v2.rs`, `screens/group_wizard.rs`, `cards/group.rs`, `dashboards/room_group.rs`. Area module: `i18n/groups.rs`, `GroupText` (move `GroupState` from `i18n/dashboards.rs` here if it fits better).
**Specifics:** member counts use `count` ("1 appareil", "3 appareils"); the default name the wizard writes uses the creation-time language. Browser test: extend `tests/group_wizard.rs` and `tests/group_card.rs`: FR shows "Nouveau groupe" and "0 appareil", not "New group".

### Task 11: Dashboards and history

**Files:** `dashboards/{config_sheet,edit,grid,history_widget,library,mod,widget}.rs`, `screens/dashboard.rs`, `screens/history_detail.rs`, `components/history_chart/{format,mod,view,geom}.rs`, `components/now.rs`. Area module: `i18n/history.rs`, `HistoryText` (dashboard words join `i18n/dashboards.rs`).
**Specifics:** this closes TODOS P1 "History chart is English only on the FR UI". `tick_labels`, `tooltip_time` and the weekday and month tables in `history_chart/format.rs` take `lang` and use `i18n::format`; "now" → "maintenant"; "Last 24 hours" → "24 dernières heures"; "min"/"max" stay. Delete the TODOS entry. Browser test: extend `tests/dashboard_screen.rs`: FR shows "Tableau de bord" in the settings sheet and "maintenant" on a history widget's axis, not "now".

### Task 12: Settings, home, rooms and shell leftovers

**Files:** `screens/settings.rs`, `screens/home.rs`, `screens/home_settings_panel.rs`, `screens/room.rs`, `screens/weather.rs`, `screens/solar.rs`, `rooms.rs`, `shell/{header,mod,route,sidebar}.rs`, `home_header/*`, `ambiances/*`, `theme.rs`, `chime.rs`, plus any `file:` line not owned by another task. Area module: `i18n/settings.rs`, `SettingsText`.
**Specifics:** settings sub-page titles (Z2M, MCP, broker, appearance, about) and their bodies; theme names "Auto", "Night Glass", "Ivoire" stay as names. Browser test: extend `tests/settings_updates.rs`: FR shows "Apparence" and "À propos", not "Appearance" or "About".

### Task 13: The web's own errors and shared components

**Files:** `api/{core,mod,ambiances,climate,history,home,series_cache}.rs`, `components/{confirm,toast,status_pill,mod}.rs`, `commands.rs`, `ws.rs`, `app.rs`, `nav.rs`, `layout.rs`, `house.rs`, `house_state.rs`, `icons.rs`. Area module: `i18n/errors.rs`, `ErrorText`.
**Specifics:** `network_error`, `decode_error` ("HTTP 500") and `BROKER_OFFLINE` become an `ApiError` whose `error` is built at display time. Add `pub enum WebError { Network(String), Http(u16), BrokerOffline }` and `pub fn web_error_text(lang: Lang, e: &WebError) -> String`. `ApiError` stays the hub's type: keep it for hub bodies and add a `From<WebError>` path so screens show `web_error_text` for web errors and the raw `error` for hub ones. FR: "Erreur réseau : …", "Le hub a répondu HTTP 500", "Broker hors ligne — commande refusée". Browser test: a stubbed 503 on a command (use `tests/support/fetch_stub.rs`) shows "Broker hors ligne — commande refusée" on FR.

---

### Task 14: Final pass and the empty allowlist

**Files:**
- Modify: `crates/oikos-web/tests/bare_text_allow.txt` (no `file:` lines left)
- Modify: CSS of any component that overflows in French (`crates/oikos-web/styles/*.css`)
- Modify: `TODOS.md` (remove the items this project closed)

- [ ] **Step 1:** Run `cargo test -p oikos-web --test bare_text`. Expected: PASS with zero `file:` lines. If any remain, translate those files with the recipe above.
- [ ] **Step 2:** Build the UI and serve it against the live hub with the scratchpad recipe from the solar PR (a scratchpad `Trunk.toml` proxying `/api` and `/ws` to the hub, see memory `reference_headless_chrome_cdp_screenshots`). With `localStorage['oikos.lang']='fr'`, take headless Chrome screenshots at 390 px and 1280 px wide of every route: `/`, `/dashboards/house`, `/rooms/<id>`, `/flows`, `/flows/<id>`, `/groups`, `/energy`, `/energy/day`, `/security`, `/alerts`, `/climate`, `/weather`, `/solar`, `/settings` and each sub-page, `/devices`, one entity sheet, one history page. Then repeat with `'en'`.
- [ ] **Step 3:** List every English word on a French screenshot (and the reverse), excluding user names and hub text. Fix each one in its file. Fix each overflow (Review Focus 4).
- [ ] **Step 4:** Full check: `cargo test --workspace`, wasm check, lint, `wasm-pack test --headless --chrome`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(web): last FR/EN gaps, empty guard allowlist`. Open the PR with the before/after screenshots in the body.
