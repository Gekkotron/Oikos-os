# Device sheet: deferred work

Follows `2026-10-20-device-sheet.md` (plan 1 of 2) and its spec `../specs/2026-10-20-device-sheet-design.md`.

1. **Plan 2: the Android app** (spec §10): `showAs` in the app's `UiOverride` (legacy `asLight` read), kind mapping and `SwitchAsLightMapper`, the Show as chips in `CustomizeSheet`, the Door opener / Siren / Chime cards and Security wording.
2. **Step 4: retire `asLight`**, once the updated app is installed: rewrite every retained override that still carries `asLight` (the house has some, for example the `0x3425b4fffefe0c80` switches) and drop the compatibility write in `UiOverride::encode` (`oikos-core/src/model/ui_override.rs`, marked with a comment).
3. Security tiles keep the plug / play glyph per kind; they could use the type's glyph (`icons::entity_icon`).
4. The Name field has no placeholder with the discovered name: `EntityView` only carries the effective name.
5. Final review minors: the settings rules read the entity as it was when the sheet opened (moving a light out of Unassigned shows the group toggle only after reopening); `door_asks_only_with_security_ask` tests `door_tap` alone, not its wiring; an invalid Position text survives "Reset to defaults"; Light then back to Switch leaves `securityAction` off; `kind_icon`'s doc comment sits above `entity_icon`.
