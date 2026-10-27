# Flow Switch and Time of day Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two flow node types, `switch` (rules with one exit each, plus `otherwise`) and `time.between` (inside / outside an HH:MM range), end to end: core model and validation, hub engine and run view, web editor, docs.

**Architecture:** Both nodes are new `NodeKind` variants in `oikos-core`. Their pure logic (rule matching, clock ranges) lives in core, so the engine and the web share it. `NodeKind::ports()` becomes param-dependent (`Vec<String>`). The engine follows every port a step returns, and `StepView` records those ports, so the web lights wires from data instead of parsing `detail`.

**Tech Stack:** Rust 2024 workspace; `oikos-core` (serde, schemars), `oikos-os` (tokio engine, chrono-tz), `oikos-api`, `oikos-web` (Leptos, wasm).

**Spec:** `docs/superpowers/specs/2026-10-27-flow-switch-and-time-of-day-design.md`

## Global Constraints

- The `if` node, its params, ports (`true`/`false`) and behaviour stay byte-for-byte compatible. Every existing `if` test keeps passing unchanged, except the `ports()` return-type adjustment in Task 1.
- Wire names: node types `switch` and `time.between`. Switch ports `"1"`..`"<n>"` (1-based rule position) and `"otherwise"`. Time ports `"in"` and `"out"`.
- Switch `rules`: 1 to 10 entries. Ops `eq`, `ne`, `gt`, `lt`, `gte`, `lte`, `between` (`value <= x <= to`, both included).
- Time: `from` included, `to` excluded, `HH:MM` 24-hour, `from != to`, house zone (`Hub::zone()`), wall-clock comparison.
- Unknown keys on nodes, params and rules are kept (`#[serde(flatten)] extra`), as on every node today.
- Generic project: no house names, rooms or hosts in tracked files, tests included.
- Web labels go through `crates/oikos-web/src/i18n/flows.rs` in EN and FR. Card library names: "Switch" / "Commuter", "Time of day" / "Plage horaire".
- Commits as `Gekkotron` (`-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`), ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage explicit paths only: another session may share the worktree.
- Before any build, check `df -h ~` and `du -sh ~/dev-scratch/cargo-target`. Over 5 GB, `cargo clean`. Under 5 GB free, run per-crate tests only.
- Branch: `feat/flow-switch-time-of-day` (already holds the spec, branched from `origin/main` after PR #76).

## Review Focus

1. **Deleting rule 2 of 3 in the editor** must leave rule 1's wires on `"1"`, move rule 3's wires to `"2"`, and drop rule 2's wires. It must never leave an edge on a port that no longer exists, which would make the hub refuse the save. Pinned in Task 5 (`delete_rule_renumbers_later_exits`).
2. **A switch after a `trigger.schedule` with no `entity`** must be refused on save, as `if` is. Otherwise it fails at run time with "no triggering entity". Pinned in Task 2 (`switch_after_schedule_needs_entity`).
3. **`time.between` at exactly `to` (22:30 for 07:00–22:30) is `out`, and at 00:00 inside 22:00–06:00 is `in`.** People notice off-by-one minutes. Pinned in Task 1 (`time_range_bounds`).
4. **A run's highlighted wires for an `if` with `average`** (detail `average 30m = 18.6 → true`) must light the `true` wire. This is broken today, and Task 4's `ports` data fixes it. Pinned in Task 4 (`highlight_reads_step_ports`).
5. **A switch on a text entity with a `gt` rule** must fail the step with the non-numeric error, not silently take `otherwise`. Pinned in Task 1 (`switch_exits_ordering_on_text_fails`).

---

### Task 1: Core model and pure logic for `switch` and `time.between`

**Files:**
- Modify: `crates/oikos-core/src/flow/model.rs` (new params, `NodeKind` variants, `type_name`, `ports`)
- Create: `crates/oikos-core/src/flow/switch.rs` (rule matching)
- Create: `crates/oikos-core/src/flow/clock.rs` (HH:MM parse and range)
- Modify: `crates/oikos-core/src/flow/mod.rs` (modules and re-exports)
- Modify: `crates/oikos-core/src/flow/validate.rs:238,293`, `crates/oikos-web/src/flow_edit.rs:54,135` (compile against `Vec<String>` ports only, no behaviour change)

**Interfaces:**
- Produces:
  - `pub enum RuleOp { Eq, Ne, Gt, Lt, Gte, Lte, Between }` (serde `snake_case`, schemars), with `pub fn wire_name(self) -> &'static str`.
  - `pub struct SwitchRule { pub op: RuleOp, pub value: Value, #[serde(default, skip_serializing_if = "Option::is_none")] pub to: Option<Value>, #[serde(flatten)] pub extra: BTreeMap<String, Value> }`
  - `pub struct SwitchParams { entity: Option<String>, average: Option<String>, rules: Vec<SwitchRule>, #[serde(default, skip_serializing_if = "is_false")] otherwise: bool, #[serde(default, skip_serializing_if = "is_false")] all: bool, extra }` (all `pub`).
  - `pub struct TimeBetweenParams { pub from: String, pub to: String, extra }`
  - `NodeKind::Switch(SwitchParams)` as `"switch"`, `NodeKind::TimeBetween(TimeBetweenParams)` as `"time.between"`. Every new type derives `Clone, Debug, PartialEq, Serialize, Deserialize` and `cfg_attr(feature = "schemars", derive(schemars::JsonSchema))` like its neighbours, so the MCP flow schema picks them up.
  - `NodeKind::ports(&self) -> Vec<String>`: `if` → `["true","false"]`, switch → `"1"`..`"n"` then `"otherwise"` when set, time → `["in","out"]`, others `[]`.
  - `pub const MAX_SWITCH_RULES: usize = 10;`
  - `pub fn switch_exits(params: &SwitchParams, left: Option<&Scalar>) -> Result<Vec<String>, CompareError>` in `switch.rs`. It returns the fired ports in rule order: the first match only unless `all`, and `["otherwise"]` when nothing matched and `otherwise` is set. With no match and no `otherwise` it returns `[]`.
  - `pub fn parse_clock(s: &str) -> Result<u16, String>` (minute of the day, 0..1440) and `pub fn in_time_range(from: u16, to: u16, now: u16) -> bool` in `clock.rs`.

- [ ] **Step 1: Write the failing tests**

In `model.rs` tests:
```rust
#[test]
fn switch_round_trips_and_lists_ports() {
    let node: Node = serde_json::from_value(json!({ "id": "s", "type": "switch", "params": {
        "entity": "t", "rules": [{ "op": "lt", "value": 18 }, { "op": "between", "value": 18, "to": 22, "x": 1 }],
        "otherwise": true } })).unwrap();
    assert_eq!(node.kind.ports(), vec!["1", "2", "otherwise"]);
    let back = serde_json::to_value(&node).unwrap();
    assert_eq!(back["params"]["rules"][1]["x"], 1);
    assert!(back["params"].get("all").is_none());
}
#[test]
fn time_between_ports() {
    let node: Node = serde_json::from_value(json!({ "id": "t", "type": "time.between",
        "params": { "from": "07:00", "to": "22:30" } })).unwrap();
    assert_eq!(node.kind.ports(), vec!["in", "out"]);
    assert_eq!(node.kind.type_name(), "time.between");
}
```
Update `ports_of_if_are_true_false_and_others_have_none` to compare with `vec!["true", "false"]`.

In `switch.rs`:
```rust
// rules [lt 18, between 18..22, gt 22], otherwise: true
fn switch_exits_first_match()        // 17.0 → ["1"]; 18.0 → ["2"]; 22.0 → ["2"]; 22.5 → ["3"]
fn switch_exits_all_matches()        // rules [gte 10, lte 20], all: true; 15.0 → ["1","2"]
fn switch_exits_unknown_value()      // None → ["otherwise"]; rules [ne "on"] with None → ["1"]; otherwise false and no match → []
fn switch_exits_ordering_on_text_fails() // Text("on") with [gt 1] → Err(CompareError::NotNumeric)
```
In `clock.rs`:
```rust
fn parse_clock_forms()  // "07:00"→420, "23:59"→1439; Err for "7:00", "24:00", "07:60", "0700", ""
fn time_range_bounds()  // (420,1350): 420→true, 1349→true, 1350→false, 419→false
                        // (1320,360) crossing midnight: 0→true, 1320→true, 359→true, 360→false, 720→false
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-core flow::`
Expected: compile errors (unknown `Switch`, `switch_exits`, `parse_clock`).

- [ ] **Step 3: Implement**

`RuleOp::Between` compares as `value <= x && x <= to` on numbers. The others map onto `Op` and go through the existing `compare`. An unknown `left` with `between` is `false`, and a known non-number is `NotNumeric`. `parse_clock` accepts exactly `HH:MM` (two digits each). Fix the four `ports()` call sites mechanically (`.iter().any(|p| p == port)`, `.first().cloned()`). Export everything from `flow/mod.rs`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-core flow:: && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS, no errors. (Match arms on `NodeKind` elsewhere may need `_ =>` or new arms to compile. Add the minimal arm and leave the real behaviour to the owning task.)

- [ ] **Step 5: Commit** `feat(core): switch and time.between node model`

---

### Task 2: Core validation

**Files:**
- Modify: `crates/oikos-core/src/flow/validate.rs`

**Interfaces:**
- Consumes: Task 1 types, `parse_clock`, `MAX_SWITCH_RULES`, `parse_duration`, `MAX_AVERAGE_SECS`.
- Produces:
  - `FlowError::MissingPort { node: String, kind: &'static str }` with message ``"node `{node}` is {kind} and needs a port"``, where `kind` carries its article: `"an if"`, `"a switch"`, `"a time.between"`. The `if` message is unchanged.
  - `FlowError::SwitchNeedsEntity { node }`: ``"node `{node}`: a switch after a schedule or sun trigger must name an entity"``.
  - Param errors as `FlowError::BadOperand { node, reason }` with exactly these reasons (`n` is 1-based):
    - `"rules must hold 1 to 10 entries"`
    - `"rule {n}: value must be a boolean, a number or a string"`
    - `"rule {n}: to is required for op between"`
    - `"rule {n}: to is only allowed with op between"`
    - `"rule {n}: between needs numbers"`
    - `"rule {n}: to must be at least value"`
    - `"rule {n}: value must be a number with average"`
    - `"average: {e}"`, `"average must be at most 24h"` (same as `if`)
    - `"from: `{s}` is not a HH:MM time"`, `"to: `{s}` is not a HH:MM time"`, `"from and to must differ"`

- [ ] **Step 1: Write the failing tests**

```rust
fn switch_flow(params: Value, edges: Value) -> Flow   // trigger.state `m` → switch `s` → commands `a`,`b`
#[test] fn valid_switch_and_time_flows_pass()
#[test] fn switch_rule_errors()          // one assert per reason above, checking BadOperand.reason exactly
#[test] fn switch_ports_checked()        // edge port "3" with 2 rules → UnknownPort{port:"3"}; "otherwise" with otherwise:false → UnknownPort; no port → MissingPort{kind:"a switch"}
#[test] fn switch_after_schedule_needs_entity()  // trigger.schedule → switch without entity → SwitchNeedsEntity; with entity → Ok
#[test] fn time_between_errors()         // "7:00", "24:00", equal from/to, edge without port → MissingPort{kind:"a time.between"}
#[test] fn time_between_after_schedule_is_fine()
```
The existing test `if_edge_without_port_is_missing_port` must still pass unchanged (its message assert stays `"node `moving` is an if and needs a port"`).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-core validate`
Expected: FAIL / compile errors.

- [ ] **Step 3: Implement**

Add the param checks next to the `if` arms. Make the edge-port check (around line 238) use `ports()` for every node with ports, with `kind` from `type_name()`. In the reachability walk (around line 288), add the switch entity rule. `time.between` needs none.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-core`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(core): validate switch and time.between`

---

### Task 3: Hub engine, run view and save checks

**Files:**
- Modify: `crates/oikos-api/src/flows.rs` (`StepView`)
- Modify: `crates/oikos-os/src/flows/engine.rs` (`run_node`, `execute`, `step`)
- Modify: `crates/oikos-os/src/flows/hub.rs:357-361`, `crates/oikos-os/src/hub.rs:2031-2036` (switch entity counts like `if`'s)

**Interfaces:**
- Consumes: `switch_exits`, `parse_clock`, `in_time_range`, `Hub::zone()`, existing `window_average`, `Hub::entity_scalar`.
- Produces:
  - `StepView.ports: Option<Vec<String>>`, `#[serde(default, skip_serializing_if = "Option::is_none")]`. It is `Some` for `if`, `switch` and `time.between` steps that succeeded (the ports followed, possibly empty), and `None` for every other node.
  - `run_node(...) -> (StepOutcome, String, Vec<String>)`. `if` arms return `vec!["true".into()]` etc., and every other node returns `vec![]`.
  - Details: switch `"{value} → {ports joined by ','}"`, average form `"average {w} = {mean:.1} → {ports}"`, unknown `"unknown → {ports}"`, nothing fired `"{value} → none"`. Time: `"{HH:MM} → in"` / `"→ out"`.

- [ ] **Step 1: Write the failing tests** (in `engine.rs` tests, reusing `engine_at`)

```rust
#[tokio::test] async fn switch_takes_first_matching_exit()   // entity at 20.0, rules [lt 18, between 18..22, gt 22] → ports ["2"], detail ends with "→ 2"
#[tokio::test] async fn switch_unknown_value_takes_otherwise()  // unknown entity, otherwise:true → ["otherwise"], StepOutcome::Ok
#[tokio::test] async fn switch_all_runs_exits_in_rule_order()  // execute(): rules [gte 10, lte 30], all:true, value 20; exit 1 → command a, exit 2 → command b; recorded step order: trigger, s, a, b
#[tokio::test] async fn time_between_uses_house_zone()  // engine_at(.., "Europe/Paris"), now = 2026-10-27T21:40Z (22:40 Paris): 07:00–22:30 → ["out"], 22:00–06:00 → ["in"]
#[tokio::test] async fn time_between_wall_clock_on_dst_day()  // 2026-10-25T06:30Z is 07:30 Paris after the switch back: 07:00–08:00 → ["in"]
#[tokio::test] async fn step_view_records_ports_for_branching_nodes_only()
```
Existing `if` tests change only `Option<&str>` → `Vec<String>` in their asserts.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-os flows::engine`
Expected: compile errors.

- [ ] **Step 3: Implement**

In `execute`, build the next nodes as the successors of each returned port in order (no port: `successors(None)`), then `stack.extend(next.into_iter().rev())`. This runs exit 1's whole subtree before exit 2's. Read the switch value as `if` does, reading it once. `time.between` reads `now().with_timezone(&hub.zone())` and takes `hour*60+minute`. Add `NodeKind::Switch(p) => p.entity.as_deref()` (or `as_ref()`) next to the `if` arms in both hub files.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-os flows && cargo test -p oikos-api`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(hub): run switch and time.between, record ports taken`

---

### Task 4: Web run view reads ports

**Files:**
- Modify: `crates/oikos-web/src/flows.rs:185-245` (`parked_node`, `highlight`)

**Interfaces:**
- Consumes: `StepView.ports`.
- Produces: unchanged signatures. A node counts as "branching" when `!node.kind.ports().is_empty()`. Its taken ports are `step.ports` when `Some`, else `[step.detail]` for an `if` (old hub fallback).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn highlight_reads_step_ports()      // if step with detail "average 30m = 18.6 → true", ports Some(["true"]) → true edge lit, false edge not
#[test] fn highlight_switch_all_exits()      // switch step ports ["1","3"] → edges on 1 and 3 lit, 2 and otherwise not
#[test] fn parked_on_delay_behind_switch_exit()  // last step switch ports ["2"], exit 2 → delay → parked_node is that delay
#[test] fn highlight_if_without_ports_falls_back_to_detail()  // existing behaviour kept
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-web flows::`
Expected: FAIL.

- [ ] **Step 3: Implement.** For `parked_node`, look at the successors of each taken port, in order.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-web flows::`
Expected: PASS.

- [ ] **Step 5: Commit** `fix(web): light branch wires from the ports a step took`

---

### Task 5: Web editor wire rules for switch exits

**Files:**
- Modify: `crates/oikos-web/src/flow_edit.rs`

**Interfaces:**
- Consumes: `NodeKind::ports()`.
- Produces (all mutate `flow` in place, returning how many edges were removed so the UI can say so):
  - `pub fn add_rule(flow: &mut Flow, node: &str, rule: SwitchRule)`: appends. `otherwise` edges are untouched.
  - `pub fn move_rule(flow: &mut Flow, node: &str, from: usize, to: usize)`: 0-based indices. Edges follow their rule.
  - `pub fn delete_rule(flow: &mut Flow, node: &str, index: usize) -> usize`
  - `pub fn set_otherwise(flow: &mut Flow, node: &str, on: bool) -> usize`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn delete_rule_renumbers_later_exits()   // 3 rules, edges on "1","2","3","otherwise" → delete index 1 → edges on "1","2"(was 3),"otherwise"; returns 1
#[test] fn move_rule_carries_its_wires()         // move 0 → 2: old "1" edge now "3", old "2" now "1", old "3" now "2"
#[test] fn set_otherwise_off_drops_its_wires()   // returns count, rule edges untouched
#[test] fn can_connect_accepts_switch_and_time_ports()  // "2" ok with 2 rules, "3" refused; "in"/"out" ok on time; None refused on both
#[test] fn splice_uses_first_port()              // dropping a switch on a wire keeps the wire on "1", a time card on "in"
```
After each mutation, `oikos_core::flow::validate` on the test flow must still be `Ok`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-web flow_edit`
Expected: compile errors.

- [ ] **Step 3: Implement.** Renumber by parsing the port as a 1-based index. Leave non-numeric ports alone.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-web flow_edit`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(web): keep switch wires on their rule`

---

### Task 6: Web canvas, card text and library

**Files:**
- Modify: `crates/oikos-web/src/flow_layout.rs` (card height, `out_anchor`)
- Modify: `crates/oikos-web/src/screens/flow_canvas/graph.rs:445-485` (pins from `ports()`, exit labels, sun badge unchanged)
- Modify: `crates/oikos-web/src/flow_text.rs` (role `Check`, titles, exit labels)
- Modify: `crates/oikos-web/src/i18n/flows.rs` (new `FlowText` entries, EN/FR)
- Modify: `crates/oikos-web/src/flow_picker.rs` (two `Group::And` library entries, plus a per-entity switch entry next to the per-entity `if`)
- Modify: `crates/oikos-web/src/icons.rs:308` (switch reuses `NODE_FORK`, time uses the existing clock/schedule icon)
- Modify: `crates/oikos-web/src/screens/flow_canvas/settings.rs` (`node_editor_body` title arms only)
- Modify: the flow canvas stylesheet that styles `.flow-node__out` (exit labels)

**Interfaces:**
- Consumes: Task 5 `can_connect` and splice behaviour, `if_value.rs` value words for exit labels.
- Produces:
  - `pub fn node_height(kind: &NodeKind) -> f64` in `flow_layout.rs`: `NODE_H` for every node except a switch, which gets `NODE_H.max(14.0 + 20.0 * exits as f64)`. Every place that uses `NODE_H` as a placed card's height (overlap, bounds, free-slot search, routing, input anchor) uses `node_height` instead.
  - `out_anchor`: an `if` keeps 0.3 / 0.7. A switch spreads its exits evenly at `(i + 0.5) / exits` of its height. A time card uses 0.3 for `in` and 0.7 for `out`.
  - `pub fn exit_label(lang: Lang, kind: &NodeKind, port: &str, ctx: &TextCtx) -> String` in `flow_text.rs`. Switch rules read in short form, with the entity's unit when known: `"< 18 °C"`, `"18–22 °C"`, `"= on"`/`"= marche"` through the #76 value words, and `"otherwise"`/`"sinon"`. Time reads `"07:00–22:30"` and `"outside"`/`"en dehors"`. `if` keeps its current pins with no labels.
  - Card sentences: switch `"Switch on {entity}"` / `"Commuter selon {entity}"`, using the trigger-value word when no entity. Time `"Between 07:00 and 22:30"` / `"Entre 07:00 et 22:30"`.
  - Library: "Switch" / "Commuter" (sub "Several exits, one per rule" / "Plusieurs sorties, une par règle"), default params `rules: [{op: eq, value: true}]`, `otherwise: true`. "Time of day" / "Plage horaire" (sub "Inside or outside two times" / "Dans ou hors d'une plage horaire"), default `07:00`–`22:00`.

- [ ] **Step 1: Write the failing tests**

```rust
// flow_layout.rs
#[test] fn switch_card_grows_with_exits()   // 1 rule → 52.0 (NODE_H); 1 rule + otherwise → 54.0; 5 rules + otherwise → 134.0; anchors strictly increasing, all inside the card
#[test] fn switch_wires_end_on_their_pins() // extend the existing "wire ends at pin" test with a 4-exit switch
// flow_text.rs
#[test] fn switch_and_time_sentences_en_fr()
#[test] fn exit_labels()                     // between with unit, otherwise, time in/out in both languages
// flow_picker.rs
#[test] fn library_lists_switch_and_time()
```
Card height is `max(52, 14 + 20 × exits)`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p oikos-web flow_layout flow_text flow_picker`
Expected: FAIL.

- [ ] **Step 3: Implement the layout, text, library and pins** so the tests pass. Pins in `graph.rs` iterate `k.ports()` (an empty list gives a single unnamed pin) and show `exit_label` beside each named switch/time pin.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p oikos-web`
Expected: PASS.

- [ ] **Step 5: Commit** `feat(web): switch and time of day cards on the canvas`

---

### Task 7: Web parameter editor for switch and time

**Files:**
- Modify: `crates/oikos-web/src/screens/flow_canvas/settings.rs` (switch and time editors, `node_editor_body` titles)
- Modify: `crates/oikos-web/src/i18n/flows.rs` (editor labels)

**Interfaces:**
- Consumes: Task 5 `add_rule` / `move_rule` / `delete_rule` / `set_otherwise`, Task 6 `FlowText` entries, the If editor's entity, average and value fields.

- [ ] **Step 1: Implement the editor** in `settings.rs`

The switch editor has the entity field and the average field as on If, then one row per rule. Each row has an op select (EN/FR op labels like `op_label`, plus "between" / "entre"), a value input from the If value picker for the chosen entity, a second number input shown only for `between`, ↑ / ↓ buttons (`move_rule`) and a delete button (`delete_rule`). Below the rows: "Add rule" (`add_rule`, disabled at 10), an "Otherwise exit" toggle (`set_otherwise`) and a "Fire every matching rule" toggle (`all`). When a call returns a count > 0, show the note "Removed {n} wire(s)" / "{n} fil(s) supprimé(s)" under the list until the next edit.

The time editor has two `<input type="time">` fields writing `HH:MM`.

- [ ] **Step 2: Verify in the browser**

Run: `cargo test -p oikos-web`, then build and serve the web against a local or dev hub, as the project's run skill does. On the Flows editor, check the following, and screenshot it in Night and Ivoire, EN and FR:
- Add a Switch from the library, give it 3 rules and *otherwise*, and wire each exit.
- Delete rule 2 and confirm the wires renumber.
- Save, and confirm the hub accepts it.
- Add a Time of day card and save.
Expected: the save succeeds, exit labels are readable, and wires end on their pins.

- [ ] **Step 3: Commit** `feat(web): switch and time of day parameter editor`

---

### Task 8: Docs

**Files:**
- Modify: `docs/reference/flow-nodes.md` (new `## switch` and `## time.between` sections after `## if`; the edge rule paragraph; the `if`-after-schedule note covers switch; the run section mentions steps record `ports`)
- Modify: `docs/users/flows.md` (short "Branch on several values" and "Only at certain hours" parts, one JSON or editor example each)
- Modify: `docs/reference/http-api.md` if it documents `StepView` fields (add `ports`)

**Interfaces:**
- Consumes: the exact param tables, ports, detail formats and error reasons from Tasks 1–3.

- [ ] **Step 1: Write the sections**, using the same table shape as `if`. The examples use generic entity ids (`sensor_temperature`).
- [ ] **Step 2: Check them against the code.** Every param, default, port name and error text in the docs must match Tasks 1–3. Run `cd website && npm run build` if the site builds locally. Expected: no broken links.
- [ ] **Step 3: Commit** `docs(flows): switch and time of day nodes`
