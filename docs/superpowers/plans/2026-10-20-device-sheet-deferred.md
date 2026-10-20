# Device sheet: deferred work

Follows `2026-10-20-device-sheet.md` (plan 1 of 2) and its spec `../specs/2026-10-20-device-sheet-design.md`.

1. ~~**Plan 2: the Android app** (spec §10)~~ — landed 2026-10-20 on branch `feat/showAs-override` at `cbe43a3..fd81376` in `<app-repo>`. `UiOverride.showAs` replaces `asLight` with the legacy decode kept; Customize's Show-as chips, Door opener / Siren / Chime cards, Security wording and strings (en + fr) are in. Install the APK on the house phone to unblock step 4.
2. **Step 4: retire `asLight`** — ready to run in the hub repo once the updated app is live: rewrite every retained override that still carries `asLight` (for example the `0x3425b4fffefe0c80` switches) and drop the compatibility write in `UiOverride::encode` (`oikos-core/src/model/ui_override.rs`, marked with a comment).
3. Security tiles keep the plug / play glyph per kind; they could use the type's glyph (`icons::entity_icon`).
4. The Name field has no placeholder with the discovered name: `EntityView` only carries the effective name.
5. Final review minors: the settings rules read the entity as it was when the sheet opened (moving a light out of Unassigned shows the group toggle only after reopening); `door_asks_only_with_security_ask` tests `door_tap` alone, not its wiring; an invalid Position text survives "Reset to defaults"; Light then back to Switch leaves `securityAction` off; `kind_icon`'s doc comment sits above `entity_icon`.
