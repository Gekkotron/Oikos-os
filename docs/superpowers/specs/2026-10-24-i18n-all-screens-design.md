# Every web screen in French and English — design

Date: 2026-10-24. Scope: the web UI (`crates/oikos-web`). This is project 5 of the Ivoire redesign (see `2026-10-23-ivoire-theme-and-sidebar-design.md`), called **project A** below. A follow-up, **project B**, translates the sentences the hub writes; it is outlined at the end so A does not get in its way, and gets its own design.

## Why

Project 1 built the FR/EN mechanism (`src/i18n.rs`: `Lang` signal, `localStorage`, browser default, `<html lang>`) and translated the shell, the home header, weather, solar and ambiances. About 34 of the 148 source files use it. Every other screen still has its English (and a few French) words written in the code, so the French UI switches language from one panel to the next. The user wants the whole UI in either language, following the toggle.

## Goals

1. **Every word the web writes itself follows the language toggle**: page and section titles, buttons, labels, placeholders, `aria-label`s and `title`s, empty states, toasts, confirmation questions, error banners, sheet headers, chart axes and legends, units spelled as words, relative times ("il y a 5 min"), day names and month names.
2. **Dates, times and numbers follow the language, not the browser**: decimal comma and `U+202F` thousands in French ("9,5 °C", "1 250 W"), decimal point in English. Weekdays, months and date order per language. The two `to_locale_date_string("default", …)` calls (alerts list and alert detail) move to the language-aware helpers.
3. **Nothing stays English-only on the French UI, and nothing stays French-only on the English UI.** About 50 hard-coded French strings exist today outside `i18n.rs`; they move into the table like the English ones.
4. **A guard keeps it that way**: a test fails when a new user-facing literal lands outside the string table (see "Guard").

## Non-goals

- **Names the user typed are never translated**: rooms, devices, entities, flows, groups, ambiances, dashboards, virtuals, zones and camera names are shown as stored.
- **Text written by the hub**: the body of a hub `ApiError`, a `CommandResponse.reason`, an ambiance `Problem.reason`, an alert's `title`/`body`, a flow run's step log. A still shows these as they come (English), inside a translated lead where there is one ("Échec de l'enregistrement : <hub text>"). Project B replaces them. A must **not** render the alerts' `loc` hint in the web: project B replaces that path with `text: {en, fr}`.
- The Android app, the docs site, the MCP tool descriptions and the hub logs.
- Adding a third language. The design must not make it hard (see "String table"), but A ships FR and EN only.
- Translating raw device values that are the device's own vocabulary (a Z2M `select` option such as `power_on_behavior: previous`, an effect name, a Frigate label shown in a raw field). They keep today's prettifying (`format::word`, `choice_word`).

## What gets translated

Every file under `crates/oikos-web/src` that writes user-facing text. The biggest by count of hard-coded strings today (rough grep): `override_editor.rs`, `security.rs`, `screens/settings.rs`, `screens/flow_canvas/settings.rs`, `flow_list.rs`, `flow_picker.rs`, `shell/route.rs`, `components/history_chart/format.rs`, `format.rs`, `climate/*`, `screens/entity_sheet.rs`, `dashboards/library.rs`, `screens/group_wizard.rs`, `screens/flow_canvas/toolbar.rs`, `cards/*`, `screens/flows/*`, `ambiances/action_forms.rs`, `schedule.rs`, `energy.rs`, `energy_day.rs`, `flow_text.rs`, `screens/devices.rs`, `screens/alerts.rs`, `screens/alert_detail.rs`, `screens/history_detail.rs`, `screens/home.rs`, `screens/groups.rs`, `screens/group_sheet_v2.rs`, `screens/generic_panel.rs`, `screens/flow_schedule.rs`. That is roughly 1,000–1,500 strings. The builder works from the code, not from this list.

Included, because they are written by the web:

- **Generated sentences**: flow card titles and descriptions (`flow_text.rs`: "Turn on X", colour and effect names), schedule summaries (`schedule.rs`, cron → "Every day at 07:00"), climate sentences and proposals, group state lines, the history chart's "Last 24 hours", "now", "min/max", weekday ticks (this closes the TODOS P1 item "History chart is English only on the FR UI").
- **Entity state words the web chooses**: on/off, open/closed/opening/closing/stopped, locked/unlocked, unavailable/unknown, heat/cool/auto/off and the other HVAC modes, battery low, motion/clear, door open/closed, `kind_word` / `type_word` (Light, Switch, Cover…), Show-as types.
- **The web's own errors**: "network error: …", "HTTP 500", `BROKER_OFFLINE`, validation messages the web computes before sending (empty name, bad duration…).
- **Document title** (`<title>`), if a screen sets one.

## How it is built

### String table

Keep the project-1 mechanism and its style: plain Rust, `match` on an enum to an `(en, fr)` pair, `fn …(lang: Lang, …) -> String` helpers for sentences with values, `use_text()` / `use_lang()` in components. No external i18n crate, no runtime file loading.

`i18n.rs` is already 2,100 lines and would grow four or five times, so it becomes a module: `src/i18n/mod.rs` keeps `Lang`, storage, `install_lang`, `use_lang`, `use_text` and the shared format helpers; one file per area holds its keys and helpers (`i18n/flows.rs`, `i18n/energy.rs`, `i18n/security.rs`, `i18n/climate.rs`, `i18n/devices.rs`, `i18n/states.rs`, `i18n/format.rs`, …). The existing `Key`, `HomeText`, `WeatherText`, `SolarText`, `WidgetText`… stay where the builder sees fit; moving them is allowed, renaming their variants is not needed. (My call: one file per area, so a screen's strings are found next to each other and two people translating two areas do not edit one file.)

Each entry gives both languages in one place, so a missing French text is a compile error, not a runtime gap. A third language later means adding one field per entry; nothing in A should rely on there being exactly two beyond the `(en, fr)` pairs themselves.

### Formatting

- One number formatter takes the language (the existing `home_number` / `solar_kw` style): French uses `,` and a narrow no-break space (`U+202F`) for thousands, English `.` and a thin space as today. Units keep a no-break space before them in both (`U+00A0`).
- `format::display_value`, `degrees`, `number`, `format_timestamp` and the chart axis formatters gain a `Lang` (or read it from context), so every value on every card follows the language.
- Dates and times: 24-hour clock in both languages (my call: it is what the UI shows today and what a French house expects; English keeps 24 h too, no AM/PM). Weekday and month names come from the table, not from the browser.
- Relative times ("5 min ago" / "il y a 5 min", "yesterday" / "hier") from the table.

### Reactivity

Text must update **without a reload** when the toggle changes, like the shell does today: components read the language through the context signal, not once at mount. Memos and signals that cache formatted text depend on the language signal.

### French typography

Match what project 1 already does: a no-break space before `:` `;` `?` `!` and inside `« »` if quotes are used; `…` as one character; sentence case for titles ("Nouvelle automatisation", not "Nouvelle Automatisation"); `U+00A0` between a number and its unit, `°C` included, in both languages.

### Wording

The words follow what the shell already uses: Automatisations, Groupes, Pièces, Ambiances (UI: "Mes ambiances"), Énergie, Sécurité, Climat, Météo, Solaire, Appareils for devices, Entité for entity, Volet for a cover, Prise for a plug. Where a French term is not obvious, the builder picks the usual Home Assistant / Homey French wording and lists the choices in the PR description so the user can correct them in one pass.

### Glossary

The main terms, fixed before the area work starts so every screen uses the same word. Terms marked **(shell)** already ship in project 1. The user reviews this list with the spec. Any later change is made here first, then applied everywhere.

| English | French |
|---|---|
| Overview | Vue d'ensemble (shell) |
| Automations / flow | Automatisations / automatisation (shell) |
| Trigger · Condition · Action | Déclencheur · Condition · Action |
| Delay | Attente |
| Schedule | Programmation |
| Device · Entity | Appareil (shell) · Entité |
| Room | Pièce (shell: "Mes pièces") |
| Group | Groupe (shell) |
| Mood | Ambiance (shell) |
| Dashboard | Tableau de bord (shell: "Nouveau tableau") |
| Widget | Widget |
| Light · Lights | Lumière · Éclairages (shell) |
| Switch · Plug | Interrupteur · Prise (shell) |
| Cover / shutter | Volet (shell) |
| Sensor · Thermometer | Capteur · Thermomètre |
| Climate · Heating · Setpoint | Climat (shell) · Chauffage · Consigne |
| Zone | Zone |
| Alert · Camera | Alerte (shell) · Caméra |
| Battery · Signal | Batterie · Signal |
| Settings | Réglages (shell) |
| Save · Cancel · Delete · Edit · Add · Rename · Back | Enregistrer · Annuler · Supprimer · Modifier · Ajouter · Renommer · Retour |
| Unavailable · Unknown | Indisponible · Inconnu |
| Light on / off | Allumé / Éteint (shell) |
| Switch on / off | Activé / Désactivé (shell) |
| Open / Closed (state) · Open / Close (button) | Ouvert / Fermé · Ouvrir / Fermer (shell) |
| Heat · Cool · Auto · Off (HVAC) | Chauffage · Climatisation · Auto · Arrêt (shell) |

### Decisions taken for the builder (my calls)

These came out of the builder check. They are mine, not the user's; the user can overturn any of them on review.

1. **Defaults the web writes into stored data** ("New flow", "New group", a new mood's name, a `notify` node's default title, a virtual's name, the "(copy)" suffix) are written **in the language active at creation**, then they are user data and are never re-translated. Where a field can stay empty and show a translated placeholder instead, prefer that.
2. **How `Lang` reaches code.** Pure functions (`format::*`, `flow_text`, `schedule`, chart formatters, `kind_word`, …) take an explicit `lang: Lang` parameter. Components read `use_lang()` / `use_text()` and pass the value down. Non-reactive code (event handlers, toast closures, chart drawing) reads the signal at call time with `.get_untracked()`, never a value captured at mount. Native tests pass the language explicitly. Falling back silently to English from a missing context is fine only at the very top (`use_lang`), never inside a formatter.
3. **State words.** A known vocabulary is translated. It comes from the Home Assistant entity model the hub already exposes: binary-sensor device classes (door, window, motion, occupancy, moisture, smoke, battery…), cover states, lock states, HVAC and fan modes, `on`/`off`, `unavailable`/`unknown`. Any other value goes through today's `word` / `choice_word` unchanged. The table is keyed by kind or device class plus state, so agreement is built in ("Porte ouverte", "Volet ouvert"). A device `select` option, an effect name and a free-text sensor value are never in the table.
4. **Search and filters** (flow card library, device search, entity pickers) match both the English and the French kind and state words, plus the raw id. Typing "cover" or "volet" both find covers, in either UI language.
5. **Plurals.** Sentence helpers take the count and hold explicit one/other forms. French treats 0 and 1 as singular ("0 lumière allumée", "1 min", "il y a 5 min"); English treats 1 as singular.
6. **PR split.** PR 1 covers steps 1–3 (module split, formatting, state words) plus the guard in **ratchet mode**: the allowlist starts with every file that is not translated yet, and each area PR removes its files from it. A translated file then fails CI if English creeps back in, while untranslated ones do not block. After PR 1, there is one PR per area (or a few areas together), each from `origin/main`. The last PR empties the file-level part of the allowlist.
7. **Glossary checkpoint.** The glossary above is approved with this spec. The first area PR (flows) also lists every new French term it introduced, for the user to correct before the other areas copy them.

### Guard

A native test (runs with `cargo test -p oikos-web`, no browser) scans `src/**/*.rs` outside `src/i18n/` and fails on a string literal that looks like user-facing text: a literal used as a `view!` text node, or as the value of `aria-label`, `title`, `placeholder` or `alt`, or passed to the toast/confirm helpers, that contains a letter. An allowlist file next to the test holds the exceptions (CSS classes, ids, icon names, MQTT keys, units like "kWh"), one per line with a short reason. The exact detection is the builder's call; it must catch the common case and have few false positives. (My call: a syntax-light scan is enough; parsing with `syn` is allowed if it stays simple.)

### Tests

- Existing tests run in English unless they give a language, so their assertions stay valid (project-1 rule).
- Per area, at least one test renders a representative screen in French and checks a few French words and the decimal comma, and the same screen in English.
- A table test checks that no entry has an empty `en` or `fr`, and that `fr != en` except for an allowlisted set (words that are the same in both, like "Solcast", "Zigbee2MQTT", "MQTT", "Wi-Fi").
- Toggling the language on a mounted screen updates its text (one browser test is enough).

## Suggested order

1. Split `i18n.rs` into the module, no behaviour change.
2. Language-aware formatting (`format.rs`, numbers, dates, relative times, chart axes) — every card benefits at once.
3. Entity state words, `kind_word` / `type_word`, cards.
4. Area by area: flows (list, canvas, picker, settings, schedule, `flow_text`), climate, energy and energy day, security, alerts (chrome only), devices and override editor, groups and group wizard, entity sheet and generic panel, dashboards and widget library, history detail and chart, settings sub-pages, home.
5. The guard is already in place from PR 1 (ratchet mode, see decision 6). The last area PR removes the last file entries, leaving only real exceptions.
6. A full pass in French on desktop and phone width (headless Chrome screenshots, see the CDP recipe already used for this crate), listing any string that slipped through.

Areas are independent after steps 1–3 and can be done in parallel.

## Done when

- Switching to Français shows no English word the web wrote itself, on every route, on desktop and phone width; switching back shows no French one. Hub-written sentences and user names are the only exceptions.
- Numbers and dates follow the toggle everywhere.
- The guard test is green with a short allowlist.
- `cargo test --workspace`, the clippy/fmt CI job and the wasm browser tests are green.

## Project B, for context (not part of A)

The hub writes about 230 sentences (validation errors in `oikos-core`, `oikos-api`, `oikos-os`, `oikos-climate`; refusals and report reasons; push and alert text in `oikos-push`, `oikos-frigate`, `oikos-watchdog`). Decided with the user:

- The hub gets its own FR/EN table and sends **every sentence it writes in all supported languages**: `text: {"en": "…", "fr": "…"}` next to the existing English field (`error`, `reason`, `body`) and the existing `loc: {key, args}` hint.
- Clients show `text[their language]` and fall back to `en`: the web uses its toggle, the Android app uses the phone's language. A new hub sentence therefore works on the phone immediately, with no app release; this is why the phone does not need to send its language and why `loc` alone was rejected.
- The English field stays for the MCP tools and the logs.

A should keep hub text in one place per screen (the error banner, the report line, the alert body) so B only has to swap `error` for `text[lang]` there.
