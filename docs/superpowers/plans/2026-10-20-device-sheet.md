# Device Sheet and "Show as" Types Implementation Plan (1 of 2: hub and web)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A switch can be shown as a Switch, Light, Door opener, Siren, Chime or Plug (one `showAs` override field replacing `asLight`), each with its own card, and the web device sheet shows only the settings that apply to the entity, with the technical details folded away.

**Architecture:** `oikos-core` owns the contract: the `showAs` field, its legacy `asLight` reading and compatibility write, and a `SwitchType` enum every crate uses. The registry derives `kind == Light` from it; every other type stays `Kind::Switch`, so commands, flows and MCP need no change. The web gets a pure `settings_rules` module (which controls show for an entity and draft, natively tested), rebuilds the override editor on it, regroups the sheet, and adds three cards.

**Tech Stack:** Rust, serde, axum + rmcp (hub), Leptos CSR + Trunk (web, wasm).

**Spec:** `docs/superpowers/specs/2026-10-20-device-sheet-design.md` (steps 1 and 2 of its §10 order of work). Plan 2 (the Android app) and step 4 (the `asLight` migration and dropping the compatibility write) are separate.

## Global Constraints

- Base: `main` at `7218a8c` or later. `crates/oikos-web/styles/main.css` already carries an uncommitted change from another session: pull and look before editing it, and stage only your own hunks (`git add -p`).
- `SHOW_AS = ["light", "door", "siren", "chime", "plug"]`; absent means Switch; an unknown stored value round-trips and reads as Switch. `showAs` only applies when `protocol_kind == Switch`.
- Decoding: a stored `asLight: true` with no `showAs` reads as `showAs: "light"`; `asLight` is never kept as a field or in `extra`. When both are present, `showAs` wins.
- Encoding (`UiOverride::encode`, the retained payload): writes `"asLight": true` next to `"showAs": "light"`, and never otherwise. The derived `Serialize` (API JSON, MCP views) never writes `asLight`.
- Patches: `showAs` accepts one of `SHOW_AS` or `null`; `asLight` is an unknown field. `UiOverride::FIELDS` keeps 19 names, `showAs` in the slot `asLight` had.
- Picking the Light type also sets `securityAction: false` in the same draft. No type change touches any other field.
- Copy (English, US spelling as the rest of the web UI): type labels "Switch", "Light", "Door opener", "Siren", "Chime", "Plug". Settings copy is the app's (`Oikos/feature/home/src/main/res/values/strings.xml`, `customize_*`): "Name" (hint "Leave empty to use the device's own name."), "Room", "Favorite", "Hide from Home", "Show on Home", "Position" (hint "Lower numbers come first. Leave empty for automatic.", error "Enter a whole number from 0 to 9999."), "Show as", "Group with the other lights in this room" / "Group with the other shutters in this room", "Turn on in" (chips "Last state", "White", "Warm", "Red", "Green", "Blue"), "Brightness", "Show on Security", "Ask before acting", "Card style" (chips "Standard", "Header temperature"), "Energy role" (chips "None", "Solar", "Grid", "Grid export", "Battery", "House", "Consumer", "Water"), "Invert sign", "Needs a power unit (W, kW)", "Icon" (chips "Plug" = none, "Water heater", "Heat pump", "Oven", "Fridge", "Washer", "Dryer", "Dishwasher"), "Energy counter" (select "Automatic" + counters by name), "Target presets (°C)" (hint "Comma-separated, e.g. 19, 21, 23. Up to 6 chips."), "Other saved settings", "Save", "Reset to defaults".
- Card copy: Door opener button "Open", question "Open <name>?", confirm button "Open"; Siren toggle label "Ringing" when on, question "Ring <name>?", confirm button "Ring"; Chime button "Play"; every confirm has "Cancel".
- The editor keeps its draft model: built once from the initial entity, follows outside changes only while not dirty, Save sends only `dirty_patch`.
- Attribution, wherever one is needed: Gekkotron. Commits use `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com` and end with the `Co-Authored-By` line.
- Every task ends green on: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo check -p oikos-web --target wasm32-unknown-unknown`.

## Review Focus

1. A real house override `{"name":"Lumière tv","asLight":true}` edited from the web (rename only): the retained payload still carries `asLight: true` and `showAs: "light"`, and the entity stays a light. Test: `rename_keeps_legacy_light_on_the_wire` (Task 1).
2. MCP `set_override` with `{"asLight": true}` from an old prompt: rejected with an error that names `showAs` among the known fields, nothing published. Test: update `set_override_rejects_unknown_field` in `oikos-os/tests/mcp.rs` (Task 1).
3. The user picks Door opener, then back to Switch, then saves without other edits: the patch is exactly `{"showAs": null}` if the stored value was absent, or empty; no stray `securityAction`. Test: `type_round_trip_is_clean` (Task 2).
4. A light whose override still holds `energyRole: "grid"` (written by MCP): the Energy controls are hidden, the value is listed under "Other saved settings", and its × clears it in the draft. Test: `other_saved_lists_hidden_values` (Task 2).
5. Door opener with `securityAsk: false`: a tap on "Open" sends `TurnOn` at once, no question; with the default `true` the first tap only asks. Test: `door_asks_only_with_security_ask` (Task 4).

---

### Task 1: `showAs` in the hub contract

**Files:**
- Create: `crates/oikos-core/src/model/switch_type.rs`
- Modify: `crates/oikos-core/src/model/ui_override.rs` (field, `SHOW_AS`, `FieldType::ShowAs`, `FIELD_TYPES`, `FIELDS`, `decode`, `apply_patch`, `encode`)
- Modify: `crates/oikos-core/src/model/mod.rs` (export `SwitchType`, `SHOW_AS`)
- Modify: `crates/oikos-core/src/registry/mod.rs:420` (kind from `SwitchType`)
- Modify: `crates/oikos-os/src/mcp/params.rs:75-81` (field list doc: `showAs` (light, door, siren, chime, plug) instead of `asLight`)
- Modify: `crates/oikos-os/src/mcp/tools.rs` (`entity_matches` also matches the type label, lowercase, of a non-Switch type)
- Modify: `crates/oikos-web/src/override_editor.rs` (`dirty_patch`: `show_as => "showAs"` replaces `as_light`; its `every_known_field_is_compared` test; the "Show as a light" flag becomes a temporary `ChoiceField` over `SHOW_AS` so the crate compiles; Task 3 replaces it)
- Modify: `docs/superpowers/specs/2026-10-18-oikos-os-design.md` §3 `UiOverride` line (`showAs` replaces `asLight`; legacy read and compat write until the app migrates)
- Test: `crates/oikos-core/tests/ui_override.rs`, `crates/oikos-core/tests/registry.rs`, `#[cfg(test)]` in `switch_type.rs`, `crates/oikos-os/tests/{hub,http,mcp}.rs` (every `asLight` patch becomes `showAs: "light"`)

**Interfaces:**
- Produces: `UiOverride.show_as: Option<String>` (`#[serde(skip_serializing_if = "Option::is_none")]`), no `as_light` field. `pub const SHOW_AS: &[&str]`.
- Produces: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SwitchType { Switch, Light, Door, Siren, Chime, Plug }` with `pub const ALL: [SwitchType; 6]` (that order), `pub fn of(protocol_kind: Kind, ui: &UiOverride) -> Option<SwitchType>` (`None` unless `protocol_kind == Kind::Switch`), `pub fn key(self) -> Option<&'static str>` (`None` for Switch), `pub fn label(self) -> &'static str`, `pub fn effective_kind(protocol_kind: Kind, ui: &UiOverride) -> Kind` (Light for the Light type, else `protocol_kind`).

- [ ] **Step 1: Write the failing tests**

In `crates/oikos-core/tests/ui_override.rs`:

```rust
#[test]
fn legacy_as_light_reads_as_light_type() {
    let ui = UiOverride::decode(br#"{"name":"Lumiere tv","asLight":true}"#).unwrap();
    assert_eq!(ui.show_as.as_deref(), Some("light"));
    assert!(ui.extra.is_empty());
    let off = UiOverride::decode(br#"{"asLight":false}"#).unwrap();
    assert_eq!(off.show_as, None);
    assert!(off.is_default());
    let both = UiOverride::decode(br#"{"asLight":true,"showAs":"siren"}"#).unwrap();
    assert_eq!(both.show_as.as_deref(), Some("siren"));
}

#[test]
fn encode_writes_as_light_only_next_to_light() {
    let light = UiOverride { show_as: Some("light".into()), ..UiOverride::default() };
    let wire: Value = serde_json::from_str(&light.encode()).unwrap();
    assert_eq!(wire, json!({"showAs": "light", "asLight": true}));
    let siren = UiOverride { show_as: Some("siren".into()), ..UiOverride::default() };
    assert_eq!(serde_json::from_str::<Value>(&siren.encode()).unwrap(), json!({"showAs": "siren"}));
    assert!(!serde_json::to_string(&light).unwrap().contains("asLight"));
}

#[test]
fn rename_keeps_legacy_light_on_the_wire() {
    let stored = UiOverride::decode(br#"{"name":"Lumiere tv","asLight":true}"#).unwrap();
    let renamed = stored.apply_patch(&patch(json!({"name": "TV light"}))).unwrap();
    let wire: Value = serde_json::from_str(&renamed.encode()).unwrap();
    assert_eq!(wire, json!({"name": "TV light", "showAs": "light", "asLight": true}));
}

#[test]
fn show_as_patch_is_validated() {
    let base = UiOverride::default();
    assert_eq!(base.apply_patch(&patch(json!({"showAs": "door"}))).unwrap().show_as.as_deref(), Some("door"));
    assert!(matches!(base.apply_patch(&patch(json!({"showAs": "fan"}))), Err(PatchError::InvalidValue { .. })));
    assert_eq!(base.apply_patch(&patch(json!({"asLight": true}))), Err(PatchError::UnknownField("asLight".into())));
    let unknown = UiOverride::decode(br#"{"showAs":"fan"}"#).unwrap();
    assert_eq!(unknown.show_as.as_deref(), Some("fan"));
}
```

In `switch_type.rs` tests: `of(Kind::Light, &ui)` is `None`; absent → `Switch`; each `SHOW_AS` value → its variant; `"fan"` → `Switch`; `ALL.map(key)` is `[None, Some("light"), Some("door"), Some("siren"), Some("chime"), Some("plug")]`; labels as in the Global Constraints; `effective_kind(Kind::Switch, light ui) == Kind::Light`, `effective_kind(Kind::Switch, door ui) == Kind::Switch`, `effective_kind(Kind::Cover, light ui) == Kind::Cover`.

In `crates/oikos-core/tests/registry.rs` rename `path_area_and_as_light` → `path_area_and_light_type`: keep its legacy `{"asLight":true}` payload and add a second entity override `{"showAs":"light"}`; both have `kind == Kind::Light`; a third `{"showAs":"door"}` keeps `Kind::Switch`. Also the existing `decodes_every_recorded_override` must still pass on the fixtures under `tests/fixtures/ui/` (two carry `asLight: true`).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p oikos-core`
Expected: compile errors on `show_as` / `SwitchType`.

- [ ] **Step 3: Implement**

- `UiOverride`: replace `as_light` by `show_as`. Legacy handling in one private `fn absorb_legacy(&mut self)`: remove `"asLight"` from `extra`; when it was `true` and `show_as` is `None`, set `show_as = Some("light")`. Call it at the end of `decode` and on the result of `apply_patch`'s `from_value`.
- `encode`: serialize to a `Map`, insert `"asLight": true` when `show_as == Some("light")`, then to string. Mark the insert with a comment naming step 4 of the spec's §10, so Plan 2 finds it.
- `FieldType::ShowAs` accepts a string in `SHOW_AS`; `expected()` is `one of light, door, siren, chime, plug`.
- `registry::recompute`: `let kind = SwitchType::effective_kind(entity.protocol_kind, &ui);`.
- Update every `asLight` patch in the hub tests to `showAs: "light"` and `view.ui.as_light` asserts to `show_as.as_deref() == Some("light")`. In `mcp.rs`, the unknown-field message test asserts `message.contains("showAs")`, and add one call with `{"asLight": true}` that comes back `is_error` with nothing published. The `raw_override` assertion at `mcp.rs:303` now reads `showAs` (and `asLight`, since the retained payload carries both).

- [ ] **Step 4: Run the workspace checks**

Run the four commands of the Global Constraints. Expected: all pass.

- [ ] **Step 5: Commit**

`feat(core): showAs override type replaces asLight`

---

### Task 2: Settings rules (`oikos-web`, pure)

**Files:**
- Create: `crates/oikos-web/src/settings_rules.rs` (add `pub mod settings_rules;` to `lib.rs`)
- Test: `#[cfg(test)]` in the same file

**Interfaces:**
- Consumes: `SwitchType` (Task 1), `Placement::{elsewhere, security_kind, action_kind}`, `GroupKind::of`, `UNASSIGNED_AREA`, `EnergyRole::{parse, signed, power}`, `PowerUnits::factor`, `crate::cards::light::has_colour`.
- Produces:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub show_as: bool,            // protocol_kind == Switch
    pub show_on_home: bool,       // Placement::elsewhere(kind, dc, draft); else "Hide from Home"
    pub group_with_room: bool,    // GroupKind::of(kind).is_some() && entity.area != UNASSIGNED_AREA
    pub light_on: bool,           // kind == Light && has_colour(color_modes)
    pub light_on_brightness: bool,// light_on && draft.light_on_preset.is_some() && capabilities.brightness
    pub security_shown: bool,     // Placement::security_kind(kind, dc)  (inverse of securityHidden)
    pub security_action: bool,    // Placement::action_kind(kind)
    pub security_ask: bool,       // security_action && (draft.security_action || type is Door or Siren)
    pub card_style: bool,         // kind == Sensor
    pub energy_role: bool,        // kind == Sensor
    pub energy_invert: bool,      // energy_role && role.signed()
    pub energy_icon: bool,        // energy_role && role == Consumer
    pub energy_meter: bool,       // energy_role && role.power()
    pub unit_warning: bool,       // energy_meter && PowerUnits::factor(unit).is_none()
    pub climate_presets: bool,    // kind == Climate
}
pub fn shown(entity: &EntityView, draft: &UiOverride) -> Shown;
pub fn set_type(draft: &mut UiOverride, t: SwitchType);
pub fn other_saved(draft: &UiOverride, shown: &Shown) -> Vec<(&'static str, String)>;
pub fn clear_field(draft: &mut UiOverride, key: &str);
pub fn order_valid(text: &str) -> bool;
```

`kind` everywhere above is `SwitchType::effective_kind(entity.protocol_kind, draft)`, so the form follows the draft's type. `other_saved` lists, in `UiOverride::FIELDS` order, each non-default field among `showAs`, `ungrouped`, `lightOnPreset`, `lightOnBrightness`, `securityHidden`, `securityAction`, `securityAsk` (non-default is `false`), `cardStyle`, `energyRole`, `energyInvert`, `energyIcon`, `energyMeter`, `climatePresets` whose control `shown` hides, as `(camelCase key, value as text)`. It never lists `name`, `area`, `hidden`, `homePinned`, `favorite`, `order`. `clear_field` sets that field to its default. `set_type` writes `show_as = t.key()`, and `security_action = false` for `Light`. `order_valid`: empty, or an integer 0..=9999 after trimming.

- [ ] **Step 1: Write the failing tests**

Build `EntityView`s with a small `entity(kind, device_class, unit)` helper (copy the one in `security.rs` tests). Tests and their assertions:
- `light_has_only_light_settings`: colour light (`color_modes = {"xy"}`, brightness) in `living`: `light_on`, `group_with_room` true; every security, energy, card style and climate flag false; `show_as` false.
- `plain_light_has_no_light_on_chips`: light with no colour modes: `light_on == false`.
- `switch_gets_type_and_security`: a switch: `show_as`, `security_action` true, `security_ask` false; after `draft.security_action = true`, `security_ask` true.
- `door_and_siren_always_offer_ask`: `set_type(Door)` then `shown`: `security_ask` true with `security_action == false`; same for Siren; not for Chime.
- `light_type_follows_the_draft`: switch with `set_type(Light)`: `group_with_room` true, `security_action` false, and `draft.security_action == false` even if it was true.
- `type_round_trip_is_clean`: stored default; `set_type(Door)` then `set_type(Switch)`: `dirty_patch(&stored, &draft)` is empty. Stored `securityAction: true`, `set_type(Door)`: patch is `{"showAs":"door"}` only.
- `power_sensor_gets_energy`: sensor with `unit = "W"`, `energy_role = "grid"`: `energy_role`, `energy_invert`, `energy_meter` true, `energy_icon` false, `unit_warning` false; with `unit = "kWh"`: `unit_warning` true; role `"consumer"`: `energy_icon` true, `energy_invert` false; role `"water"`: `energy_meter` false.
- `door_sensor_gets_security_shown`: binary sensor `device_class = "door"`: `security_shown` true, `security_action` false.
- `elsewhere_switches_hidden_for_show_on_home`: sensor with an energy role: `show_on_home` true; without: false.
- `climate_gets_presets`.
- `group_needs_a_room`: light in `UNASSIGNED_AREA`: `group_with_room` false.
- `other_saved_lists_hidden_values`: light with `energy_role = Some("grid")`, `security_ask = false`, `favorite = true`: `other_saved` is `[("energyRole", "grid"), ("securityAsk", "false")]` in `FIELDS` order; `clear_field(&mut d, "energyRole")` leaves `energy_role == None`.
- `order_bounds`: `""`, `" 7 "`, `"9999"` valid; `"-1"`, `"10000"`, `"1.5"`, `"x"` invalid.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p oikos-web settings_rules`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement `settings_rules.rs`** per the Interfaces block.

- [ ] **Step 4: Run tests and the workspace checks.** Expected: all pass.

- [ ] **Step 5: Commit**

`feat(web): per-entity settings rules for the device sheet`

---

### Task 3: The settings form

**Files:**
- Modify: `crates/oikos-web/src/override_editor.rs` (rewrite the form body on `settings_rules`; keep `dirty_patch`, the draft/stored/effect/save/clear machinery and its tests)
- Modify: `crates/oikos-web/styles/main.css` (only what chip rows and the "Other saved settings" line need; reuse `.chips`, `.chip`, `.chip--selected`)

**Interfaces:**
- Consumes: `settings_rules::{shown, set_type, other_saved, clear_field, order_valid, Shown}` (Task 2), `SwitchType` (Task 1), `EnergyMeters::counters`, `crate::energy::energy_entities`, `ConsumerIcon`, `icons::consumer_icon`.
- Produces: `OverrideEditor(entity: EntityView)` unchanged signature, used by Task 5.

- [ ] **Step 1: Add a `ChipField` component** in `override_editor.rs`: `label: &'static str`, `options: Vec<(&'static str /*shown text*/, Option<&'static str> /*value*/)>`, `get: fn(&UiOverride) -> Option<String>`, `on_pick: Callback<Option<&'static str>>`; renders a `.chips` row of `<button type="button" class="chip">` with `class:chip--selected` and `aria-pressed`. A stored value not in `options` shows as an extra selected chip with its raw text (as `ChoiceField` does for unknown values today).

- [ ] **Step 2: Rebuild the form.** `let shown = Memo::new(move |_| draft.with(|d| settings_rules::shown(&entity, d)));` and render, each block behind its `shown` flag (`<Show>`), in this order and with the copy of the Global Constraints:
  1. General: Name (`TextField`, placeholder = the discovered name, hint below), Room (`AreaField`, label "Room"), Favorite (flag), Position (text input: invalid shows the error line and disables Save through `order_valid`), then `show_on_home` ? "Show on Home" (checked = `home_pinned && !hidden`; on → `home_pinned = true, hidden = false`; off → `home_pinned = false`) : "Hide from Home" (`hidden`).
  2. "Show as" chips over `SwitchType::ALL` (labels), `on_pick` → `set_type`.
  3. Light: group toggle (checked = `!ungrouped`; label by kind: lights or shutters), "Turn on in" chips ("Last state" clears `light_on_preset` and `light_on_brightness`), the existing `BrightnessField` relabelled "Brightness".
  4. Security: "Show on Security" for `security_shown` (checked = `!security_hidden`) and for `security_action` (`security_action`); "Ask before acting" (`security_ask`).
  5. Sensor: "Card style" chips (`None` / `"hero_weather"`), "Energy role" chips (`None` + `ENERGY_ROLES` with their labels), "Invert sign", "Icon" chips (`None` = "Plug" + each `ConsumerIcon` by key, glyph from `consumer_icon` before the text), "Energy counter" `<select>` ("Automatic" = `None`, then `EnergyMeters::counters(&energy_entities(&house.entities.get()))` by name, value = id), the unit warning line.
  6. Climate: presets text field with the hint.
  7. "Other saved settings": when `other_saved` is non-empty, one line per entry `key: value` with a `×` button (`aria-label="Clear <key>"`) calling `clear_field`.
  8. Error line, then Save (disabled when not dirty, busy, or `!order_valid`) and "Reset to defaults" (today's clear).
  Remove the old `Group` fieldsets, the free-text `cardStyle` / `energyIcon` / `energyMeter` fields and the temporary `SHOW_AS` `ChoiceField` from Task 1. Groups get a small title (`override__legend` style) only when at least one control in them shows.

- [ ] **Step 3: Run the workspace checks.** Expected: all pass (the `dirty_patch` tests are unchanged).

- [ ] **Step 4: Check it in the browser.** `cd crates/oikos-web && trunk serve` against the running hub (or `tools/restart.sh`), then open, at 500 px wide and at desktop width, a colour light, a switch, a power sensor, a door sensor and a climate. Each shows only the blocks the rules give it. On the switch, picking Door opener reveals "Ask before acting", and picking Light reveals the group toggle and hides Security. Screenshots are taken through headless Chrome over CDP: the extension times out on the wasm page.

- [ ] **Step 5: Commit**

`feat(web): device settings show only what applies`

---

### Task 4: Type cards, icons and Security wording

**Files:**
- Create: `crates/oikos-web/src/cards/door.rs` (`DoorCard`), `crates/oikos-web/src/cards/siren.rs` (`SirenCard`), `crates/oikos-web/src/cards/chime.rs` (`ChimeCard`), `crates/oikos-web/src/components/confirm.rs` (`ConfirmBar`)
- Modify: `crates/oikos-web/src/cards/mod.rs`, `components/mod.rs`, `screens/home.rs:104` (`card_for`), `icons.rs` (new `SIREN`, `CHIME` glyphs; `entity_icon`), `components/card_frame.rs:37`, `format.rs` (`type_word`), `security.rs` (`ActionRow.switch_type`, `tap`, wording), `screens/security.rs:53-80` (verb and question from the row)
- Test: `#[cfg(test)]` in `security.rs`, `icons.rs`, `format.rs`; a pure `fn door_tap(ask: bool) -> Tap` style helper in `cards/door.rs` tested natively

**Interfaces:**
- Consumes: `SwitchType::of` (Task 1).
- Produces: `pub fn entity_icon(entity: &EntityView) -> &'static str` (Light → `LIGHT`, Door → `DOOR`, Siren → `SIREN`, Chime → `CHIME`, Plug → `PLUG`, else `kind_icon(kind, device_class)`); `pub fn type_word(entity: &EntityView) -> String` (the `SwitchType` label for a non-Switch type, else `kind_word(entity.kind)`); `ConfirmBar(question: String, yes: &'static str, on_yes: Callback<()>, on_cancel: Callback<()>)`; `ActionRow { …, switch_type: Option<SwitchType> }` with `pub fn verb(&self) -> &'static str` ("Open" Door, "Ring" Siren, "Play" Chime, "Turn on" Switch/Plug, "Press" Button) and `question()` = `"<verb> <name>?"`.

- [ ] **Step 1: Write the failing tests**
  - `security.rs`: `door_action_always_opens`: Door row with `on == Some(true)` taps `TurnOn` (not `TurnOff`), `confirm == ask`; `chime_action_always_plays` (same); `siren_action_toggles`: on → `TurnOff` no confirm, off → `TurnOn` with `confirm == ask`; `verb` and `question` per type (`"Open Gate?"` for a Door row named "Gate").
  - `door.rs`: `door_asks_only_with_security_ask`: `door_tap(true)` is "ask first", `door_tap(false)` is "send `TurnOn` now".
  - `icons.rs`: `entity_icon` returns `DOOR`, `SIREN`, `CHIME`, `PLUG`, `LIGHT` for those types on a switch, `SWITCH` for a plain one, and ignores `showAs` on a non-switch.
  - `format.rs`: `type_word` is "Door opener" for a door-typed switch, "Switch" for a plain one, "Light" for a native light.

- [ ] **Step 2: Run to verify failure.** `cargo test -p oikos-web` → FAIL.

- [ ] **Step 3: Implement**
  - `card_for`: for `Kind::Switch`, match `SwitchType::of`: Door → `DoorCard`, Siren → `SirenCard`, Chime → `ChimeCard`, others → `SwitchCard`. Light-typed switches already arrive as `Kind::Light`.
  - `DoorCard`: a wide "Open" button (as `ButtonCard`), sending `TurnOn { brightness: None, color: None, effect: None }`; with `ui.security_ask` the first tap swaps the controls for a `ConfirmBar` ("Open <name>?", "Open"). While the state reads on, a small "On" line shows under the button.
  - `SirenCard`: `PowerToggle`, label "Ringing" when on (and the card gets an alert modifier class, red from the existing danger token), else the usual value label; turning on with `security_ask` asks through `ConfirmBar` ("Ring <name>?", "Ring"); turning off sends at once.
  - `ChimeCard`: a wide "Play" button sending `TurnOn`, no confirmation.
  - `card_frame.rs` and the sheet header (Task 5) use `entity_icon`.
  - `security.rs`: fill `switch_type` from `SwitchType::of(e.protocol_kind, &e.ui)`; `tap` per the tests; `screens/security.rs` takes `verb` and `question` from the row.

- [ ] **Step 4: Run tests and the workspace checks.** Expected: pass.

- [ ] **Step 5: Commit**

`feat(web): door opener, siren and chime cards`

---

### Task 5: Sheet layout and end-to-end check

**Files:**
- Modify: `crates/oikos-web/src/screens/entity_sheet.rs` (`SheetBody`, `device_panel`, `state_panel`)
- Modify: `crates/oikos-web/styles/main.css` (the `<details>` summary style, reusing `sheet__panel` / `sheet__heading`)
- Modify: `README.md` (MCP field list, if it names `asLight`)

**Interfaces:**
- Consumes: `entity_icon`, `type_word` (Task 4), `OverrideEditor` (Task 3).

- [ ] **Step 1: Regroup `SheetBody`.** Order: header (icon from `entity_icon`, subtitle `area · type_word · availability`), Controls panel, `ColourPanel`, a "Settings" panel holding `OverrideEditor`, then one `<details class="sheet__panel sheet__details">` closed by default, `<summary>` "Device & diagnostics", containing today's Device rows and "Other entities" links, then the State subsections (Value, Attributes, Last update). A device-less entity shows only the State part inside it.

- [ ] **Step 2: Run the workspace checks.** Expected: pass.

- [ ] **Step 3: Build and run the real thing.** `tools/restart.sh` (hub on 8081). In headless Chrome over CDP at 500 px and 1280 px: open a colour light, the stair switch, a power sensor, a door sensor and a climate. Each shows the Controls, only its settings, and the folded diagnostics. Turn a test switch into Door opener, save, and check: the Home card is "Open", it asks first, Security (after "Show on Security") says "Open <name>?". Then put it back to Switch. Read its retained override over MCP `get_entity` (`raw_override`): it is back to what it was. Turn a switch into Light and check `raw_override` carries both `showAs: "light"` and `asLight: true`.

- [ ] **Step 4: Commit**

`feat(web): device sheet keeps diagnostics folded`

- [ ] **Step 5: Record the follow-ups** in `docs/superpowers/plans/2026-10-20-device-sheet-deferred.md`: Plan 2 (the app, spec §10), step 4 (migrate stored `asLight`, drop the compat write in `UiOverride::encode`), and anything found in Step 3.
