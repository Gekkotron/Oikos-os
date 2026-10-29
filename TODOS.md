# TODOS

Running backlog of ideas and deferred follow-ups. Priority: **P1** = correctness or user-visible safety (do next), **P2** = polish or test coverage, **P3** = cosmetic or opportunistic.

## Dashboards (landed on `feat/dashboards`, 2026-10-22; triaged 2026-10-24)

### P3 — cosmetic / backlog

- Two concurrent PUTs can pick the same `next_free_order` (lock released between read and publish). Fine single-user; add a guard if the hub grows multi-writer.

## Climate engine v1 (landed on sc-critical-meissner-823c, 2026-10-22)

Spec: `docs/superpowers/specs/2026-10-22-climate-engine-design.md`. Plan: `docs/superpowers/plans/2026-10-22-climate-engine.md`.

- Follow-up designs, each its own brainstorm: shutters, comfort ↔ cost slider with prices, ML layers.
- **Open:** run the DB-gated climate tests against a TimescaleDB (`TEST_DATABASE_URL=… cargo test -p oikos-os --test history_db climate learn_zone`).

### Deferred follow-ups (from the SDD reviews; none blocks enabling the feature)

- **T3** hold_devices skips a device override the device can't do; device keeps zone target (decide.rs hold_devices)
- **T5** day_agrees measures against the raw run median, not the final fitted setpoint; 80 % slot rule looser than spec's "start/end within 30 min" for long blocks (learn.rs day_agrees)
- **T5** combine() float summation order depends on device order (learn.rs combine)
- **T7** no test pins seq tie-break for equal times (history_db.rs)
- **T8** unconfirmed entry expiry 120 s can turn a late-applied engine command into a hand change — Ruling: real but rare; holds the engine's own value (safe) — deferred.
- **T8** safety alerts not persisted, re-pushed after restart — Ruling: real, low impact — deferred.
- **T8** overheat/overcool reason text is false while latched inside the hysteresis band (decide.rs:264, 303)
- **T8** a dropped pass's abandoned command is not moved to unconfirmed (engine.rs:61)
- **T8** broker dropping during a confirm wait still counts a failure (engine.rs:629)
- **T9** autumn repeated hour merges samples in micros_local (learning.rs:68) — Ruling: real, ≤1 h/year of evidence — deferred.
- **T9** daily push limit seeded from a pending proposal that may never have been pushed (one suppressed push worst case)
- **T10** decision-log `before` truncated to whole seconds can omit a same-second row (climate/hub.rs:131)
- **T11** until/override text refreshes only on zone data change, not clock ticks (zone_card.rs)
- **T12** Off→On restores base/nearest, not the block's own pre-Off temperature (timeline.rs BlockEditor)
- **T12** handles tiny on one-step blocks (each is a quarter of the bar at most)
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
- [ ] System health screen + uptime history — record the hub's uptime, CPU, RAM, disk and temperature over time (and the broker / Z2M / database up or down), and add a screen that shows the current values and their history. Today `oikos-watchdog` only pushes gateway and ACL alerts; nothing is stored or shown.
- [ ] Network presence by ping — let the hub ping a configured host (IP or hostname) on an interval and publish it as a binary sensor (`connectivity`), so flows and the UI can tell whether a device is on the network (a phone at home, a TV, a NAS), with a debounce so one lost reply does not flip it.

### Flows

- [ ] Bulk actions on the Flows page — enable, disable, delete or move several flows at once from the list. Deferred from the flow-list redesign (2026-10-22): the list acts on one flow at a time.
- [ ] Automatic room detection for flows — suggest a flow's `area` from its trigger and action entities instead of leaving it unassigned; today the room is set by hand only.
- [ ] Android app support for a flow's `area` — the app does not read or write it yet; the hub and web UI do.
