# Flow Math Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `math` flow card that computes one number from named entity inputs with a formula, saves it into a number entity and hands it to later `command` / `if` / `switch` / `notify` cards of the same run.

**Architecture:** The formula grammar, evaluator, model and save-time validation live in `oikos-core::flow` (pure, shared by the hub and the wasm editor). The hub engine threads a per-run results map through `execute`, runs math steps, and lets the other cards read results; the hub's `check_house` adds the house-aware checks. The web editor gets a Math drawer (inputs, formula with function menu + suggestions + live preview, result name, save to) and result pickers on the other cards.

**Tech Stack:** Rust 2024 workspace; `oikos-core` (serde, thiserror), `oikos-os` (tokio engine, tests in `crates/oikos-os/tests/flows.rs`), `oikos-web` (Leptos, native unit tests + `wasm-pack test --headless --chrome`).

**Spec:** `docs/superpowers/specs/2026-10-30-flow-math-card-design.md` (mockup `docs/superpowers/specs/2026-10-30-flow-math-card-mockup.html`). Read both before any task.

## Global Constraints

- No new crate dependency; the parser is hand-written in `oikos-core` in the style of `crates/oikos-core/src/flow/template/parse.rs`.
- Formula: at most 500 characters, nesting depth at most 32; at most 10 inputs per card; `interp` tables 2 to 50 points.
- Names: `[a-z][a-z0-9_]{0,31}`, never a function name (`min max clamp abs round floor ceil if interp`) nor `and`, `or`, `not`, `trigger`.
- A math step that cannot produce a finite number fails the step; a missing value never becomes 0.
- The evaluator is the same code on the hub and in wasm; the editor never re-implements it.
- Results live for one run only, in one map per run (no per-branch copies).
- UI strings in EN and FR (`crates/oikos-web/src/i18n/flows.rs`); the project is generic: no user hosts, rooms or names in tracked files (the water-heater example in docs uses generic entity ids).
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`), message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; stage explicit paths only.
- Before building: check `df -h ~` and the size of `~/dev-scratch/cargo-target`; over 5 GB, `cargo clean` first; under 5 GB free disk, test per crate only.
- Each task ends green on `cargo clippy -p <crate> --all-targets -- -D warnings` and `cargo fmt --all --check` for the crates it touched.

## Review Focus

1. A text input (an energy meter's `"sign"` text sensor picked by mistake) → the step fails with `grid is not a number`, never a panic or 0. Test in Task 3.
2. A French user typing a decimal comma (`0,5 * grid`) or a pasted unicode minus (`−`) → a clear parse error pointing at that character. Test in Task 1.
3. The `save_to` entity refuses the command or does not echo → the math step fails, the run stops, no later command runs. Test in Task 3.
4. A math card renamed in the editor while a command still has `value_from` the old name → save refused naming the node and the name. Test in Task 2.
5. Abusive formulas (33 nested parentheses, 501 characters, a 51-point `interp`, a product overflowing to infinity) → refused at save or failed at run, no stack overflow. Tests in Task 1.

---

### Task 1: Formula grammar and evaluator (core)

**Files:**
- Create: `crates/oikos-core/src/flow/math/mod.rs`, `crates/oikos-core/src/flow/math/parse.rs`, `crates/oikos-core/src/flow/math/eval.rs`
- Modify: `crates/oikos-core/src/flow/mod.rs` (declare `pub mod math;`, re-export below)
- Test: unit tests in `crates/oikos-core/src/flow/math/mod.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces (all in `oikos_core::flow::math`, re-exported from `oikos_core::flow`):
  - `pub const MAX_FORMULA_CHARS: usize = 500; pub const MAX_DEPTH: usize = 32; pub const MAX_INPUTS: usize = 10; pub const MAX_TABLE_POINTS: usize = 50;`
  - `pub struct Formula` (`Clone, Debug, PartialEq`), opaque AST.
  - `pub struct FormulaError { pub reason: String, pub at: Option<usize> }` (`thiserror`, `#[error("{reason}")]`; `at` = byte offset of the offending character, for the editor).
  - `pub fn parse_formula(text: &str) -> Result<Formula, FormulaError>`
  - `impl Formula { pub fn names(&self) -> Vec<&str>; pub fn eval(&self, read: &dyn Fn(&str) -> Result<f64, String>) -> Result<f64, String>; }` — `names` = distinct identifiers read (not function names), first-seen order, `trigger` included; `eval` calls `read` for each name it needs.
  - `pub fn is_valid_name(name: &str) -> bool`
  - `pub struct Function { pub name: &'static str, pub signature: &'static str, pub min_args: usize, pub max_args: Option<usize> }` and `pub const FUNCTIONS: [Function; 9]` in the order `min, max, clamp, abs, round, floor, ceil, if, interp` (`signature` e.g. `"clamp(x, low, high)"`, `"interp(x, [xs], [ys])"`, `"round(x, n)"` with `min_args 1, max_args Some(2)`; `min`/`max` have `max_args: None`).
  - `pub fn fit_to_entity(value: f64, min: Option<f64>, max: Option<f64>, step: Option<f64>) -> (f64, bool)` — rounds to `step` (counted from `min` when known), clamps into `min..=max`, rounds to the step's decimals to drop float noise; the bool is true when clamping changed the value.

Grammar (spec "Grammar"): numbers `\d+(\.\d+)?`, identifiers, `( )`, `+ - * / %`, unary `-`, `< <= > >= == !=` → 1/0, `and or not` (non-zero is true) → 1/0, calls `name(args)`, list literals `[n, …]` only as `interp`'s 2nd and 3rd arguments. Precedence low→high: `or`, `and`, `not`, comparisons (non-associative), `+ -`, `* / %`, unary `-`, call/primary. `if(c, a, b)` evaluates only the taken branch. Arity, list placement and literal `interp` tables, literal `clamp` bounds and literal `round` `n` are checked at parse time.

- [ ] **Step 1: Write the failing tests** (in `math/mod.rs`), each asserting exact values or exact `reason` strings:
  - `precedence_and_unary`: `2 + 3 * 4 - -1` = 15; `(2 + 3) * 4` = 20; `7 % 4` = 3.
  - `comparisons_and_logic`: `3 > 2` = 1, `3 <= 2` = 0, `1 and 0` = 0, `0 or 2` = 1, `not 0` = 1, `2 == 2 and 1 != 2` = 1.
  - `functions`: `min(3, 1, 2)` = 1, `max(4)` = 4, `clamp(-5, 0, 10)` = 0, `abs(-2.5)` = 2.5, `round(2.5)` = 3, `round(-2.5)` = -3, `round(1.2345, 2)` = 1.23, `floor(-1.5)` = -2, `ceil(1.1)` = 2.
  - `if_is_lazy`: with `read` giving `b = 0`, `if(b != 0, 1 / b, 7)` = 7.
  - `interp_table`: `interp(x, [0, 38, 81, 160, 274, 438, 621, 843, 1062, 1270, 1450, 1650], [0, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60])` with x = 1132 → `45 + 70.0 / 208.0 * 5.0` (assert within 1e-9); x = -10 → 0; x = 5000 → 60; x = 38 → 10.
  - `names_in_order`: `parse_formula("clamp(heater - 100 - grid, 0, max(grid, trigger))").names()` == `["heater", "grid", "trigger"]`.
  - `run_time_errors`: `1 / 0` → `"division by zero"`; `5 % 0` → `"division by zero"`; a product overflowing to infinity (`read` x = 1e300, `x * x`) → `"result is not a finite number"`; `clamp(1, hi, lo)` with hi = 5, lo = 0 → `"clamp: low is above high"`; a `read` error string (`"grid is unknown"`) is returned unchanged.
  - `parse_errors` (exact `reason`, `at` where noted): `""` → `"empty formula"`; `"(1 + 2"` → `"missing )"`; `"1 +"` → `"unexpected end"`; `"0,5 * grid"` → `"unexpected ,"` with `at == Some(1)`; `"2 − 1"` (U+2212) → `"unexpected −"` with `at == Some(2)`; `"foo(1)"` → `"unknown function foo"`; `"clamp(1, 2)"` → `"clamp takes 3 arguments"`; `"min()"` → `"min takes at least 1 argument"`; `"[1, 2]"` → `"a list is only allowed in interp"`; `"interp(x, [0, 1], [0])"` → `"interp needs two lists of 2 to 50 numbers of the same length"`; `"interp(x, [0, 2, 1], [0, 1, 2])"` → `"interp: x values must increase"`; `"interp(x, xs, ys)"` → `"interp tables must be written as lists"`; `"clamp(x, 5, 0)"` → `"clamp: low is above high"`; `"round(x, 7)"` → `"round: n must be 0 to 6"`; `"clamp(▢, 0, 1)"` → `"fill in the ▢ slots"`.
  - `limits`: 501 characters → `"formula is longer than 500 characters"`; 33 nested parentheses → `"formula nests deeper than 32"`; a 51-point `interp` → the table-size message above; none of them overflow the stack.
  - `valid_names`: `is_valid_name` true for `grid`, `heater_2`; false for `Grid`, `2x`, `_x`, `clamp`, `trigger`, `and`, a 33-character name.
  - `fit_to_entity_rounds_and_clamps`: `(47.6, Some(0.0), Some(100.0), Some(1.0))` → `(48.0, false)`; `(-5.0, Some(0.0), Some(100.0), Some(1.0))` → `(0.0, true)`; `(0.30000000000000004, None, None, Some(0.1))` → `(0.3, false)`; `(12.34, None, None, None)` → `(12.34, false)`; `(19.3, Some(10.0), Some(30.0), Some(0.5))` → `(19.5, false)`.

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p oikos-core flow::math`
  Expected: compile errors (module missing).

- [ ] **Step 3: Implement `parse.rs` (tokenizer + recursive-descent parser with a depth counter) and `eval.rs` (tree walk returning `Result<f64, String>`, finiteness check on the final value and after each operation), plus `FUNCTIONS`, `is_valid_name`, `fit_to_entity` in `mod.rs`.**
  Run-time `round` with a non-literal `n` outside 0..=6 fails with `"round: n must be 0 to 6"`; halves round away from zero (`f64::round`).

- [ ] **Step 4: Run the tests**
  Run: `cargo test -p oikos-core flow::math`
  Expected: all pass.

- [ ] **Step 5: Commit** — `feat(core): flow math formula grammar and evaluator` (stage the three new files and `flow/mod.rs`).

---

### Task 2: Math card model and save-time validation (core), compile arms elsewhere

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (new types, `NodeKind::Math`, new optional params, `type_name`, `ports`, `is_trigger`)
- Modify: `crates/oikos-core/src/flow/validate.rs` (math checks, `check_ifs_after_valueless_triggers`, new `FlowError::BadMath`)
- Modify: `crates/oikos-core/src/flow/mod.rs` (re-exports)
- Modify (compile arms only): `crates/oikos-os/src/flows/engine.rs`, `crates/oikos-os/src/flows/hub.rs`, and every `oikos-web` file with an exhaustive `NodeKind` match: `flow_edit.rs`, `flow_layout.rs`, `flow_picker.rs`, `flow_text.rs`, `flows.rs`, `icons.rs`, `screens/flow_canvas/graph.rs`, `screens/flow_canvas/settings.rs`
- Test: `crates/oikos-core/src/flow/validate.rs` `mod tests`

**Interfaces:**
- Consumes: Task 1 (`parse_formula`, `Formula::names`, `is_valid_name`, `MAX_INPUTS`, `FUNCTIONS`).
- Produces (re-exported from `oikos_core::flow`):
  - `pub struct MathInput { pub name: String, pub entity: String, pub average: Option<String> }` (`average` skipped when `None`).
  - `pub struct MathParams { pub inputs: Vec<MathInput>, pub formula: String, pub name: String, pub save_to: Option<String>, pub extra: BTreeMap<String, Value> }` (`inputs` defaults to empty; `extra` flattened like the other params; `schemars` derive behind the feature like its neighbours, doc comments are the MCP schema text).
  - `NodeKind::Math(MathParams)` serialized as `"math"`; `type_name() == "math"`; one output, no ports; not a trigger.
  - `CommandParams.value_from: Option<String>`, `IfParams.result: Option<String>`, `SwitchParams.result: Option<String>` (all skipped when `None`).
  - `pub fn math_names(flow: &Flow) -> BTreeSet<&str>` — the `name`s of the flow's math cards (in `model.rs`).
  - `pub fn command_with_number(command: &Command, value: f64) -> Option<Command>` (in `model.rs`): `SetNumber` → `value`; `SetTemperature` → `temperature`; `SetPosition` → `position` rounded and kept in 0..=100; `TurnOn` → `brightness` rounded and kept in 0..=255, other fields kept; any other command → `None`.
  - `FlowError::BadMath { node: String, reason: String }` displayed `node `{node}`: {reason}`.

Validation rules (exact reasons):
- `inputs` over 10 → `at most 10 inputs`; a bad input/result name → `` `X` is not a valid name ``; duplicate input name → `` input `X` is used twice ``; an input `average` that does not parse or is over `MAX_AVERAGE_SECS` → the same reason the `if` card gives; the formula's parse error → its `reason`; a formula name that is not an input, `trigger`, or another math card's `name` → `` unknown name `X` ``; the card's own `name` read in its own formula (and not an input) → `` a math card cannot read its own result `X` ``.
- `value_from` not in `math_names` → `` no math card is named `X` ``; `value_from` on a command `command_with_number` returns `None` for → `value_from works with set_number, set_temperature, set_position and turn_on`.
- `if`/`switch` `result` not in `math_names` → same `no math card` reason; `result` with `entity`, `average` or (on `if`) `missing` → `result cannot be combined with entity, average or missing`; an `if`/`switch` with `result` counts as naming its value for `check_ifs_after_valueless_triggers`.
- Two math cards may share a `name` (branches computing the same result).

- [ ] **Step 1: Write the failing tests** in `validate.rs` `mod tests`, one per rule above, each decoding a small flow JSON with `Flow::decode` and asserting `validate(&flow) == Err(FlowError::BadMath { node, reason })` with the exact reason; plus:
  - `math_flow_is_valid`: the spec appendix's three chained math cards (`desired`, `smoothed` with input `prev`, `slider` with the `interp` table) after a `trigger.state`, a command with `value_from: "slider"` on `set_number`, an `if` with `result: "desired"`, `op: "gt"`, `value: 300` → `Ok(())`.
  - `math_round_trips`: decode → `encode` → decode equals, with `average`, `save_to` and an unknown extra key kept.
  - `renamed_math_card_refuses_stale_value_from` (Review Focus 4): rename `slider` to `percent`, keep `value_from: "slider"` → `no math card is named `slider``.
  - `command_with_number_cases`: `SetPosition` 104.6 → 100; `TurnOn{brightness: None, color: Some(..)}` with 127.5 → brightness `Some(128)` and colour kept; `Toggle` → `None`.

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p oikos-core flow::validate`
  Expected: compile errors (`NodeKind::Math` missing).

- [ ] **Step 3: Implement the model, `math_names`, `command_with_number` and the validation rules.**

- [ ] **Step 4: Add the compile arms** so the workspace builds:
  - `engine.rs` `run_node`: `NodeKind::Math(_) => failed("math cards are not run yet".to_owned())` (Task 3 replaces it).
  - `hub.rs` `check_house`: `NodeKind::Math(_) => None` (Task 3 replaces it).
  - Web, final versions: `flow_text.rs` role `Role::Action`; the card sentence `"{name} = {formula}"` plus `" → {label of save_to}"` when set (use the existing entity-label helper the command sentence uses); `flows.rs` card label = new `FlowText::MathCard` ("Math" / "Calcul"); `icons.rs` a new `ƒ`-style icon or the closest existing one; `flow_edit.rs` new node default `MathParams { inputs: vec![], formula: String::new(), name: <first free of "result", "result2", …>, save_to: None, extra: default }`; `flow_layout.rs`, `flow_picker.rs`, `graph.rs` treat it like `Command` (single-output action). `settings.rs` `kind_fields`: `NodeKind::Math(_) => ().into_any()` (Task 4 replaces it).

- [ ] **Step 5: Run the tests and the workspace build**
  Run: `cargo test -p oikos-core && cargo clippy --workspace --all-targets -- -D warnings`
  Expected: pass, no warnings.

- [ ] **Step 6: Commit** — `feat(core): math flow card model and validation`.

---

### Task 3: The hub runs math cards and the other cards read results

**Files:**
- Modify: `crates/oikos-os/src/flows/engine.rs` (`execute`, `run_node`, `averaged_windows`, new `run_math`)
- Modify: `crates/oikos-os/src/flows/hub.rs` (`check_house`)
- Test: `crates/oikos-os/tests/flows.rs`

**Interfaces:**
- Consumes: Task 1 (`Formula::eval`, `fit_to_entity`), Task 2 (`MathParams`, `math_names`, `command_with_number`, the new params).
- Produces: `execute` keeps `let mut results: BTreeMap<String, f64>` for the run and passes `&mut results` to `run_node(…, results: &mut BTreeMap<String, f64>)`.

Behaviour (spec "Results in the run", "Saving the result", "Run history"):
- Math step: read every input first (`average` via `window_average`, else `hub.entity_scalar`); number → itself, bool → 1/0, text → fail `{name} is not a number`, unknown → fail `{name} is unknown`. `eval` reads inputs, then `results` (a math name with no entry → `no result {name} in this run`), then `trigger` (number or bool from `TriggerInfo.value`, else `trigger has no number`). On success insert into `results`, then if `save_to`: `fit_to_entity` with the entity's `capabilities.{min,max,step}`, `send_command(save_to, Command::SetNumber { value })`; refused or not accepted → fail with the command's reason.
- Detail: `grid=-412, heater=820 → desired = 1132`; with save `… → desired = 47.6 → <save_to> = 48`; ` (kept in range)` appended when clamped. Numbers print with at most 2 decimals, trailing zeros trimmed.
- `command` with `value_from`: the result (missing → fail `no result {name} in this run`) through `command_with_number`, then sent as today.
- `if` / `switch` with `result`: left = `Some(Scalar::Number(v))`; missing → fail as above.
- `notify`: the lookup reads `results` first (`Reading { value: Scalar::Number(v), unit: None, device_class: None }`), else `hub.reading`.
- `averaged_windows`: math inputs with `average` register their entity's window.
- `check_house`: every math input entity and `save_to` exists (`unknown entity `X` in node `Y``); `save_to` is `Kind::Number` else `` node `Y`: `X` is not a number entity ``; a math `name` that is an entity id → `` node `Y`: result name `X` is an entity id ``; the notify unknown-entity check skips placeholders that are in `math_names(flow)`.

- [ ] **Step 1: Write the failing tests** in `tests/flows.rs`. Set up: a virtual number `target` (min 0, max 100, step 1) via `hub.set_virtual`, a second virtual number `meter` (min -5000, max 5000) standing in for the grid reading, and an echo helper that re-ingests every publish on `oikos/virtual/<id>` as the broker would (retained echo) besides the existing LED echo. Tests:
  - `math_saves_rounded_result`: trigger on `virtual_meter`; math `slider = meter / 2` save to `virtual_target`; ingest meter 95 → run Done, `virtual_target` = 48 (47.5 rounded), detail ends `→ slider = 47.5 → virtual_target = 48`.
  - `math_clamps_and_says_so`: meter 500 → target 100, detail contains `(kept in range)`.
  - `math_result_feeds_command_if_and_notify`: math `half = meter / 2` (no save) → `if result half gt 10` → true: command `set_number` `value_from: "half"` on `virtual_target`; assert target = 30 for meter 60; a `notify` body `Half {half:0}` renders `Half 30` (inspect the recorded alert as other notify tests do).
  - `unknown_input_fails_the_step`: input on an entity whose state is unknown → run Failed, last step detail `meter is unknown`, nothing published to `virtual_target`.
  - `text_input_fails_the_step` (Review Focus 1): input on a text sensor fixture (any discovery fixture whose state is text; ingest a text state) → Failed, detail `<name> is not a number`.
  - `refused_save_stops_the_run` (Review Focus 3): `save_to` a virtual number with no echo for it (or a command timeout) → math step Failed, the following command node never runs.
  - `division_by_zero_fails`: `meter / (meter - meter)` → Failed, `division by zero`.
  - `result_missing_on_path_fails`: a command `value_from: "half"` reached on a branch that skipped the `half` card → Failed, `no result half in this run`.
  - `trigger_in_formula`: `trigger * 2` on a state trigger → result twice the reading; on a `trigger.manual` without value → Failed, `trigger has no number`.
  - `set_flow_refuses_bad_math_targets`: `save_to` the LED (not a number) → `InvalidFlow` containing `is not a number entity`; math `name` equal to `MOTION_ID` → `result name ... is an entity id`; unknown input entity → `unknown entity`.
  - `average_input_registers_window`: a math input with `average: "15m"` makes the engine keep that entity's readings (assert the same way the existing `if_average_*` tests do).

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p oikos-os --test flows math`
  Expected: failures (`math cards are not run yet`).

- [ ] **Step 3: Implement the engine and `check_house` changes.**

- [ ] **Step 4: Run the tests**
  Run: `cargo test -p oikos-os --test flows && cargo clippy -p oikos-os --all-targets -- -D warnings`
  Expected: all pass (existing flow tests unchanged).

- [ ] **Step 5: Commit** — `feat(hub): run math flow cards and read their results`.

---

### Task 4: Math card drawer in the web editor

**Files:**
- Create: `crates/oikos-web/src/flow_math.rs` (pure helpers, native-tested), `crates/oikos-web/src/screens/flow_canvas/math.rs` (the drawer view)
- Modify: `crates/oikos-web/src/lib.rs` (module), `crates/oikos-web/src/screens/flow_canvas/mod.rs` (module), `settings.rs` (`NodeKind::Math(p) => math::math_fields(form, id, p).into_any()`), `library.rs` (Math entry next to If / Switch), `crates/oikos-web/src/i18n/flows.rs` (EN/FR strings), the flow CSS file the drawer rows use
- Test: `#[cfg(test)]` in `flow_math.rs`; `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces (in `oikos_web::flow_math`):
  - `pub enum Preview { Value { value: f64, averaged_now: bool }, Unknown(String), Error(String) }`
  - `pub fn preview(params: &MathParams, flow: &Flow, value: &dyn Fn(&str) -> Option<Scalar>) -> Preview` — `Error` = the core validation reason for this card (parse error, unknown name; run `validate` logic via the core helpers, not a copy); `Unknown("desired")` when the formula reads another card's result or an input with no value; `averaged_now` true when any input has `average`.
  - `pub fn input_name_for(label: &str, taken: &[&str]) -> String` — lowercase, accents folded (`é`→`e`), runs of other characters → `_`, trimmed of `_`, cut to 32, prefixed `v_` when it does not start with a letter, suffixed `2`, `3`… when taken or not `is_valid_name`.

Drawer (spec "The web editor", mockup section 2): Inputs rows (name field, entity picker as the other cards use, `now ▾` select with `now, 1m, 5m, 15m, 1h` mapping to `average`), `＋ Input` (disabled at 10; a new row's name from `input_name_for(entity label)`); Formula text field with the error line (red) or the preview line (`= 1132`, plus the "now, not the 1m average" note when `averaged_now`); Result name (validated with `is_valid_name`); Save to (entity picker limited to `Kind::Number`, with a "not saved" choice).

- [ ] **Step 1: Write the failing native tests** in `flow_math.rs`:
  - `preview_computes_with_current_values`: inputs `grid` = 1330, `heater` = 5, formula `clamp(heater - 100 - grid, 0, 1650)` → `Value { value: 0.0, averaged_now: false }`.
  - `preview_reports_errors_and_unknowns`: `gird + 1` → `Error("unknown name `gird`")`; a formula reading another card's `desired` → `Unknown("desired")`; input with no value → `Unknown("grid")`.
  - `preview_marks_averages`: one input with `average: Some("1m")` → `averaged_now: true`.
  - `input_names`: `"Réseau"` → `reseau`; `"Chauffe eau"` → `chauffe_eau`; `"2nd floor"` → `v_2nd_floor`; `"Réseau"` with `reseau` taken → `reseau2`; `"Min"` → `min2`.
- [ ] **Step 2: Run them and see them fail** — `cargo test -p oikos-web flow_math` → compile errors.
- [ ] **Step 3: Implement `flow_math.rs`, the drawer, the library entry and the strings.**
- [ ] **Step 4: Write the wasm tests** in `tests/flow_canvas.rs`: `math_card_from_library_opens_its_drawer` (add Math from the library, the drawer shows Inputs/Formula/Result name/Save to); `math_formula_error_shows_and_blocks_save` (type `gird + 1` → error line, Save refused); `math_preview_uses_house_values` (house fixture with a power sensor, formula `p * 2` → preview `= <2×value>`).
- [ ] **Step 5: Run all web tests**
  Run: `cargo test -p oikos-web flow_math && (cd crates/oikos-web && wasm-pack test --headless --chrome -- --test flow_canvas)`
  Expected: pass.
- [ ] **Step 6: Commit** — `feat(web): math card drawer with live preview`.

---

### Task 5: Picking functions and names (menu, chips, suggestions)

**Files:**
- Modify: `crates/oikos-web/src/flow_math.rs`, `crates/oikos-web/src/screens/flow_canvas/math.rs`, `crates/oikos-web/src/i18n/flows.rs`, flow CSS
- Test: `#[cfg(test)]` in `flow_math.rs`; `crates/oikos-web/tests/flow_canvas.rs`

**Interfaces:**
- Consumes: Task 1 `FUNCTIONS`, Task 4 drawer.
- Produces (in `oikos_web::flow_math`):
  - `pub const SLOT: char = '▢';`
  - `pub fn function_snippet(f: &Function) -> String` — `clamp(▢, ▢, ▢)`, `round(▢)`, `min(▢, ▢)`, `interp(▢, [▢], [▢])`, `if(▢, ▢, ▢)` (`min_args` slots; `min`/`max` get 2).
  - `pub fn insert_at(text: &str, cursor: usize, snippet: &str) -> (String, usize, usize)` — new text and the selection over the first `▢` of the inserted snippet (or a caret after it when it has none). Cursor values are char offsets.
  - `pub struct Suggestion { pub label: String, pub insert: String, pub is_function: bool }`
  - `pub fn suggestions(text: &str, cursor: usize, names: &[&str]) -> Vec<Suggestion>` — the word before the cursor (`[a-z0-9_]`, at least 2 characters) prefix-matched against function names (inserting `function_snippet`) and `names` (inputs, other math results, `trigger`); functions first, then names, each alphabetical; empty under 2 characters.
  - `pub fn accept(text: &str, cursor: usize, s: &Suggestion) -> (String, usize, usize)` — replaces the partial word, selection as `insert_at`.
  - `pub fn function_help(lang: Lang, name: &str) -> &'static str` in `i18n/flows.rs` — the one-line EN/FR description per function (EN: the mockup's table wording).

UI (spec "Picking functions and names", mockup section 3 option C): chips under the formula for each input name and `trigger` (tap → `insert_at`); an `ƒ Functions` chip opening a menu, one line per function (`signature` + `function_help`), tap → `insert_at(function_snippet)` then focus the field with the returned selection; while typing, `suggestions` shown under the field, ↑/↓ move, Tab/Enter/tap accept, Escape closes.

- [ ] **Step 1: Write the failing native tests**: `snippets` (the five above); `insert_selects_first_slot` (`insert_at("a + ", 4, "clamp(▢, ▢, ▢)")` → `("a + clamp(▢, ▢, ▢)", 10, 11)`); `insert_without_slot` (`insert_at("", 0, "grid")` → `("grid", 4, 4)`); `suggest_after_two_letters` (`"heater - cl"`, cursor 11, names `["heater"]` → labels `["clamp"]`; `"he"` → `["heater"]`; `"h"` → `[]`; `"ma"` with names `["max_w"]` → `["max", "max_w"]`); `accept_replaces_word` (`"heater - cl"` + clamp → `("heater - clamp(▢, ▢, ▢)", 15, 16)`); `help_exists_for_every_function` (every `FUNCTIONS` name has non-empty EN and FR help).
- [ ] **Step 2: Run them and see them fail** — `cargo test -p oikos-web flow_math`.
- [ ] **Step 3: Implement the helpers and wire chips, menu and suggestions into the drawer.**
- [ ] **Step 4: wasm tests**: `functions_menu_inserts_with_slots` (open ƒ Functions, tap clamp → field reads `clamp(▢, ▢, ▢)`, error line says `fill in the ▢ slots`); `typing_suggests_and_tab_accepts` (type `cl`, a suggestion list shows clamp, Tab → field reads `clamp(▢, ▢, ▢)`); `input_chip_inserts_name`.
- [ ] **Step 5: Run** `cargo test -p oikos-web flow_math && (cd crates/oikos-web && wasm-pack test --headless --chrome -- --test flow_canvas)` → pass.
- [ ] **Step 6: Commit** — `feat(web): function menu and suggestions in the math card`.

---

### Task 6: Other cards read math results in the editor

**Files:**
- Modify: `crates/oikos-web/src/screens/flow_canvas/settings.rs` (command, if, switch fields), `crates/oikos-web/src/screens/flow_canvas/template_field.rs` (notify ＋ Value), `crates/oikos-web/src/flow_text.rs` (sentences), `crates/oikos-web/src/i18n/flows.rs`
- Test: `crates/oikos-web/tests/flow_canvas.rs`, native tests in `flow_text.rs`

**Interfaces:**
- Consumes: Task 2 (`math_names`, `command_with_number`, `value_from`, `result`).

Behaviour (spec "The web editor" last bullets): command card — when `command_with_number` accepts the command type and the flow has math cards, a **Value from result** select (`—` + each math name) replaces the number field while set (writes `value_from`, keeps the literal). If / Switch — the value source picker lists the flow's math results after the entities; choosing one sets `result` and clears `entity`, `average`, `missing`. Notify ＋ Value — the picker lists math results first; picking inserts `{name}`. Sentences: command `Set <entity> to <name>`; if `If <name> > 300`; switch rules read `<name>`.

- [ ] **Step 1: Write failing tests**: native `flow_text` tests for the three sentences (EN and FR); wasm `command_value_from_result` (flow with a math card `half` and a `set_number` command: choose Value from result `half` → saved JSON has `value_from: "half"`), `if_compares_a_result` (pick `half` in the If value source → JSON has `result`, no `entity`), `notify_inserts_result` (＋ Value → `half` → body gets `{half}`).
- [ ] **Step 2: Run them and see them fail** — `cargo test -p oikos-web flow_text` and the wasm suite.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p oikos-web && (cd crates/oikos-web && wasm-pack test --headless --chrome -- --test flow_canvas)` → pass.
- [ ] **Step 5: Commit** — `feat(web): commands, conditions and pushes read math results`.

---

### Task 7: MCP description and docs

**Files:**
- Modify: `crates/oikos-api/src/flows.rs` (the `set_flow` flow description: add `math`, `value_from`, `result`, `{name}` placeholders, with a short example)
- Modify: `docs/reference/flow-nodes.md` (new `## math` section after `switch`: params table, grammar, functions table, run-time errors, results in the run, saving; `value_from` row on `command`, `result` row on `if` and `switch`, results in `notify` placeholders)
- Modify: `docs/users/flows.md` (a "Compute a value" how-to: the solar-surplus water heater as the example with generic ids `sensor_grid_power`, `sensor_heater_power`, `virtual_heater_target`, `group_heater_power`)
- Modify: `docs/reference/mcp-tools.md` if it lists node types
- Test: `crates/oikos-os/tests/mcp.rs`

- [ ] **Step 1: Write a failing MCP test** `set_flow_accepts_a_math_card` in `tests/mcp.rs` following the existing `set_flow` MCP tests: a flow with a math card saving to a virtual number is accepted and `get_flow` returns it with `type: "math"`.
- [ ] **Step 2: Run** `cargo test -p oikos-os --test mcp math` — expected to pass already if Tasks 2–3 are in (then it is a regression guard); if it fails, fix the cause.
- [ ] **Step 3: Write the docs and the description.** The water-heater example is the spec appendix with generic ids; no user names, rooms or hosts.
- [ ] **Step 4: Full verification**
  Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check && (cd crates/oikos-web && wasm-pack test --headless --chrome)`
  Expected: all green.
- [ ] **Step 5: Commit** — `docs: math flow card`.

---

## After the plan (not tasks)

- PR against `main`; after merge, rebuild the live hub and web.
- Then build the user's water-heater flow (spec appendix) with MCP: create `group_water_heater_power` (number group over `water_heater_power_0/1/2`) and `virtual_heater_target` (0..2200 W, step 1); ask the user which temperature maps to Jeedom 4811 (force mode) and 4812 (72 °C cut-off).
- `feat/flow-value-of` touches the same `if` / `switch` params and pickers: whichever lands second rebases.
