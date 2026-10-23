# Ambiances, web side — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The web UI shows pinned ambiances as one-tap tiles on Home. It adds a "Mes ambiances" page to list, pin, reorder, apply, create, edit and delete them, and it shows what happened after each tap.

**Architecture:** A new `oikos-web/src/ambiances/` module. `mod.rs` holds pure, natively tested helpers (Home selection, ids, examples, action sentences, report summary). Each screen is a component in its own file. Data comes from Plan 1's routes through `api/ambiances.rs`, into two new `House` signals kept live by the WebSocket. The category rules come from `oikos_api::category_members`, so the editor's counts match what the hub will do.

**Tech Stack:** Leptos (CSR, wasm32), leptos_router, gloo-net, wasm-bindgen-test (`wasm-pack test --headless --chrome`).

**Spec:** `docs/superpowers/specs/2026-10-24-ambiances-design.md`. **Depends on:** `docs/superpowers/plans/2026-10-24-ambiances-hub.md` being done (types, routes, WS events).

## Global Constraints

- Every new label goes through `crates/oikos-web/src/i18n.rs`, in FR and EN: fixed text as `Key` variants, and text with numbers or names as `pub fn …(lang: Lang, …) -> String` helpers, like `lights_chip`. The copy table at the end of this plan is exact.
- Themes change only the look. Use the tokens from `styles/tokens.css` only, no literal colours. Check the result in Night Glass and Ivoire.
- Generic: no house-specific names or ids in code, tests or fixtures. Examples use categories only.
- Phone layout: no horizontal page scroll. The Home row is a 2×2 grid below 700 px and 4 across above. The other screens are single-column below 700 px.
- Same Back behaviour as every other page: links push onto `nav.rs` `BackStack`, so do not add any special-casing.
- Every commit: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p oikos-web --target wasm32-unknown-unknown`, native `cargo test -p oikos-web`, and `wasm-pack test --headless --chrome` in `crates/oikos-web` for the tests the task adds. Commit identity and trailer as in Plan 1.
- Build hygiene as in Plan 1: a private target dir, no debug info, and `cargo clean` if `~/dev-scratch/cargo-target` is over 5 GB.

## Decisions made while planning (refining the spec)

- **Report and "How it works"** are routes (`/ambiances/{id}/report`, `/ambiances/about`) styled with the existing `.sheet` classes. Entity and group sheets are routes too, and this keeps Back consistent.
- **The device action's command** is picked from a list built per entity (Task 4, `device_commands`), with a number field for commands that carry a value. There is no free JSON field.
- **"Try it"** applies the saved version, so it is disabled while the form has unsaved changes. Its tooltip says "Save first".
- **The category count** expands inline into the list of names, grouped by room, each with a checkbox. Unticking one adds it to `except`.
- **The Home row** is hidden entirely until the ambiance list has loaded once. This avoids a flash of the invitation tile.

## Review Focus

1. **A tap while the WebSocket is reconnecting.** The tile must still call the REST route, and show the error toast if that fails. It must never stay stuck in the busy state. Pinned by `tile_busy_clears_on_error` (Task 5).
2. **Pinning a fifth ambiance.** Home shows the first four by `order`, the page shows the hint, and nothing is refused. Pinned by `pinned_for_home_caps_at_four_by_order` (Task 1) and `page_shows_only_four_hint` (Task 3).
3. **Clicking "Add the four examples" twice quickly.** It must not create duplicates: the button is disabled while writing, and ids come from `slug_id` against the current ids. Pinned by `examples_button_disables_while_writing` (Task 3).
4. **An ambiance that names an entity or flow that no longer exists.** The editor shows the row as missing instead of crashing or dropping it, and Save keeps it unless the user removes it. Pinned by `missing_entity_row_is_marked` (Task 4).
5. **An invalid stored ambiance** (the hub lists it with `invalid`). The page shows it with its reason and only offers Delete. Home ignores it. Pinned by `invalid_ambiance_card_offers_delete_only` (Task 3) and `pinned_for_home_skips_invalid` (Task 1).

---

### Task 1: API client, House signals and pure helpers

**Files:**
- Create: `crates/oikos-web/src/api/ambiances.rs`; `crates/oikos-web/src/ambiances/mod.rs`
- Modify:
  - `crates/oikos-web/src/api/mod.rs` and `crates/oikos-web/src/lib.rs` (modules);
  - `crates/oikos-web/src/house.rs` (signals, refresh, the WS arms that replace Plan 1's ignore arms);
  - `crates/oikos-web/src/house_state.rs:79` (keep ignoring);
  - `crates/oikos-web/src/app.rs:153` (call `house.refresh_ambiances()` next to `refresh_weather`);
  - `crates/oikos-web/src/ws.rs:41` (the same, on reconnect);
  - `crates/oikos-web/src/screens/group_wizard.rs:42` (`slug_group_id` delegates to `slug_id`).

**Interfaces:**
- Consumes: from Plan 1, `oikos_api::{Ambiance, AmbianceAction, AmbianceView, ApplyReport, Outcome, CategoryAction, Category, Target, FlowDo, WsEvent::{AmbiancesChanged, AmbianceApplied}}`, and `category_members`.
- Produces:
  - `api::ambiances`:
    - `list_ambiances() -> Result<Vec<AmbianceView>, ApiError>`;
    - `put_ambiance(a: &Ambiance) -> Result<AmbianceView, ApiError>`;
    - `delete_ambiance(id: &str) -> Result<(), ApiError>`;
    - `apply_ambiance(id: &str) -> Result<ApplyReport, ApiError>`.

    Follow `api/core.rs` `set_group` and `delete_group`, with `check` and `read_json`.
  - `House`:
    - `ambiances: RwSignal<Vec<AmbianceView>>`;
    - `ambiances_loaded: RwSignal<bool>`;
    - `ambiance_reports: RwSignal<BTreeMap<String, ApplyReport>>`, the last report per id for this session;
    - `pub fn refresh_ambiances(&self)`.

    `AmbiancesChanged` sets the list; `AmbianceApplied` inserts the report.
  - `ambiances` (pure):
    - `pub fn pinned_for_home(list: &[AmbianceView]) -> Vec<Ambiance>`: valid, pinned, sorted by `(order, id)`, at most 4;
    - `pub fn slug_id(name: &str, taken: &BTreeSet<String>, fallback: &str) -> String`: moved from `slug_group_id`, which now calls `slug_id(name, taken, "group")`;
    - `pub fn next_order(list: &[AmbianceView]) -> u32`: the maximum plus 1, or 1;
    - `pub fn failed_count(report: &ApplyReport) -> usize`: the sum of `failed.len()`;
    - `pub fn examples(lang: Lang, taken: &BTreeSet<String>, first_order: u32) -> Vec<Ambiance>`: the four from spec §6.4, in this order, pinned, with consecutive orders and names, subtitles and icons from the copy table. Their actions:
      - Bonne nuit: lights off, plugs off, covers close, house-wide;
      - Je m'absente: lights off, plugs off;
      - Soirée tranquille: lights on, brightness 30;
      - Un peu de lecture: none.
    - `pub fn action_sentence(action: &AmbianceAction, house: &HouseView, lang: Lang) -> String`, where `HouseView<'a> { entities: &'a BTreeMap<String, EntityView>, areas: &'a [AreaView], flows: &'a [(String, String)], zones: &'a [(String, String)] }`. Examples (EN):
      - "Every light in the house: off";
      - "Every light in Living room: on at 30 %";
      - "Every shutter in the house: close";
      - "Desk lamp: turn off" (reuse `flow_text::sentence` for device commands, through a `CommandParams` node);
      - "Heating, every zone: 16 °C for 12 h";
      - "Heating, every zone: back to the schedule";
      - "Automation Away lights: switch on".

      An unknown entity, area or flow is written as its id followed by " (missing)" / " (introuvable)".

- [ ] **Step 1: Write the failing native tests** in `ambiances/mod.rs`:
  - `pinned_for_home_caps_at_four_by_order`: 6 pinned, with orders 5, 1, 3, 2, 6, 4. Gives the ids with orders 1 to 4.
  - `pinned_for_home_skips_invalid`, and `…skips_unpinned`.
  - `slug_id_suffixes_on_collision`: `"Bonne nuit"` with `{"bonne_nuit"}` taken → `"bonne_nuit_2"`; `"!!!"` → `"ambiance"`.
  - `next_order_after_max`.
  - `examples_fr_and_en`: the names in FR, and the EN names from the copy table. Ids are unique against `taken`. Soirée tranquille's action is `Category { what: Lights, to: On, brightness: Some(30), area: None, except: [] }`.
  - `action_sentences`: one per example above, in EN and FR, plus the missing-entity case.
  - `failed_count_sums_actions`.
  - In `house.rs` tests: `ambiances_changed_event_sets_list` and `ambiance_applied_event_stores_report`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-web ambiances`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the API functions, the House wiring and the helpers.** The keys and functions in `i18n.rs` that `examples` and `action_sentence` need are added in this task: the "Examples", "Action sentences" and "Common" rows of the copy table.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p oikos-web && cargo check -p oikos-web --target wasm32-unknown-unknown`
Expected: PASS. The `group_wizard` tests still pass.

- [ ] **Step 5: Commit** with `feat(web): ambiance client, house signals and helpers`.

### Task 2: Icons

**Files:**
- Modify: `crates/oikos-web/src/icons.rs`

**Interfaces:**
- Produces: `pub fn ambiance_icon(name: Option<&str>) -> &'static str`, returning one SVG per `oikos_api::AMBIANCE_ICONS` name. Unknown or `None` gives `SPARKLE`. Also produces `pub const SPARKLE: &str`, used by the sidebar.

- [ ] **Step 1: Write the failing test** `every_ambiance_icon_has_a_glyph`: for each name in `AMBIANCE_ICONS`, `ambiance_icon(Some(name))` is non-empty and distinct from the others. `ambiance_icon(None) == SPARKLE`.
- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p oikos-web icons`
Expected: FAIL.

- [ ] **Step 3: Draw the 12 glyphs** with the `svg!` macro, in the existing 24×24 stroke style. Reuse `MOON`, `SUN` and `HOUSE` where they exist.
- [ ] **Step 4: Run the test**

Run: `cargo test -p oikos-web icons`
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(web): ambiance icons`.

### Task 3: "Mes ambiances" page, report and "How it works"

**Files:**
- Create:
  - `crates/oikos-web/src/ambiances/page.rs` (`AmbiancesScreen`);
  - `crates/oikos-web/src/ambiances/report.rs` (`AmbianceReportScreen`);
  - `crates/oikos-web/src/ambiances/about.rs` (`AmbiancesAboutScreen`);
  - `crates/oikos-web/src/ambiances/actions.rs`:
    - `pub async fn apply_and_toast(id: String, name: String, toasts: Toasts, lang: Lang) -> bool`. It toasts `ambiance_done(lang, name)` when `failed_count == 0`, `ambiance_partial(lang, name, n)` otherwise, or the `ApiError` text when the request failed. It returns whether the request succeeded. Toasts are text-only, so the partial toast carries no link; the report stays reachable from the page;
    - `pub async fn add_examples(house: House, toasts: Toasts, lang: Lang)`. It PUTs `examples(lang, ids, next_order)` one by one, and an error toasts;
  - `crates/oikos-web/styles/ambiances.css`.
- Modify:
  - `crates/oikos-web/index.html` (link `styles/ambiances.css` after `cards.css`);
  - `crates/oikos-web/src/app.rs`: routes `/ambiances`, `/ambiances/about`, `/ambiances/new`, `/ambiances/:id`, `/ambiances/:id/report`. Declare `about` and `new` before `:id`. `new` and `:id` point at Task 4's editor; until then, a placeholder that Task 4 replaces;
  - `crates/oikos-web/src/shell/route.rs`:
    - `Section::Ambiances`;
    - `["ambiances", ..] => Section::Ambiances`;
    - crumbs: My moods → New mood / the ambiance name / Last run / How it works;
    - `Lookup::Ambiance(&str)`;
  - `crates/oikos-web/src/shell/header.rs` (resolve `Lookup::Ambiance` from `house.ambiances`);
  - `crates/oikos-web/src/shell/sidebar.rs`: the entry with the `SPARKLE` glyph and `Key::Ambiances`, after the dashboards `<li>` and before Automations;
  - `crates/oikos-web/src/i18n.rs` ("Page", "Report", "About" and "Toasts" rows of the copy table; update the `all_is_complete` count).
- Test: `crates/oikos-web/tests/ambiances_page.rs` (new; same harness as `tests/group_wizard.rs`)

**Behaviour:**
- **List order:** cards in `(order, id)` order. Each shows the icon, name and subtitle, plus:
  - a pin toggle, which PUTs with `pinned` flipped;
  - **Apply**, which calls `apply_and_toast`;
  - **Edit**, which links to `/ambiances/{id}`;
  - **Delete**, through `ConfirmBar`;
  - ↑ and ↓ buttons, which swap `order` with the neighbour through two PUTs;
  - a "Last run" link to `/ambiances/{id}/report`, shown only when `ambiance_reports` has one.
- **Invalid entries:** an invalid entry shows its id, its `invalid` reason and Delete only.
- **Hint:** when more than 4 valid ambiances are pinned, show `Key::AmbianceOnlyFour` above the list.
- **Empty state** (zero ambiances, valid or not): a short line, a "New mood" button and "Add the four examples". The examples button runs `add_examples`, and is disabled from the click until it returns. It is re-enabled after an error.
- **Header:** a "New mood" button (→ `/ambiances/new`) and a "How it works" link.
- **Report page:** the ambiance name, `started_at` as a local time (reuse `i18n::home_time`), and one line per action: `action_sentence` and the outcome label. Under it, the failed entities, then the skipped ones, each with its name (or id) and reason. Without a report it shows `Key::AmbianceNoReport`.
- **About page:** the three paragraphs of the copy table, and a link to `/ambiances`.

- [ ] **Step 1: Write the failing wasm tests:**
  - `sidebar_has_my_moods_between_overview_and_automations`;
  - `page_lists_in_order_with_actions`;
  - `page_shows_only_four_hint`;
  - `invalid_ambiance_card_offers_delete_only`;
  - `empty_state_offers_examples`;
  - `examples_button_disables_while_writing`: stub `fetch` as the existing tests that intercept REST do; if none do, assert the disabled attribute right after the click, before `tick()` resolves the request;
  - `report_page_lists_failed_and_skipped`;
  - `crumbs_for_ambiance_routes`: native, in `route.rs` tests.
- [ ] **Step 2: Run them to see them fail**

Run: `wasm-pack test --headless --chrome --test ambiances_page`, in `crates/oikos-web`.
Expected: FAIL.

- [ ] **Step 3: Implement the three screens, the routes, the sidebar entry and the CSS.** Cards reuse the look of `.flow-card` from `screens/flows/card.rs`; check that the classes exist before reusing them.
- [ ] **Step 4: Run the tests**

Run: the same wasm test, then `cargo test -p oikos-web`.
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(web): my moods page, report and how-it-works`.

### Task 4: Editor

**Files:**
- Create:
  - `crates/oikos-web/src/ambiances/editor.rs` (`AmbianceEditorScreen`: the form, Save, Try it, Delete);
  - `crates/oikos-web/src/ambiances/action_forms.rs` (one component per action type, plus the "Add an action" chooser).
- Modify:
  - `crates/oikos-web/src/app.rs` (point `/ambiances/new` and `/ambiances/:id` at the editor);
  - `crates/oikos-web/src/flow_picker.rs:82` (make `commands` `pub(crate)`);
  - `crates/oikos-web/src/i18n.rs` (the "Editor" rows);
  - `crates/oikos-web/styles/ambiances.css`.
- Test: `crates/oikos-web/tests/ambiance_editor.rs`

**Interfaces:**
- Consumes: `apply_and_toast` (Task 3) for Try it; `category_members` (Plan 1); `flow_picker::commands`; `members_by_room` from `group_wizard.rs` (its room grouping, reused for the inline list); `api::climate::get_climate()` for the zones and `enabled`; `House.{entities, areas, flows}`.
- Produces: `pub fn device_commands(e: &EntityView) -> Vec<Command>` in `action_forms.rs`. It is `flow_picker::commands(e, None)` plus:
  - `SelectOption` for each select option (`capabilities` options, as `cards/select.rs` reads them);
  - `SetHvacMode` for each climate mode;
  - `SetPosition { 50 }` for covers with position.

  Commands with a value (`TurnOn` brightness on a dimmable light, `SetPosition`, `SetTemperature`, `SetNumber`) show a number field beside the choice.

**Behaviour:**
- **Fields:**
  - name (required, at most 60), subtitle (at most 80);
  - an icon picker: a grid of the 12 `ambiance_icon`s, as radio buttons with `aria-label` set to the icon name;
  - a "Show on Home" toggle.

  New ambiances default to pinned when fewer than 4 are pinned.
- **Action list:** each row shows `action_sentence`, ↑, ↓, ✕ and an expand toggle that opens its form. "Add an action" offers the four types. The heating type is listed only when `get_climate()` reports `enabled`.
- **Category form:**
  - What (lights / plugs / covers);
  - To, offering only the valid pairs;
  - Brightness %, shown only for lights + on;
  - Where (the whole house, or each area).

  The live count comes from `category_members` over `House.entities`, minus the `except` ids. A "see which" toggle shows the inline list with checkboxes (see "Decisions made while planning").
- **Device form:** a search box over `House.entities` by name (accents folded with `fold_accents`), then a select of `device_commands`, then the value field if any.
- **Heating form:**
  - the zone (every zone, or each zone);
  - "Hold" or "Back to the schedule";
  - for Hold: a setpoint number (5 to 30, step 0.5) or an "Off" checkbox, an optional mode (heat/cool) and hours (1 to 720).
- **Automation form:** a flow select from `House.flows`, and Switch on / Switch off / Run.
- **Missing references:** a row whose entity, flow or area is not in the house gets the class `ambiance-action--missing` and the "(missing)" sentence. It is kept on Save.
- **Save:**
  - a new ambiance gets `id = slug_id(name, ids, "ambiance")` and `order = next_order`;
  - an edit keeps both;
  - unknown fields are kept: start from the stored `Ambiance` and replace only the edited fields;
  - success goes back to `/ambiances`. A 422 toasts the hub's text and stays on the form.
- **Try it:** disabled while the form is dirty or new; otherwise it applies, as Task 5 does.
- **Delete:** only on an existing ambiance, through `ConfirmBar`.

- [ ] **Step 1: Write the failing tests:**
  - native: `device_commands_for_select_lists_options`, `device_commands_for_climate_lists_modes` and `device_commands_for_light`;
  - wasm:
    - `new_editor_saves_slug_and_next_order`: inspect the PUT body through the existing request-capture approach, or by asserting on `House.ambiances` after a stubbed reply;
    - `category_count_follows_area_and_except`;
    - `brightness_field_only_for_lights_on`;
    - `heating_type_hidden_when_climate_off`;
    - `missing_entity_row_is_marked`;
    - `try_it_disabled_while_dirty`;
    - `unknown_fields_survive_edit`.
- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p oikos-web device_commands`, then `wasm-pack test --headless --chrome --test ambiance_editor`.
Expected: FAIL.

- [ ] **Step 3: Implement the editor and the forms.**
- [ ] **Step 4: Run the tests**

Run: the same commands.
Expected: PASS.

- [ ] **Step 5: Commit** with `feat(web): ambiance editor`.

### Task 5: Home row

**Files:**
- Create: `crates/oikos-web/src/ambiances/row.rs` (`HomeAmbiances`)
- Modify:
  - `crates/oikos-web/src/screens/home.rs:118` (`<HomeAmbiances />` right after `<HomeHeader />`);
  - `crates/oikos-web/src/i18n.rs` (the "Home row" rows);
  - `crates/oikos-web/styles/ambiances.css`.
- Test: `crates/oikos-web/tests/home_ambiances.rs`

**Interfaces:**
- Consumes: `apply_and_toast` and `add_examples` (Task 3); `pinned_for_home` (Task 1).

**Behaviour:**
- **Visibility:** hidden until `ambiances_loaded`.
- **Header:** the title `Key::AmbiancesRowTitle`, with the "How it works" link on the right (→ `/ambiances/about`).
- **Tiles:** the `pinned_for_home` list: an icon in a soft square, the name, and the subtitle. An ambiance with no actions (such as the "Un peu de lecture" example) shows `Key::AmbianceAddLamp` as its subtitle, and links to its editor instead of applying.
- **Layout:** 4 across, or 2×2 below 700 px.
- **A tap:**
  1. sets `aria-busy="true"` and a pressed class;
  2. calls `apply_and_toast`;
  3. clears both when it returns, whatever happened.
- **Invitation tile**, when there are no pinned ambiances:
  - with zero ambiances at all: "Create a mood" (→ `/ambiances/new`) and an "Add the four examples" button that runs `add_examples`;
  - otherwise: "Choose your moods" (→ `/ambiances`).

- [ ] **Step 1: Write the failing wasm tests:**
  - `row_hidden_until_loaded`;
  - `row_shows_first_four_pinned`;
  - `tile_tap_toasts_done`, with a stubbed reply where `failed` is empty;
  - `tile_tap_toasts_partial_count`;
  - `tile_busy_clears_on_error`: a stubbed 503;
  - `invitation_with_no_ambiances_offers_examples`;
  - `invitation_with_unpinned_links_to_page`;
  - `empty_reading_tile_links_to_editor`.
- [ ] **Step 2: Run them to see them fail**

Run: `wasm-pack test --headless --chrome --test home_ambiances`
Expected: FAIL.

- [ ] **Step 3: Implement the row.**
- [ ] **Step 4: Run the tests**

Run: the same command, plus the existing `--test home_header` and `--test rooms`.
Expected: PASS, with no layout regression in those tests.

- [ ] **Step 5: Check the result live.** Use headless Chrome over CDP (see the memory note "Headless Chrome CDP screenshots"; the window must be at least 500 px wide). Take screenshots of Home at 1280 and 500 px wide in Ivoire and Night Glass, of the page, and of the editor. Attach them in the task report.
- [ ] **Step 6: Commit** with `feat(web): one mood, one tap on Home`.

### Task 6: Docs

**Files:**
- Modify: `docs/users/web-ui.md` (a "Moods" section: the Home row, the page, the editor, the report); `docs/users/ambiances.md` from Plan 1 (link to the web section).

- [ ] **Step 1: Write the section** with generic wording only. Mention that categories include devices added later, and that Try it applies the saved version.
- [ ] **Step 2: Run the full workspace checks**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p oikos-web --target wasm32-unknown-unknown`, then in `crates/oikos-web`: `wasm-pack test --headless --chrome`.
Expected: PASS.

- [ ] **Step 3: Commit** with `docs: moods in the web UI`.

## Copy table (EN / FR, exact)

| Row | Key or fn | EN | FR |
|---|---|---|---|
| Common | `Ambiances` | My moods | Mes ambiances |
| Common | `NewAmbiance` | New mood | Nouvelle ambiance |
| Common | `AmbianceMissing` (suffix) | (missing) | (introuvable) |
| Examples | names / subtitles | Good night / The house goes to sleep; I'm heading out / Peace of mind; Quiet evening / Dimmed lights; A little reading / Your favourite spot | Bonne nuit / La maison s'endort; Je m'absente / L'esprit tranquille; Soirée tranquille / Lumières tamisées; Un peu de lecture / Votre coin préféré |
| Examples | icons | moon, door_out, sofa, book | (same) |
| Action sentences | category | Every light / Every plug / Every shutter, then " in the house" or " in {area}", then ": off", ": on", ": on at {n} %", ": open" or ": close" | Tous les éclairages / Toutes les prises / Tous les volets, then " de la maison" or " — {area}", then " : éteindre", " : allumer", " : allumer à {n} %", " : ouvrir" or " : fermer" |
| Action sentences | climate | Heating, every zone / Heating, {zone}: {t} °C (or off) for {h} h; …: back to the schedule | Chauffage, toutes les zones / Chauffage, {zone} : {t} °C (ou arrêt) pendant {h} h ; … : retour au programme |
| Action sentences | flow | Automation {name}: switch on / switch off / run | Automatisation {name} : activer / désactiver / lancer |
| Page | `AmbianceApply` | Apply | Lancer |
| Page | `AmbianceShowOnHome` | Show on Home | Afficher sur l'accueil |
| Page | `AmbianceOnlyFour` | Only the first 4 pinned moods show on Home. | Seules les 4 premières ambiances épinglées s'affichent sur l'accueil. |
| Page | `AmbianceEmpty` | No mood yet. | Aucune ambiance pour l'instant. |
| Page | `AmbianceAddExamples` | Add the four examples | Ajouter les quatre exemples |
| Page | `AmbianceLastRun` | Last run | Dernier lancement |
| Report | `AmbianceNoReport` | Not run since this page was opened. | Pas lancée depuis l'ouverture de la page. |
| Report | outcomes | Done / Failed / Skipped | Fait / Échec / Ignoré |
| About | `AmbiancesHowItWorks` | How it works | Comment ça marche |
| About | paragraph 1 | A mood puts the house in a known state with one tap: it runs its actions once, in order. | Une ambiance met la maison dans un état connu d'un seul geste : elle lance ses actions une fois, dans l'ordre. |
| About | paragraph 2 | "Every light", "every plug" and "every shutter" include devices you add later. Door, siren and chime switches are never counted as plugs. | « Tous les éclairages », « toutes les prises » et « tous les volets » incluent les appareils ajoutés plus tard. Les interrupteurs de porte, sirènes et carillons ne comptent jamais comme des prises. |
| About | paragraph 3 | After a tap, a message says it is done or how many devices did not respond; the last run of each mood is on its page. | Après un geste, un message dit si c'est fait ou combien d'appareils n'ont pas répondu ; le dernier lancement de chaque ambiance est sur sa page. |
| Editor | labels | Name; Subtitle; Icon; Actions; Add an action; Every light, plug or shutter; One device; Heating; Automation; Where; Whole house; See which; Hold; Back to the schedule; Hours; Every zone; Switch on; Switch off; Run; Save; Try it; Save first | Nom; Sous-titre; Icône; Actions; Ajouter une action; Éclairages, prises ou volets; Un appareil; Chauffage; Automatisation; Où; Toute la maison; Voir lesquels; Maintenir; Revenir au programme; Heures; Toutes les zones; Activer; Désactiver; Lancer; Enregistrer; Essayer; Enregistrez d'abord |
| Editor | fn `category_count(lang, n)` | 1 device / {n} devices | 1 appareil / {n} appareils |
| Home row | `AmbiancesRowTitle` | One mood, one tap | Une ambiance, un geste |
| Home row | `AmbianceInvite` | Create a mood | Créer une ambiance |
| Home row | `AmbianceChoose` | Choose your moods | Choisir vos ambiances |
| Home row | `AmbianceAddLamp` | Add a lamp | Ajoutez une lampe |
| Toasts | fn `ambiance_done(lang, name)` | {name}: done | {name} : c'est fait |
| Toasts | fn `ambiance_partial(lang, name, n)` | {name}: 1 device did not respond / {n} devices did not respond | {name} : 1 appareil n'a pas répondu / {n} appareils n'ont pas répondu |
