# Camera and Lock dashboard widgets

Date: 2026-10-26. Status: design approved in conversation; awaiting review of this document.

## Why

The dashboard model already has `WidgetKind::Lock` and `WidgetKind::Camera`. The widget library does not offer them, and `dashboards/widget.rs` renders them as a labelled placeholder. They are the last dashboard P1 in `TODOS.md`. Two things make a dashboard useful at a glance and are missing today: watching the cameras live, and locking or unlocking a door.

The project is generic: it must work for any house. The current house has no lock device and runs Frigate cameras. Neither fact may leak into tracked files. Use example camera names such as `front_door` and `garden`.

## Goals

- A **Camera widget** that shows **1 to 4 Frigate cameras live** in one tile, laid out by the tile's size.
- A **Lock widget** that shows a lock's state and locks or unlocks it in one tap, with a confirmation before unlocking.
- Both widgets appear in the widget library, can be configured in the widget config sheet, and are translated in English and French.

## Non-goals

- Audio, PTZ, recordings or a timeline, two-way talk.
- WebRTC, MSE or go2rtc streams. MJPEG and JPEG frames only.
- Cameras that are not in Frigate.
- The Android app. The widgets are web only.
- Lock codes or PINs. The hub has no authentication to protect them with (see `docs/users/security.md`).
- The lock's `open` (unlatch) control. The widget only locks and unlocks.

## Anti-goals (a failure even if it works)

- Camera tiles that use up the browser's six connections per host and freeze the rest of the dashboard (API calls, history charts).
- Video pulled from Frigate while nobody is looking: a hidden tab, a scrolled-away tile, a closed dashboard.
- The hub turning into an open proxy to Frigate's API. The only Frigate paths it may fetch are the ones listed below.
- A mis-tap unlocking a door.

## Camera widget

### What the user sees

A widget holds an ordered list of 1 to 4 Frigate camera ids. Its span decides the layout. The allowed spans already exist: `2x1`, `1x2`, `2x2`.

| Span | Cameras | Layout |
|------|---------|--------|
| `2x1`, `1x2` | 1 | The camera fills the tile. |
| `2x1`, `1x2` | 2–4 | One camera at a time. Left/right arrows switch camera (and a swipe on touch screens). Dots show the position. The order is the configured order. |
| `2x2` | 1 | The camera fills the tile. |
| `2x2` | 2 | Two stacked halves, top and bottom. |
| `2x2` | 3 | A 2×2 grid with the fourth square empty. |
| `2x2` | 4 | A 2×2 grid. |

The user chose these layouts. On the `2x2` tile they ruled out a layout with one wide camera on top and two below.

Every camera picture shows:
- a small red "Live" mark in the top-left corner (only while live frames are flowing);
- the camera's display name in the bottom-left corner;
- an amber badge in the top-right corner with that camera's latest alert, for example "Car · 12 min", when it falls within the last 24 hours (24 hours is my call). Tapping the badge opens that alert's existing page (`screens/alert_detail.rs`), which plays the clip.

Tapping a picture (outside edit mode) opens a **full-screen live view** of that camera. The full-screen view has arrows to step through the widget's other cameras, and closes with a close button, Escape or Back. In edit mode a tap does what it does on every other widget.

### Live frames: how they flow (my call)

- **Tiles** show frames from Frigate's `latest.jpg`, fetched through the hub about **once per second** per camera. The tile loads the next frame off-screen, swaps it in once it has loaded, and only then waits and asks for the next one, so requests never pile up. Each fetch is a short request, so four cameras never hold browser connections open. Frigate's own camera grid uses the same approach.
- **The full-screen view** uses Frigate's **MJPEG stream** (`multipart/x-mixed-replace`) in an `<img>`, through the hub, at the camera's detect frame rate. This holds a single long connection.
- Frames are requested at the displayed height times `devicePixelRatio`, capped (for example at 720 px). A tile doesn't need the full stream.
- **Paused when unseen.** A tile stops fetching when it is off screen (IntersectionObserver) or the tab is hidden (`document.visibilityState`), and resumes when it is shown again. Closing the full-screen view sets the `<img>` source to empty, which ends the MJPEG connection. The hub drops the upstream connection to Frigate when the browser disconnects.

### Hub side

A new config switch, `[frigate] live = false` by default. Live frames reach anyone on the LAN through the hub, so they stay off until the user opts in. When the switch is off:
- the tile shows the camera's latest event snapshot (the newest alert for that camera, through the existing `/api/v1/media` path) with a short "Live view off" note, and no "Live" mark;
- the full-screen view shows the same snapshot;
- with no alert for that camera yet, the tile shows a camera icon and "No events yet".

New routes under `/api/v1`. They are separate from `/media?url=`, so that route's event-only allowlist (`media::is_event_media`) stays exactly as narrow as it is today:

- `GET /cameras` returns `{ "live": bool, "cameras": [{ "id": "front_door", "name": "Front Door" }] }`. The hub reads the camera list from Frigate's `/api/config` with its existing login, keeps only the camera ids (it never passes the config through), and caches the list for about a minute. The name comes from the same `pretty()` rule the Frigate bridge uses for alert titles. With no `[frigate]` section, or when Frigate can't be reached, it returns an empty list. `live` reflects the switch.
- `GET /cameras/{id}/latest.jpg?h=` fetches Frigate's `/api/{id}/latest.jpg` at height `h`.
- `GET /cameras/{id}/stream?h=` fetches Frigate's MJPEG endpoint `/api/{id}` at height `h`, and streams the body through without a total timeout.
- Both frame routes return 403 when `live` is off, and 404 when `{id}` is not in the cached camera list. This check stops arbitrary path segments from reaching Frigate. When an id is missing from the cache, the hub refreshes the list once before refusing, at most once every 10 seconds, so a camera just added to Frigate works straight away (my call).
- `/api/config` and the frame paths are fetched from the `[frigate] url` base (the hub's login port), never from `media_url`. They go through a **new `MediaProxy` method** (for example `fetch_camera(path)`) that builds the URL from that base and shares the login cookie, the re-login retry, the connect timeout and the no-redirect policy. `fetch` and `is_event_media` stay unchanged. The MJPEG route must not hit a total timeout (my call).

**For the builder to verify:** the exact names of the MJPEG endpoint's query parameters (`h` or `height`, `fps`) and of `latest.jpg`'s parameters differ between Frigate versions. Check them against the Frigate version that runs behind the hub's login port (8971).

### Matching alerts to cameras

The alerts the Frigate bridge raises carry the camera only in their title, prettified (`front_door` → "Front Door"). Add the raw Frigate camera id as an optional `camera` field to the alert the bridge builds, and pass it through to `AlertView`. The widget matches on `camera`. For older stored alerts without the field, it falls back to comparing the title with `pretty(id)`.

The widget filters the web's existing alerts signal (`House::alerts`) on the client and adds no hub query. That signal mirrors the push history, which keeps the last 100 alerts for 7 days by default (`[push] history_size`, `history_days`). The 24-hour badge always fits inside that window. The "live off" snapshot does not: a camera with no alert in the history (a quiet camera, or one left out by the bridge's `cameras` or `labels` filters) shows "No events yet" even though Frigate has older events. This is an accepted limit of the live-off fallback (my call).

### Widget config

Add a new `WidgetConfig` variant:

```rust
Cameras {
    cameras: Vec<String>,   // Frigate camera ids, in display order
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}
```

`WidgetConfig` is untagged. `cameras` is a key no other variant uses, so where the variant goes in the list doesn't matter for decoding. Add a decode test next to `widget_config_history_decodes_before_entity` anyway.

Hub validation (`crates/oikos-os/src/dashboards/store.rs`): `WidgetKind::Camera` requires `WidgetConfig::Cameras` with 1 to 4 distinct, non-empty ids. Today it requires `Entity`. Ids are not checked against Frigate, because Frigate may be down when a dashboard is saved. No saved dashboard can hold a Camera widget, since the library never offered one, so nothing needs migrating.

### Library and config sheet (my calls)

- The library gets a **Cameras** section, hidden when `GET /cameras` returns no cameras. It holds one entry per camera, which adds a `2x1` widget with that camera, and an **All cameras** entry, which adds a `2x2` widget with the first four.
- The config sheet for a Camera widget shows a title field, the existing span select, and an ordered camera list. Each row has up/down arrows and a remove button, and an "Add camera" select offers the cameras not yet in the list. You can't add a camera once the list holds 4, or remove the last one.

## Lock widget

### What the user sees (1×1 or 2×1, spans already allowed)

- A padlock icon and the state in words: Locked, Unlocked, Locking…, Unlocking…, Jammed, Open, Opening…, Offline (availability), or Unknown.
- Unlocked, Open, Opening and Jammed are shown in a warning colour (for example `--warning`). Locked is calm.
- One button, chosen by the state:
  - **Locked** → "Unlock".
  - **Unlocked, Open or Jammed** → "Lock". Jammed offers Lock as a retry.
  - **Locking…, Unlocking…, Opening…** → disabled, with the in-progress text.
  - **Offline or Unknown** → disabled.
- **Unlock asks first.** Pressing Unlock opens the existing confirm component (`components/confirm.rs`): "Unlock {name}?" with Cancel and Unlock. Locking happens straight away. The user agreed to this.
- The command is the lock type's `locked` on/off control: `Command::Control { control: "locked", value: true | false }`, as the generic card already sends it (`cards/generic.rs`).

### Where the card is used (my call)

The new `cards/lock.rs` `LockCard` is used by the Lock widget, and also replaces the generic card for lock entities on Home and room pages, so a lock looks and behaves the same everywhere. The TODO asks for the widgets "with their cards".

### Library

The library gets a **Locks** section listing the entities of the `lock` type (`types/table.json`). It is hidden when the house has none. An entry adds a `1x1` widget with `WidgetConfig::Entity`. Hub validation for `Lock` already requires `Entity`.

## Text

Every new string goes through the i18n modules in English and French: the widget kind words (`Verrou` already exists), the lock states and buttons, the confirm question, "Live", "Live view off", "No events yet", "All cameras", the badge's time format, and the config sheet labels.

## Docs and config

- `oikos.example.toml`: add a commented `# live = false` to `[frigate]`, with a one-line warning that live frames are readable by the whole LAN.
- `docs/users/cameras.md`: a section on the live dashboard view and the switch.
- `docs/users/security.md`: add live frames to the Frigate paragraph.
- `docs/users/web-ui.md` § Dashboards: the two new widgets.
- `TODOS.md`: remove the Camera/Lock P1.

## Testing

- Hub: `GET /cameras` with and without Frigate. The frame routes return 403 when `live` is off, 404 for an unknown id, and pass a frame through from a fake Frigate (follow the existing `media.rs` test fakes). The stream route is not cut off by a total timeout.
- Core: decoding `WidgetConfig::Cameras`, and the validation rules (0, 1, 4 and 5 cameras, a duplicate id, an empty id, `Entity` config on a Camera widget).
- Web: a pure function from (span, camera count) to layout, covering every table row. A pure function from lock state and availability to (label, button, warning, disabled). The library sections show or hide with and without cameras or locks. The i18n tests.
- The bridge's alert carries `camera`.

## Left to the builder

- Frame cadence and the height cap, within the "about once per second" and "small" intent.
- The exact styling, using the existing card and widget tokens in Night Glass and Ivoire.
- How the full-screen view is built: an existing sheet or overlay, or a new one.
- The order of work and how the PRs are split. One PR for the Lock widget and one for the Camera widget is a sensible cut, because they share nothing but the library.
