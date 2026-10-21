# Flexible groups (Homey-style) — hub + web implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Oikos's derived per-room groups with hand-made Homey-style groups, built as retained MQTT entities so the hub, the web and later the Android app all see the same group as one entity.

**Architecture:** A group is a hub-published entity, same pattern as `crates/oikos-os/src/virtual_entities.rs`. The definition (`name`, `kind`, `members`, `hideMembers`) is retained at `oikos/group/<id>`; from it the hub publishes a retained HA discovery config on the first discovery prefix, subscribes to the command topic, fans commands out to online members, recomputes and republishes the combined state on every member echo, and keeps `oikos/ui/group_<id>` as the ordinary override topic. The web consumes `group_<id>` like any entity: Home shows a card, there is a `/group/:id` sheet, Devices has a `＋ New group` wizard, and member cards are hidden on Home when the group's `hideMembers` says so.

**Tech Stack:** Rust, Axum (REST), rmcp (MCP), Leptos (wasm web), rumqttc + home-assistant discovery.

**Spec:** `docs/superpowers/specs/2026-10-21-flexible-groups-design.md`

## Global Constraints

- Reserve the `oikos/group/` sub-tree alongside `ui`, `automation`, `virtual`; it is never classified as discovery and never appears in `GET /api/v1/ignored`.
- Group ids are `[a-z0-9_]{1,48}` (same validator as virtual entities); the entity is `group_<id>`.
- One kind per group; kinds in scope are the controllable kinds listed in spec §4 (`light`, `switch`, `cover`, `climate`, `number`, `select`, `button`, `vacuum`, `water_heater`). `sensor`, `binary_sensor` and `alarm_control_panel` are refused with `422`.
- A group's effective member kind includes `showAs` (a switch shown as a light counts as a light).
- Members are device entities only, never `group_*`; the same entity can belong to several groups.
- Groups never dissolve by themselves; a group with no present or matching member stays, greyed, with `availability: offline`.
- Hub config switch `[groups] enabled` (default `false`) gates publishing group entities and running the §9 migration. REST and MCP endpoints are reachable only when the switch is on; otherwise `503 service unavailable`.
- The hub listens on 8081; `target/` fills the disk, so clean before big builds.
- Follow the virtual-entities file structure for retained discovery, reserved topics and id validation.
- Commit as Gekkotron: `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`.
- The pre-existing failing wasm test `tests/sheet.rs::state_echo_keeps_the_override_draft` is not this work's.

## Review Focus

Five input classes the spec implies but which no single task's main assertions prove on its own. Each line also appears as an explicit step in the task that owns the code.

- **Retained-sync race on migration (§9).** The hub starts; `oikos/group/_migrated` has not been seen yet; the broker is still delivering retained messages. If migration fires before entities arrive, it sees an empty house, publishes nothing, and sets the marker — leaving the house ungrouped forever. Expected: migration must defer until the sync is settled, and must not set the marker when it did nothing because there was no house. Covered in Task 9.
- **Group command when every member is offline (§5).** A command on `group_<id>`'s command topic with no reachable member must reply `accepted: false, reason: "no reachable member"` and must not change the group's published state. Covered in Task 6.
- **Member kind drifts after a `showAs` edit (§5).** A member of a light group whose `showAs` is cleared no longer matches. The definition is not touched; fan-out and the controls skip it; the sheet lists it with "Mismatch". The `showAs` write itself is not blocked. Covered in Task 7 and Task 12.
- **Hide-members with cross-group membership (§8).** A lamp belongs to two groups, group A has `hideMembers: true`, group B has `hideMembers: false`. The lamp is hidden on Home room views (yes wins). A lamp a user favourited on its own still shows under Favorites regardless. Covered in Task 14.
- **Delete group names its flows (§7).** Deleting a group used in a flow confirms with the flow names listed in the dialog; the user must confirm again before the DELETE is sent. Covered in Task 12.

---

## Task 1: Freeze the MQTT contract in the design spec

**Files:**
- Modify: `docs/superpowers/specs/2026-10-21-flexible-groups-design.md` (append Appendix A)
- Modify (out-of-tree, if the file is reachable): the Android app spec section titled "§6.1 Groups contract" under `<app-repo>/` so it points at Appendix A of this design doc. If the exact path is unknown, note it in the appendix as a follow-up for the Android app plan.

**Interfaces:**
- Consumes: nothing.
- Produces: a frozen contract later tasks and the Android plan both cite verbatim.

- [ ] **Step 1: Draft Appendix A — MQTT contract**

Append a section `## Appendix A — MQTT contract (frozen 2026-10-21)` to the spec covering, in this order:

1. **Topic layout.** For discovery prefix `P` and ui prefix `U` (defaults `oikos` and `oikos/ui`):
   - Definition: `oikos/group/<id>` (retained JSON). Reserved sub-tree, same rank as `ui`, `automation`, `virtual`.
   - Migration marker: `oikos/group/_migrated` (retained, empty payload is `absent`, any non-empty payload is `present`).
   - Discovery config: `P/<component>/group/<id>/config` where `<component>` is the HA component for the group's kind.
   - State + command topic: `P/group/<id>` (hub subscribes for commands, publishes state retained).
   - Availability: `P/group/<id>/avail` (retained `online`/`offline`).
   - UI override: `U/group_<id>` (same shape as any entity override).
2. **Definition JSON schema** (strict):

   ```json
   { "name": "Living lights",
     "kind": "light",
     "members": ["0x00124b00_1_switch", "lamp_floor"],
     "hideMembers": true }
   ```

   Required: `name` (non-empty, trimmed ≤ 64 chars), `kind` (one of the controllable kinds from spec §4), `members` (array of entity ids, 0 or more). Optional: `hideMembers` (bool, default `true`). Unknown fields rejected.
3. **Discovery config per kind.** For each kind, the exact HA keys the hub publishes: `name`, `unique_id = group_<id>`, `device = { "identifiers": ["oikos_group"], "name": "Groups" }`, `command_topic`, `state_topic`, `availability_topic`, `retain: true`, plus the per-kind keys named in spec §4 (brightness, position, HVAC modes, min/max/step, options, etc.). The intersection rule from §4 is restated here as the rule the hub follows when the config is republished.
4. **Combined state rules per kind.** The exact state payload published on `P/group/<id>` for each kind, word-for-word from spec §4, including the format for cover text (`"Opening…"`, `"Closing…"`, `"All open"`, `"All closed"`, `"N open · M closed"`) and the `"Mixed"` wording for climate HVAC mode.
5. **Fan-out contract.** Command on `P/group/<id>` is JSON (same shape as `send_command`); the hub translates it per present-and-matching member and reports `accepted` only after every targeted member has echoed, otherwise `accepted: false` naming the members that did not within 2 seconds.
6. **Lifecycle.** Replacing a definition keeps the state topic (initial state is published only when a group is new). Deleting a definition publishes empty retained to the config topic, the state topic and the availability topic.

- [ ] **Step 2: Commit the appendix**

```bash
git add docs/superpowers/specs/2026-10-21-flexible-groups-design.md
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -m "docs(superpowers): freeze MQTT contract for flexible groups"
```

- [ ] **Step 3: Update the app spec pointer (out-of-tree)**

If the Android app spec is reachable at the path above, edit its §6.1 "Groups contract" paragraph to read "See Appendix A of `docs/superpowers/specs/2026-10-21-flexible-groups-design.md` in the hub repo." Otherwise add a note at the bottom of Appendix A: `TODO (app plan): point app spec §6.1 at this appendix once the app worktree is created.`

---

## Task 2: `GroupSpec` type in `oikos-api`

**Files:**
- Create: `crates/oikos-api/src/groups.rs`
- Modify: `crates/oikos-api/src/lib.rs` (declare and re-export)
- Test: `crates/oikos-api/src/groups.rs` (unit `#[cfg(test)]`)

**Interfaces:**
- Consumes: Appendix A §2.
- Produces:
  - `pub struct GroupSpec { pub kind: oikos_core::model::Kind, pub name: String, pub members: Vec<String>, #[serde(default = "default_hide")] pub hide_members: bool }` with `Serialize`, `Deserialize`, `JsonSchema`, `Clone`, `Debug`, `PartialEq`, `#[serde(deny_unknown_fields)]`, `#[serde(rename_all = "camelCase")]` on the field names where needed to match the JSON in Appendix A.
  - `pub fn default_hide() -> bool { true }`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn group_spec_round_trip_matches_appendix_a() {
    let json = r#"{"name":"Living lights","kind":"light","members":["a","b"],"hideMembers":true}"#;
    let spec: GroupSpec = serde_json::from_str(json).unwrap();
    assert_eq!(spec.name, "Living lights");
    assert_eq!(spec.kind, oikos_core::model::Kind::Light);
    assert_eq!(spec.members, vec!["a", "b"]);
    assert!(spec.hide_members);
    assert_eq!(serde_json::to_value(&spec).unwrap(), serde_json::from_str::<serde_json::Value>(json).unwrap());
}

#[test]
fn hide_members_defaults_to_true() {
    let json = r#"{"name":"X","kind":"switch","members":[]}"#;
    assert!(serde_json::from_str::<GroupSpec>(json).unwrap().hide_members);
}

#[test]
fn unknown_fields_are_refused() {
    let json = r#"{"name":"X","kind":"switch","members":[],"color":"red"}"#;
    assert!(serde_json::from_str::<GroupSpec>(json).is_err());
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p oikos-api groups`
Expected: FAIL with `GroupSpec` not found.

- [ ] **Step 3: Implement `GroupSpec` in `crates/oikos-api/src/groups.rs` with the signature in Interfaces**

The implementer picks the serde attribute shape; the tests pin the behaviour.

- [ ] **Step 4: Declare the module in `lib.rs` and re-export `GroupSpec`**

- [ ] **Step 5: Run `cargo test -p oikos-api` and `cargo clippy -p oikos-api -- -D warnings` — both pass.**

- [ ] **Step 6: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(api): GroupSpec body for hand-made groups"
```

---

## Task 3: `groups` module in `oikos-os` — pure helpers

**Files:**
- Create: `crates/oikos-os/src/groups/mod.rs`
- Create: `crates/oikos-os/src/groups/topics.rs`
- Create: `crates/oikos-os/src/groups/discovery.rs`
- Create: `crates/oikos-os/src/groups/state.rs`
- Modify: `crates/oikos-os/src/lib.rs` (declare `pub mod groups`)

**Interfaces:**
- Consumes: `oikos_api::GroupSpec`, `oikos_core::model::Kind`, Appendix A.
- Produces:
  - `topics::validate_id(id: &str) -> Result<(), String>` — same rules as `virtual_entities::validate_id`.
  - `topics::unique_id(id: &str) -> String` → `group_<id>`.
  - `topics::definition_topic(id: &str) -> String` → `oikos/group/<id>` (prefix hard-coded, since §2 reserved it).
  - `topics::migration_marker_topic() -> &'static str` → `oikos/group/_migrated`.
  - `topics::state_topic(prefix: &str, id: &str) -> String` → `<prefix>/group/<id>`.
  - `topics::availability_topic(prefix: &str, id: &str) -> String` → `<prefix>/group/<id>/avail`.
  - `topics::config_topic(prefix: &str, kind: Kind, id: &str) -> String` → `<prefix>/<component>/group/<id>/config`; `component_for(kind)` is a private helper matching HA components for the kinds in spec §4.
  - `topics::is_config_topic(prefix: &str, id: &str, topic: &str) -> bool` — true if `topic` is `config_topic` for *any* in-scope kind.
  - `discovery::build_config(prefix: &str, id: &str, spec: &GroupSpec, members: &[EntityView]) -> serde_json::Value` — intersection rules from spec §4.
  - `state::GroupState` (enum, one variant per kind) and `state::combine(kind: Kind, members: &[EntityView]) -> GroupState`.
  - `state::encode(state: &GroupState) -> String` — exact payload per Appendix A §4.

- [ ] **Step 1: Write failing tests**

Add tests covering:
- `validate_id` matches `virtual_entities::validate_id` behaviour (reuse its rejected and accepted ids).
- `topics::config_topic("oikos", Kind::Climate, "water_tank")` returns `"oikos/climate/group/water_tank/config"`.
- `topics::state_topic("oikos", "lights")` is `"oikos/group/lights"` and is distinct from the config topic.
- `topics::is_config_topic` is true for every in-scope kind's config topic and false for a `virtual/` or `ui/` topic.
- `discovery::build_config` for a `light` group with two members, one dimmable, returns `brightness: true` only when every member has brightness (intersection); the one `color` key only when every member supports colour; `unique_id` is `group_<id>`; `device.identifiers == ["oikos_group"]`.
- `state::combine` on a `switch` group with [`ON`, `OFF`] returns a value whose `encode` is `"ON"` (the "any member on" rule from §4).
- `state::combine` on a `cover` group where any member is `opening` returns a state whose `encode` is `"Opening…"`.
- `state::combine` on a `climate` group whose members have `heat`/`heat` returns common mode `heat`; mixed modes return `None` encoding `"Mixed"`.

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test -p oikos-os groups::`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement the module. Reuse `virtual_entities::validate_id` by extracting it into a shared `topics::validate_id` function or by calling it; the implementer picks.**

The algorithm the signature does not already determine:
- `component_for(kind)` maps `Kind::Light → "light"`, `Kind::Switch → "switch"`, `Kind::Cover → "cover"`, `Kind::Climate → "climate"`, `Kind::WaterHeater → "water_heater"`, `Kind::Number → "number"`, `Kind::Select → "select"`, `Kind::Button → "button"`, `Kind::Vacuum → "vacuum"`. Any other kind returns `Err` through the caller — this helper is private but its result shape must be a `Result` so callers can refuse non-controllable kinds.

- [ ] **Step 4: Run the tests and watch them pass**

Run: `cargo test -p oikos-os groups::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): group id validation, topic helpers, discovery and state"
```

---

## Task 4: `[groups] enabled` config switch

**Files:**
- Modify: `crates/oikos-os/src/config.rs` (add `GroupsConfig`, hang it off `Config`)
- Test: `crates/oikos-os/src/config.rs` (unit)

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)] pub struct GroupsConfig { pub enabled: bool }` with `Default` giving `enabled: false`.
  - `Config` grows a `pub groups: GroupsConfig` field.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn groups_default_is_disabled() {
    assert!(!Config::default().groups.enabled);
}

#[test]
fn groups_enabled_true_from_toml() {
    let cfg: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::string("[groups]\nenabled = true\n"))
        .extract()
        .unwrap();
    assert!(cfg.groups.enabled);
}
```

- [ ] **Step 2: Run the tests and verify they fail**

- [ ] **Step 3: Add the struct and field as in Interfaces**

- [ ] **Step 4: Run the tests and verify they pass; also `cargo clippy -p oikos-os -- -D warnings` is clean**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): [groups] enabled config switch, default off"
```

---

## Task 5: `GroupStore` — retained definitions + lifecycle

**Files:**
- Create: `crates/oikos-os/src/groups/store.rs`
- Modify: `crates/oikos-os/src/groups/mod.rs` (`pub mod store`)
- Modify: `crates/oikos-os/src/hub.rs` (field `group_store: Mutex<GroupStore>`, `Hub::new` builds it, `Hub::ingest` routes `oikos/group/<id>` and `oikos/group/_migrated` into it)

**Interfaces:**
- Consumes: Task 2's `GroupSpec`, Task 3's helpers.
- Produces:
  - `pub struct GroupStore { defs: BTreeMap<String, GroupSpec>, migrated: bool }`.
  - `impl GroupStore { pub fn apply(&mut self, id: &str, payload: &[u8]) -> ApplyOutcome; pub fn get(&self, id: &str) -> Option<&GroupSpec>; pub fn all(&self) -> &BTreeMap<String, GroupSpec>; pub fn migrated(&self) -> bool; pub fn mark_migrated(&mut self); }`.
  - `pub enum ApplyOutcome { Created, Replaced, Deleted, Marker, Unchanged, Rejected(String) }`.
  - `apply` parses payload as `GroupSpec`; empty payload deletes the def; `_migrated` topic sets `migrated` to true on any non-empty payload. Unknown fields or an unknown kind return `Rejected`.

- [ ] **Step 1: Write failing unit tests on `GroupStore` only (no hub)**

Tests to add:
- `apply("living", r#"{...}"#.as_bytes())` → `Created`, `get("living").is_some()`.
- Re-applying the same JSON → `Unchanged`.
- Different JSON for the same id → `Replaced`.
- `apply("living", b"")` → `Deleted`, `get("living").is_none()`.
- `apply("_migrated", b"1")` → `Marker`, `migrated()` is true.
- `apply("x", garbage)` → `Rejected(..)`, state unchanged.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p oikos-os groups::store`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement `GroupStore`**

- [ ] **Step 4: Hook ingest in `Hub::ingest`**

Where the hub already recognises `ui_prefix`, `automation_prefix` and the virtual sub-tree, add a branch for topics under `oikos/group/`. The branch calls `group_store().apply(id, payload)` and, when the outcome is `Created` or `Replaced`, enqueues a "republish discovery for this id" job (see Task 7). An outcome of `Rejected` logs at WARN and does nothing.

- [ ] **Step 5: Add an integration test in `crates/oikos-os/tests/hub.rs`**

```rust
#[tokio::test]
async fn retained_group_definition_is_loaded() {
    let (hub, broker) = test_hub().await;
    broker.publish_retained("oikos/group/living", br#"{"name":"Living lights","kind":"light","members":[]}"#).await;
    broker.sync(&hub).await;
    assert!(hub.read_group_store().get("living").is_some());
}
```

(Signature of `test_hub` / `read_group_store` is whatever the test infrastructure already offers; the plan commits the behaviour, not the harness.)

- [ ] **Step 6: Run `cargo test -p oikos-os` and watch the test pass**

- [ ] **Step 7: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): GroupStore keeps retained group definitions and the migration marker"
```

---

## Task 6: `Hub::set_group`, `Hub::delete_group` and `list_groups`

**Files:**
- Modify: `crates/oikos-os/src/hub.rs` (new methods, following `set_virtual` / `delete_virtual` shape line-for-line)
- Modify: `crates/oikos-os/src/http/api.rs` (routes `GET /api/v1/groups`, `PUT /api/v1/groups/{id}`, `DELETE /api/v1/groups/{id}`)
- Modify: `crates/oikos-api/src/views.rs` (new `GroupView { id, name, kind, members: Vec<GroupMemberView>, hide_members, entity: EntityView, used_by_flows: Vec<String> }` and `GroupMemberView { id, name, present: bool, mismatch: bool }`)
- Test: `crates/oikos-os/tests/hub.rs` and `crates/oikos-os/tests/api.rs`

**Interfaces:**
- Consumes: `GroupSpec`, Task 3 helpers, `GroupStore`.
- Produces:
  - `pub async fn set_group(&self, id: &str, spec: GroupSpec) -> Result<GroupView, HubError>` — rejects when `[groups] enabled` is false (`HubError::Publish("groups disabled")` → mapped to 503), when `validate_id` fails (`Rejected` → 422), when `spec.kind` is out of spec §4 scope (`Rejected`), when any member is a `group_*` (`Rejected`), when a member's effective kind does not match the group's kind AND the member is not already in a stored definition under this id (`Rejected`).
  - `pub async fn delete_group(&self, id: &str) -> Result<(), HubError>` — publishes empty retained on definition, config, state and availability topics.
  - `pub fn list_groups(&self) -> Vec<GroupView>` — enriched with `used_by_flows` (the names of flows whose nodes reference `group_<id>`).

- [ ] **Step 1: Write failing tests**

Add tests to `tests/hub.rs`:
- `set_group` with `[groups] enabled = false` returns `HubError::Publish` and does not publish.
- `set_group` with an unknown kind (`Kind::Sensor`) returns `Rejected` and does not publish.
- `set_group` with a `group_*` member returns `Rejected`.
- `set_group` on a fresh id publishes the definition retained on `oikos/group/<id>`, the discovery config retained on the right component topic, and `online` to the availability topic; `set_virtual` already exercises `publish` so no new broker double is needed.
- Re-`set_group` with a changed kind clears the old discovery topic (empty retained) before publishing the new one.
- `delete_group` publishes empty on definition, config, state, availability.
- `list_groups` returns a `GroupView` whose `used_by_flows` names a flow whose `trigger.state.entity` is `group_<id>`.

Add tests to `tests/api.rs`:
- `GET /api/v1/groups` returns 200 with `[]` when the switch is on and no group exists.
- `GET /api/v1/groups` returns 503 when the switch is off.
- `PUT /api/v1/groups/living` with a valid body returns 200.
- `PUT /api/v1/groups/BAD-id` returns 422.

- [ ] **Step 2: Run the tests and watch them fail**

- [ ] **Step 3: Implement `set_group`, `delete_group`, `list_groups` in `hub.rs` and the four routes in `http/api.rs`**

The approach mirrors `set_virtual`:
1. Validate id and kind.
2. Build the discovery config from the members currently present (Task 3).
3. Publish retained: config topic, availability topic (`online`), definition topic, and when the group is new, publish initial state (Task 7 formats it).
4. If the id already existed and the component changed, publish empty on the old config topic.
5. Return `GroupView`.

Delete publishes empty on the four topics in a fixed order (definition, config, state, availability).

- [ ] **Step 4: Run the tests and verify they pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): set/delete/list groups and the three /api/v1/groups routes"
```

---

## Task 7: Fan-out, state recomputation and member drift

**Files:**
- Create: `crates/oikos-os/src/groups/fanout.rs`
- Modify: `crates/oikos-os/src/groups/mod.rs` (`pub mod fanout`)
- Modify: `crates/oikos-os/src/hub.rs` (`on_group_command`, `on_member_state_change`, called from the existing ingest and command paths)

**Interfaces:**
- Consumes: `GroupStore`, `state::combine`, `discovery::build_config`.
- Produces:
  - `fanout::targets(spec: &GroupSpec, members: &[EntityView]) -> Vec<String>` — the ids of members that are present AND whose effective kind matches `spec.kind` AND are online.
  - `fanout::translate(spec: &GroupSpec, cmd: &Command, member: &EntityView) -> Option<Command>` — the per-member command, honouring the capability intersection (brightness only on dimmable, position only on position-capable, HVAC mode only on modes shared, etc.). `None` when this member cannot take `cmd`.
  - `Hub::on_group_command(&self, id: &str, cmd: Command) -> CommandResponse` — publishes to each target, waits up to 2 s for echoes on `state_seen`, and reports `accepted: true` only when every targeted member echoed within the window; otherwise `accepted: false` with `reason: "<id1>, <id2>: no echo"`.
  - `Hub::on_member_state_change(&self, member_id: &str)` — for every group whose definition lists `member_id`, recompute state and publish retained on the group's state topic (only when the encoded state changed); republish availability (`online` when any targeted member is online, else `offline`).

- [ ] **Step 1: Write failing tests**

Pure unit tests on `fanout`:
- `targets` with one offline member and one online returns only the online one.
- `targets` on a light group where member A is `showAs: light` on a switch returns A.
- `targets` on a light group whose member had its `showAs` cleared drops that member.
- `translate(light_group, TurnOn{brightness: Some(80)}, member_without_brightness)` → `None`.
- `translate(cover_group, SetPosition{position: 40}, member_without_position)` → `None`.
- `translate(climate_group, SetHvacMode(Heat), member_without_heat_mode)` → `None`.

Hub integration tests in `tests/hub.rs`:
- A group command with every member offline returns `CommandResponse { accepted: false, reason: Some(..) }` naming the members, and the group's state topic is untouched. **(Review Focus item 2.)**
- A member state echo recomputes and publishes the new combined state on `oikos/group/<id>` retained.
- A `showAs` change on a member that makes its kind drift leaves the member in the definition but out of `targets`; a `GET /api/v1/groups` reports `mismatch: true` for that member. **(Review Focus item 3.)**

- [ ] **Step 2: Run the tests and verify they fail**

- [ ] **Step 3: Implement `fanout`, wire `on_group_command` and `on_member_state_change`**

Hook `on_group_command` from the existing MQTT command path (where the hub notices a message on a known command topic). Hook `on_member_state_change` from `Hub::ingest` after it dispatches state echoes.

For `accepted` tracking: subscribe to `state_seen` for the targeted ids, start a `tokio::time::timeout(Duration::from_millis(2000), ..)`, drop subscribers as echoes arrive.

- [ ] **Step 4: Run the tests and verify they pass; `cargo clippy -p oikos-os -- -D warnings` clean**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): group command fan-out, state recomputation and member drift"
```

---

## Task 8: MCP tools `list_groups`, `set_group`, `delete_group`

**Files:**
- Modify: `crates/oikos-os/src/mcp/tools.rs` (new tool methods)
- Modify: `crates/oikos-os/src/mcp/params.rs` (`ListGroupsParams`, `SetGroupParams`, `DeleteGroupParams`)
- Modify: `crates/oikos-os/src/mcp/mod.rs` (one-sentence addition to the server instructions)
- Test: `crates/oikos-os/tests/mcp.rs`

**Interfaces:**
- Consumes: `Hub::list_groups`, `Hub::set_group`, `Hub::delete_group`.
- Produces: three MCP tools shaped like `set_virtual` / `delete_virtual`, each annotated `idempotent_hint = true`.

- [ ] **Step 1: Write failing tests**

In `tests/mcp.rs`:
- `set_group` through the MCP server round-trips to `Hub::set_group` and returns the `GroupView`.
- `list_groups` returns an empty list before any `set_group`.
- `delete_group` for a non-existent id returns an error with `code = "unknown"`.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Add the three tools and params; add one sentence to the server instructions block (`crates/oikos-os/src/mcp/mod.rs`) explaining that groups are hub-published entities fan-out happens on**

- [ ] **Step 4: Run and watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): MCP list/set/delete group tools"
```

---

## Task 9: One-shot migration from derived groups

**Files:**
- Create: `crates/oikos-os/src/groups/migrate.rs`
- Modify: `crates/oikos-os/src/groups/mod.rs` (`pub mod migrate`)
- Modify: `crates/oikos-os/src/hub.rs` (`Hub::run_group_migration_once` and the task scheduler that calls it)
- Test: `crates/oikos-os/tests/hub.rs`

**Interfaces:**
- Consumes: `GroupStore::migrated`, derived-group rule from `oikos_core::model::GroupKey::candidate`, `Hub::set_group`.
- Produces:
  - `migrate::plan(entities: &BTreeMap<String, EntityView>, areas: &[AreaView]) -> Vec<(String, GroupSpec)>` — the groups to create from today's derived rule (kinds `light`, `cover`, ≥2 members, not `ungrouped`, not Unassigned), each with name from the area's `groupNames[kind]` or the kind's default name, members in Home order, `hideMembers: true`.
  - `Hub::run_group_migration_once(&self) -> MigrationOutcome` — returns `MigrationOutcome { created: Vec<String>, skipped_reason: Option<&'static str> }`. The method:
    1. Returns `skipped_reason: Some("groups disabled")` when `[groups] enabled` is false.
    2. Returns `skipped_reason: Some("already migrated")` when the store's `migrated` flag is set.
    3. Returns `skipped_reason: Some("house empty, retained sync not settled")` when the registry holds zero entities. **(Review Focus item 1.)**
    4. Otherwise, calls `plan`, calls `set_group` for each entry, then publishes a retained non-empty payload to `oikos/group/_migrated` and returns the created ids.

- [ ] **Step 1: Write failing tests**

In `tests/hub.rs`:
- `run_group_migration_once` on an empty registry returns `skipped_reason: Some("house empty, retained sync not settled")` and does NOT publish the marker. **(Review Focus item 1.)**
- A house with two lights and two covers in the "living" area produces exactly two groups (one light, one cover) and sets the marker.
- Re-running after the marker is set returns `skipped_reason: Some("already migrated")` and creates nothing.
- With `[groups] enabled = false`, returns `skipped_reason: Some("groups disabled")`.
- A migrated group has `hideMembers: true` and its name is the area's `groupNames["light"]` if present.

Unit tests on `migrate::plan` (pure):
- An `ungrouped: true` member is skipped.
- Fewer than two members in a (kind, area) → no group for that pair.
- Unassigned area → no group.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement `migrate::plan` and `Hub::run_group_migration_once`**

Scheduling: `main.rs` already starts the hub and the MQTT task. Add a small `tokio::spawn` after boot that:
- waits for the broker to reach `Online`,
- then waits an idle period on `Hub::state_seen` (e.g. 1500 ms with no new seen entity) — this is the "retained sync settled" heuristic the spec left to the builder,
- then calls `run_group_migration_once`.

- [ ] **Step 4: Run and watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(os): one-shot migration from derived groups behind [groups] enabled"
```

---

## Task 10: Web API client for groups

**Files:**
- Modify: `crates/oikos-web/src/api/mod.rs` (or `crates/oikos-web/src/api.rs` — follow the file layout already there)
- Test: colocate `#[cfg(test)]` where other API client tests live, otherwise add one in a new `tests/api_groups.rs`

**Interfaces:**
- Consumes: `oikos_api::GroupSpec`, `GroupView`.
- Produces:
  - `pub async fn list_groups() -> Result<Vec<GroupView>, ApiError>`
  - `pub async fn set_group(id: &str, spec: &GroupSpec) -> Result<GroupView, ApiError>`
  - `pub async fn delete_group(id: &str) -> Result<(), ApiError>`

- [ ] **Step 1: Write a failing test or wasm integration test (follow the file's own style)** verifying `set_group` sends `PUT /api/v1/groups/{id}` with the JSON body.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement the three functions**

- [ ] **Step 4: Run and watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): groups API client"
```

---

## Task 11: Devices screen — "Groups" section + "＋ New group" wizard

**Files:**
- Create: `crates/oikos-web/src/screens/group_wizard.rs`
- Modify: `crates/oikos-web/src/screens/devices.rs` (new "Groups" section at the top, "＋ New group" button opening the wizard route)
- Modify: `crates/oikos-web/src/app.rs` / wherever routes live (`/groups/new`, `/groups/:id/edit`)

**Interfaces:**
- Consumes: Task 10's `list_groups`/`set_group`.
- Produces:
  - Route `/groups/new` — three steps (type, members, name + room + hide-members toggle), classes `card`, `pick`, `sheet__panel`, `btn` (no ad-hoc spacing).
  - Route `/groups/:id/edit` — same three steps, pre-filled, type field disabled.
  - Submitting Step 3 calls `set_group(id, &spec)` with a slugged id and navigates to `/group/<id>`.

- [ ] **Step 1: Write a wasm UI test for Step 1 of the wizard** that checks every kind in spec §4 appears and that no `sensor`/`binary_sensor`/`alarm_control_panel` tile appears.

- [ ] **Step 2: Write a test for Step 2** that filters members to the chosen kind (`showAs` honoured) and groups them by room.

- [ ] **Step 3: Write a test for Step 3** that includes a "Hide members on Home" toggle defaulting to on.

- [ ] **Step 4: Watch them fail, then implement**

Id generation: slug the name on save (`[a-z0-9_]`, `-` → `_`, `.` → `_`, non-ASCII fold via `fold_accents`), truncate to 48 chars, suffix with `_2`, `_3` on collision.

- [ ] **Step 5: Watch pass, `cargo clippy -p oikos-web --all-targets -- -D warnings` clean**

- [ ] **Step 6: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): + New group wizard and Devices Groups section"
```

---

## Task 12: `/group/:id` sheet

**Files:**
- Create: `crates/oikos-web/src/screens/group_sheet_v2.rs` (new file so Task 16 can delete the old one cleanly)
- Modify: `crates/oikos-web/src/app.rs` or router (new route `/group/:id`)
- Modify: `crates/oikos-web/src/nav.rs` (back target writes `group_<id>`'s room)

**Interfaces:**
- Consumes: `House` entities (where `group_<id>` now lives), Task 10's `delete_group`, Task 11's edit route.
- Produces:
  - A sheet with header `name, room · N <kind>`, Controls panel (reusing the entity's kind-specific controls), Members panel (name, "room · state", own toggle, offline greyed, tap opens the member sheet), Settings panel (Show in Favorites, Hide members on Home, Edit group → `/groups/:id/edit`, Delete group).
  - Delete confirms in-page (no browser dialog) and lists the flows that use the group (from `GroupView.used_by_flows`). **(Review Focus item 5.)**
  - Members marked `mismatch: true` in `GroupView` render with a "Mismatch" chip and are not clickable from Controls. **(Review Focus item 3 continuation of Task 7.)**

- [ ] **Step 1: Write failing tests**

- Snapshot test for the header "{name}, {room} · {N} {kind}s".
- Clicking Delete opens a confirmation naming the flow `kitchen-lights-at-dusk` when that flow references the group.
- A `mismatch` member shows "Mismatch" and no toggle.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement the sheet. The Controls panel reuses the single-entity controls for the group's kind, driven by the `group_<id>` entity's own state**

- [ ] **Step 4: Run and watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): /group/:id sheet with mismatch, flow-aware delete and settings"
```

---

## Task 13: Home card bodies for the new kinds

**Files:**
- Modify: `crates/oikos-web/src/cards/group.rs` (new wide-card variants or wrappers that render each in-scope kind using its single-entity card body + `×N` badge)
- Modify: `crates/oikos-web/src/screens/home.rs` (route `group_<id>` entities through the group card picker)

**Interfaces:**
- Consumes: `group_<id>` entity, its combined state.
- Produces: for each in-scope kind, a card that reuses the existing single-entity card body (`PowerToggle` for switch, cover row for cover, `stepper` for climate/water_heater, `pick` for select, number input for number, a button row for button, the existing light group card for light, cover group card for cover). All use `card`, `card--wide` and `group__count` for the `×N` badge; no new classes.
  - Climate/water heater: current label shows the member range (`"19–21 °C"`) when members disagree, the shared mode name when they agree, else `"Mixed"`.
  - Button group: body shows a single `Press all` button.

- [ ] **Step 1: Write tests on card text**

- Switch group with `"OFF"` state renders "All off".
- Switch group with `"ON"` state renders "All on".
- Climate group with targets `[19, 21]` renders `"19–21 °C"`.
- Button group renders a `Press all` button whose click sends `Command::Press` to the group entity.

- [ ] **Step 2: Watch fail**

- [ ] **Step 3: Implement the card wrappers**

- [ ] **Step 4: Watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): Home card bodies for every in-scope group kind"
```

---

## Task 14: Hide members on Home

**Files:**
- Modify: `crates/oikos-web/src/groups.rs` (new `home_items` that uses hand-made groups) or new `crates/oikos-web/src/home_layout.rs`
- Modify: `crates/oikos-web/src/screens/home.rs` to call the new layout

**Interfaces:**
- Consumes: `House` entities (with `group_<id>` entries and definitions), `AreaView`.
- Produces:
  - `pub fn home_items(entities: ..., areas: ..., chip: &str) -> Vec<HomeItem>` where `HomeItem::Entity(id)` is a single card and `HomeItem::Group(group_id)` is a group card. The rule:
    - For each `group_<id>` entity, remember its `members` and `hideMembers` from the definition.
    - On a room chip, show every `group_<id>` whose override says this room; skip single-member cards hidden by ANY group whose `hideMembers = true` includes them.
    - On the Favorites chip, show favourited groups whole, and still include single members the user marked `favorite: true` even when a `hideMembers = true` group hides them in rooms. **(Review Focus item 4.)**

- [ ] **Step 1: Write failing tests**

Three cases cover Review Focus item 4:
- Lamp in two groups, A `hideMembers = true`, B `hideMembers = false` → lamp hidden on its room chip; both A and B shown.
- Lamp hidden by its group but `favorite = true` → shown on Favorites.
- `hideMembers: false` on every group → every member keeps its own card.

- [ ] **Step 2: Watch fail**

- [ ] **Step 3: Implement the new layout**

- [ ] **Step 4: Watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): hideMembers rule for hand-made groups on Home"
```

---

## Task 15: Entity sheet "In groups" line, flow picker and energy/security

**Files:**
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs` (read-only list of groups the entity belongs to, each linking to the group sheet)
- Modify: `crates/oikos-web/src/flow_picker.rs` (mark group entities with `×N` or a "Group" tag)
- Modify: `crates/oikos-web/src/screens/energy.rs`, `crates/oikos-web/src/screens/security.rs` (filter out `group_*` entities)

**Interfaces:** none new; this task reads existing views.

- [ ] **Step 1: Write failing tests**

- Entity sheet renders "In groups: Living lights, Hallway lights" when the entity belongs to both.
- Flow picker lists `group_living_lights` with a `×3` badge, and clicking it inserts the group's id in the node.
- Energy screen's entity list has zero rows whose id starts with `group_`.

- [ ] **Step 2: Watch fail**

- [ ] **Step 3: Implement**

- [ ] **Step 4: Watch pass**

- [ ] **Step 5: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "feat(web): In groups line, flow picker group tag, energy/security filter"
```

---

## Task 16: Remove derived groups

**Files:**
- Delete: `crates/oikos-core/src/model/room_groups.rs`
- Delete: `crates/oikos-web/src/screens/group_sheet.rs` (the old one)
- Modify: `crates/oikos-web/src/groups.rs` (drop derived helpers, keep the hand-made layout)
- Modify: `crates/oikos-web/src/cards/group.rs` (drop `LightGroupCard` / `CoverGroupCard` or route them through the new path if they still read right)
- Modify: `crates/oikos-core/src/model/mod.rs` (remove `pub mod room_groups` and the re-exports `GroupKey`, `GroupKind`, `MIN_GROUP_MEMBERS`)
- Modify: `crates/oikos-os/src/http/api.rs` (remove `PATCH /api/v1/areas/{id}/groups/{kind}` and `set_area_group` handler)
- Modify: `crates/oikos-os/src/hub.rs` (remove `set_area_group`; `AreaView.group_names` and `group_favorites` stay as read-only fields for the broker's existing retained payloads but are no longer read)
- Modify: `crates/oikos-web/src/screens/group_sheet.rs`'s `Customize` panel is gone — this is the `Ungroup all` button and the per-group rename / favourite that the old sheet carried
- Update any wasm test asserting on the old `/group/:area/:kind` route

**Interfaces:** none new.

**Prerequisite:** the hub has run `[groups] enabled = true` once, migration has set the marker, and `GET /api/v1/groups` lists the migrated groups. The executor confirms this before cutting the derived code.

- [ ] **Step 1: Confirm migration ran** against the live broker by `GET /api/v1/groups | jq length` and record the count in the commit message.

- [ ] **Step 2: Delete and modify files as listed**

- [ ] **Step 3: Run `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`; fix any compile fallout (Home layout, old tests)**

- [ ] **Step 4: Commit**

```bash
git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit -am "chore(core,web,os): remove derived room groups and /areas/:id/groups/:kind"
```

---

## Self-review (performed 2026-10-21)

- **Spec coverage:** §1-2 goals → every goal has a task (hand-made build: 11; one entity shared across clients: 2, 3, 5, 6, 7; all kinds grouped: 3, 13; migration: 9; app parity: 1 freezes the contract). §3 state and availability rules → 3, 6, 7. §4 kinds table → 3, 13. §5 architecture → 5–7. §6 REST/MCP → 6, 8, and 16 removes the stale PATCH. §7 web → 11, 12, 13, 15. §8 hide members → 14. §9 migration → 9. §10 release gate → 4, 9, 16. §11 constraints are in the Global Constraints section. §12 "left to the builder" items (topic layout, Mixed wording, confirmation tracking, wizard styling) are decided in Tasks 1, 3, 7, 11.
- **Step scan:** every step is one checkable action; no "add appropriate validation" lines; algorithmic code blocks appear only for `component_for` where the signature doesn't pin it.
- **Type consistency:** `GroupSpec` is the same body everywhere (api → hub → MCP → web). `GroupView.used_by_flows` is referenced by Task 6 and Task 12 under the same name. `MigrationOutcome.skipped_reason` is `Option<&'static str>` in Task 9 and the three strings are quoted verbatim in the tests.
- **Review Focus:** each of the five items has a test step in the owning task (1 → Task 9; 2 → Task 7; 3 → Tasks 7 and 12; 4 → Task 14; 5 → Task 12).
- **Proportion:** this plan is roughly 1.5× the spec. Code blocks are limited to the ones the task implementers cannot derive from signatures and tests alone.
