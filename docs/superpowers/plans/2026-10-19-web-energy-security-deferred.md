# Web Energy and Security tabs — deferred findings

Status: written 2026-10-19 after the implementation wave of `2026-10-19-web-energy-security.md`. Everything here is known, decided and left for a later wave.

## Needs the history service (OikosHistory, LAN :8090)

- **"x.x kWh today"** under each Energy node and card (app history spec §5.4), and the water node's "today" volume. Needs a `[history] url` in the hub config, a read-only proxy `GET /api/v1/history/{today,daily,series,last_change}` (OikosHistory has no CORS and is LAN-only), and a port of the app's `EnergyToday.requests` (which counters to ask, the derived-house subtraction).
- **Sensor "since"** on Security tiles ("open for 5 min", "unchanged for over 30 days"): the app reads `/v1/last_change` and merges a flip it saw itself. The hub only keeps `last_update`, which Zigbee2MQTT bumps on every link-quality report, so it is not a change time. Either the hub tracks `changed_at` per entity or the proxy above lands first.
- The app's history detail screens (24 h / 7 d / 30 d charts) for sensors and Energy nodes.

## Web-only differences kept on purpose

- Alarm codes are typed in an inline field, not the app's keypad sheet; a button that needs a code is disabled until something is typed.
- Security tiles link to the entity sheet instead of long-press sheets; "Show on Security", "Ask before turning on" and "Hidden from security" are edited in the override editor there. The editor's labels could match the app's wording ("Show on Security" instead of "Hidden from security").
- The "+N" consumers node expands a list under the cards (plus a real "Show all N consumers" button, since the svg is `role="img"`) instead of opening a sheet.
- Energy cards show the live value above the lifetime kWh; the app's card shows only the counter.
- The diagram writes the node label above its value; the app draws the value first.
- No "stale" (last known) state: the web has no cache, so nothing is dimmed as cached.
- `energyIcon` is edited as text in the override editor; the app has a chip picker for the seven icons.
- Security tile names are the hub's effective names, so a Zigbee2MQTT device without an override shows as `alarm/siren_living Alarm`; naming it in the entity sheet fixes it, as on Home.

## Port notes (core energy)

- `VolumeUnits`: the plan listed `hL`; the app's `VolumeUnits.kt` does not have it, so the Rust follows the app.
- The app's `POWER_TOKEN` regex is a hand-written ASCII matcher (no regex crate in `oikos-core`); non-ASCII letters next to "power" count as non-letters.
- Where the Kotlin compares whole `Entity` values, the Rust compares ids (`reverse_for`'s solar case, `node_meters` de-duplication).
- `NodeStatus` is `Live | Unavailable`; an unavailable derived-house input still leaves the house unknown ("—"), never unavailable, as in the app.
- `aria_sentence` follows the app's `EnergyDescription.kt` (order solar, grid, battery, house, consumers, water; kilowatts only from 1000 W), not the spec's rough example sentence.

## Hub and MCP

- `AreaView::primary_count` and `EntityView::is_primary` still mean "shown by default" for the API and MCP; only the web Home applies `Placement::on_home` on top. An MCP tool answering "what is on Home / Energy / Security" would reuse `oikos_core::model::Placement` and `oikos_core::energy::EnergyFlow` (both pure, native) without touching the web.

## Process

- The dev machine's disk filled up mid-wave: this repo's `target/` had grown to 29 GB (15 GB debug, 13 GB wasm). Deleting `target/debug/incremental` and `target/wasm32-unknown-unknown/debug/incremental` freed enough. Fixed the same evening: `cargo clean`, then a `[profile.dev]` with line tables only and no debug info for dependencies brought a full rebuild to 4.2 GB. Still open: `cargo sweep` for stale artifacts and a shared `CARGO_TARGET_DIR`.
