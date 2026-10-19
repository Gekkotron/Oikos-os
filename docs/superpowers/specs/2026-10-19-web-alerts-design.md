# Oikos-os web UI: push alerts (camera events and the rest)

Author: Gekkotron. Date: 2026-10-19. Status: approved for planning.

## 1. Intent

The Oikos Android app shows the house's alerts: Frigate camera events with their snapshot and clip, alarm and security events, and whatever else a producer publishes through OikosPush. The web UI served by the hub shows none of this today. This design gives the web UI the same alert screens as the app, so the last camera event (and every other alert) is visible from a browser on the LAN.

The user's words: "display the last camera event in the web UI", then "do the same as the mobile app". The app's behaviour is the reference; where this document and the app differ, the app wins unless the difference is listed in §4.

## 2. Where the alerts come from

OikosPush (the Python service, still running unchanged in v1) normalises every producer's alert and keeps a **retained catch-up history** on the broker: `oikos-push/history`, a JSON array of normalised alerts, newest first, at most 100 entries and 7 days, republished after every change and on every (re)connect. The Frigate bridge is one producer: its alerts have `channel: "camera"`, `image_url` = Frigate's `/api/events/<id>/snapshot.jpg` and, once the event ends with a clip, `video_url` = `/api/events/<id>/clip.mp4` (sent again under the same `id`, so the history entry gains the clip in place).

The hub reads that one retained topic. It does not talk to Frigate, does not read `frigate/events`, and does not subscribe to `oikos-push/notification/#`. The history is the source of truth the app uses too.

### History entry (from OikosPush `history.py`)

```json
{ "v": 1, "id": "frigate-front-1696", "channel": "camera", "severity": "warning",
  "title": "Front Door", "body": "Person in Porch",
  "image_url": "http://192.168.1.20:5000/api/events/1696/snapshot.jpg",
  "video_url": "http://192.168.1.20:5000/api/events/1696/clip.mp4",
  "entity_id": "binary_sensor.front_door",
  "actions": [ { "id": "disarm", "label": "Disarm" } ],
  "silent": false, "sent_at": 1760880000, "source": "frigate",
  "loc": { "key": "frigate.object", "args": { "label": "person", "zone": "Porch" } },
  "cancel": true }
```

`image_url`, `video_url`, `entity_id`, `loc` and `cancel` are present only when set. `channel` is one of `alarm`, `security`, `camera`, `energy`, `climate`, `system`, `misc`; `severity` is `info`, `warning` or `critical`. Unknown values fall back to `misc` and `info`, as in the app.

### Future topics

The user wants the push namespace under the hub's own prefix later (`oikos/push/…` for notifications, tokens and responses, `oikos/history` for the catch-up buffer), once the hub absorbs OikosPush. Nothing in this design hard-codes `oikos-push`: the topics are configuration (§3.1), and the move is a config change.

## 3. Hub (`oikos-os`, `oikos-api`)

### 3.1 Configuration

New section, with defaults that match OikosPush today:

```toml
[push]
history_topic = "oikos-push/history"
responses_topic = "oikos-push/responses"
```

Both keys follow the existing layering (defaults, `oikos.toml`, `OIKOS_PUSH__HISTORY_TOPIC` and `OIKOS_PUSH__RESPONSES_TOPIC`). They are documented in `oikos.example.toml` and the README with the note that they become `oikos/history` and `oikos/push/responses` when the hub takes over push.

### 3.2 Ingest

- `history_topic` is added to the static MQTT subscription filters, next to the discovery prefixes, the Z2M bridge and the UI prefix.
- A message on `history_topic` is handled by the hub **before** the registry routes it (the registry must never see it as discovery or state). The payload is parsed as a JSON array; entries that are not objects or have no string `id` are dropped, the rest are parsed leniently into `AlertView`s (unknown keys ignored, bad optional fields treated as absent, so one odd entry never costs the list). An unparsable payload is logged once per distinct payload hash at `warn` and the previous list is kept. An empty payload (the topic was cleared) empties the list.
- **Cancel entries.** OikosPush stores a cancel in one of two ways: a cancel for a known `id` sets `cancel: true` on the existing entry in place (title, body, media and actions stay); a cancel for an `id` the history never held is stored as its own entry with `cancel: true` and an empty `title` and `body`. The first becomes a resolved alert. The second has nothing to show and is dropped by the parser (Claude's call; the app keeps such rows, but with no title there is nothing to render). A resolved alert keeps its actions and its buttons, as in the app.
- The hub keeps the parsed list in memory. On a change (compared by value), it broadcasts `WsEvent::AlertsChanged(Vec<AlertView>)`. The list is also part of the `Snapshot` sent on connect.
- Nothing is persisted: the broker's retained message is the store, exactly as for overrides.

### 3.3 Wire types (`oikos-api`)

```rust
pub struct AlertView {
    pub id: String,
    pub channel: AlertChannel,      // alarm | security | camera | energy | climate | system | misc
    pub severity: AlertSeverity,    // info | warning | critical
    pub title: String,
    pub body: String,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
    pub entity_id: Option<String>,
    pub actions: Vec<AlertAction>,  // { id, label }, at most 3 kept (the app shows at most 3)
    pub source: Option<String>,
    pub silent: bool,
    pub resolved: bool,             // the entry's `cancel: true`
    pub sent_at: u64,               // unix seconds
}
```

- `Snapshot` gains `alerts: Vec<AlertView>` (newest first, as received), marked `#[serde(default)]` so a snapshot without the field still parses. The MCP `oikos://snapshot` resource grows by this field; additive changes to it are allowed.
- `WsEvent` gains `AlertsChanged(Vec<AlertView>)`.
- The `loc` hint is not carried: the web UI is English-only and shows the server's `body` (§4). The `ttl` and `v` fields are not carried.
- Both types derive `JsonSchema` like the rest of the API, so the MCP schema stays consistent even though no MCP tool is added.

### 3.4 REST

| Route | What it does |
| --- | --- |
| `GET /api/alerts` | The current list, same shape as the snapshot field. |
| `POST /api/alerts/{id}/actions/{action}` | Publishes the reply to `responses_topic` (JSON, QoS 1, not retained). Replies `202` with no body. `404` when `id` is not in the list or `action` is not one of its actions (a resolved alert still accepts its actions); `503` with the existing "broker link down" error when the hub refuses writes while offline. |

The reply payload is byte-for-byte the shape the app's `ReplyWorker` publishes (verified in the app's source, `core/push/.../ReplyWorker.kt` and `ReplyQueue.kt`):

```json
{ "id": "frigate-front-1696", "action": "disarm", "install_id": "oikos-web", "ts": 1760880000 }
```

`ts` is unix **seconds**. `install_id` is the app's install id; the web has none, so it sends the constant `oikos-web`. OikosPush itself never subscribes to the responses topic: producers (Jeedom, scripts) subscribe to it to receive the button the user tapped, and they key on `id` and `action`. There is no registered-installs check to fail.

The hub does not proxy media. Snapshot and clip URLs are loaded by the browser straight from the alert (§4.4).

### 3.5 MCP

No new tool in this change. (A `list_alerts` tool is an obvious follow-up and must reuse `AlertView`.)

## 4. Web UI (`oikos-web`)

The three screens of the app's push-alerts spec §7, with the differences listed here. Everything is reactive off the house state: a new `alerts: Vec<AlertView>` field on `HouseState`, replaced wholesale on `Snapshot` and `AlertsChanged`.

### 4.1 Home: "Recent alerts" section

- Under the room cards of the Home screen, after the selected room's grid, a section titled "Recent alerts" with a "See all" link to `/alerts`.
- Up to **5** alerts from the `alarm`, `security` and `camera` channels, newest first.
- Each row: a 56 px rounded thumbnail (`image_url`) or the channel glyph; the title; one line of body; the relative time ("Just now" under a minute, then "2 min ago", "3 h ago", "2 d ago"); " · Resolved" after the time when resolved.
- Empty state: "No alerts yet". The section is shown even when empty, so the user knows the feature exists; the app does the same.
- A row links to `/alerts/<id>`.
- The relative time re-renders every minute (one shared ticking signal, like the app's `rememberNow`).
- Not carried over: the "Notifications are off" banner (no push on the web).

### 4.2 Alerts page (`/alerts`)

- Title "Alerts" and a back link to Home, in the same header style as the entity sheet.
- Filter chips in the `chips` style of the Home screen: All, Alarm, Security, Camera, Energy, Climate, System, Misc. The selection lives in a signal (not the URL) and defaults to All.
- Rows as in §4.1, grouped by day with headings "Today", "Yesterday", then the date in the browser's locale (`toLocaleDateString`, medium style). Day boundaries use the browser's local time zone.
- Empty states: "No alerts yet" for All, "No alerts in this channel" for a filter.
- Not carried over: the "Clear all" menu (the web keeps nothing locally; the list is the broker's).
- Navigation: "Alerts" is added to the top bar next to Home, Devices and Settings.

### 4.3 Detail (`/alerts/:id`)

A full screen in the sheet style of `/entity/:id` (the web has no bottom sheet). If the id is not in the list: "This alert is no longer available" and the back link.

- Media block, full width, 16:9, rounded, glass background:
  - the snapshot (`<img>`, `object-fit: cover`) when `image_url` is set;
  - a centred play button over it when `video_url` is set; clicking swaps the image for `<video controls autoplay playsinline>` with the clip URL;
  - with no image but a clip: the play button on the empty surface;
  - on an image load error: "Media no longer available" on the surface; on a video error the same message (the browser's own error is not shown).
- Title; body; chips for the channel and, when resolved, "Resolved"; the full date and time in the browser's locale.
- Action buttons for the alert's actions (at most 3). Clicking one calls `POST /api/alerts/{id}/actions/{action}`; a toast says "Sent" or "Couldn't send <label>" (the existing toast queue).
- "Open device" when `entity_id` matches an entity, using the app's `EntityMatch` rules in order: the exact unique id; the part after the first dot; a slug of that part compared with the slug of each entity's name or unique id. It links to `/entity/<unique id>`.
- Source line at the bottom: "From frigate · 19 Oct 2026, 14:03" (muted), so a producer is identifiable.

### 4.4 Media

The browser loads `image_url` and `video_url` directly. This works with Frigate's unauthenticated port (5000). If the alert URLs point at Frigate's login port (8971), the browser has no session cookie and the media shows "Media no longer available". A hub-side media proxy with the Frigate login (mirroring the app's `FrigateAuth`) is a follow-up, not part of this change. Assumption, stated to the user: the alert URLs use the unauthenticated port.

Images are not cached beyond what the browser does. No `loc`-based localisation: the web is English and shows `body` as the server wrote it.

### 4.5 Glyphs

One glyph per channel, in the existing `icons.rs` style (inline SVG): alarm (bell), security (shield), camera (camera), energy (bolt), climate (thermometer), system (gear), misc (info). They are used in rows without an image and as the chip icons on the Alerts page.

## 5. Goals, non-goals, anti-goals

**Goals**
- The last camera event is visible on the web Home screen within a second of OikosPush publishing it, with its snapshot, and its clip once the event ends.
- The web's alert screens behave like the app's, so the user needs no new mental model.
- The hub's push topics are configuration, ready for the move to `oikos/push/…` and `oikos/history`.

**Non-goals**
- Reading `frigate/events` or Frigate's API from the hub; live video; Frigate controls.
- Porting the Frigate bridge or FCM sending into the hub (a later phase).
- A media proxy or Frigate login on the hub (follow-up, §4.4).
- Alert history beyond what the retained topic holds (100 entries / 7 days).
- Translations.
- An MCP tool (follow-up).

**Anti-goals**
- Breaking the app: the hub never publishes on `history_topic`; it only reads. It publishes on `responses_topic` exactly the shape the app publishes.
- Polling: no timer re-reads the topic; the retained message and its republishes drive everything. (The one-minute tick only re-renders relative times in the browser.)
- A new store: no file, no database; the broker is the state.

## 6. Constraints

- Rust only, no Node (workspace rule). Leptos CSR with Trunk, hand-written CSS with the existing tokens and class conventions (`block__element--modifier`).
- `oikos-api` must keep compiling for wasm32 and native.
- The existing test layers apply: pure parsing and matching logic unit-tested (history parsing, `EntityMatch`, relative time, day grouping); the hub's REST route and WebSocket event covered by the existing axum tests pattern; the wasm smoke test still passes.
- Phone width first: rows wrap, the media block keeps 16:9, chips scroll horizontally like the room chips.
- Author attribution: Gekkotron.

## 7. Decisions

| Decision | Whose |
| --- | --- |
| Source is the retained `oikos-push/history`, not `frigate/events`. | Claude (the app does the same) |
| Push topics are config keys, defaults match OikosPush, future `oikos/push/…` and `oikos/history`. | User (future), Claude (as config) |
| Same three screens as the app: Home section, Alerts page, detail. | User ("do the same as the mobile app") |
| Media loaded directly by the browser; proxy later. | Claude, under the stated port assumption |
| Actions are supported through a hub route that publishes on the responses topic. | Claude |
| No `loc` localisation, English body. | Claude |
| No "Clear all", no notifications banner. | Claude (web has no local store, no push) |
| Detail is a route, not a modal. | Claude (matches `/entity/:id`) |

## 8. Left to the builder

- Exact CSS values within the token system; glyph shapes.
- Whether the alerts list in `HouseState` is a `Vec` or an id-keyed map (the API sends a `Vec`, newest first; order must be preserved).
- How the one-minute tick is implemented in Leptos.
- The hub's internal placement of the alert store (a field on `Hub` guarded like the registry, or a small module); it must not live in `oikos-core`'s registry.
- Test fixtures: use the history entry in §2 and a cancel-only entry.
