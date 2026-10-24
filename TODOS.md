# TODOS

Running backlog of ideas and deferred follow-ups. Priority: **P1** = correctness or user-visible safety (do next), **P2** = polish or test coverage, **P3** = cosmetic or opportunistic.

## Dashboards (landed on `feat/dashboards`, 2026-10-22; triaged 2026-10-24)

### P1 — correctness / forward-compat

- **History chart is English only on the FR UI** (widget and history page): "Last 24 hours", "now", weekday names, "min/max". Route `components/history_chart/{format,view}.rs` through `i18n.rs`.
- **HistoryChart widget ranges.** The chart only knows Day/Week/Month, so the widget's 1h, 6h and 24h ranges all show a day. Either add short ranges to the chart or limit the widget's range choice to what it can draw.
- **EnergyStat kWh never refreshes.** Today/week/month are fetched once (when the counter set changes); a dashboard left open overnight keeps yesterday's "Today". Refetch on a timer or at midnight.
- `Camera` / `Lock` widgets: not offered by the library yet; add them with their cards.

### P2 — polish / test coverage

- `Grid` keys tiles by widget id alone and `WidgetView` takes the widget once, so a title change from the config sheet only shows after a remount.
- Dashboards fetched once at startup; no retry/refresh on WS reconnect. Trigger a re-fetch on the WS reconnect event.
- Overlay z-index 1 vs tools z-index 2 depends on no inner card element exceeding z-index 1 — audit the new widget cards (`widgets.css`).
- Edits made to the draft while a Save is in flight are overwritten by the hub's reply.
- Deep-linking an authored dashboard id shows "Unknown dashboard" briefly before the fetch lands. Add a loaded flag and a loading state in `screens/dashboard.rs`.
- `WidgetConfigSheet` has no entity/target picker — swap-target requires delete + re-add.
- HistoryChart entry filter in the widget library uses Sensor entities only; spec says "any entity with history".

### P3 — cosmetic / backlog

- Two concurrent PUTs can pick the same `next_free_order` (lock released between read and publish). Fine single-user; add a guard if the hub grows multi-writer.
- `path/body id mismatch` returns 422 while flows PUT returns 400 — normalize across PUT routes.

## Climate engine v1 (landed on sc-critical-meissner-823c, 2026-10-22)

Spec: `docs/superpowers/specs/2026-10-22-climate-engine-design.md`. Plan: `docs/superpowers/plans/2026-10-22-climate-engine.md`.

- Follow-up designs, each its own brainstorm: shutters, comfort ↔ cost slider with prices, ML layers.
- **Open:** run the DB-gated climate tests against a TimescaleDB (`TEST_DATABASE_URL=… cargo test -p oikos-os --test history_db climate learn_zone`).

### Deferred follow-ups (from the SDD reviews; none blocks enabling the feature)

- **T1** ZoneSettings::validate does not range-check away_setpoint 0..=40 (doc.rs:444)
- **T1** ModeRules::validate does not check to <= 1440 for programmatic HhMm (doc.rs:277)
- **T2** no boundary test for age == stale_after_s (fresh when <=) (schedule.rs:129)
- **T2** no boundary test for outdoor == heat_below / cool_above (hold band) (schedule.rs:84)
- **T2** Stale.last prefers stale sensor over newer device value — display only (schedule.rs:152)
- **T3** hold_devices skips a device override the device can't do; device keeps zone target (decide.rs hold_devices)
- **T4** from_decisions depends on iteration order; hub must not let an older decision overwrite a newer one (ledger.rs:62)
- **T4** no boundary tests at exactly 30/31 s echo and 120/121 s drift
- **T4** mode comparison case-sensitive (ledger.rs:109)
- **T5** day_agrees measures against the raw run median, not the final fitted setpoint; 80 % slot rule looser than spec's "start/end within 30 min" for long blocks (learn.rs day_agrees)
- **T5** combine() float summation order depends on device order (learn.rs combine)
- **T6** effective_areas rebuilt per climate entity (climate/hub.rs:341)
- **T6** disabled test's last ingest_message has no assertion (climate_hub.rs)
- **T6** ingest_climate applies non-retained messages too (climate/hub.rs:314)
- **T7** no test pins seq tie-break for equal times (history_db.rs)
- **T8** unconfirmed entry expiry 120 s can turn a late-applied engine command into a hand change — Ruling: real but rare; holds the engine's own value (safe) — deferred.
- **T8** safety alerts not persisted, re-pushed after restart — Ruling: real, low impact — deferred.
- **T8** overheat/overcool reason text is false while latched inside the hysteresis band (decide.rs:264, 303)
- **T8** a dropped pass's abandoned command is not moved to unconfirmed (engine.rs:61)
- **T8** broker dropping during a confirm wait still counts a failure (engine.rs:629)
- **T8** stray blank lines before `cause: None,` in struct literals
- **T9** autumn repeated hour merges samples in micros_local (learning.rs:68) — Ruling: real, ≤1 h/year of evidence — deferred.
- **T9** daily push limit seeded from a pending proposal that may never have been pushed (one suppressed push worst case)
- **T10** approve route parses its body outside Body<T>/JsonRejection (inconsistent 422 path)
- **T10** pre-existing workspace clippy failure in oikos-web dashboards/grid.rs:57 (not from this branch)
- **T10** decision-log `before` truncated to whole seconds can omit a same-second row (climate/hub.rs:131)
- **T11** until/override text refreshes only on zone data change, not clock ticks (zone_card.rs)
- **T11** NumField in settings_sheet.rs has no is_finite guard
- **T11** decision rows show HH:MM without the day for older rows
- **T12** Off→On restores base/nearest, not the block's own pre-Off temperature (timeline.rs BlockEditor)
- **T12** handles tiny on one-step blocks; duplicate .climate-timeline__handle CSS rule (main.css:4619)
- **T12** temperature input doesn't track refresh after a refused/clamped value
- **Final** re-activating a zone (Off→Active) can hold devices changed by hand while it was off, up to manual_hold_max — Ruling: defensible by analogy with §7's restart rule, frost still acts; real, deferred (fix: forget the zone's ledger entries when it leaves Active).
- **Final** overlapping learning sequences / manual learn_climate_zone bypasses claim_run — Ruling: practically unreachable, conclude is serialised — deferred.
- **Final** one failed re-assert right after reconnect turns drift into a hold after ~121 s — Ruling: never fights the user; frost overrides — deferred.

## Other backlog ideas (unrelated to dashboards)

### Homey-gap features

- [ ] Category views — the Home grouped by what a device *does*, not just by room. Not started (the `feat/category-views` branch is gone).

### Hub & web polish

- [ ] Dashboard-wide search bar — the current search is only on the Z2M diagnostics screen (`screens/devices.rs`); add an equivalent on the dashboard/Home so any entity can be found by name across rooms.
- [ ] Card titles break mid-word on the entity sheet readout card.
- [ ] Vacuum card animation — while the vacuum is cleaning, show a small brush in the card's top-right corner that moves (spins or sweeps), idle otherwise. See `cards/vacuum.rs`, `components/robot.rs` and the vacuum rules in `styles/cards.css`; respect `prefers-reduced-motion`.
- [ ] Address the ~47 deferred minors from the flow-canvas redesign SDD ledger (`.superpowers/sdd/2026-10-20-flow-canvas-redesign/`).
- [ ] Housekeeping: cap/clean `target/` so it stops filling the disk.

### Solar card and page — readability (reported 2026-10-24)

The Home solar card and the `/solar` page read as messy (`solar/card.rs`, `solar/curve.rs`, `screens/solar.rs`, strings in `i18n.rs`):

- [ ] Headline "4,0 / 3,8 kWh" has no labels — say which figure is measured and which is the forecast (e.g. "4,0 kWh produits · prévu 3,8").
- [ ] **"reste ≈ 1,6" reads as nonsense once production beats the forecast.** It is `now.remaining_today` = the forecast summed from now to midnight (`oikos-solcast` `remaining()`), so "4,0 / 3,8 · reste ≈ 1,6" seems to say 1,6 kWh are still missing although 4,0 > 3,8 is already done. Replace it with an end-of-day projection when the measured figure is known: "fin de journée ≈ 5,6 kWh" = measured + forecast still to come (EN "≈ 5.6 kWh by tonight"); fall back to "encore ≈ 1,6 kWh prévus" without a measurement. Always print the kWh unit. Applies to the card and `/solar` (`i18n::solar_bottom`).
- [ ] The card caption wraps mid-group ("Demain 1,9 kWh (1–" / "3)"); keep "(1–3)" with its value (non-breaking space or a shorter line), or drop the range on the card.
- [ ] The measured fill is opaque and drawn over the forecast: before "now" it hides the forecast band and the dashed median, so measured vs forecast can't be compared. Draw measured as a line (or a translucent fill) above the band, and keep the median visible across the whole day.
- [ ] The measured series ends with a steep straight drop to a vertical edge just before the now line (linear join of the last hourly point to the current reading). Smooth or step it, and end it exactly at now.
- [ ] The p10–p90 band colour is a muddy grey-brown on the dark theme; use a translucent tint of the solar accent.
- [ ] The page chart has a single gridline ("1,0 kW") and no time axis; add hour ticks (e.g. 6h, 12h, 18h) and a second kW gridline.
- [ ] The dashed now line overshoots the top of the plot area; clip it to the chart.

### Flows

- [ ] Bulk actions on the Flows page — enable, disable, delete or move several flows at once from the list. Deferred from the flow-list redesign (2026-10-22): the list acts on one flow at a time.
- [ ] Automatic room detection for flows — suggest a flow's `area` from its trigger and action entities instead of leaving it unassigned; today the room is set by hand only.
- [ ] Android app support for a flow's `area` — the app does not read or write it yet; the hub and web UI do.
