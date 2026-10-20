# Device Sheet and "Show as" Types Implementation Plan (2 of 2: the Android app)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Repo:** `<app-repo>` (a separate git repo from Oikos-os). All paths below are relative to it.

**Goal:** The Android app reads and writes the new `showAs` override field the hub and web agreed on in Plan 1, and shows a switch as a Switch, Light, Door opener, Siren, Chime or Plug — the Customize sheet picks the type with chips, the Home card draws the type, and Security's Actions tab speaks the type's wording.

**Architecture:** `core/model` owns the contract — one `showAs: String?` on `UiOverride` (legacy `asLight: true` still reads as `showAs = "light"`), a `SwitchType` enum mirroring the hub's. `MqttDiscoverySource` promotes a switch to `Kind.LIGHT` when the type is Light, so `KindMappers.forEntity` keeps dispatching `SwitchAsLightMapper` unchanged. `OverrideDraft` and `CustomizeSheet` swap the "Show as a light" row for a Show-as chip row and drive the Ask toggle from the type. The Home layer gains three per-type card renderers (Door opener, Siren, Chime) and `IconSpec.Static` glyph choices for Plug/Door/Siren/Chime. Security's `ActionRow` carries a nullable `SwitchType`, `tap()` branches per type, and `SecurityScreen`'s AlertDialog takes its wording from the row.

**Tech Stack:** Kotlin (common + Android), kotlinx.serialization, Jetpack Compose, Material 3, Kover for coverage. JVM unit tests under `*/src/{commonTest,test,jvmTest}/`.

**Spec:** `~/superconductor/projects/Oikos-os/docs/superpowers/specs/2026-10-20-device-sheet-design.md` (step 3 of its §10 order of work). Plan 1 (hub + web) was executed on 2026-10-19/20 and is at `.../plans/2026-10-20-device-sheet.md`. Step 4 (the hub's one-shot `asLight` migration and dropping the compat write in `UiOverride::encode`) stays in Oikos-os and happens only once this plan ships and the updated app is installed.

## Global Constraints

- `SHOW_AS = ["light", "door", "siren", "chime", "plug"]`; absent means Switch; an unknown stored value round-trips and reads as Switch. `showAs` only applies when `protocolKind == Kind.SWITCH`.
- Decoding: a stored `{"asLight": true}` with no `showAs` reads as `showAs = "light"`; `asLight` is never kept as a field. When both are present, `showAs` wins. The data class has no `asLight` property.
- Encoding: `UiOverrides.encode(override)` never writes `asLight` (even for the Light type) — the hub still writes it for backward compatibility until step 4; the app does not, because the app is the only writer the user cares about and the contract is being retired.
- The sheet keeps the current draft model: it builds from the card once, lives in `rememberSaveable`, and only publishes on Save.
- Picking the Light type also clears `securityAction` in the same draft. No other type change touches any other field.
- Copy (English, en-US and the same strings in `values-fr/strings.xml` with French equivalents): type labels "Switch" / "Interrupteur", "Light" / "Lumière", "Door opener" / "Ouvre-porte", "Siren" / "Sirène", "Chime" / "Carillon", "Plug" / "Prise". Sheet section title "Show as" / "Afficher comme". Card copy — Door opener button "Open" / "Ouvrir"; confirm title "Open <name>?" / "Ouvrir <name> ?"; confirm button "Open" / "Ouvrir". Siren toggle label "Ringing" / "Sonne" when on; confirm title "Ring <name>?" / "Faire sonner <name> ?"; confirm button "Ring" / "Sonner". Chime button "Play" / "Déclencher". Every confirm has "Cancel" / "Annuler" (reuse `security_cancel`). Ask toggle label stays `customize_ask_before` ("Ask before acting" / "Demander avant").
- The retained `customize_as_light` string (and its French twin) is removed in the same commit as the chip row; grep must come back clean before Task 3's commit.
- Attribution for commits: `-c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com`, and the trailer `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`.
- Every task ends green on: `./gradlew :core:model:jvmTest :core:source:jvmTest :core:state:jvmTest :feature:home:test :feature:security:test` plus the task's own module's `lint` where it applies. Kover reports stay on the modules Plan 1 did not touch.

## Review Focus

1. **House already has `{"asLight": true}` on the wire.** The 0x3425b4fffefe0c80 switches (and others) are stored as plain `asLight: true` because step 4 is deferred. The app must still boot them as lights, both at decode (`UiOverride`) and after `MqttDiscoverySource` builds the kind. Covered by Task 1's `legacy_as_light_reads_as_light_type` and Task 2's `legacy_as_light_still_promotes_to_light`.
2. **Draft round-trip is clean.** The user opens Customize on a plain switch, picks Door opener, then picks Switch, then taps Save without touching anything else: the published override is `UiOverride()` (not `UiOverride(showAs = null)` round-tripped, not a stray `securityAsk = true`). The `applyTo` call must behave the same way the hub's web does. Covered by Task 1's `type_round_trip_is_clean`.
3. **Picking Light clears a left-over `securityAction`.** A stored action switch is turned into a Light: the published override has `securityAction = false` without the user touching the Security toggle. Covered by Task 3's `picking_light_clears_security_action` (Compose UI test).
4. **Door opener on Home with `securityAsk = true` asks first; `false` sends `TurnOn` at once.** Siren toggles off without asking (same rule the Security Actions tab already carries). Covered by Task 4's `door_tap` and `siren_tap` tests.
5. **French locale carries every new string.** A `values-fr/strings.xml` without the Show-as labels would show the resource name on screen. Covered by Task 3's `localized_strings_present` test (reads both XMLs and asserts the key set matches).

---

### Task 1: `showAs` in `UiOverride` and `OverrideDraft`

**Files:**
- Modify: `core/model/src/commonMain/kotlin/com/gekkotron/oikos/model/UiOverride.kt` (field, `SHOW_AS` constant, custom serializer that reads legacy `asLight` and writes nothing but `showAs`, `SwitchType` enum, retire the `asLight` property)
- Create: `core/model/src/commonTest/kotlin/com/gekkotron/oikos/model/SwitchTypeTest.kt`
- Modify: `core/model/src/commonTest/kotlin/com/gekkotron/oikos/model/UiOverrideTest.kt` (if present; otherwise add it next to the existing tests — grep for one)
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/presentation/OverrideDraft.kt` (field, `applyTo`, `from`)
- Modify: `feature/home/src/test/kotlin/com/gekkotron/oikos/feature/home/presentation/OverrideDraftTest.kt` (the two `asLight` tests become `showAs`; add the round-trip test)

**Interfaces:**
- Produces: `UiOverride.showAs: String?` (nullable; `null` = Switch). No `asLight` property. `pub val SHOW_AS: List<String>` top-level in the same file: `["light","door","siren","chime","plug"]`.
- Produces: `enum class SwitchType { SWITCH, LIGHT, DOOR, SIREN, CHIME, PLUG }` with
  - `val key: String?` — `null` for `SWITCH`, lowercase enum name otherwise;
  - `companion object { fun of(protocolKind: Kind, ui: UiOverride): SwitchType? }` — returns `null` unless `protocolKind == Kind.SWITCH`; `UiOverride.showAs == null` → `SWITCH`; `showAs` not in `SHOW_AS` → `SWITCH`;
  - `fun effectiveKind(protocolKind: Kind, ui: UiOverride): Kind` top-level — `Kind.LIGHT` for `LIGHT`, else `protocolKind`.
- Produces: `OverrideDraft.showAs: String?` replaces `asLight`, defaulting to `null`. `applyTo` sets `showAs = showAs` on `UiOverride.copy`. `from(card)` reads `card.ui.showAs`.

- [ ] **Step 1: Write the failing tests**

In the model's unit tests (same package as `UiOverride`):

```kotlin
@Test fun `legacy as_light reads as light type`() {
    val ui = UiOverrides.decode("""{"name":"Lumiere tv","asLight":true}""")!!
    assertEquals("light", ui.showAs)

    val off = UiOverrides.decode("""{"asLight":false}""")!!
    assertNull(off.showAs)
    assertEquals(UiOverride(), off)

    val both = UiOverrides.decode("""{"asLight":true,"showAs":"siren"}""")!!
    assertEquals("siren", both.showAs)
}

@Test fun `encode never writes as_light`() {
    val light = UiOverride(showAs = "light")
    val wire = UiOverrides.encode(light)
    assertFalse(wire.contains("asLight"))
    assertTrue(wire.contains("\"showAs\":\"light\""))
}

@Test fun `unknown show_as round trips but reads as switch`() {
    val ui = UiOverrides.decode("""{"showAs":"fan"}""")!!
    assertEquals("fan", ui.showAs)
    val wire = UiOverrides.encode(ui)
    assertTrue(wire.contains("\"showAs\":\"fan\""))
}
```

In `SwitchTypeTest.kt`:

```kotlin
@Test fun `of returns null for non-switch kinds`() {
    assertNull(SwitchType.of(Kind.LIGHT, UiOverride(showAs = "light")))
    assertNull(SwitchType.of(Kind.COVER, UiOverride()))
}

@Test fun `of maps every known value`() {
    assertEquals(SwitchType.SWITCH, SwitchType.of(Kind.SWITCH, UiOverride())!!)
    assertEquals(SwitchType.LIGHT, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "light"))!!)
    assertEquals(SwitchType.DOOR, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "door"))!!)
    assertEquals(SwitchType.SIREN, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "siren"))!!)
    assertEquals(SwitchType.CHIME, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "chime"))!!)
    assertEquals(SwitchType.PLUG, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "plug"))!!)
    assertEquals(SwitchType.SWITCH, SwitchType.of(Kind.SWITCH, UiOverride(showAs = "fan"))!!)
}

@Test fun `effective kind promotes only the light type`() {
    assertEquals(Kind.LIGHT, effectiveKind(Kind.SWITCH, UiOverride(showAs = "light")))
    assertEquals(Kind.SWITCH, effectiveKind(Kind.SWITCH, UiOverride(showAs = "door")))
    assertEquals(Kind.COVER, effectiveKind(Kind.COVER, UiOverride(showAs = "light")))
}

@Test fun `keys`() {
    assertEquals(listOf(null, "light", "door", "siren", "chime", "plug"), SwitchType.entries.map { it.key })
}
```

In `OverrideDraftTest.kt`, rewrite the two `asLight` tests to `showAs = "light"` on both sides, add:

```kotlin
@Test fun `type round trip is clean`() {
    val plain = testEntity("sw", Kind.SWITCH, "Switch", ui = UiOverride()).copy(protocolKind = Kind.SWITCH)
    val card = card(plain)
    val draft = OverrideDraft.from(card).copy(showAs = "door").copy(showAs = null)
    assertEquals(UiOverride(), draft.applyTo(card))
}

@Test fun `picking light clears a leftover security action`() {
    val action = testEntity("sw", Kind.SWITCH, "Switch", ui = UiOverride(securityAction = true)).copy(protocolKind = Kind.SWITCH)
    val card = card(action)
    val draft = OverrideDraft.from(card).copy(showAs = "light", securityAction = false)
    assertEquals(UiOverride(showAs = "light"), draft.applyTo(card))
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `./gradlew :core:model:jvmTest :feature:home:test --tests "*UiOverride*" --tests "*SwitchType*" --tests "*OverrideDraft*"`
Expected: compile errors on `showAs` / `SwitchType.of` / the retired `asLight` symbol.

- [ ] **Step 3: Implement**

- `UiOverride`: delete the `asLight: Boolean = false` field. Add `val showAs: String? = null` in the same slot (keep the data-class parameter order stable: name, area, hidden, order, cardStyle, favorite, energyRole, energyInvert, securityHidden, energyIcon, energyMeter, homePinned, securityAction, securityAsk, ungrouped, **showAs**, lightOnPreset, lightOnBrightness, climatePresets). Write a KDoc line pointing at the spec's §5.1.
- Legacy decoding: `UiOverrides.decode` reads the payload once into a `JsonObject` (via `OikosJson.parseToJsonElement`); if `showAs` is absent and `asLight` is `true`, inject `"showAs":"light"`; drop any `asLight` key; then feed back through `OikosJson.decodeFromJsonElement(UiOverride.serializer(), ...)`. On any failure, keep the current `getOrNull()` return. This keeps the data class free of a custom serializer and keeps `UiOverrides.encode(UiOverrides.decode(x)!!)` idempotent for every payload the house holds today.
- `encode`: unchanged — the derived serializer skips a null `showAs` because the data class defaults it to `null` and `OikosJson` is configured to drop defaults in this module (verify: `grep encodeDefaults core/model/src/commonMain/kotlin/com/gekkotron/oikos/model/OikosJson.kt` — adjust the config if `encodeDefaults = true`; the "never writes `asLight`" test pins it either way).
- Add the top-level `SHOW_AS`, the `SwitchType` enum and the `effectiveKind` function in the same file (one file keeps the contract together, matching `LightOnPreset` and `ConsumerIcon`).
- `OverrideDraft`: replace `val asLight: Boolean = false` with `val showAs: String? = null`. In `applyTo`, pass it through on `ui.copy(..., showAs = showAs, ...)`. In `from`, read `card.ui.showAs`. Keep the data-class order so Compose `rememberSaveable` keys do not shift unexpectedly.

- [ ] **Step 4: Run the module tests and clippy equivalents**

Run: `./gradlew :core:model:jvmTest :feature:home:test`
Expected: all pass.

- [ ] **Step 5: Commit**

```
feat(model): showAs override type replaces asLight
```

---

### Task 2: Kind promotion from `showAs`

**Files:**
- Modify: `core/source/src/jvmMain/kotlin/com/gekkotron/oikos/source/MqttDiscoverySource.kt:411` (the `ui.asLight` check becomes `effectiveKind(d.entity.protocolKind, ui) == Kind.LIGHT`, importing `effectiveKind` and `Kind` from `com.gekkotron.oikos.model`)
- Modify: `core/source/src/jvmTest/kotlin/com/gekkotron/oikos/source/MqttDiscoverySourceTest.kt` (the two `asLight` tests + a new legacy test)
- Modify: `core/state/src/commonTest/kotlin/com/gekkotron/oikos/state/kinds/SwitchAsLightMapperTest.kt` (any `asLight = true` fixture becomes `showAs = "light"` — the mapper's own logic is unchanged, `KindMappers.forEntity` already dispatches on `protocolKind == SWITCH && kind == LIGHT`)
- Modify: `feature/home/src/test/kotlin/com/gekkotron/oikos/feature/home/presentation/RoomGroupsTest.kt:182` (the `asLight = true` fixture becomes `showAs = "light"`)

**Interfaces:**
- Consumes: `effectiveKind`, `SwitchType`, `UiOverride.showAs` (Task 1).
- Produces: no new public types. The behaviour of `KindMappers.forEntity` is unchanged, because it dispatches on `protocolKind == SWITCH && kind == LIGHT` and `MqttDiscoverySource` now sets `kind = LIGHT` on every switch the type promotes.

- [ ] **Step 1: Write the failing tests**

In `MqttDiscoverySourceTest.kt`, rename `a switch with asLight is a light that still speaks switch payloads` to `a switch with showAs light is a light`, keep its body but publish `{"showAs":"light"}` instead; keep the `asLight leaves a real light alone` test but rewrite the payload to `{"showAs":"light"}`. Add:

```kotlin
@Test fun `legacy as_light still promotes to light`() = runTest {
    val h = fakeHarness()
    h.transport.deliver(discoveryTopicFor("a"), discoveryJson("a", Kind.SWITCH))
    h.transport.deliver("oikos/ui/a", """{"asLight":true}""")
    advanceUntilIdle()
    assertEquals(Kind.LIGHT, h.source.entities.value.getValue("a").kind)
    assertEquals("light", h.source.entities.value.getValue("a").ui.showAs)
}

@Test fun `showAs door keeps a switch as switch`() = runTest {
    val h = fakeHarness()
    h.transport.deliver(discoveryTopicFor("a"), discoveryJson("a", Kind.SWITCH))
    h.transport.deliver("oikos/ui/a", """{"showAs":"door"}""")
    advanceUntilIdle()
    assertEquals(Kind.SWITCH, h.source.entities.value.getValue("a").kind)
}
```

(Use the test file's existing `fakeHarness`, `discoveryTopicFor`, `discoveryJson` helpers — grep for them if the names differ.)

- [ ] **Step 2: Run to verify failure**

Run: `./gradlew :core:source:jvmTest --tests "*MqttDiscoverySourceTest*"`
Expected: FAIL on the renamed tests until Step 3 lands.

- [ ] **Step 3: Implement**

Replace the inline `if (d.entity.kind == Kind.SWITCH && ui.asLight) named.copy(kind = Kind.LIGHT) else named` with
`named.copy(kind = effectiveKind(d.entity.protocolKind, ui))` — this covers Light (promotion) and the other types (unchanged `Kind.SWITCH`) in one line.

- [ ] **Step 4: Run the module tests**

Run: `./gradlew :core:source:jvmTest :core:state:jvmTest :feature:home:test`
Expected: all pass. `SwitchAsLightMapperTest` and `RoomGroupsTest` reach it with their new fixtures.

- [ ] **Step 5: Commit**

```
feat(source): promote switch to light by effective kind
```

---

### Task 3: Show-as chips in `CustomizeSheet`

**Files:**
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/ui/CustomizeSheet.kt` (replace the `SwitchRow(customize_as_light, …)` row with a chip row; drive the Ask toggle from the type; clear `securityAction` when the user picks Light)
- Modify: `feature/home/src/main/res/values/strings.xml` (add Show-as labels and the type labels; delete `customize_as_light`)
- Modify: `feature/home/src/main/res/values-fr/strings.xml` (same, French)
- Create: `feature/home/src/test/kotlin/com/gekkotron/oikos/feature/home/ui/CustomizeSheetShowAsTest.kt` (Robolectric or Compose UI test — look at the pattern an existing `*Test` file in `feature/home/src/test/` already uses; if the module runs no Compose UI tests, assert through the draft instead by extracting the chip-row's state transition into a small `fun pickType(draft: OverrideDraft, type: SwitchType?): OverrideDraft` helper tested natively)
- Create: `feature/home/src/test/kotlin/com/gekkotron/oikos/feature/home/ui/StringParityTest.kt`

**Interfaces:**
- Consumes: `SwitchType`, `SHOW_AS` (Task 1).
- Produces: a `pickType(draft: OverrideDraft, type: SwitchType?): OverrideDraft` top-level function in `CustomizeSheet.kt`'s file (visible to the test). `null` → `showAs = null` (Switch); `LIGHT` → `showAs = "light", securityAction = false`; else → `showAs = type.key`.

- [ ] **Step 1: Write the failing tests**

```kotlin
// CustomizeSheetShowAsTest.kt
@Test fun `picking door writes the key`() {
    val d = baseDraft(showAs = null)
    assertEquals("door", pickType(d, SwitchType.DOOR).showAs)
}

@Test fun `picking switch clears show_as`() {
    val d = baseDraft(showAs = "door")
    assertNull(pickType(d, SwitchType.SWITCH).showAs)
}

@Test fun `picking light clears security action`() {
    val d = baseDraft(showAs = null, securityAction = true, securityAsk = true)
    val next = pickType(d, SwitchType.LIGHT)
    assertEquals("light", next.showAs)
    assertFalse(next.securityAction)
    assertTrue(next.securityAsk)  // only securityAction clears; the preference stays
}

@Test fun `picking door keeps security action`() {
    val d = baseDraft(showAs = null, securityAction = true)
    assertTrue(pickType(d, SwitchType.DOOR).securityAction)
}
```

```kotlin
// StringParityTest.kt
@Test fun `localized strings present`() {
    val en = parse("feature/home/src/main/res/values/strings.xml")
    val fr = parse("feature/home/src/main/res/values-fr/strings.xml")
    val required = setOf(
        "customize_show_as",
        "customize_show_as_switch",
        "customize_show_as_light",
        "customize_show_as_door",
        "customize_show_as_siren",
        "customize_show_as_chime",
        "customize_show_as_plug",
    )
    assertTrue(required.all { it in en }, "English missing: ${required - en.keys}")
    assertTrue(required.all { it in fr }, "French missing: ${required - fr.keys}")
    assertFalse("customize_as_light" in en, "customize_as_light should be removed")
    assertFalse("customize_as_light" in fr, "customize_as_light should be removed")
}
```

(`parse` is a tiny helper reading the XML into a `Map<String, String>`; look for an existing one in `feature/home/src/test/` — the module already reads XML in at least one test, grep `strings.xml` under `src/test`. If none exists, write a 10-line one in the file.)

- [ ] **Step 2: Run to verify failure**

Run: `./gradlew :feature:home:test --tests "*CustomizeSheetShowAs*" --tests "*StringParity*"`
Expected: FAIL — strings missing, `pickType` undefined.

- [ ] **Step 3: Implement**

In `CustomizeSheet.kt`:
- Add `pickType(draft, type)` as described in Interfaces.
- Replace line 83 (`var asLight by …`) with `var showAs by rememberSaveable(card.id) { mutableStateOf(start.showAs) }`.
- Rebuild the `draft` on line 90 with `showAs = showAs` instead of `asLight`.
- Delete the `if (card.protocolKind == Kind.SWITCH) SwitchRow(…customize_as_light…)` line (117). Insert, at the same spot, a chip row driven by `SwitchType.entries` (SWITCH first), filter-chip style like the existing `ChipRow` block, each chip's `onClick` calling a local
  `fun apply(t: SwitchType) { val next = pickType(currentDraft(), t); showAs = next.showAs; securityAction = next.securityAction }`,
  where `currentDraft()` builds an `OverrideDraft` from the current `by` vars (extract this to one line, since it is already assembled on line 90). The chip's `selected` compares the stored `showAs` to the chip's `type.key`.
- The Ask row (line 134): keep today's `securityAction` branch, and OR an additional branch that shows the Ask toggle when the current type is Door or Siren (`SwitchType.of(card.protocolKind, draft).let { it == SwitchType.DOOR || it == SwitchType.SIREN }`). The toggle writes to the same `securityAsk` state.

In `strings.xml` (both locales):
- Remove `customize_as_light`.
- Add `customize_show_as`, `customize_show_as_switch`, `customize_show_as_light`, `customize_show_as_door`, `customize_show_as_siren`, `customize_show_as_chime`, `customize_show_as_plug`. Copy per the Global Constraints.

Grep check before commit: `rg -n "customize_as_light|R.string.customize_as_light|\.asLight\b" feature/ core/` must come back empty.

- [ ] **Step 4: Run tests and the module checks**

Run: `./gradlew :feature:home:test :feature:home:lint`
Expected: pass.

- [ ] **Step 5: Commit**

```
feat(home): customize sheet picks the switch type with chips
```

---

### Task 4: Door opener, Siren and Chime cards on Home

**Files:**
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/presentation/HomeUiBuilder.kt` (tap-for and the "on" text per type)
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/presentation/HomeViewModel.kt` (TOGGLE branch asks first when the type is Door or Siren with `securityAsk`, else sends `TurnOn` outright for Door/Chime, else today's toggle)
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/ui/KindIcon.kt` (`iconSpec` for `Kind.SWITCH`: dispatch on `SwitchType.of` for the glyph)
- Modify: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/ui/HomeScreen.kt` (hoist a `confirmId` + `AlertDialog`, mirroring `SecurityScreen.kt:258-273`'s pattern, driven by `HomeEvent.AskBeforeAction` emitted from the view model)
- Create: `feature/home/src/main/kotlin/com/gekkotron/oikos/feature/home/presentation/CardAction.kt` (`fun cardTap(type: SwitchType?, on: Boolean, ask: Boolean): CardTap` and the `CardTap` sealed interface — `Send(command)` and `Confirm(command, question)`)
- Create: `feature/home/src/test/kotlin/com/gekkotron/oikos/feature/home/presentation/CardActionTest.kt`
- Modify: `core/designsystem/src/main/kotlin/com/gekkotron/oikos/designsystem/icons/StaticIcon.kt` (add two glyph entries: `SIREN` and `CHIME`. Builder's call for the exact SVG; starting points: `SIREN` can be a dome over a cone, `CHIME` is a bell or a music note. If the builder wants to defer the real art, point them at the existing `BULB`-style simple paths. A third option is to reuse `OikosGlyph.PLAY` for Chime, since the sheet already treats it as "Play" — do that if the SVG work is not wanted in this pass. Record whichever choice in a one-line comment.)

**Interfaces:**
- Consumes: `SwitchType` (Task 1).
- Produces:

```kotlin
sealed interface CardTap {
    data class Send(val command: Command) : CardTap
    data class Confirm(val command: Command, val labelResId: Int) : CardTap  // "Open <name>?" picked at the UI
    data object None : CardTap
}

fun cardTap(type: SwitchType?, on: Boolean, ask: Boolean): CardTap
```

`cardTap` rules:
- `null` or `SwitchType.SWITCH` or `SwitchType.PLUG`: today's TOGGLE (`Send(if (on) TurnOff else TurnOn())`).
- `LIGHT`: unreachable from the Home tap path (the entity reaches HomeUiBuilder with `kind == LIGHT`); return `None`.
- `DOOR`: always `TurnOn`; `Confirm(..., R.string.card_confirm_open)` when `ask`, else `Send`.
- `SIREN`: `on` → `Send(TurnOff)`; `off` → `Confirm(TurnOn(), R.string.card_confirm_ring)` when `ask`, else `Send(TurnOn())`.
- `CHIME`: always `Send(TurnOn())`, no confirm.

- [ ] **Step 1: Write the failing tests**

```kotlin
// CardActionTest.kt
@Test fun `door tap asks only with security ask`() {
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.DOOR, on = false, ask = false))
    assertEquals(
        CardTap.Confirm(Command.TurnOn(), R.string.card_confirm_open),
        cardTap(SwitchType.DOOR, on = false, ask = true),
    )
    // The relay reads "on" meanwhile — the user tap still means Open.
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.DOOR, on = true, ask = false))
}

@Test fun `siren tap toggles`() {
    assertEquals(CardTap.Send(Command.TurnOff), cardTap(SwitchType.SIREN, on = true, ask = true))
    assertEquals(
        CardTap.Confirm(Command.TurnOn(), R.string.card_confirm_ring),
        cardTap(SwitchType.SIREN, on = false, ask = true),
    )
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.SIREN, on = false, ask = false))
}

@Test fun `chime always plays`() {
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.CHIME, on = false, ask = true))
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.CHIME, on = true, ask = false))
}

@Test fun `plain switch and plug keep the toggle`() {
    assertEquals(CardTap.Send(Command.TurnOff), cardTap(SwitchType.SWITCH, on = true, ask = true))
    assertEquals(CardTap.Send(Command.TurnOn()), cardTap(SwitchType.PLUG, on = false, ask = true))
    assertEquals(CardTap.Send(Command.TurnOff), cardTap(null, on = true, ask = true))
}
```

- [ ] **Step 2: Run to verify failure**

Run: `./gradlew :feature:home:test --tests "*CardActionTest*"`
Expected: FAIL — `cardTap` undefined.

- [ ] **Step 3: Implement**

- Write `CardAction.kt` to match the Interfaces block.
- In `HomeViewModel.kt:132` TOGGLE branch, compute `val tap = cardTap(SwitchType.of(card.protocolKind, card.ui), card.on, card.ui.securityAsk)` and dispatch: `Send` goes through the same `send(card.id, command)` path; `Confirm` emits a new `HomeEvent.AskBeforeAction(cardId, question, command)` for the UI to pick up.
- In `HomeScreen.kt`, add `var confirmTap by rememberSaveable { mutableStateOf<HomeEvent.AskBeforeAction?>(null) }`, collect the event into it, render an `AlertDialog` mirroring `SecurityScreen.kt:260`'s shape (title from `question`'s resource with `name` as the argument, confirm button "Open"/"Ring", cancel button `security_cancel`). Confirm button calls `onSend(cardId, command)` and clears the state.
- In `KindIcon.kt:68`, replace the single `Kind.SWITCH -> IconSpec.Static(OikosGlyph.PLUG, card.on)` branch with a `when` on `SwitchType.of(card.protocolKind, card.ui)`:
  ```kotlin
  Kind.SWITCH -> when (SwitchType.of(card.protocolKind, card.ui)) {
      SwitchType.DOOR -> IconSpec.Swing(SwingShape.DOOR, open = card.on.takeIf { card.available })
      SwitchType.SIREN -> IconSpec.Static(OikosGlyph.SIREN, card.on)  // or BOLT if the glyph is deferred
      SwitchType.CHIME -> IconSpec.Static(OikosGlyph.CHIME, card.on)  // or PLAY if deferred
      SwitchType.PLUG, SwitchType.SWITCH, null -> IconSpec.Static(OikosGlyph.PLUG, card.on)
      SwitchType.LIGHT -> error("promoted to LIGHT kind")
  }
  ```
- Add the new strings: `card_confirm_open` ("Open <name>?" / "Ouvrir <name> ?"), `card_confirm_ring` ("Ring <name>?" / "Faire sonner <name> ?"), `card_action_open` ("Open" / "Ouvrir"), `card_action_ring` ("Ring" / "Sonner") in both XMLs. The AlertDialog's confirm uses `card_action_open` or `card_action_ring`.

- [ ] **Step 4: Run tests and the module checks**

Run: `./gradlew :feature:home:test :feature:home:lint`
Expected: pass.

- [ ] **Step 5: Commit**

```
feat(home): door opener, siren and chime cards
```

---

### Task 5: Security Actions speak the type

**Files:**
- Modify: `feature/security/src/main/kotlin/com/gekkotron/oikos/feature/security/presentation/SecurityModels.kt` (add `switchType: SwitchType?` to `ActionRow`; rewrite `fun ActionRow.tap()`)
- Modify: `feature/security/src/main/kotlin/com/gekkotron/oikos/feature/security/presentation/SecurityUiBuilder.kt:55` (fill `switchType`)
- Modify: `feature/security/src/main/kotlin/com/gekkotron/oikos/feature/security/ui/ActionTile.kt` (glyph per type: Door → `OikosGlyph.PLUG` or the chosen `DOOR` glyph via `DoorIcon`, Siren → `SIREN`, Chime → `PLAY`, Plug → today's `PLUG`)
- Modify: `feature/security/src/main/kotlin/com/gekkotron/oikos/feature/security/ui/SecurityScreen.kt:258-273` (the AlertDialog title and confirm button pick the strings from the row's `switchType`)
- Modify: `feature/security/src/main/res/values/strings.xml` and `.../values-fr/strings.xml` (new `security_action_confirm_open`, `security_action_confirm_ring`, `security_action_confirm_play`, and the button labels `security_action_open`, `security_action_ring`, `security_action_play`)
- Modify: `feature/security/src/test/kotlin/com/gekkotron/oikos/feature/security/presentation/SecurityUiBuilderTest.kt` (new tests)

**Interfaces:**
- Consumes: `SwitchType` (Task 1).
- Produces: `ActionRow.switchType: SwitchType?` (nullable: present only when `kind == Kind.SWITCH`, matches `SwitchType.of`).

- [ ] **Step 1: Write the failing tests**

```kotlin
// SecurityUiBuilderTest.kt
@Test fun `door action tap is always open`() {
    val row = row(kind = Kind.SWITCH, showAs = "door", on = true, ask = true)
    assertEquals(ActionTap(Command.TurnOn(), confirm = true), row.tap())
    val quiet = row.copy(ask = false)
    assertEquals(ActionTap(Command.TurnOn(), confirm = false), quiet.tap())
}

@Test fun `siren action tap toggles off without ask`() {
    val on = row(kind = Kind.SWITCH, showAs = "siren", on = true, ask = true)
    assertEquals(ActionTap(Command.TurnOff, confirm = false), on.tap())
    val off = on.copy(on = false)
    assertEquals(ActionTap(Command.TurnOn(), confirm = true), off.tap())
}

@Test fun `chime action always plays`() {
    val row = row(kind = Kind.SWITCH, showAs = "chime", on = null, ask = true)
    assertEquals(ActionTap(Command.TurnOn(), confirm = false), row.tap())
}

@Test fun `plain switch tap unchanged`() {
    val row = row(kind = Kind.SWITCH, showAs = null, on = true, ask = true)
    assertEquals(ActionTap(Command.TurnOff, confirm = false), row.tap())
}
```

(`row(...)` is a tiny builder — add it next to the test's existing fakes, filling `switchType` with `SwitchType.of(Kind.SWITCH, UiOverride(showAs = showAs))`.)

- [ ] **Step 2: Run to verify failure**

Run: `./gradlew :feature:security:test --tests "*SecurityUiBuilderTest*"`
Expected: FAIL — new fields and tap rules not yet in place.

- [ ] **Step 3: Implement**

- `ActionRow`: add `val switchType: SwitchType?` as the last constructor parameter (default `null` keeps existing fakes compiling, but update the fakes in the same commit).
- `ActionRow.tap()`: branch on `switchType` first — `DOOR` → `ActionTap(TurnOn(), confirm = ask)`; `SIREN` → `on == true` → `ActionTap(TurnOff, false)` else `ActionTap(TurnOn(), ask)`; `CHIME` → `ActionTap(TurnOn(), false)`; `PLUG`, `SWITCH`, `null` → the current switch rule (on → `TurnOff` no confirm, off → `TurnOn()` with `confirm = ask`); `LIGHT` is unreachable (lights are not in Security Actions) — `error("light is not an action")`.
- `SecurityUiBuilder` line 55: pass `switchType = SwitchType.of(e.protocolKind, e.ui)`.
- `ActionTile.kt`: replace the hard-coded `OikosGlyph.PLUG` with `glyphFor(row.kind, row.switchType)` — a small private helper that returns the Chime glyph (or `PLAY`), Siren glyph (or `BOLT`), the Plug glyph for `null`/`PLUG`/`SWITCH`, and today's `OikosGlyph.PLAY` for `Kind.BUTTON`.
- `SecurityScreen.kt:258-273`: the title and confirm button pick resources from `confirmRow.switchType`:
  - Door → `security_action_confirm_open` / `security_action_open`;
  - Siren → `security_action_confirm_ring` / `security_action_ring`;
  - Chime is unreachable here because its tap carries `confirm = false`, so leave the SWITCH branch covering it, but add it to the glyph helper anyway;
  - everything else keeps today's `security_action_confirm_on` / `security_action_turn_on`.

- [ ] **Step 4: Run tests and the module checks**

Run: `./gradlew :feature:security:test :feature:security:lint :core:source:jvmTest`
Expected: pass. (`core:source:jvmTest` catches ActionRow changes that leak into fakes it shares via tests.)

- [ ] **Step 5: Commit**

```
feat(security): action tap speaks the switch type
```

---

### Task 6: End-to-end sweep and the house's switches

**Files:**
- Modify: `README.md` — the override field list (if it names `asLight`, replace with `showAs`; mention the type values).
- Modify: `docs/superpowers/specs/2026-10-20-device-sheet-design.md` in the hub repo — add a one-line note at the top of §10 ("App Plan 2 landed <commit sha> on <date>") so a reader of the spec knows the step 4 cleanup is unblocked.

**Interfaces:** none new.

- [ ] **Step 1: Run every workspace check from a clean state**

Run: `./gradlew clean :core:model:jvmTest :core:source:jvmTest :core:state:jvmTest :feature:home:test :feature:security:test :feature:home:lint :feature:security:lint`
Expected: all pass. If a lint rule flags a now-dead `R.string.customize_as_light` reference anywhere, remove it in the same session.

- [ ] **Step 2: Grep for stragglers**

Run: `rg -n "asLight|customize_as_light" --type kotlin --type xml -g '!**/build/**'`
Expected: only `UiOverrides.decode`'s legacy-reading JSON path (the private helper that injects `showAs:"light"` when it sees `asLight:true`). Every call site, test and resource must be gone.

- [ ] **Step 3: Build the APK and install it on the house phone**

Run: `./gradlew :app:assembleDebug` and sideload the APK (or push a release through the usual Play lane if the user prefers). Open the app on the house phone against the live hub (:8081): a few checks, one line each:
  - A switch-as-light that still carries `asLight: true` on the broker (e.g. 0x3425b4fffefe0c80 — see memory `project_hub_plan_execution_state.md` for the full list) opens Home as a light with the bulb glyph.
  - A plain switch, long-press → Customize → Show as → Door opener → Save: the Home card becomes an "Open" button; a first tap shows the "Open <name>?" dialog; confirming sends the open; the broker's retained override reads `{"showAs":"door"}`.
  - Same device → Show as → Switch → Save: the Home card is back to the toggle; the broker's retained override reads `{}`.
  - A switch set to Siren: tapping on sends after confirm, tapping off sends `TurnOff` with no confirm.
  - Security Actions: a Door opener action's confirm dialog reads "Open <name>?" with an "Open" confirm button.
  - French locale: switch the phone to French and open Customize on a switch — every Show-as chip reads French, and the Door opener confirm reads "Ouvrir <name> ?".

- [ ] **Step 4: Commit the docs**

```
docs: app plan 2 landed; step 4 unblocks
```

- [ ] **Step 5: Record the follow-up**

In the hub repo's `docs/superpowers/plans/2026-10-20-device-sheet-deferred.md`, mark item 1 ("Plan 2: the Android app") as landed with the merge sha, and note that step 4 (the `asLight` migration + dropping the compat write in `UiOverride::encode`) is now ready to run in the hub repo.
