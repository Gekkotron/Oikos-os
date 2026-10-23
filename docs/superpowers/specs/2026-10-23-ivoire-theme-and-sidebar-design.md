# Ivoire theme, sidebar shell and FR/EN — design

Date: 2026-10-23. Scope: the web UI (`crates/oikos-web`). The Android app is out of scope.

This is project 1 of a redesign toward a soft look inspired by Homey and Apple Home. Projects 2–5 (rich cards, home header + weather, ambiances, translating every screen) are separate and build on this one.

## Why

The web UI works, but it looks and reads like a tool: a dark glass top bar of icons, English labels, a light theme that is an afterthought. The user wants Oikos to feel calm and domestic — an ivory page, sage accents, amber for what is lit — and to be usable in French as well as English. This project lays the foundation every later piece sits on: the look (a new theme), the frame of the page (a sidebar) and the language layer.

## Decisions taken with the user

1. **A theme only changes the look**: colours, type, radii, shadows. It never changes layout or features. Everything later (weather, ambiances, rich cards, home header) works in every theme.
2. **Two themes**: Night Glass (dark, unchanged in spirit) and **Ivoire** (light), plus Auto, which follows the system. **Ivoire replaces Warm Minimal**; Warm Minimal and its serif font go away.
3. **New layout for every theme**: the sticky top bar becomes a **left sidebar**.
4. **Phone**: the same sidebar, hidden behind a ☰ button, slides over the page (off-canvas drawer).
5. **UI in two languages, FR and EN.** This project builds the mechanism and translates the shell (sidebar, content header). Translating the remaining screens is project 5.
6. **Sidebar content** (approved from a mockup):
   - Top: brand mark; **Vue d'ensemble / Overview** (the dashboards — when there are several they unfold underneath, with "New dashboard"); **Automatisations / Automations** (flows); **Groupes / Groups**.
   - **Mes pièces / My rooms**: every area with its device count.
   - **Maison / Home** section: Énergie, Sécurité, Climat (with the Beta pill, shown only when climate is enabled, as today), Alertes (with the unseen count badge).
   - Bottom: Réglages / Settings, then the connection status ("La maison est connectée", three states as today's status pill).
   - "Mes ambiances" and a "Mode Maison" indicator appear only once their own projects exist — no empty entries. No avatar: Oikos has no user accounts.
   - Content header: a breadcrumb on the left ("⌂ Ma maison / Vue d'ensemble"), on the right a ☾ theme button and a FR·EN language switch.

## Goals

- Ivoire as the light theme, faithful to the mockup's feel, every text colour passing WCAG AA.
- One shell (sidebar + content header) shared by all screens, all themes, desktop and phone.
- Rooms reachable from the sidebar by URL (reload and Back work).
- An FR/EN string layer that later projects use for every new label.
- No regression of existing screens: the flow canvas, dashboard editing and its library panel, the dashboards menu, toasts, sheets and Back navigation keep working.

## Non-goals

- New card designs (project 2), home header and weather (3), ambiances (4), translating all screens (5).
- A per-theme layout or a user-editable theme.
- Changes to the hub, the API or the Android app.

## Anti-goals (would be a failure even if it works)

- A theme switch that moves things around.
- A half-French UI where the shell is French and nothing says how to switch back.
- The flow canvas or a floating panel ending up under the sidebar or misplaced.
- Text that is pretty but unreadable (the mockup's grey and honey fail AA as text — see tokens).

## How it fits the current code

- Themes are CSS custom properties in `styles/tokens.css`: `:root` is Night Glass, `:root[data-theme="light"]` is Warm Minimal, and the same light block is duplicated under `@media (prefers-color-scheme: light)` for Auto. `src/theme.rs` holds `Theme::{Night, Warm, Auto}`, stored in `localStorage` as `"night" | "warm" | "auto"`, applied as `data-theme`. Settings has a ThemeSwitch (`src/screens/settings.rs`).
- The top bar lives in `src/app.rs` (`<header class="topbar">`: logo with the double-click doorbell egg, `StatusPill`, nav with `DashboardsPicker`, Energy, Security, Climate (conditional), Alerts (badge), Flows, Groups, Settings). CSS in `styles/main.css` (`.topbar*`, `.nav__*`).
- Things that depend on the top bar and must be reworked:
  - `src/layout.rs::top_offset()` (bottom of `.topbar` + 8px) feeds `--flow-top` (the flow canvas is `position: fixed`) and `--dashboard-top` (the dashboard library panel).
  - Flow overlays with hard-coded 68/72/124px tops in `main.css`.
  - `DashboardsPicker` positions a fixed menu and compensates for the top bar's `backdrop-filter` becoming the containing block (`src/dashboards/picker.rs`, comment in `main.css`).
- Rooms come from `house.areas` (`AreaView { id, name, primary_count }`); Home builds its room chips with `chips()` in `src/screens/home.rs` (Favorites first, empty areas dropped, Unassigned last). The selected room is in-memory only (`HomeRoom` in `src/nav.rs`); there is no room URL. `/` redirects to a dashboard; the built-in house dashboard renders Home.
- Fonts are bundled locally (`styles/fonts.css`: Inter 500/600/700, Fraunces 500). No third-party font hosts.
- No wasm test queries the top bar; `tests/settings_updates.rs` provides `Theme::Auto`.

## The design

### Themes

- `Theme::Warm` becomes `Theme::Ivoire`. Stored value `"ivoire"`; `parse` keeps `"warm"` as an alias so nobody's saved choice is lost. The `data-theme` attribute value may stay `light` (builder's call).
- Remove the duplicated light block: write the Ivoire variables once and apply them to both `:root[data-theme="light"]` and `@media (prefers-color-scheme: light) :root:not([data-theme="dark"])` via a shared selector list.
- Ivoire values (starting point, tune by eye; contrast on `--bg` / `--card-bg` noted):

| Token | Ivoire | Note |
|---|---|---|
| `--bg` | `#f3f0e8` | page ivory; `--bg-gradient` flat |
| `--ink` | `#1f2420` | 13.9:1 |
| `--muted` **(new)** | `#5f665c` | ~5:1; the mockup's `#8a8f86` fails AA |
| `--card-bg` / `--card-border` | `#fffdf8` / `#ebe6da` | white card, hairline |
| `--card-on-bg` / `--card-on-ink` | `#fbf0d4` / `#5c4208` | lit = honey wash (was a dark card) |
| `--shadow` | `0 2px 6px rgb(60 50 20/6%), 0 10px 28px rgb(60 50 20/8%)` | soft |
| `--accent-mint` / `--connected` / `--select` | `#4f6b4a` | sage, text-safe |
| `--connected-bg` / `--select-soft` | `#e3eadf` | soft sage |
| `--accent-amber` | `#b0801f` | text-safe honey; `#c9973d` only for fills/icons (`--solar`) |
| `--accent-cyan` | `#3f7f9c` | temperature |
| `--accent-rose` / `--danger` | `#b4552f` | terracotta |
| `--offline` | `#a23b4f` | |
| `--warning` / `--warning-bg` | `#8a5f12` / `#fbf0d4` | |
| `--accent-soft` / `--accent-ring` | `#fbf0d4` / `rgb(201 151 61/45%)` | |
| `--raised` / `--menu-bg` | `#f7f5ef` / `#fffdf8` | |
| `--radius-card` | `24px` | very round |
| `--sidebar-bg` / `--sidebar-border` **(new)** | `#f7f5ef` / `#ebe6da` | |
| `--nav-active-bg` / `--nav-active-ink` **(new)** | `#e3eadf` / `#2f4a2b` | selected entry |
| `--btn-dark-bg` / `--btn-dark-ink` **(new)** | `#232a26` / `#fffdf8` | dark pill buttons |
| `--cool-bg` **(new)** | `#eaf0f5` | for project 2's temperature card |

- Every new token also gets a Night Glass value (glass tones, `--nav-active-bg: var(--accent-soft)`, `--muted` ≈ ink at 65%).
- Lit cards change from dark-on-light to honey in Ivoire: check every card's text inside a lit card for contrast.
- Font: bundle one geometric sans for headings and the brand (Plus Jakarta Sans or Manrope, 600/700, latin subset woff2, OFL) — builder's pick between the two. `--font-display` stays a token; Night Glass may use the same face. Drop Fraunces and its file. Add Inter 400 if body text at 400 is wanted (today it falls back to 500).

### Shell

- A `Shell` component in place of the `<header class="topbar">` markup: sidebar + content column. The content column has the header (breadcrumb, ☾, FR·EN) and `<main class="screen">`.
- Sidebar entries as listed in Decisions §6, active entry marked (`aria-current`) with `--nav-active-bg`. Keep the existing memos: solar recolouring on Énergie, armed recolouring on Sécurité, climate gating, alerts badge (`TrackAlertsSeen` unchanged).
- Dashboards: Vue d'ensemble links to the current/first dashboard; with two or more dashboards, the list unfolds inline under it with "New dashboard". This replaces the floating `DashboardsPicker` menu and its containing-block workaround.
- The doorbell egg moves to the new brand mark (same behaviour, `prefers-reduced-motion` guard kept).
- The sidebar has no `backdrop-filter`.
- Floating panels: the flow canvas and the dashboard library panel must sit beside the sidebar, not under it. Preferred: make the content column the positioning context and drop the JS measurement; acceptable: a `left_offset()` next to `top_offset()`. Replace the hard-coded overlay tops.
- ☾ cycles Auto → Night → Ivoire (Settings keeps its full control). FR·EN switches the language.
- Breadcrumb: "⌂ Ma maison / <page>" derived from the route (dashboard name, room name, screen name). In-sheet Back buttons and the BackStack stay as they are.

### Rooms

- New route `/rooms/:id` showing that room's devices exactly as Home shows a selected room today (reuse, don't fork, the Home room view: room title with rename, groups, cards). Sidebar room links go there; reload and Back work. Back from a sheet opened on `/rooms/x` returns to `/rooms/x` through the existing BackStack.
- **Decided with the user: the room chips row at the top of Home is removed.** Rooms are reached only from the sidebar. The in-memory `HomeRoom` selection goes away with it; `home_room_for` (Back target from a group sheet) becomes a `/rooms/<id>` path instead.
- *(my call)* With no chips, the house overview (the built-in house dashboard) shows every room, one section per room in the hub's area order, each with its room title, then Unassigned last. Favorites stays a section at the top if any favourites exist. `RecentAlerts` stays on the overview only, not on room pages.
- `chips()` becomes a shared room list used by the sidebar (and the overview sections). *(my call)* The sidebar lists real areas only, not Favorites or Unassigned; the count next to a room is the same number Home used on that room's chip, so the sidebar and the room page never disagree.
- Renaming a room (existing `RoomTitle`) updates the sidebar for free through `house.areas`.

### Navigation details *(my calls, from the builder check)*

- **Active entry**: Vue d'ensemble is active on `/` and every `/dashboards/:id`; when the list is unfolded, the matching dashboard is highlighted too. The built-in house dashboard is listed first in the unfolded list; the list unfolds when there are two or more dashboards counting it. A room is active on `/rooms/:id`.
- **Routes without their own entry highlight their parent**: `/flows/*` → Automatisations; `/groups/*` and `/group/:id` → Groupes; `/alerts/:id` → Alertes; `/devices` → Réglages. `/entity/:id` and `/history/:id` highlight whichever entry the BackStack says the user came from, else nothing.
- **Breadcrumb**: "⌂ Ma maison / <section> [/ <item>]", e.g. "Ma maison / Automatisations / <flow name>", "Ma maison / <room name>", "Ma maison / <dashboard name>". Section names are translated; user data (room, dashboard, flow, entity names) is shown as is.
- **Scrolling**: the page scrolls as today. The sidebar is `position: sticky; top: 0; height: 100dvh` with its own scroll if it overflows. The content header is not sticky. The sidebar width is a CSS variable (`--sidebar-w`); the fixed flow canvas uses `inset: var(--flow-top) 0 0 var(--sidebar-w)` (0 on phone), and `top_offset()` measures the bottom of the content header instead of `.topbar`. The hard-coded overlay tops are derived from `--flow-top`.
- **Theme button**: stored value is `"ivoire"` with `"warm"` read as an alias; Settings keeps its three options (Auto, Night Glass, Ivoire). The header button shows the current choice (icon per state, builder's pick of icons) and cycles Auto → Night → Ivoire.
- **Language in the shell**: components rendered by the shell (status pill, badges, aria-labels, dashboards list, breadcrumb) are translated at the component level, wherever else they are reused. Tests run with EN unless they provide a language, so existing English assertions stay valid.
- No placeholder markup for "Mes ambiances" or "Mode Maison".

### Phone

- Below a breakpoint (builder's call, around 720–900px) the sidebar is off-canvas: a ☰ in the content header opens it over a scrim; tapping an entry, the scrim or Escape closes it; focus is trapped while open and returns to ☰ on close. Touch targets ≥ `--touch-min`.

### FR/EN

- A small string table in the web crate: keys → `{ en, fr }`, a `Lang` signal provided at the root like the theme, stored in `localStorage` (fallible, silent, like the theme), defaulting to the browser language (`fr*` → FR, else EN). `<html lang>` follows it.
- Every label in the shell goes through it. Other screens stay English until project 5; new code in later projects must use the table.
- No external i18n crate is required; the builder may pick a light one if it fits Leptos and wasm size.

## Suggested order

1. Tokens: dedupe, Warm → Ivoire with alias, new tokens in both themes, font bundle. No layout change.
2. FR/EN mechanism (table, signal, storage, default).
3. Shell: sidebar + content header on desktop, dashboards list, status, badges, doorbell; fix the flow canvas, library panel and overlays.
4. Rooms route and sidebar rooms; remove Home's chips row and `HomeRoom`; overview shows every room.
5. Phone drawer.
6. Visual QA of every screen in both themes (headless Chrome over CDP), lit-card contrast pass.

## Testing

- Unit tests: theme parse (incl. the `"warm"` alias), language default/parse, breadcrumb derivation, room list (shared `chips()`).
- wasm tests: the sidebar renders entries, climate entry gated, alerts badge, active entry follows the route, room link navigates; update fixtures that provide `Theme`.
- Visual check of every route in both themes and at phone width; the flow canvas and the dashboard library panel in particular.

## Left to the builder

The exact breakpoint, the heading font (Plus Jakarta Sans vs Manrope), the `data-theme` attribute value, the theme button icons, the Night Glass values of the new tokens, and the string-table implementation details.
