---
title: Cameras (Frigate)
sidebar_position: 9
description: Serve Frigate snapshots and clips to the browser through the hub, and turn Frigate detections into camera alerts.
---

Alert snapshots and clips are URLs on Frigate. When they point at Frigate's login port (8971) the browser cannot load them, so give the hub the Frigate login and it serves them itself: media on that origin is rewritten to `/api/v1/media?url=…`, and the hub fetches it with Frigate's session cookie (logging in on demand, once more on a 401). Any other origin is refused. Leave `user` and `password` out for the unauthenticated port (5000).

```toml
[frigate]
url = "http://<frigate-host>:8971"
user = "oikos"
password = "…"
```

## The Frigate bridge

The hub also replaces OikosPush's Frigate bridge: with `bridge = true` (and a `url` or `media_url`) it reads `<topic_prefix>/events` (subscribed only then) and raises a `camera` alert for each tracked object that qualifies. The first qualifying message (`new`, or the `update` that enters a watched zone) rings with the snapshot; `end` with a clip re-sends the same alert id silently with the clip; an event that only qualifies at `end` gets one audible alert with its clip. The title is the camera, the body `Person detected`, `Person in Porch` or `Alice (person) detected`, and `loc` (`frigate.object`) lets the app write it in the phone's language.

```toml
[frigate]
bridge = true
media_url = "http://<frigate-host>:5000"  # what the phone fetches media from; `url` when unset
labels = ["person", "car"]              # empty: all
cameras = []                            # empty: all
zones = []                              # empty: any; otherwise alert once the object enters one
cooldown = 0                            # seconds to ignore new events on a camera after an alert
severity = "warning"
ttl = 43200
topic_prefix = "frigate"                # Frigate's MQTT topic_prefix
```

The broker user needs read on `<topic_prefix>/events`. Frigate needs `snapshots.enabled: true`, and event clips retained (`record.enabled`) for the clip update.

The alerts are delivered as described in [Push notifications](push-notifications.md). Use a Frigate *viewer* account for `user` and `password`, not the admin; see [Security](security.md).
