# Web Room Groups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Home in the web UI folds the covers and the lights of each room into one group card, as the Oikos app does, and edits the same retained fields, so a group renamed, favourited or ungrouped on one client shows the same on the other.

**Architecture:** Groups are not stored anywhere: every client derives them from what is already retained. The membership rule and the group keys go in `oikos-core` (shared by the web and, later, MCP). The hub exposes the two area fields the web lacks (`groupNames`, `groupFavorites`) on `AreaView` and gains one endpoint to edit them. The web folds its Home list into group cards and adds a group sheet. Nothing new is written to MQTT: `ungrouped` stays on `oikos/ui/<id>`, names and favourites stay on `oikos/ui/_areas`.

**Tech Stack:** Rust, axum (hub), Leptos + Trunk (web, wasm), serde.

**Spec:** the app's `~/Documents/SourcesPerso/Oikos/docs/superpowers/specs/2026-10-09-oikos-room-groups-design.md`, as amended by the app's current code (`feature/home/.../presentation/{RoomGroups,GroupMath,GroupCommands,HomeUiBuilder,OverrideEditor}.kt`). Where the two disagree, the code wins (see the constraints).

## Where the groups live (the contract)

| Piece | Where | Written by |
|---|---|---|
| Membership | derived: `(area, kind)` with kind cover or light, area not `Unassigned`, shown on Home, `ungrouped == false`, at least 2 members | nobody |
| Group id | `group:<areaId>:<kindKey>`, kind key `cover` or `light` | nobody |
| Leave a group | `ungrouped: true` in `oikos/ui/<entityId>` | app, web (existing override PATCH) |
| Custom name | `groupNames: {"<kindKey>": "<name>"}` on the area in `oikos/ui/_areas` | app, web (new endpoint) |
| Group on Favorites | `groupFavorites: ["<kindKey>"]` on the area in `oikos/ui/_areas` | app, web (new endpoint) |

The broker check on 2026-10-19 found `ungrouped` on 3 overrides and no `groupNames` or `groupFavorites` yet. The hub already decodes, keeps and re-encodes both area fields (`oikos-core/src/model/area.rs`). The only gap is that `AreaView` drops them.

## Global Constraints

- Base: `main` at `319891e` or later (the sheet Back / readout-card wave is in). Pull before starting: another session works on `oikos-web` too.
- `PUT /api/v1/areas` stays a raw replace (the app's own publish carries every field). The MCP `set_areas` tool must not erase what its caller could not see: see Task 2.
- Group kinds: `Kind::Cover` and `Kind::Light` only. `MIN_MEMBERS = 2`. Never group in `UNASSIGNED_AREA`.
- A member is an entity for which `shows_on_home(e)` is true, `e.area` is not Unassigned and `!e.ui.ungrouped`. `e.kind` is the effective kind (an `asLight` switch counts as a light).
- Group cards show under a room chip and under Favorites. Under Favorites a group shows when its kind key is in the area's `groupFavorites`, and then it takes all its members (even non-favourite ones). A member that is `favorite` itself but whose group is not favourited shows as a single card under Favorites (app `favoriteGrouped`).
- Order: sort entities as today (`ui.order`, then lowercase name), fold each group into its first member's slot, then stable-sort by (`ui.order` with groups as `i32::MAX`, area index in the hub's list, `!is_group`), so within a room the groups come before the unpinned single cards (app `HomeUiBuilder` lines 84–95).
- Default names: "Shutters" for cover, "Lights" for light. A custom name is trimmed; blank removes the entry.
- Cover group text: "Opening…" / "Closing…" while any member moves (opening wins); else "All open" / "All closed" when uniform; else "N open · M closed". A member is open by `format::is_on`. Member position for the mini bars: `position` clamped to 0..=100, else 100 when open, 0 when closed.
- Light group text: "All on", "All off", else "N of M on".
- Group availability: any member online. Pending: any member pending.
- Sends: to every available member, all at once (one `fire_command` per member). `SetPosition` only to members with `capabilities.position`; `TurnOn { brightness: Some(_) }` only to members with `capabilities.brightness`. The light toggle is absolute: any member on → `TurnOff`, else `TurnOn` with no fields.
- Slider starting values: "All to" = rounded mean of known positions of position-capable members, else 0; brightness = rounded mean brightness of the dimmable members that are on, else 255.
- Hub endpoint: `PATCH /api/v1/areas/{id}/groups/{kind}`, body `{"name"?: string | null, "favorite"?: bool}`. `200` with the area list; `400` for a kind other than `cover`/`light`, for `Unassigned` or for an empty body; `404` for an unknown area; `503` while the broker link is down (existing `publish` rule).
- UI copy is English, like the rest of the web UI. Attribution, where any is needed: Gekkotron.

## Review Focus

0. An assistant calls MCP `list_areas`, edits a room's name or order and sends the list back through `set_areas`: the custom group names, group favourites, icons and unknown app fields must survive. Test: `mcp_set_areas_keeps_group_fields` (Task 2).
1. A room with exactly 2 lights, one of them `ungrouped` → no group: the other light is a single card (the threshold counts members after `ungrouped`). Test: `ungrouped_member_dissolves_a_pair` (Task 3).
2. Renaming a room from the Home title after a group was renamed keeps `groupNames` (rename goes through the registry's document, not `AreaView`). Test: `rename_keeps_group_fields` (Task 2).
3. A group whose members are all offline still shows, greyed, and a tap sends nothing. Test: `targets_skip_unavailable` + `targets_empty_when_all_offline` (Task 3).
4. An entity id containing `:` or `/` in a group member row link still opens its sheet (`encode_id`), and a group route with an area id containing `é` (`bedroom3`) resolves. Test: `group_route_round_trips_accented_area` (Task 3).
5. Favourite group in a room the Favorites chip would otherwise not show: the Favorites chip appears when only a group is favourited (count > 0). Test: `favorites_chip_counts_favourited_groups` (Task 4).

---

### Task 1: Group keys and membership (`oikos-core`)

**Files:**
- Create: `crates/oikos-core/src/model/room_groups.rs`
- Modify: `crates/oikos-core/src/model/mod.rs` (add `mod room_groups;` and `pub use room_groups::{GroupKind, GroupKey, MIN_GROUP_MEMBERS};`)
- Modify: `crates/oikos-core/src/model/area.rs` (add `AreasDocument::set_group`)
- Test: `#[cfg(test)]` in `room_groups.rs`; `crates/oikos-core/tests/areas.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const MIN_GROUP_MEMBERS: usize = 2;
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum GroupKind { Cover, Light }
  impl GroupKind {
      pub fn of(kind: Kind) -> Option<GroupKind>;
      pub fn key(self) -> &'static str;              // "cover" | "light"
      pub fn from_key(key: &str) -> Option<GroupKind>;
      pub fn default_name(self) -> &'static str;     // "Shutters" | "Lights"
  }
  #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub struct GroupKey { pub area: String, pub kind: GroupKind }
  impl GroupKey {
      /// The group this entity would join, before the 2-member threshold; None for other kinds, Unassigned or `ui.ungrouped`.
      pub fn candidate(area: &str, kind: Kind, ui: &UiOverride) -> Option<GroupKey>;
      pub fn id(&self) -> String;                    // "group:<area>:<key>"
      pub fn parse(id: &str) -> Option<GroupKey>;    // inverse of id(); splits on the last ':'
  }
  impl AreasDocument {
      /// `name`: None = leave, Some(None) or blank = remove, Some(Some(n)) = set trimmed. `favorite`: None = leave.
      /// Declares an implicit area like `rename`. Returns false (and changes nothing) for Unassigned.
      pub fn set_group(&mut self, area: &str, kind: GroupKind, name: Option<Option<&str>>, favorite: Option<bool>) -> bool;
  }
  ```

- [ ] **Step 1: Write the failing tests**
  - `kinds_and_keys`: `GroupKind::of(Kind::Cover) == Some(Cover)`, `of(Kind::Switch) == None`, `from_key("light") == Some(Light)`, `from_key("switch") == None`, `Cover.default_name() == "Shutters"`.
  - `candidate_rules`: cover in `"living"` → `Some`; same with `ui.ungrouped = true` → `None`; in `UNASSIGNED_AREA` → `None`; `Kind::Sensor` → `None`.
  - `id_round_trip`: `GroupKey{area:"bedroom3",kind:Light}.id() == "group:bedroom3:light"`; `parse` of it gives the key back; `parse("group:a:b:cover")` gives area `"a:b"`; `parse("entity")`, `parse("group:x:switch")` → `None`.
  - in `tests/areas.rs`, `set_group_edits_only_its_kind`: from `{"areas":[{"id":"dehors","name":"Dehors","groupNames":{"light":"Spots"},"newKey":true}]}`, `set_group("dehors", Cover, Some(Some("  Volets ")), Some(true))` encodes `"groupNames":{"cover":"Volets","light":"Spots"}`, `"groupFavorites":["cover"]` and keeps `newKey`; then `Some(None)` removes `cover` from `groupNames`; `Some(Some("  "))` removes too; `set_group("Unassigned", …)` returns false.
- [ ] **Step 2: Run** `cargo test -p oikos-core room_groups set_group` — expected FAIL (unresolved names).
- [ ] **Step 3: Implement** the module and `set_group` per the interfaces.
- [ ] **Step 4: Run** `cargo test -p oikos-core` — expected PASS.
- [ ] **Step 5: Commit** `feat(core): room group keys and membership`.

### Task 2: Group fields on the API and the edit endpoint (`oikos-api`, `oikos-os`)

**Files:**
- Modify: `crates/oikos-api/src/views.rs` (`AreaView` gains the fields; `AreaView::new` fills them)
- Modify: `crates/oikos-os/src/hub.rs` (add `set_area_group` next to `rename_area`)
- Modify: `crates/oikos-os/src/http/api.rs` (route + handler)
- Modify: every `AreaView { … }` literal in tests (`oikos-web/src/{house_state,security}.rs`, `screens/home.rs` tests, `oikos-api` tests) — add the two fields as empty
- Test: `crates/oikos-os/tests/http.rs`

**Interfaces:**
- Consumes: `GroupKind::from_key`, `AreasDocument::set_group` (Task 1).
- Produces:
  ```rust
  // AreaView, JSON snake_case like its other fields, both #[serde(default)]:
  pub group_names: BTreeMap<String, String>,
  pub group_favorites: BTreeSet<String>,
  // Hub:
  pub async fn set_area_group(&self, id: &str, kind: &str, name: Option<Option<String>>, favorite: Option<bool>) -> Result<Vec<AreaView>, HubError>;
  ```
  The body type distinguishes an absent `name` from `"name": null` (use `#[serde(default, deserialize_with = …)]` producing `Option<Option<String>>`, or `serde_with::double_option` if the crate is already a dependency).

- [ ] **Step 1: Write the failing tests** in `tests/http.rs`, following `patch_area_renames_and_keeps_the_app_fields`:
  - `patch_area_group_sets_name_and_favorite`: PATCH `/api/v1/areas/dehors/groups/cover` with `{"name":"Volets","favorite":true}` → 200; the retained `_areas` payload has `"groupNames":{"cover":"Volets"}`, `"groupFavorites":["cover"]` and the area's other fields; the returned `dehors` view has `group_names == {"cover":"Volets"}` and `group_favorites == {"cover"}`.
  - `patch_area_group_null_name_clears`: then `{"name":null}` → `groupNames` gone, `groupFavorites` kept.
  - `patch_area_group_rejects`: kind `switch` → 400; area `Unassigned` → 400; body `{}` → 400; area `attic` → 404.
  - `rename_keeps_group_fields`: after setting a cover name, PATCH `/api/v1/areas/dehors` `{"name":"Outdoor"}` → the retained payload still carries `groupNames`.
  - in `tests/mcp.rs`, `mcp_set_areas_keeps_group_fields`: retained `_areas` = `{"areas":[{"id":"dehors","name":"Dehors","icon":"tree","groupNames":{"cover":"Volets"},"groupFavorites":["cover"],"newKey":true}]}`; call `set_areas` with `[{"id":"dehors","name":"Outdoor","order":3}]` → the retained payload has `name` `Outdoor`, `order` 3, and still `icon`, `groupNames`, `groupFavorites`, `newKey`. An area present in the old document and absent from the call is removed (replace semantics kept for the list itself).
- [ ] **Step 2: Run** `cargo test -p oikos-os --test http patch_area_group rename_keeps` and `cargo test -p oikos-os --test mcp mcp_set_areas_keeps` — expected FAIL (404 route; fields lost).
- [ ] **Step 3: Implement** `AreaView` fields, `Hub::set_area_group` (same shape as `rename_area`: validate, clone `areas_document()`, `set_group`, `set_areas`) and the route `.route("/areas/{id}/groups/{kind}", patch(set_area_group))`. In the MCP tool, merge each incoming area with the declared area of the same id: fields the caller leaves empty (`icon`, `group_names`, `group_favorites`, `extra`) keep their current value; `order` is taken as sent. Say so in the tool's doc comment (it is the tool description), and name the group endpoint as the way to change group names. Add the REST row to the README's table.
- [ ] **Step 4: Run** `cargo test --workspace` — expected PASS (the literal updates in other crates included).
- [ ] **Step 5: Commit** `feat(hub): group names and favourites on areas`.

### Task 3: The fold and the group maths (`oikos-web`, pure)

**Files:**
- Create: `crates/oikos-web/src/groups.rs`
- Modify: `crates/oikos-web/src/lib.rs` (`pub mod groups;`)
- Modify: `crates/oikos-web/src/screens/home.rs` (move `shows_on_home` callers to the new list; `entity_ids` is replaced)
- Test: `#[cfg(test)]` in `groups.rs`; existing tests in `screens/home.rs` move to `home_items`

**Interfaces:**
- Consumes: `GroupKind`, `GroupKey`, `MIN_GROUP_MEMBERS` (Task 1); `AreaView.group_names/group_favorites` (Task 2); `shows_on_home`, `without_paired_humidity`, `FAVORITES` from `screens/home.rs` (make them `pub(crate)`).
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Hash)]
  pub enum HomeItem { Entity(String), Group(GroupKey) }
  /// What Home shows under `chip`, in display order (see Global Constraints: order, Favorites).
  pub fn home_items<'a>(entities: impl Iterator<Item = &'a EntityView>, areas: &[AreaView], chip: &str) -> Vec<HomeItem>;
  /// The members of `key` in Home order; empty when the group does not exist (< 2 members).
  pub fn members(entities: &BTreeMap<String, EntityView>, key: &GroupKey) -> Vec<EntityView>;
  pub fn group_name(areas: &[AreaView], key: &GroupKey) -> String;
  pub fn is_favorite(areas: &[AreaView], key: &GroupKey) -> bool;
  /// How many items Favorites shows: favourite single entities plus favourited groups (a group counts once).
  pub fn favorites_count<'a>(entities: impl Iterator<Item = &'a EntityView>, areas: &[AreaView]) -> usize;
  pub struct GroupSummary { pub on: usize, pub total: usize, pub moving: Option<Motion>, pub positions: Vec<u8>, pub available: bool }
  pub enum Motion { Opening, Closing }
  pub fn summary(members: &[EntityView]) -> GroupSummary;
  pub fn cover_text(s: &GroupSummary) -> String;
  pub fn light_text(s: &GroupSummary) -> String;
  pub fn targets(members: &[EntityView], cmd: &Command) -> Vec<String>;
  pub fn light_toggle(s: &GroupSummary) -> Command;
  pub fn mean_position(members: &[EntityView]) -> u8;
  pub fn mean_brightness(members: &[EntityView]) -> u8;
  ```
  Moving comes from the member's `CoverState` state (`opening`/`closing`) or from `cards::cover::is_moving` over its folded children; take the children from the `entities` map in `members`/`summary` callers (pass them in, keep `summary` pure).

- [ ] **Step 1: Write the failing tests**
  - `two_lights_fold_into_one_group`: living has lamp_a, lamp_b, plug → `[Group(living/light), Entity(plug)]`.
  - `ungrouped_member_dissolves_a_pair`: lamp_b `ungrouped` → `[Entity(lamp_a), Entity(lamp_b), Entity(plug)]` in name order.
  - `unassigned_never_groups`, `hidden_and_energy_members_leave` (a hidden cover and a grid-role light do not count toward the 2).
  - `group_takes_first_slot_and_goes_before_singles`: a pinned (`order: Some(0)`) plug stays first, then the group, then unpinned singles.
  - `favorites_take_favourited_groups_whole`: `groupFavorites: {"cover"}` on living → Favorites shows `Group(living/cover)` even though no cover is `favorite`; a `favorite` light whose group is not favourited shows as `Entity`.
  - `favorites_order_follows_area_index`: favourited groups of two rooms come in the hub's area order.
  - `existing_home_tests_still_hold`: the five current `entity_ids` tests, ported to `home_items` (humidity fold, placement rule).
  - `cover_texts`: all open → "All open"; 2 of 3 → "2 open · 1 closed"; one opening and one closing → "Opening…"; positions `[Some(150)→100, None open→100, None closed→0]`.
  - `light_texts`: "All on", "All off", "2 of 3 on"; `light_toggle` → `TurnOff` when any on, else `TurnOn{None,None,None}`.
  - `targets_skip_unavailable`, `targets_empty_when_all_offline`, `set_position_only_to_capable`, `brightness_only_to_dimmable` (plain `TurnOn` goes to all).
  - `means`: positions 20 and 50 → 35, none known → 0; brightness of on dimmables 100 and 201 → 151, none on → 255.
  - `group_route_round_trips_accented_area`: `GroupKey{area:"bedroom3",kind:Light}` → `/group/{encode_id("bedroom3")}/light` parsed back by the route helper `group_path(&GroupKey) -> String` / `parse_group_path(area: &str, kind: &str) -> Option<GroupKey>` (add both to `groups.rs`).
  - `favorites_chip_counts_favourited_groups`: no favourite entity, one favourited group → `favorites_count == 1`.
- [ ] **Step 2: Run** `cargo test -p oikos-web groups` — expected FAIL.
- [ ] **Step 3: Implement** `groups.rs` per the interfaces and constraints.
- [ ] **Step 4: Run** `cargo test -p oikos-web` — expected PASS.
- [ ] **Step 5: Commit** `feat(web): room group fold and group maths`.

### Task 4: Group cards on Home

**Files:**
- Create: `crates/oikos-web/src/cards/group.rs` (`CoverGroupCard`, `LightGroupCard`)
- Modify: `crates/oikos-web/src/cards/mod.rs` (`pub mod group;`; a `Sender::send_group(&self, ids: &[String], cmd: Command)` that calls `send` per id with no optimistic state)
- Modify: `crates/oikos-web/src/screens/home.rs` (`ids` memo becomes `items: Memo<Vec<HomeItem>>` from `home_items`; `favorites` memo uses `favorites_count`; `<For>` keyed by the `HomeItem` renders `card_for` or the group card)
- Modify: `crates/oikos-web/styles/main.css` (`.card--wide` spanning two grid columns if it is not there yet, `.group__count` badge, `.group__bars` mini bars)

**Interfaces:**
- Consumes: Task 3's `HomeItem`, `members`, `summary`, `cover_text`, `light_text`, `targets`, `light_toggle`, `group_name`, `group_path`.
- Produces: `#[component] pub fn CoverGroupCard(key: GroupKey) -> impl IntoView` and `#[component] pub fn LightGroupCard(key: GroupKey) -> impl IntoView`. Each reads `House` itself through one `Memo` of its members, so a member's state echo repaints only that card.

Behaviour (spec §3): wide card, header = icon + group name + `×N` badge, pending dot when any member is pending, greyed when none is available. Cover: header tap opens the group sheet; body shows `cover_text`, inline ▲ ■ ▼ (`aria-label` "Open all shutters" / "Stop all shutters" / "Close all shutters") sending `Open`/`Stop`/`Close` to `targets`, and one mini bar per member. Light: body shows `light_text` and a `PowerToggle` sending `light_toggle`; header tap opens the group sheet (the web has no long-press). Card `aria-label`: "Shutters, Salon, 2 open, 1 closed" / "Lights, Salon, 2 of 3 on".

- [ ] **Step 1: Write the failing test** `crates/oikos-web/tests/home_groups.rs`, set up like `tests/cover.rs` (`wasm_bindgen_test_configure!(run_in_browser)`, `House`, `Pending`, `Toasts` contexts, a `Router`): `two_lights_show_one_group_card` mounts `HomeScreen` with two off lights in `salon` and asserts one `.card--wide` whose text contains "Lights", "×2" and "All off"; `cover_group_buttons_reach_every_member` mounts `CoverGroupCard` for three covers (one offline) and asserts the "Close all shutters" click marks the two online ones pending (`Pending` holds both ids) and not the offline one.
- [ ] **Step 2: Run** `wasm-pack test --headless --chrome crates/oikos-web --test home_groups` — expected FAIL (unresolved `cards::group`).
- [ ] **Step 3: Implement** the cards and the Home wiring.
- [ ] **Step 4: Run** `cargo test -p oikos-web`, `wasm-pack test --headless --chrome crates/oikos-web` (all browser tests, `smoke` and `cover` included) and `trunk build` in `crates/oikos-web` — expected PASS / build OK.
- [ ] **Step 5: Commit** `feat(web): room group cards on Home`.

### Task 5: The group sheet and its customize section

**Files:**
- Create: `crates/oikos-web/src/screens/group_sheet.rs`
- Modify: `crates/oikos-web/src/screens/mod.rs`, `crates/oikos-web/src/app.rs` (route `/group/:area/:kind` → `GroupSheetScreen`)
- Modify: `crates/oikos-web/src/api.rs` (`pub async fn set_area_group(id: &str, kind: &str, name: Option<Option<&str>>, favorite: Option<bool>) -> Result<Vec<AreaView>, ApiError>`; it omits absent fields from the JSON and sends `null` for `Some(None)`)
- Modify: `crates/oikos-web/styles/main.css`

**Interfaces:**
- Consumes: Task 3 (`parse_group_path`, `members`, `summary`, `targets`, `light_toggle`, `mean_position`, `mean_brightness`, `group_name`, `is_favorite`), Task 4 (`Sender::send_group`), existing `api::set_override` with an `OverridePatch` setting only `ungrouped`, `open_entity`.

Behaviour (spec §4, §6):
- Header: back link to `/` (Home keeps its room through `HomeRoom`), title = group name, subtitle "<room> · N shutters" / "N lights".
- Cover: Open / Stop / Close; an "All to" range input shown when any member has position, starting at `mean_position`, sending `SetPosition` on `change` (release), not on `input`.
- Light: master `PowerToggle` (`light_toggle`); a brightness range shown when any member is dimmable, starting at `mean_brightness`, sending `TurnOn{brightness}` on `change`.
- One row per member: name, state text (`format::display_value`), its own ▲ ■ ▼ or switch; tapping the name opens `/entity/<id>`. `nav::return_target` already treats `/group/…` as a return target (only `/entity/` is excluded), so that sheet's Back lands on the group sheet; the group sheet's own Back goes to `/`. Pin it with `group_sheet_is_a_return_target` in `nav.rs` tests (`return_target("/group/salon/light", "") == Some("/group/salon/light")`).
- Customize: name field (Save sends `name`, blank sends `null`), "Show in Favorites" switch (sends `favorite`), one checkbox per member (unticking PATCHes that override with `ungrouped: true`), "Ungroup all". Failures go to `Toasts` as "Couldn't save the group".
- When the route's group no longer exists (fewer than 2 members after an ungroup, or bad params), show "This group is gone." with the back link.

- [ ] **Step 1: Write the failing tests**: in `api.rs` tests, `group_body_distinguishes_null_from_absent` (the body builder `group_body(name, favorite) -> serde_json::Value` gives `{"name":null}` for `Some(None)`, `{}` for `(None, None)`, `{"favorite":true}` for `(None, Some(true))`).
- [ ] **Step 2: Run** `cargo test -p oikos-web group_body` — expected FAIL.
- [ ] **Step 3: Implement** `group_body`, `set_area_group`, the screen and the route.
- [ ] **Step 4: Run** `cargo test --workspace`, `trunk build`; then rebuild and restart the hub with `tools/restart.sh` and check by hand against the house: the living-room covers open, stop and close together; "All to" 50 %; the lights toggle together; rename the cover group in the web and see the name on the phone; ungroup one light on the phone and see the web fold change.
- [ ] **Step 5: Commit** `feat(web): group sheet with customize`.

### Task 6: Docs

**Files:**
- Modify: `README.md` (web UI section: Home groups; REST table row from Task 2 if not added there)
- Create: `docs/superpowers/plans/2026-10-19-web-room-groups-deferred.md` (what was left out: MCP tool for group names, group "Retry" merge of failures (app spec §5 failure merge), group tiles anywhere but Home, search/hidden views that the web does not have)

- [ ] **Step 1: Write** both files.
- [ ] **Step 2: Commit** `docs(web): room groups`.
