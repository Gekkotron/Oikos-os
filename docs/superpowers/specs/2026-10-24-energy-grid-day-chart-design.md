# Energy page: grid day chart — design

Date: 2026-10-24. Status: awaiting the user's review.

## Intent

The Energy page shows the house live (the flow diagram and cards) but says nothing about the
day so far. The user wants a chart of energy usage in the style of Hellowatts: what crossed the
grid meter, half hour by half hour, with purchases above the zero line coloured by tariff and
solar export below it. It answers "when did I buy, how much of it at the peak price, and when
did the panels cover the house and push the surplus out" at a glance.

References the user brought: the Home Assistant Energy dashboard (usage bars, solar chart,
distribution circles, gauges, per-device bars) and the Hellowatts consumption chart. The user
picked the Hellowatts reading: **only what crosses the grid meter**. Solar used directly by the
house is not on this chart.

## Goals

- A day chart on the web `/energy` page, under the live flow diagram, which stays as it is.
- One bar per half hour of one calendar day (house time zone).
- Above zero: kWh bought from the grid, stacked by tariff, off-peak (light blue) under peak
  (dark blue).
- Below zero: kWh sent to the grid (solar export, yellow).
- A half hour that contains a tariff change shows both colours, each carrying the kWh bought
  on its side of the change.
- A totals row above the chart: bought (with the off-peak / peak split) and sold, for the day
  shown.
- Day navigation: ‹ › to the previous / next day, between the first day with data and today. The label reads
  "Today", "Yesterday", or the date.
- Today's chart is live: the current half hour fills up as the day goes on, refreshed about
  once a minute while the page is visible.
- Off-peak hours are a hub setting: any number of windows at minute precision (this house:
  01:34–07:04 and 12:05–14:34; a window may cross midnight, for example 22:00–06:00). Edited
  from the web Settings.
- Works for any house (the project is generic): whatever entity carries the `grid` energy role,
  whatever off-peak windows are set.

## Non-goals (this round)

- The Android app. Web only.
- Week, Month and Year views (the mockup showed them greyed; leave them out or disabled).
- A solar production chart, the "net consumed" and "self-consumed solar" gauges, per-device
  usage bars, the temperature overlay, costs in euros.
- Showing solar the house used directly.

## Anti-goals

- Numbers that disagree with the meter. Day totals must equal the counter increase over the
  day, and the off-peak + peak split must add up to the bought total.
- Anything that slows or changes the live flow diagram and cards.
- Anything specific to this house in tracked code, docs or defaults (no device names, no fixed
  off-peak hours).

## Data the chart is built from

Energy roles are already set by the user on their meters (`ui.energyRole`: `grid`,
`grid_export`, `solar`, `consumer`, …). This house's grid meter is a two-clamp Zigbee power
meter: clamp A carries the `grid` role, with a lifetime "bought" counter (`energy_a`, kWh) and a
lifetime "sold" counter (`energy_produced_a`, kWh). There is no Linky / tariff-indexed counter,
so the tariff split comes from the off-peak setting, not the meter.

Counters, not power: the bars are counter increases, which are exact. A rough mockup from
10-minute power averages showed almost no export because averaging nets import against export
inside each window; the counters do not have that problem.

The counters are resolved with the existing pairing rules in `oikos-core`
(`crates/oikos-core/src/energy/meters.rs`), the same ones the diagram uses:

- bought = `EnergyMeters::meter_for(s)` for every sensor `s` with role `grid`, summed;
- sold = `EnergyMeters::reverse_for(s)` for every `grid` sensor, plus `meter_for(x)` for every
  sensor `x` with role `grid_export`, summed, each counter counted once.

A house with no `grid` sensor, or whose grid sensor has no counter, gets no chart (see empty
states). A house with no export counter gets a chart with no bars below zero. Only lifetime,
increasing counters are supported (one for bought, one for sold); a meter that exposes only a
net counter or only signed power gets the "no grid meter" state in this round.

History: raw rows are kept forever in Timescale (`docs/superpowers/specs/2026-10-20-timescale-history-design.md`),
so readings at any minute of any past day are available. Counter increases follow the existing
rule in `crates/oikos-core/src/history/buckets.rs` (`increase += max(0, Δ)`, so a reset or a
replaced meter never counts negative).

## Hub

### Off-peak setting

Add to `HomeSettings` (`crates/oikos-api/src/home.rs`), the retained home settings document the
hub already validates, stores and broadcasts (`crates/oikos-os/src/home.rs`):

```json
"off_peak": [ { "start": "01:34", "end": "07:04" }, { "start": "12:05", "end": "14:34" } ]
```

- Local wall-clock times, `HH:MM`, 24-hour. `start == end` is refused. `end < start` means the
  window crosses midnight. Windows that overlap are refused.
- Absent or empty means a single tariff: everything bought is "peak"/plain (see web).
- Validated in `home::validate` with a clear message per bad window; the MCP
  `get_home_settings` / `set_home_settings` tools pick it up through the shared type (update
  their descriptions to mention it).
- The tariff at a given instant is decided from the house-local wall-clock time of that instant.
- **Past days keep the windows that applied then** (the user's decision). The hub keeps a
  dated history next to the current windows, for example
  `"off_peak_history": [ { "since": "2026-10-24", "windows": [ … ] }, … ]`, appending an entry
  whenever a save changes the windows. Rules:
  - a change applies from the start of the house-local day it is saved on (that whole day uses
    the new windows); several saves on one day keep only the last;
  - the oldest entry also covers every day before it, so history recorded before the first
    windows were set (imported data) is coloured with the first windows, not left single-tariff;
  - clearing the windows is an entry too (`"windows": []`): from that day on, one tariff;
  - the history is written by the hub only; a client that sends `off_peak_history` has it
    ignored, and the web edits only the current windows.
  Where the history lives (in the retained home settings document, or a small table in the
  history database) is the builder's call; it changes rarely and must survive a hub restart and
  a rebuild of the container.
- The same windows apply every day of the week. Weekday- or season-dependent windows are out of
  scope; if they come later, an optional `days` field per window extends the schema without
  breaking stored documents.

### Day query

A new read endpoint next to the other history routes in `crates/oikos-os/src/http/api.rs`:

```
GET /api/v1/history/grid_day?date=YYYY-MM-DD
```

`date` defaults to today in the house time zone. The web omits it for today and always takes
"today" from the reply, never from the browser clock, so a browser in another zone still
labels the right day. Reply:

```json
{
  "date": "2026-10-23",
  "today": "2026-10-24",
  "first_date": "2026-01-15",
  "slots": [
    { "start": 1792706400, "off_peak": 0.112, "peak": 0.0, "export": 0.0 },
    …
  ],
  "totals": { "off_peak": 7.214, "peak": 9.905, "export": 3.120 },
  "has_export": true,
  "tariffs": true
}
```

- `slots`: one per half hour of the local day, in order, `start` in unix seconds. A normal day
  has 48; a DST day has 46 or 50 (the slots follow real time, so the repeated hour on the
  autumn change is two separate hours, labelled by their local time). Today stops at the
  current half hour, which is partial.
- kWh rounded to 3 decimals like the other history replies. `off_peak + peak` is bought.
- Each half hour is cut at the tariff changes inside it; each piece's counter increase goes to
  the tariff in force for that piece. The counter value at any cut is the last reading at or
  before it, carried forward, as the other history queries do; so each increase lands in the
  piece where the reading that shows it arrives, and the pieces telescope: the day total equals
  exactly (last reading of the day − last reading before midnight). A meter that went quiet
  shows zero for those slots, and its catch-up increase lands in the slot of the next reading.
  Totals must equal the sum of slots.
- `today`: the current house-local date. `first_date`: the first day with a reading of a
  bought counter (the ‹ button stops there).
- `has_export`: an export counter exists. `tariffs`: off-peak windows are set.
- Errors: 503 `history_unavailable` when `[history] url` is not set (as the other routes);
  404 `no_grid_meter` when no `grid` sensor resolves to a counter; 400 for a bad date or a
  future date.
- Built from raw rows. The builder picks the query shape (per-slot carries, one ordered scan,
  hourly rollups for the finished hours plus raw for the cuts). A past day must answer fast
  enough for ‹ › to feel instant, well under a second.
- Expose it to the MCP server as well (a `what: "grid_day"` on `get_history` or a separate
  tool, builder's call) so an assistant can answer "how much did I buy at peak yesterday".

## Web

### Placement

On `/energy` (`crates/oikos-web/src/screens/energy.rs`), a card under the live flow diagram and
above the meter cards. The diagram, its subtitle and the cards are unchanged.

### The card

Mockup shown and approved in the brainstorm (`.superpowers/brainstorm/…/day-chart.html`, not
committed): header with ‹ day ›, totals row, chart, legend.

- **Header:** ‹ label ›. › is disabled on today.
- **Totals:** "Bought 17.1 kWh" with "off-peak 7.2 · peak 9.9" under it; "Sold 3.1 kWh" when the
  house has export. Without tariffs, only "Bought N kWh".
- **Chart:** SVG bars, one per slot; zero line; dashed gridlines with kWh labels; hour ticks
  (every 3 h on wide screens, every 6 h on a phone). Off-peak windows shaded lightly behind the
  bars so the tariff periods read even where nothing was bought. The y scale fits the day's
  largest bar above and below zero.
- **Legend:** Off-peak, Peak, Solar export (only the entries that apply).
- **Tap / hover a bar:** a small tooltip with the half hour ("14:30–15:00") and its off-peak,
  peak and sold kWh. Keyboard focus reaches the bars, and the chart has an accessible summary
  sentence, like the diagram's `aria_sentence`.
- **Colours:** theme tokens, readable in both themes (Night Glass and Ivoire). Light blue /
  dark blue / yellow as in the mockup, adjusted to each theme.
- **Text:** French and English through the existing i18n keys.
- **Phone:** fits the width with no horizontal scroll; bars get thinner, labels sparser.

### Behaviour

- Today: refetch about every 60 s while the tab is visible, paused when hidden, reusing the
  pattern of `install_history_refresh` / `next_fire_delay` in `crates/oikos-web/src/energy.rs`.
  Past days are fetched once per visit.
- The chosen day lives in the page only (back to Today on a fresh visit). Builder may keep it in
  the URL if that is simpler with the router.
- Empty states, each a short line in the card instead of the chart:
  - history not configured: "History is off" and a pointer to the docs;
  - no grid meter: "Give your grid meter the Grid role on its device page" with a link;
  - a day with no data at all: "No data for this day".
- No off-peak windows: one colour for bought ("Bought"), no split in the totals.

### Settings

Settings › Home (`crates/oikos-web/src/screens/home_settings_panel.rs`) gets an "Off-peak
hours" section: a list of start–end time pairs with add and remove, saved with the rest of the
home settings (the panel already merges untouched fields). The row description in the
Settings list (`crates/oikos-web/src/screens/settings.rs`) mentions off-peak hours. Hub
validation messages are shown per field like the existing ones.

## Docs

Update the user docs for the Energy page and Settings › Home, and the API / MCP reference for
the new endpoint and setting (`docs/users`, `docs/reference`; not the README).

## Testing expectations

- Pure functions for slicing a day by half hours and tariff windows (midnight-crossing window,
  DST days with 46 and 50 slots, windows at odd minutes like 01:34) with unit tests.
- Hub query tested against the history test database (`crates/oikos-os/tests/history_db.rs`):
  totals equal counter increase, split adds up, counter reset does not go negative, today is
  partial.
- Settings validation tests (bad time, equal start/end, overlap).
- Tariff history tests: a day before and after a change splits with its own windows; same-day
  saves keep the last; a cleared entry gives one tariff; the oldest entry covers earlier days.
- Web presentation (scale, labels, tooltip text) as pure functions with tests, like
  `crates/oikos-web/src/energy.rs`.

## Calls made during the brainstorm (not asked of the user)

- Off-peak windows live in `HomeSettings`, edited on Settings › Home (not a new Energy settings
  page).
- The hub resolves the grid counters itself with the `oikos-core` pairing rules, rather than
  the web sending counter ids.
- Tooltip on tap/hover, shaded off-peak bands, totals row.
- Live refresh about once a minute for today.
- DST days keep real-time slots (46 / 50).
- Only lifetime increasing counters; net counters get the "no grid meter" state.
- Value at a cut = last reading carried forward; quiet meters show zero, catch-up lands later.
- The hub owns "today" (`today` in the reply); the web never uses the browser date.
- ‹ stops at `first_date`.
- Same off-peak windows every day of the week; an optional `days` field can come later.
- Tariff history: a change applies from the start of the day it is saved; the oldest windows
  also cover earlier days.

## Left to the builder

Query shape and performance strategy, how a straddling reading is shared at a cut, the MCP
surface for the day query, bar chart component structure (new SVG component; may reuse
`components/history_chart` geometry helpers), keeping the chosen day in the URL or not.
