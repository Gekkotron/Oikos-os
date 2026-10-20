# Device sheet clean-up and "Show as" types

Author: Gekkotron · 2026-10-20 · Status: design, awaiting approval (revised: app in scope)

## 1. Intent

Opening a device in the web UI (`/entity/<id>`, `screens/entity_sheet.rs`) shows everything at once: controls, colour, a Device panel full of Zigbee internals, raw JSON state, and an override form of about 20 fields (`override_editor.rs`) that are the same for every kind. A light gets Energy, Security and Climate fields it can never use. Free-text fields (`cardStyle`, `energyIcon`, `energyMeter`) expect magic strings. The sheet is hard to use for its main job: set up how a device appears and acts.

The user's idea drives the redesign: **a device has a type you can change, and the type decides which settings show up.** On Zigbee2MQTT a "switch" is often a relay driving something else: a light, a siren, a gate or door opener, a chime, a plug. Picking what it really is changes the sheet and how the device looks and acts across the UI.

## 2. Goals

1. The sheet shows only the settings that apply to this entity, by the same rules the Android app's Customize sheet uses (`SourcesPerso/Oikos`, `feature/home/.../ui/CustomizeSheet.kt`), plus the new type selector.
2. A switch entity (`protocol_kind == Switch`) gets a **Show as** selector: Switch, Light, Door opener, Siren, Chime, Plug.
3. Each type has its own card on Home, in room lists and on the sheet, with its own icon and wording.
4. Technical information (model, vendor, IEEE, battery, link quality, other entities, raw state) moves into one folded **Device & diagnostics** section at the bottom.
5. No free-text field remains where the valid values are known; they become chips or selects.

## 3. Non-goals

- Backward compatibility with old app builds. The Android app is in development and its author is its only user (user, 2026-10-20), so the override contract changes cleanly and the app is updated to match (§10).
- No new protocol `Kind`. `Kind` mirrors Home Assistant discovery and the app's enum; the new types are presentation on top of `Switch`.
- No type selector for kinds other than Switch (binary sensors already carry a `device_class`; covers, climates and so on are what they are).
- No timed auto-off for door openers or chimes (see §9).

## 4. Anti-goals

- The hub and the app disagreeing about a device's type once both are updated, or an existing switch-as-light reverting to a switch during the change.
- A sheet that hides a setting the user has already set without any way to see or clear it (see §6.4).
- Losing unsaved typing on a WebSocket state echo (the current editor's guarantee stays).

## 5. The "Show as" type

### 5.1 Storage

One override field, **`showAs`**, replaces `asLight`:

| Choice in the selector | `showAs` |
|---|---|
| Switch (default) | absent |
| Light | `"light"` |
| Door opener | `"door"` |
| Siren | `"siren"` |
| Chime | `"chime"` |
| Plug | `"plug"` |

- `showAs` is an optional string field of `UiOverride` (`oikos-core/src/model/ui_override.rs`), camelCase on the wire, omitted when absent, like the other fields. Valid values: `SHOW_AS = ["light", "door", "siren", "chime", "plug"]`; a patch rejects others (new `FieldType::ShowAs`). A stored value the hub does not know decodes as a string, round-trips, and reads as Switch.
- `showAs` only applies when `protocol_kind == Switch`; it is ignored on other kinds.
- **`asLight` is retired.** Decoding still reads a stored `asLight: true` (the house has some today) as `showAs: "light"` when `showAs` is absent; encoding never writes `asLight`. Patches no longer accept `asLight` (MCP callers use `showAs`). On startup, once retained overrides are loaded, the hub rewrites every override that still carries `asLight` in the new form (one retained publish each), so the broker holds a single shape; this must run only after the app can read `showAs` (see §10, order of work) or the app would show those lights as switches. Builder's call whether the migration is automatic at startup or an explicit one-off (an MCP tool or CLI flag); automatic is fine.
- Field list updates: `FIELD_TYPES`, `UiOverride::FIELDS` (still 19 names: `asLight` out, `showAs` in, same position), the web `dirty_patch` list and its "every known field is compared" test, the MCP `set_override` docs (`oikos-os/src/mcp/params.rs`) and the server instructions string, and the `UiOverride` line of `docs/superpowers/specs/2026-10-18-oikos-os-design.md` §3.

### 5.2 Effect on the model

- `kind` is `Light` when a switch's type is Light (`showAs: "light"`, or the legacy `asLight`), else the protocol kind (`registry/mod.rs::recompute`). Door opener, Siren, Chime and Plug keep `kind == Switch`, so commands (`TurnOn`/`TurnOff`), flows (`trigger.state`, `command` nodes) and MCP work unchanged.
- `EntityView` gains nothing; the web reads `entity.ui.show_as` through one helper, `fn switch_type(&EntityView) -> SwitchType` (enum `Switch, Light, Door, Siren, Chime, Plug`) in `oikos-core` (natively testable), used by cards, icons, the sheet and Security.
- MCP `list_entities`/`get_entity` already return `ui`, so `showAs` is visible there. `search` should match the type word ("siren", "door opener") as well as the name; builder's call how.

### 5.3 Behaviour per type

| Type | Card (Home, room, sheet Controls) | Icon | Security |
|---|---|---|---|
| Switch | today's `SwitchCard` (power toggle) | today's switch glyph | opt-in action, as today |
| Light | today's `LightCard` (unchanged path: `kind == Light`) | light | none |
| Door opener | one **Open** button that sends `TurnOn`; with `securityAsk` it asks "Open <name>?" first; shows the last state as a small "On/Off" line only while the relay reports on | door | opt-in action |
| Siren | power toggle; when on, the card turns red and reads **Ringing**; turning on asks first when `securityAsk` | siren/bell | opt-in action |
| Chime | one **Play** button that sends `TurnOn` | music note | opt-in action |
| Plug | today's power toggle | plug (`icons.rs` already has `PLUG`) | opt-in action |

- `card_for` (`screens/home.rs`) dispatches on `switch_type` for `Kind::Switch`.
- Security Actions (`security.rs`, `ActionTap`) use the type's wording: "Open" / "Open <name>?" for a door opener, "Ring" for a siren, "Play" for a chime, the current wording for the others.
- Picking a type never adds the device to Security: every type stays on Home by default, and the user moves it with **Show on Security** (`securityAction`), which then takes it off Home unless **Show on Home** is on, as today (user decision, 2026-10-20). Switching between switch types keeps `securityAction` (they are all action kinds). Picking **Light** clears `securityAction` in the same patch, since a light cannot be a Security action and the value would otherwise linger unseen (my call).
- The Door opener and Siren cards ask before acting wherever they are (Home, room, sheet, Security) while `securityAsk` is true, its default. So their **Ask before acting** toggle is shown whether or not Security action is on (my call); for the other types it shows only while Security action is on, as in the app.
- Room groups: only lights and covers group. A Switch shown as Light already groups through `kind`; nothing else changes.

## 6. The sheet

### 6.1 Layout, top to bottom

1. **Header** (as today): back link, icon, title, subtitle "Area · Type word · availability". The type word is the `SwitchType`'s ("Door opener") for switches.
2. **Controls**: the type's card, plus the existing Colour panel for colour lights.
3. **Settings**: the form described below, with Save and "Reset" (today's "Clear override") at the bottom.
4. **Device & diagnostics**: a `<details>` closed by default, with today's Device rows, the "Other entities" links and the State panel (value, attributes, last update) inside.

### 6.2 Settings rules

Settings are one form with small titled groups; a group with nothing to show is not rendered. Rules follow the app's `CustomizeSheet`, using the **draft's** type so the form updates as the user picks:

**General** (every entity)
- Name (with the discovered name as placeholder), Room (today's `AreaField`), Favorite, Order (number, optional, invalid input blocks Save with a message, as the app does).
- Visibility, as the app does it: if `Placement::elsewhere` for the draft (energy role set or on Security), show **Show on Home**, whose value is `homePinned && !hidden`; turning it on sets `homePinned` and clears `hidden`, turning it off clears `homePinned`. Otherwise show **Hidden** (`hidden`). Because the toggle's value already reflects `hidden`, neither `hidden` nor `homePinned` is ever listed in §6.4.

**Type** (only `protocol_kind == Switch`)
- **Show as**: chips Switch · Light · Door opener · Siren · Chime · Plug, per §5.1.

**Light** (effective kind Light)
- **Group with the room** (inverse of `ungrouped`), only when the room-group rules consider the entity groupable.
- **When turned on**: chips Resume · White · Warm · Red · Green · Blue, only for colour lights (`has_colour`). Resume clears `lightOnPreset` and `lightOnBrightness`.
- **Brightness when turned on**: today's slider, only when a preset is chosen and the light has brightness.

**Cover**: **Group with the room** when groupable.

**Security**
- **Show on Security** (inverse of `securityHidden`) when `Placement::security_kind` (alarm panels, door/motion/smoke/… binary sensors).
- **Show on Security** (`securityAction`) when `Placement::action_kind` (switch types and buttons), and **Ask before acting** (`securityAsk`) while that is on, or always for Door opener and Siren (§5.3).

**Sensor**
- **Card style**: chips Standard · Weather hero (`cardStyle` null or `"hero_weather"`).
- **Energy role**: chips None · the `ENERGY_ROLES`, with the app's labels.
- **Invert** only for signed roles (grid, battery, as the app's `EnergyRole.signed`).
- **Icon** (`energyIcon`) only for the consumer role: chips of `ConsumerIcon` with their glyphs (`icons.rs::consumer_icon`).
- **Energy counter** (`energyMeter`) when a power role is set: a select of "Automatic" plus the house's energy-counter sensors, labelled by name.
- A warning line when a power role is set but the unit is not a power unit, as the app shows.

**Climate**: **Presets** (`climatePresets`, comma-separated °C), kept as text with the hint "e.g. 19,21,23".

### 6.3 Removed from the form

Free-text Card style, Energy icon and Energy meter; every group and field the rules above do not show for this entity.

### 6.4 Values set but not shown

When the stored override has a non-default value for a field the rules hide (for example `energyRole` on a switch, written by MCP), list it under a small **Other saved settings** line at the end of Settings ("energyRole: grid") with a clear (×) button per field that sends `null`. This keeps the anti-goal in §4 without showing the full form.

## 7. Constraints

- Leptos CSR → WebAssembly, existing CSS tokens and sheet classes (`sheet__panel`, `override__*`); chips reuse the existing chip style from Home or Security.
- The editor keeps its draft model: built once from `initial`, follows outside changes only while not dirty, Save sends only `dirty_patch`.
- Rules that decide what shows (§6.2) and `switch_type` are pure functions in `oikos-core` or plain Rust in `oikos-web` with native unit tests, like `Placement`.
- Phone width works (single column, chips wrap or scroll).
- Hub change is backward compatible: an override without `showAs` reads exactly as today.

## 8. Testing

- Core: `showAs` patch validation (and `asLight` now rejected), round-trip of an unknown value, a legacy `asLight: true` reading as Light when `showAs` is absent and `showAs` winning when both are present, encoding never writing `asLight`, the migration rewriting only overrides that carry `asLight`, `switch_type` for every combination.
- Web (native tests): the per-kind visibility rules (a light has no Energy/Security/Climate groups; a power sensor gets Energy; a switch gets Type and Security; Ask appears only with Security action), the selector's patch for each choice (Light clearing `securityAction`, no type adding it), the "Other saved settings" list.
- Manual check in headless Chrome on the running hub (see the CDP screenshots memory): a light, a switch turned into each type, a power sensor, a climate, a door sensor.

## 9. Left to the builder

- Exact glyphs for door, siren and chime, and the wording of hints.
- Whether the Door opener and Chime buttons show a brief "sent" state after the echo.
- How `search` matches type words.
- Group ordering and spacing inside Settings.
- Timed auto-off (pulse) for door openers or chimes is out of scope; many Z2M relays do it in firmware ("inching"). If a device stays on, the user can add a flow.

## 10. The app (`<app-repo>`), same work

The app gets the same contract and the same type behaviour:

- `UiOverride.kt`: `showAs: String?` replaces `asLight`, with the same values; decoding still reads a legacy `asLight: true` as `"light"`, encoding never writes it. Update `OverrideDraft`, `UiOverrides` encoding and their tests.
- `MqttDiscoverySource` (kind mapping) and `SwitchAsLightMapper`: driven by the type Light instead of `asLight`.
- `CustomizeSheet`: the **Show as** chips (replacing the "Show as a light" switch row), and the Ask toggle rule of §5.3.
- Cards and Security Actions: the per-type card, icon and wording of §5.3 (Door opener Open button, Siren Ringing state, Chime Play button, Plug glyph).
- Strings in the app's existing languages.

**Order of work:** (1) hub reads and writes `showAs`, still reading `asLight`, no migration yet; (2) web sheet and cards; (3) app; (4) turn on the hub's `asLight` migration once the updated app is installed. Existing switch-as-lights keep working throughout, because every reader understands `asLight` until step 4. A switch turned into a Light from the web between steps 1 and 3 shows as a plain switch in the not-yet-updated app; acceptable, since only its author uses the app (my call).

The app's own design specs (`docs/superpowers/specs/` in that repo) get a short note pointing at this document; the app's other customize rules are already the ones §6.2 ports.
