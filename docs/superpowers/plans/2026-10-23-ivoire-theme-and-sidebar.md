# Ivoire theme, sidebar shell and FR/EN Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Warm Minimal with the Ivoire theme, replace the top bar with a left sidebar (drawer on phone) for every theme, give rooms their own URL, and add an FR/EN string layer used by the shell.

**Architecture:** Themes stay pure CSS custom properties switched by `data-theme`; a new `i18n` module provides a `Lang` signal and a compile-checked `Key` table. A new `shell` module owns the sidebar, the content header (breadcrumb, theme and language buttons) and the phone drawer; its routing logic (active entry, breadcrumb) is pure and unit-tested. Home's room chips go away: the house overview lists every room, and `/rooms/:id` shows one room through a shared `RoomSection`.

**Tech Stack:** Rust, Leptos 0.8 (CSR, wasm32), leptos_router, Trunk, wasm-bindgen-test (headless Chrome), plain CSS.

**Spec:** `docs/superpowers/specs/2026-10-23-ivoire-theme-and-sidebar-design.md`

## Global Constraints

- Web crate only: `crates/oikos-web`. No hub, API or Android change.
- A theme changes only the look. No layout or feature may depend on the theme.
- Themes: Night Glass, Ivoire, Auto. Stored values `"night" | "ivoire" | "auto"`; `"warm"` is read as Ivoire.
- Every text colour in Ivoire passes WCAG AA (4.5:1) on `--bg` and `--card-bg`. `#c9973d` is never a text colour.
- Fonts are bundled locally under `assets/fonts/` (no third-party font host at runtime), OFL licence, noted in `styles/fonts.css`.
- Storage access (`localStorage`) is fallible and silent, as in `src/theme.rs`.
- Every label rendered by the shell goes through `i18n`. User data (room, dashboard, flow, entity names) is never translated. Other screens stay English (project 5).
- With no `Lang` in context, text is English, so existing tests keep their English assertions.
- Generic project: no house-specific names, hosts or IPs in code, tests, fixtures or docs.
- Before each commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p oikos-web`, and from `crates/oikos-web`: `wasm-pack test --headless --chrome`.
- Commits as `Gekkotron <60887050+Gekkotron@users.noreply.github.com>` (`git -c user.name=… -c user.email=… commit`), message ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A browser whose saved theme is `"warm"` opens in Ivoire, not Auto — pinned in Task 1 (`parses_known_values`).
2. Reloading `/rooms/<id>` for a room that no longer exists (renamed away, deleted) shows the "This room is gone." state, not a blank page or a panic — pinned in Task 6 (`room_route_unknown_room`).
3. The phone drawer is open and the user taps a link: the page changes and the drawer closes; Escape also closes it — pinned in Task 7 (`drawer_closes_on_navigation_and_escape`).
4. A house with only the built-in house dashboard shows "Vue d'ensemble" without an unfolded list; adding a second shows both — pinned in Task 4 (`dashboards_unfold_from_two`).
5. Opening a flow on a desktop: the canvas starts right of the sidebar and below the content header, never under the sidebar — pinned by the offset test in Task 5 (`canvas_inset_clears_the_sidebar`) and checked visually in Task 8.

---

## File Structure

| File | Responsibility |
|---|---|
| `styles/tokens.css` (modify) | Night Glass `:root`, one Ivoire block applied by a shared selector list, new tokens in both |
| `styles/fonts.css`, `assets/fonts/` (modify) | Drop Fraunces, add Manrope 600/700 |
| `src/theme.rs` (modify) | `Theme::Ivoire`, `"warm"` alias, `next()`, token parity + contrast tests |
| `src/i18n.rs` (create) | `Lang`, `install_lang`, `Key`, `text`, `use_text` |
| `src/shell/mod.rs` (create) | `Shell` component: sidebar + content column + drawer state |
| `src/shell/sidebar.rs` (create) | Sidebar markup: entries, dashboards list, rooms, Maison section, footer |
| `src/shell/header.rs` (create) | Content header: ☰, breadcrumb, theme button, language switch |
| `src/shell/route.rs` (create) | Pure: `Section`, `section_for`, `Crumb`, `crumbs` |
| `src/dashboards/picker.rs` (delete) | Replaced by the sidebar's inline list; `create_dashboard` moves to `src/dashboards/mod.rs` |
| `src/app.rs` (modify) | Uses `Shell`, adds `/rooms/:id`, installs `Lang` |
| `src/layout.rs` (modify) | `top_offset()` measures `.content-header`; `left_offset()` measures `.sidebar` |
| `src/rooms.rs` (create) | Pure room list shared by sidebar and overview (moved from `home.rs`) |
| `src/screens/home.rs` (modify) | Overview: every room as a section, no chips, no `HomeRoom` |
| `src/screens/room.rs` (create) | `RoomScreen` for `/rooms/:id` |
| `src/nav.rs` (modify) | `HomeRoom` and `home_room_for` removed |
| `src/screens/group_sheet_v2.rs` (modify) | Stops setting `HomeRoom` |
| `src/components/status_pill.rs` (modify) | Translated text |
| `src/screens/settings.rs` (modify) | ThemeSwitch shows Ivoire |
| `styles/main.css` (modify) | `.topbar*` → `.sidebar*`, `.content-header*`, drawer, overview sections |
| `docs/users/web-ui.md` (modify) | Themes, sidebar, language, rooms |

---

### Task 1: Ivoire theme and tokens

**Files:**
- Modify: `crates/oikos-web/src/theme.rs`, `crates/oikos-web/src/screens/settings.rs:121-140`, `crates/oikos-web/styles/tokens.css`, `crates/oikos-web/styles/fonts.css`, `crates/oikos-web/Cargo.toml` (web-sys features, if needed)
- Add: `crates/oikos-web/assets/fonts/Manrope-SemiBold.woff2`, `Manrope-Bold.woff2`; delete `Fraunces-Medium.woff2`

**Interfaces:**
- Produces: `Theme::{Night, Ivoire, Auto}`; `Theme::as_str(self) -> &'static str` (`"night"|"ivoire"|"auto"`); `Theme::next(self) -> Theme` (Auto→Night→Ivoire→Auto); `Theme::label(self) -> &'static str` (`"Auto"`, `"Night Glass"`, `"Ivoire"`, not translated: they are names). New CSS tokens in both themes: `--muted`, `--sidebar-bg`, `--sidebar-border`, `--sidebar-w`, `--nav-active-bg`, `--nav-active-ink`, `--btn-dark-bg`, `--btn-dark-ink`, `--cool-bg`.

- [ ] **Step 1: Update the unit tests in `src/theme.rs`**

```rust
#[test]
fn parses_known_values() {
    assert_eq!(parse(Some("night")), Theme::Night);
    assert_eq!(parse(Some("ivoire")), Theme::Ivoire);
    assert_eq!(parse(Some("warm")), Theme::Ivoire); // saved before Ivoire replaced Warm Minimal
    assert_eq!(parse(Some("auto")), Theme::Auto);
    assert_eq!(parse(None), Theme::Auto);
    assert_eq!(parse(Some("gibberish")), Theme::Auto);
}
#[test]
fn maps_to_the_data_theme_attribute() {
    assert_eq!(super::attribute(Theme::Night), Some("dark"));
    assert_eq!(super::attribute(Theme::Ivoire), Some("light"));
    assert_eq!(super::attribute(Theme::Auto), None);
}
#[test]
fn serialises_known_values() {
    assert_eq!(Theme::Ivoire.as_str(), "ivoire");
}
#[test]
fn the_header_button_cycles() {
    assert_eq!(Theme::Auto.next(), Theme::Night);
    assert_eq!(Theme::Night.next(), Theme::Ivoire);
    assert_eq!(Theme::Ivoire.next(), Theme::Auto);
}
#[test]
fn both_themes_define_the_same_tokens() {
    // Every custom property set in `:root` is set by the Ivoire block, and the reverse.
    let css = include_str!("../styles/tokens.css");
    let (night, ivoire) = token_sets(css);
    assert_eq!(night, ivoire);
    for t in ["--muted", "--sidebar-bg", "--sidebar-w", "--nav-active-bg", "--cool-bg"] {
        assert!(night.contains(t), "{t}");
    }
    assert!(!css.contains("Fraunces"));
}
#[test]
fn ivoire_text_passes_aa() {
    let v = ivoire_values(include_str!("../styles/tokens.css"));
    for text in ["--ink", "--muted", "--accent-mint", "--accent-amber", "--danger", "--offline"] {
        for surface in ["--bg", "--card-bg"] {
            assert!(contrast(&v[text], &v[surface]) >= 4.5, "{text} on {surface}");
        }
    }
    assert!(contrast(&v["--nav-active-ink"], &v["--nav-active-bg"]) >= 4.5);
    assert!(contrast(&v["--card-on-ink"], &v["--card-on-bg"]) >= 4.5);
}
```

Test helpers (in the test module): `token_sets(css) -> (BTreeSet<String>, BTreeSet<String>)` collects `--name:` declarations in the first `:root {…}` block and in the Ivoire block (the block whose selector list contains `[data-theme="light"]`); `ivoire_values(css) -> HashMap<String, String>` maps names to `#rrggbb` values in the Ivoire block; `contrast(a, b) -> f64` is the WCAG 2.1 ratio from sRGB relative luminance.

- [ ] **Step 2: Run, expect failures**

Run: `cargo test -p oikos-web theme::`
Expected: FAIL (`Theme::Ivoire` not found).

- [ ] **Step 3: Implement**

- `src/theme.rs`: rename `Warm` → `Ivoire`, add the alias in `parse`, `next`, `label`; module doc "Night Glass, Ivoire, or follow the system".
- `src/screens/settings.rs` ThemeSwitch: segments use `Theme::label()`: Auto, Night Glass, Ivoire.
- `styles/tokens.css`:
  - Night Glass stays in `:root`; add the new tokens to it: `--muted: rgb(232 236 255 / 65%)`, `--sidebar-bg: rgb(255 255 255 / 4%)`, `--sidebar-border: var(--card-border)`, `--sidebar-w: 248px`, `--nav-active-bg: var(--accent-soft)`, `--nav-active-ink: var(--ink)`, `--btn-dark-bg: var(--ink)`, `--btn-dark-ink: var(--bg)`, `--cool-bg: rgb(88 225 255 / 10%)`.
  - ONE Ivoire block, selector `:root[data-theme="light"]`, with every token of `:root` (parity test). Values: the token table in the spec ("Themes"), verbatim, plus `--sidebar-w: 248px`, `--radius-card: 24px`, `--font-display: "Manrope", Inter, system-ui, sans-serif`, `--type-display: 700 28px/32px var(--font-display)`, `--type-display-spacing: -0.03em`.
  - Delete the `@media (prefers-color-scheme: light)` duplicate. Auto is resolved in Rust instead: `attribute(Theme::Auto)` stays `None` (the stored choice), and the wasm effect in `install_theme` sets `data-theme` to `light` or `dark` from `window.matchMedia("(prefers-color-scheme: light)")` while the choice is Auto, re-applying on the query's `change` event (listener removed when the choice leaves Auto). Add the `MediaQueryList` / `MediaQueryListEvent` web-sys features if needed.
- `styles/fonts.css` + `assets/fonts/`: add Manrope 600 and 700 (latin subset woff2, e.g. from the `@fontsource/manrope` package files `manrope-latin-600-normal.woff2` / `-700-`), remove Fraunces; update the header comment (source, OFL 1.1).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-web theme::` then the full gate from Global Constraints.
Expected: PASS. If `ivoire_text_passes_aa` fails on a value from the spec, darken that value until it passes and note it in the commit message.

- [ ] **Step 5: Commit** — `feat(web): Ivoire replaces Warm Minimal`

---

### Task 2: FR/EN string layer

**Files:**
- Create: `crates/oikos-web/src/i18n.rs`
- Modify: `crates/oikos-web/src/lib.rs` (module), `crates/oikos-web/src/components/status_pill.rs`

**Interfaces:**
- Produces:
  - `pub enum Lang { En, Fr }` with `as_str() -> &'static str` (`"en"|"fr"`), `next() -> Lang` (toggles), `pub fn parse(Option<&str>) -> Option<Lang>`, `pub fn from_browser(tag: &str) -> Lang` (`fr*` case-insensitive → Fr, else En).
  - `pub fn install_lang() -> RwSignal<Lang>`: stored under `localStorage["oikos.lang"]`, default `from_browser(navigator.language)`, keeps `<html lang>` in sync; provide it as context (done in Task 4's `App`).
  - `pub enum Key { … }` with `pub const ALL: &[Key]` and `pub fn text(lang: Lang, key: Key) -> &'static str`.
  - `pub fn use_text() -> impl Fn(Key) -> &'static str + Copy + 'static`: reads `RwSignal<Lang>` from context reactively; English when absent.
  - `pub fn unseen_alerts(lang: Lang, badge: &str) -> String` ("3 unseen alerts" / "3 alertes non lues").
- Keys and copy (en / fr): `MyHome` My home / Ma maison; `Overview` Overview / Vue d'ensemble; `Automations` Automations / Automatisations; `Groups` Groups / Groupes; `MyRooms` My rooms / Mes pièces; `HomeSection` Home / Maison; `Energy` Energy / Énergie; `Security` Security / Sécurité; `Climate` Climate / Climat; `Beta` Beta / Bêta; `Alerts` Alerts / Alertes; `Settings` Settings / Réglages; `Devices` Devices / Appareils; `NewDashboard` New dashboard / Nouveau tableau; `NewFlow` New flow / Nouvelle automatisation; `NewGroup` New group / Nouveau groupe; `EditGroup` Edit group / Modifier le groupe; `History` History / Historique; `Alert` Alert / Alerte; `Online` Connected / La maison est connectée; `Reconnecting` Reconnecting… / Reconnexion…; `Offline` Offline / Hors ligne; `OpenMenu` Open menu / Ouvrir le menu; `CloseMenu` Close menu / Fermer le menu; `ChangeTheme` Change theme / Changer de thème; `ChangeLanguage` Français / English (the label names the language you switch TO); `Navigation` Main / Navigation principale; `RoomGone` This room is gone. / Cette pièce n'existe plus.

- [ ] **Step 1: Write the unit tests in `src/i18n.rs`**

```rust
#[test]
fn every_key_has_both_languages() {
    for &k in Key::ALL {
        assert!(!text(Lang::En, k).is_empty() && !text(Lang::Fr, k).is_empty(), "{k:?}");
    }
}
#[test]
fn browser_language_picks_french_only_for_fr() {
    assert_eq!(from_browser("fr-FR"), Lang::Fr);
    assert_eq!(from_browser("FR"), Lang::Fr);
    assert_eq!(from_browser("en-US"), Lang::En);
    assert_eq!(from_browser("de-DE"), Lang::En);
    assert_eq!(from_browser(""), Lang::En);
}
#[test]
fn stored_values_parse() {
    assert_eq!(parse(Some("fr")), Some(Lang::Fr));
    assert_eq!(parse(Some("en")), Some(Lang::En));
    assert_eq!(parse(Some("xx")), None);
    assert_eq!(parse(None), None);
}
#[test]
fn shell_copy() {
    assert_eq!(text(Lang::Fr, Key::Overview), "Vue d'ensemble");
    assert_eq!(text(Lang::Fr, Key::Online), "La maison est connectée");
    assert_eq!(unseen_alerts(Lang::Fr, "3"), "3 alertes non lues");
}
```

A test that `Key::ALL` lists every variant: implement `ALL` next to the enum and add `#[test] fn all_is_complete()` that matches exhaustively on a `Key` inside a helper (a non-exhaustive match fails to compile when a variant is added without updating it).

- [ ] **Step 2: Run** — `cargo test -p oikos-web i18n::` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement `src/i18n.rs`** (one `match` returning `(en, fr)` per key) and translate `StatusPill` text through `use_text()` (Online/Reconnecting/Offline; the offline reason stays as the hub sends it).
- [ ] **Step 4: Run** the full gate. Expected: PASS (status pill tests, if any, see English).
- [ ] **Step 5: Commit** — `feat(web): FR/EN string table for the shell`

---

### Task 3: Route model — active entry and breadcrumb

**Files:**
- Create: `crates/oikos-web/src/shell/route.rs` (and `src/shell/mod.rs` declaring it, `pub mod shell;` in `lib.rs`)

**Interfaces:**
- Consumes: `Key`, `Lang`, `text` (Task 2).
- Produces:
  - `pub enum Section { Overview, Room(String), Automations, Groups, Energy, Security, Climate, Alerts, Settings, None }`
  - `pub fn section_for(path: &str, back: &[String]) -> Section`
  - `pub enum Lookup<'a> { Dashboard(&'a str), Room(&'a str), Flow(&'a str), Group(&'a str), Entity(&'a str), Alert(&'a str) }`
  - `pub struct Crumb { pub label: String, pub href: Option<String> }` (`PartialEq, Debug`)
  - `pub fn crumbs(path: &str, lang: Lang, name_of: impl Fn(Lookup) -> Option<String>) -> Vec<Crumb>`

Rules (from the spec's "Navigation details"): `/` and `/dashboards/*` → Overview; `/rooms/:id` → Room(id); `/flows*` → Automations; `/groups*` and `/group/:id` → Groups; `/energy` Energy; `/security` Security; `/climate` Climate; `/alerts*` Alerts; `/settings` and `/devices` → Settings; `/entity/:id` and `/history/:id` → the section of the latest entry of `back` that is not an entity/history page, else None. Crumbs always start with `Crumb { label: MyHome, href: Some("/") }`; the last crumb has `href: None`; a missing name falls back to the id (Alert falls back to `Key::Alert`).

- [ ] **Step 1: Write the tests**

```rust
fn names(l: Lookup) -> Option<String> {
    match l {
        Lookup::Dashboard("d1") => Some("Wall tablet".into()),
        Lookup::Room("r1") => Some("Kitchen".into()),
        Lookup::Flow("f1") => Some("Night lights".into()),
        Lookup::Entity("light.a") => Some("Lamp".into()),
        _ => None,
    }
}
fn labels(c: &[Crumb]) -> Vec<&str> { c.iter().map(|c| c.label.as_str()).collect() }

#[test]
fn sections() {
    assert_eq!(section_for("/dashboards/house", &[]), Section::Overview);
    assert_eq!(section_for("/rooms/r1", &[]), Section::Room("r1".into()));
    assert_eq!(section_for("/flows/f1", &[]), Section::Automations);
    assert_eq!(section_for("/group/g1", &[]), Section::Groups);
    assert_eq!(section_for("/groups/g1/edit", &[]), Section::Groups);
    assert_eq!(section_for("/alerts/a1", &[]), Section::Alerts);
    assert_eq!(section_for("/devices", &[]), Section::Settings);
    let back = vec!["/rooms/r1".to_string(), "/entity/light.a".to_string()];
    assert_eq!(section_for("/entity/light.a", &back), Section::Room("r1".into()));
    assert_eq!(section_for("/history/light.a", &[]), Section::None);
}
#[test]
fn breadcrumbs() {
    assert_eq!(labels(&crumbs("/rooms/r1", Lang::Fr, names)), ["Ma maison", "Kitchen"]);
    assert_eq!(labels(&crumbs("/flows/f1", Lang::Fr, names)), ["Ma maison", "Automatisations", "Night lights"]);
    assert_eq!(labels(&crumbs("/flows/new", Lang::En, names)), ["My home", "Automations", "New flow"]);
    assert_eq!(labels(&crumbs("/dashboards/d1", Lang::En, names)), ["My home", "Wall tablet"]);
    assert_eq!(labels(&crumbs("/history/light.a", Lang::En, names)), ["My home", "Lamp", "History"]);
    assert_eq!(labels(&crumbs("/rooms/zz", Lang::En, names)), ["My home", "zz"]);
    let c = crumbs("/flows/f1", Lang::En, names);
    assert_eq!(c[0].href.as_deref(), Some("/"));
    assert_eq!(c[1].href.as_deref(), Some("/flows"));
    assert_eq!(c[2].href, None);
}
```

- [ ] **Step 2: Run** — `cargo test -p oikos-web shell::route` — FAIL.
- [ ] **Step 3: Implement** `section_for` and `crumbs` per the rules.
- [ ] **Step 4: Run** the full gate — PASS.
- [ ] **Step 5: Commit** — `feat(web): route model for the sidebar and breadcrumb`

---

### Task 4: Sidebar shell (desktop)

**Files:**
- Create: `crates/oikos-web/src/shell/mod.rs` (`Shell`), `src/shell/sidebar.rs`, `src/shell/header.rs`
- Modify: `crates/oikos-web/src/app.rs`, `src/dashboards/mod.rs`, `styles/main.css`, `tests/smoke.rs`
- Delete: `crates/oikos-web/src/dashboards/picker.rs` (and its CSS `.dashboards-picker*`)
- Test: `crates/oikos-web/tests/shell.rs` (create)

**Interfaces:**
- Consumes: `Theme::next`, `Theme::label` (Task 1); `use_text`, `Key`, `install_lang`, `Lang` (Task 2); `section_for`, `crumbs`, `Section`, `Lookup` (Task 3).
- Produces:
  - `#[component] pub fn Shell(…props below…, children: Children) -> impl IntoView` — renders `<div class="shell"> <aside class="sidebar" id="sidebar">…</aside> <div class="shell__content"> <header class="content-header">…</header> <main class="screen">{children}</main> </div> </div>`. Rendered inside `<Router>` around `<Routes>`.
  - `pub(crate) async fn create_dashboard(house: House) -> Option<String>` in `src/dashboards/mod.rs` (moved from the picker's "New dashboard" click handler; returns the new id).
  - The `climate_on`, `alarm_armed`, `solar_active` and `alerts_badge` memos stay in `App` and reach the sidebar as props: `Shell(climate_on: Signal<bool>, alarm_armed: Signal<bool>, solar_active: Signal<bool>, alerts_badge: Signal<Option<String>>, children: Children)`, passed on to `Sidebar` with the same names.

Content (spec "Decisions §6" and "Shell"): brand (`/assets/icon.svg` + "oikos." wordmark, doorbell double-click kept with class `sidebar__logo--ringing` and the reduced-motion guard); Overview link to the first authored dashboard or house (same target rule as `HouseRedirect`, extract `fn landing_dashboard(list: &[DashboardView]) -> Option<String>` in `app.rs` and reuse it); when `house.dashboards.len() >= 2` an inline `<ul class="sidebar__dashboards">` listing them in order (house first), active one marked, then a "New dashboard" button calling `create_dashboard` and navigating to `/dashboards/{id}?edit=1`; Automations `/flows`; Groups `/groups`; "My rooms" heading + rooms (Task 6 fills the list; render an empty `<ul class="sidebar__rooms">` now); "Home" heading + Energy (solar glyph class), Security (armed glyph class), Climate (only when `climate_on`, Beta pill), Alerts (badge with `aria-label=unseen_alerts(…)`); footer: Settings `/settings`, then `StatusPill`. Active entry: `aria-current="page"` from `section_for(pathname, back_stack)`. Header: breadcrumb `<nav class="breadcrumb" aria-label=…>` from `crumbs` (names from `house.dashboards`, `house.areas`, `house.flows`, `house.entities`, `house.alerts`), theme button (`aria-label=ChangeTheme`, `title=Theme::label()`, icon per theme — sun-moon for Auto, moon for Night, sun for Ivoire; add the two missing glyphs to `src/icons.rs`), language button (label `ChangeLanguage`, toggles `Lang::next`). CSS: delete `.topbar*`, `.nav__*` rules scoped to the top bar and `.dashboards-picker*`; add `.shell` (grid: `var(--sidebar-w) 1fr`), `.sidebar` (`position: sticky; top: 0; height: 100dvh; overflow-y: auto; background: var(--sidebar-bg); border-right: 1px solid var(--sidebar-border)`; no `backdrop-filter`), entries using `--nav-active-bg` / `--nav-active-ink`, `.content-header` (not sticky). `.screen` keeps `max-width: 1200px` inside the content column.

- [ ] **Step 1: Write `tests/shell.rs`** (mount `App` like `tests/smoke.rs`, seed `House` through context where needed or mount `Shell` directly inside a `Router` with a `House` and a `RwSignal<Lang>`):

```rust
#[wasm_bindgen_test] async fn sidebar_lists_the_main_entries() {
    // EN: Overview, Automations, Groups, Energy, Security, Alerts, Settings present; no ".topbar".
}
#[wasm_bindgen_test] async fn climate_entry_follows_the_engine() {
    // climate_on=false → no "Climate" link; true → link with a "Beta" pill.
}
#[wasm_bindgen_test] async fn dashboards_unfold_from_two() {
    // house.dashboards = [house] → no ".sidebar__dashboards"; [house, d1] → two items + "New dashboard".
}
#[wasm_bindgen_test] async fn french_shell() {
    // Lang::Fr in context → "Vue d'ensemble", "Automatisations", "Réglages", breadcrumb starts "Ma maison".
}
#[wasm_bindgen_test] async fn theme_button_cycles() {
    // click ".content-header__theme" three times from Auto → Night → Ivoire → Auto (read the RwSignal<Theme>).
}
```

Each asserts with `host.query_selector(…)` / `inner_html().contains(…)`; follow the mounting helpers in `tests/settings_updates.rs`.

- [ ] **Step 2: Run** — `cd crates/oikos-web && wasm-pack test --headless --chrome --test shell` — FAIL.
- [ ] **Step 3: Implement** `Shell`, `Sidebar`, `ContentHeader`, move `create_dashboard`, wire `App` (`install_lang()` + `provide_context`, `<Shell>` around `<Routes>`), CSS. Update `tests/smoke.rs` assertions: `"oikos."`, `"Alerts"`, `"Automations"` (the label was "Flows"; the route stays `/flows`), `"pill"`.
- [ ] **Step 4: Run** the full gate — PASS. Then `trunk serve`, open `/` in Ivoire and Night, check the sidebar by eye.
- [ ] **Step 5: Commit** — `feat(web): sidebar shell replaces the top bar`

---

### Task 5: Floating panels beside the sidebar

**Files:**
- Modify: `crates/oikos-web/src/layout.rs`, `src/screens/flow_canvas/mod.rs:430-490`, `src/screens/dashboard.rs:36-48`, `styles/main.css` (`.flow-canvas-screen` ~2990, `.dashboard-library-panel` ~5197)
- Test: `crates/oikos-web/tests/flow_canvas.rs` (add one test)

**Interfaces:**
- Consumes: `.content-header`, `.sidebar` classes (Task 4).
- Produces: `pub(crate) fn top_offset() -> f64` = bottom of `.content-header` (0 when absent); `pub(crate) fn left_offset() -> f64` = right edge of `.sidebar` when it is laid out in flow (desktop), 0 when it is off-canvas (phone, Task 7: the drawer sidebar has `position: fixed` and `left_offset` returns 0 when `getComputedStyle(sidebar).position == "fixed"`).

Notes: the 68/72/124px tops in the flow CSS are relative to the canvas (`.flow-stage` is absolute inside the fixed canvas), so they stay. The canvas becomes `inset: var(--flow-top, 0px) 0 0 var(--flow-left, 0px)`; both values set from Rust on mount and on resize, as today. The dashboard library panel's sticky top no longer depends on a bar: `top: 12px`, `max-height: calc(100dvh - 24px)`; remove `--dashboard-top` and the `top_offset` effect from `dashboard.rs`.

- [ ] **Step 1: Write the test** in `tests/flow_canvas.rs`:

```rust
#[wasm_bindgen_test]
async fn canvas_inset_clears_the_sidebar() {
    // Mount the App at /flows/<fixture flow> (history.pushState before mount); after tick:
    // the ".flow-canvas-screen" style has "--flow-top:" equal to the ".content-header" bottom, and
    // "--flow-left:" equal to the ".sidebar" bounding rect's right edge when the sidebar's computed
    // position is not "fixed", else 0 (the test browser window may be phone-sized after Task 7).
}
```

- [ ] **Step 2: Run** — FAIL (`--flow-left` missing).
- [ ] **Step 3: Implement** per the notes.
- [ ] **Step 4: Run** the full gate — PASS; `trunk serve`: open a flow and enter dashboard edit mode, check nothing sits under the sidebar.
- [ ] **Step 5: Commit** — `fix(web): flow canvas and widget library sit beside the sidebar`

---

### Task 6: Rooms get a URL; overview lists every room

**Files:**
- Create: `crates/oikos-web/src/rooms.rs`, `crates/oikos-web/src/screens/room.rs`
- Modify: `src/screens/home.rs`, `src/screens/mod.rs`, `src/app.rs` (route), `src/shell/sidebar.rs` (rooms list), `src/nav.rs`, `src/screens/group_sheet_v2.rs:116,147-150`, `styles/main.css`
- Test: unit tests move with `chips` into `src/rooms.rs`; `crates/oikos-web/tests/rooms.rs` (create)

**Interfaces:**
- Consumes: `Section::Room`, `Key::RoomGone`, `Key::MyRooms` (Tasks 2–3), `Sidebar` (Task 4).
- Produces:
  - In `src/rooms.rs`: `pub(crate) fn home_counts(...)` and `pub(crate) fn chips(...)` moved verbatim from `home.rs` (same signatures), plus `pub(crate) fn sidebar_rooms(chips: &[AreaView]) -> Vec<AreaView>` = `chips` without `FAVORITES` and `UNASSIGNED_AREA`; `FAVORITES` moves here too. `pub(crate) fn use_room_chips() -> Memo<Vec<AreaView>>` builds the chip list from `House` (the memo now in `HomeScreen`).
  - `#[component] pub(crate) fn RoomSection(id: String) -> impl IntoView` — `<section class="room">` with `RoomTitle` (rename allowed except Favorites/Unassigned), the grid of `home_items` cards, and "Nothing here yet." when empty: extracted from today's `HomeScreen` body.
  - `#[component] pub fn RoomScreen() -> impl IntoView` at `/rooms/:id`: `RoomSection` for that id when it is in `use_room_chips()`, else `<p class="room__gone">` with `Key::RoomGone` (once loaded; skeleton while loading, as Home does).
  - `HomeScreen` (the house overview): skeleton while loading, then one `RoomSection` per chip in `use_room_chips()` order (Favorites first, Unassigned last), then `RecentAlerts`. No `.chips` nav, no `HomeRoom`.
  - Removed: `HomeRoom`, `home_room_for` and their tests from `nav.rs`; `install_nav` provides only `BackStack`. `group_sheet_v2.rs` no longer sets a Home room (Back already follows the stack).

- [ ] **Step 1: Write tests**
  - Unit (`src/rooms.rs`): keep the existing `chips` tests (moved); add
    ```rust
    #[test] fn sidebar_rooms_drop_favorites_and_unassigned() {
        // chips = [Favorites, living, kitchen, Unassigned] → ids ["living", "kitchen"], counts unchanged.
    }
    ```
  - wasm (`tests/rooms.rs`):
    ```rust
    #[wasm_bindgen_test] async fn overview_shows_every_room() {
        // House with areas a, b (one light each) and an unassigned switch, at /dashboards/house:
        // three ".room" sections in order a, b, Unassigned; no "nav.chips".
    }
    #[wasm_bindgen_test] async fn room_route_shows_one_room() {
        // at /rooms/a: one ".room" with title "a"'s name and its light card; sidebar link to /rooms/a has aria-current="page".
    }
    #[wasm_bindgen_test] async fn room_route_unknown_room() {
        // at /rooms/zz with areas loaded: ".room__gone" text "This room is gone."; no panic.
    }
    #[wasm_bindgen_test] async fn sidebar_lists_rooms_with_counts() {
        // sidebar ".sidebar__rooms" has links to /rooms/a and /rooms/b with count "1"; no Favorites/Unassigned.
    }
    ```
- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement** per the interfaces; add `<Route path=path!("/rooms/:id") view=RoomScreen />`; CSS for stacked `.room` sections (spacing between sections; the existing `.room` title styles stay) and remove `.chips` rules only if nothing else uses them (`grep -n "chips" src` first: `flow`/`settings` screens use `.chips` too — keep those).
- [ ] **Step 4: Run** the full gate — PASS. Fix `tests/*` that relied on Home's chips (grep `"chips"` / `area_chip` in `tests/`).
- [ ] **Step 5: Commit** — `feat(web): rooms get their own page; overview lists every room`

---

### Task 7: Phone drawer

**Files:**
- Modify: `src/shell/mod.rs`, `src/shell/header.rs`, `src/shell/sidebar.rs`, `styles/main.css`
- Test: `crates/oikos-web/tests/shell.rs` (add)

**Interfaces:**
- Consumes: `Shell`, `Key::OpenMenu`, `Key::CloseMenu`.
- Produces: `Shell` owns `drawer_open: RwSignal<bool>`; header ☰ button `.content-header__menu` (`aria-controls="sidebar"`, `aria-expanded`); sidebar gets `class:sidebar--open`; scrim `<div class="shell__scrim">` rendered while open.

Behaviour: below **900px** (my call for the spec's open breakpoint) the sidebar is `position: fixed; inset: 0 auto 0 0; width: min(85vw, 320px); transform: translateX(-100%)`, `.sidebar--open` slides it in (`--motion-medium`, `--ease`; no transition under `prefers-reduced-motion`), drawer z-index 30, scrim 29 (toasts stay at 50, above both). ☰ hidden at ≥900px. Closing: route change, scrim click, Escape (`window_event_listener(keydown)`), any sidebar link click. On open, focus the first sidebar link; on close, return focus to ☰. Touch targets ≥ `var(--touch-min)`.

- [ ] **Step 1: Write the test**

```rust
#[wasm_bindgen_test]
async fn drawer_closes_on_navigation_and_escape() {
    // click ".content-header__menu" → ".sidebar--open" and ".shell__scrim" exist, aria-expanded="true";
    // dispatch keydown Escape on window → closed; open again, click the Automations link → closed and path is /flows;
    // open again, click ".shell__scrim" → closed.
}
```

- [ ] **Step 2: Run** — FAIL.
- [ ] **Step 3: Implement** behaviour + CSS media query.
- [ ] **Step 4: Run** the full gate — PASS; `trunk serve` at 390px width: open/close by eye.
- [ ] **Step 5: Commit** — `feat(web): the sidebar becomes a drawer on phones`

---

### Task 8: Visual QA and user docs

**Files:**
- Modify: `docs/users/web-ui.md`; any CSS fixes found
- No new tests unless a bug is found (then a test first, in the owning area)

- [ ] **Step 1:** `cd crates/oikos-web && trunk build --release`, serve `dist/` (e.g. `trunk serve`), and capture screenshots with headless Chrome over CDP (window width ≥ 500px; the extension times out on the wasm page) of: `/` overview, `/rooms/<id>`, `/flows`, a flow canvas, `/dashboards/<id>?edit=1`, `/energy`, `/security`, `/alerts`, `/settings`, each in Ivoire and Night, at 1280px and 390px (drawer open once). Check: nothing under the sidebar, no text below AA in Ivoire (lit light cards in particular: `--card-on-bg` honey with `--card-on-ink`), Night Glass unchanged apart from the shell.
- [ ] **Step 2:** fix what the screenshots show (one commit per fix, `fix(web): …`).
- [ ] **Step 3:** update `docs/users/web-ui.md`: themes are Night Glass, Ivoire and Auto (Warm Minimal is gone; a saved Warm choice opens in Ivoire), the sidebar and its sections, rooms in the sidebar (no more room chips), ☾ and FR·EN buttons, the phone drawer. Keep it generic.
- [ ] **Step 4:** full gate — PASS.
- [ ] **Step 5: Commit** — `docs(users): Ivoire, sidebar and FR/EN`
