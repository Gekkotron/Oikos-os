# Climate room card and room settings popup — design

Date: 2026-10-30 · Mockup: [2026-10-30-climate-room-card-mockup.html](2026-10-30-climate-room-card-mockup.html)

## Why

The Climate screen mixes two kinds of things on one long page:

- **Daily controls** (the reading, the target, hold, eco, the zone state) sit in a cluttered
  status card: a free-text hold field with three buttons, and eco's date and time inputs shown
  inline.
- **Set-once settings** are split across the page: the house **Settings** button at the top, and
  this room's thermometer, correction (7 number fields), presence, learning and decisions at the
  bottom of the room.

Users have to scroll past settings to reach the schedule and don't know which "settings" is
which. The redesign gives each room a calm card for daily use and moves every set-once room
setting behind a ⚙ on that card.

## Goals

- The room card shows only what is used day to day: the reading, the target with − / +, why the
  target is what it is, active holds, the zone state, and eco.
- A ⚙ on the card opens a **room settings popup** holding everything set once for that room.
- The page header's house **Settings** button becomes a ⚙ icon too: ⚙ in the page header is the
  whole house, ⚙ on a card is that room.
- Under the card, the page keeps only the week schedule and, when one is waiting, the proposal.

## Non-goals

- No hub, API, engine or data-model change. Everything here is `crates/oikos-web` plus
  `docs/users/climate.md`.
- No change to the week schedule editor, the proposal card's content, or the house settings'
  fields.
- The Android app is not covered.

## Anti-goals (a failure even if it works)

- Settings end up scattered again (a room setting left on the page, or a house setting in the
  room popup).
- A refetch (the hub's `climate_rev` tick) closes an open popup, resets a draft in it, or
  swallows a − / + change that hasn't been sent yet. The existing rule holds: panels stay
  mounted per zone and their local state lives outside the closures that react to a refetch.
- Every − / + tap sends its own hold to the hub.

## The page

Top to bottom (mockup, left pane):

1. **Header**: "Climat" + Beta badge + subtitle, as today; on the right a round ⚙ icon button
   (accessible name "House settings" / "Réglages de la maison") that opens the house settings.
   The banners (error, house invalid, history off) stay under the header as today.
2. **Room list** on the left, unchanged.
3. **Room card** (below).
4. **Week card**: today's `RulesPanel` + temperature legend, unchanged.
5. **Proposal card**: today's `ProposalPanel` (what changes, the proposed week on the timeline,
   Edit / Approve / Reject), unchanged, shown only while a proposal is waiting (already the case).

Today's lower block (learning + thermometer, correction card, presence card, decisions) leaves the
page and moves to the room popup.

## The room card

Mockup option "A · Thermostat row".

- **Top line**: room name; the existing badges (no climate device, invalid settings) stay; a round
  ⚙ icon button at the top right (accessible name "Room settings" / "Réglages de la pièce").
- **Figures row**:
  - Left: the reading label ("Now · 20 min ago"), the reading, and today's thermometer line when
    the thermometer is older than 15 min (unchanged logic).
  - Right: the label "Target · <mode word>" (e.g. "Consigne · Chauffage"), then − , the target
    temperature, + .
- **Why line** under the figures: the decision's source word and reason as plain muted text
  (today's source chip + reason, without the chip styling).
- **Active holds**: when overrides are live, one chip per override with today's text
  (`override_text`: value, until when, set here / on the device) and a single ✕ that clears
  them (today's **Clear**, same API call).
- **Action row**: the Off / Pending / Active segmented switch (same behaviour as today), then the
  **Eco** pill.

### Holding with − / +

This replaces the hold field and its 1 h / 2 h / until-next-change buttons.

- The shown target starts from the decision's temperature, else the rounded reading (today's
  `start` rule), and follows each refetch while no change is pending.
- A change is **pending** from the first tap until the first refetch after its send returns (or
  the send fails). While pending, the shown target is the drafted / sent value and refetches do
  not overwrite it, so there is no flicker back to the old value and no swallowed tap.
- Each tap moves the shown target by **0.5 °C** *(my call)*, clamped to 0–40 °C, the range
  `check_setpoint` accepts.
- After **1.5 s** without a tap *(my call)* one hold is sent: `set_override` with that setpoint
  and `minutes` = until the next schedule change when there is one (`minutes_to_next_change`
  with the decision's mode, Heat when it has none, as today's `until_change`), else **120**
  *(my call)*. The **Until next change** chip shows only when that returns a value.
- Once sent, a row of chips appears under the figures: **1 h**, **2 h**, **Until next change**
  (the last only when a next change exists). A chip re-sends the hold with the same setpoint
  and that duration. The row goes away when a chip is picked or after **10 s** *(my call)*.
- − / + are disabled when the zone can't hold (`can_hold(state)` false: Off **and Pending**, which
  the hub refuses an override in) and when there is no temperature to start from (the decision's
  setpoint isn't a temperature and there is no reading). The target still shows; no extra hint,
  the why line already says why.
- A failed send shows a toast (today's `act`), and the shown target returns to the hub's value
  on the next refetch.

### Eco pill

- Hidden when the zone is Off (today's `eco_allowed`).
- No eco running: the pill reads **Eco…** and opens a small **eco popup** with today's fields:
  date (default tomorrow in the house time zone) and time (default 00:00), **Start eco** and
  **Cancel**. Same validation and toast as today. The popup is mounted once per room panel and
  its draft signals (today's `eco_open`, `eco_date`, `eco_time`) live outside any closure that
  reacts to a refetch. **Start eco** closes it.
- Eco running: the pill reads the existing "Eco until …" line with a ✕ that cancels eco
  (today's **Cancel eco**).

## The room settings popup

Opened by the card's ⚙. Mockup right pane. Title "Room settings · <room>" / "Réglages · <pièce>",
a ✕, then these sections, top to bottom:

1. **Room thermometer**: today's `ThermometerField`.
2. **Thermometer correction**: today's `CorrectionCard` toggle. Under it a one-line summary
   (step, tolerance, interval) and a **Advanced settings (7)** fold that holds the seven fields
   *(my call: folded by default)*. The summary reads the zone's correction settings with the same
   units as the fields. The fold is presentation only: the seven fields keep today's
   save-on-change flow.
3. **Room presence**: today's `PresenceCard`.
4. **Learning**: the learn status line and **Learn now**, as today.
5. **Recent decisions**: today's list (10 recent, older rows behind "Older"). It is fetched when
   the popup opens and refetched on each zone change while it is open (today's `zone.track()`
   rule, no timer). Fetching is gated on popup open **and** panel shown.

## Popups (room settings, eco, house settings)

- All three use one popup shell: a modal `<dialog>` like `entity_picker` and `camera_viewer`,
  closed by ✕, Escape, a click on the backdrop and the browser's Back (`BackClose`).
- On a phone-width screen the room and house popups fill the screen *(my call)*; the eco popup
  stays small.
- The house settings popup holds today's `SettingsSheet` unchanged (it moves from an inline card
  into the shell; its draft-on-open rule stays).
- The room popup and its local state belong to the room's panel (one `open` signal per panel), so
  a refetch leaves it as it was. The dialog is modal, so the room can't change under it.

## Text

New EN/FR strings in `i18n/climate.rs`: house settings / room settings accessible names, the
room popup title, "Advanced settings", the eco pill words, the − / + accessible names ("Lower
target" / "Baisser la consigne", "Raise target" / "Monter la consigne"), the duration chips.
Strings that become unused (the hold field's placeholder, the 1 h / 2 h buttons if they had
their own keys, the decisions toggle) are removed.

## Docs

`docs/users/climate.md`, section "The Climate screen": rewrite the panel list for the new card,
the ⚙ room popup and the ⚙ house popup. The docs screenshot of the Climate screen is retaken
after the hub is rebuilt (separate step, docs screenshot recipe).

## Left to the builder

- Component split (e.g. a `RoomCard`, a `RoomSettings` popup, a shared popup shell) and CSS
  class names, following the existing `climate-*` naming and the Ivoire / Night Glass tokens.
- Icons for ⚙, −, + from the existing icon set.
- Test layout; `crates/oikos-web/tests/climate_rooms.rs` is the existing place for room tests.
