---
title: Security
sidebar_position: 13
description: What the hub does not protect in v1, what that means for your LAN and how to keep it off the internet.
---

There is no authentication yet, and that is deliberate for v1. The hub binds on `0.0.0.0`, answers any origin (CORS `*`) and its MCP endpoint accepts any `Host` header. Any device on the LAN, and any web page open in a browser on the LAN (through DNS rebinding), can therefore read the whole house and send commands, alarm arming and disarming included. Authentication and TLS arrive in a later phase.

:::danger Keep the hub off the internet
No port forwarding, and no exposure through a reverse proxy without authentication in front of it.
:::

## The native alarm

The hub can run the alarm itself, optional and off by default (see [Alarm](alarm.md)). It has **no codes or PINs**: the panel asks for none to arm or disarm, because the hub has no authentication to protect a code with. Anyone who can reach the hub on your LAN can disarm it, and the remote and push Disarm buttons work without any check. Keep the alarm's sirens and the hub on your own network, and read the caveat above before you rely on it.

## What else to know

- `get_entity` and the `bindings` it returns hand out the raw discovery config; for a bridge other than Zigbee2MQTT that config could carry credentials in topics or templates.
- With a `[frigate]` section the hub also fetches alert media from Frigate with the configured login. It only ever requests an event's `snapshot.jpg`, `clip.mp4` or `thumbnail.jpg` on that origin and follows no redirect (plus, with `live = true`, a camera's `latest.jpg` and MJPEG stream, and `/api/config` for the camera list, which it never passes on), but any LAN client can request those, so give it a Frigate *viewer* account, not the admin. See [Cameras (Frigate)](cameras.md).
- Flows are retained on the broker in clear, so the hub refuses a flow whose `command` node carries an alarm code. Arm and disarm from a flow only on a panel that needs none. See [Flows](flows.md).
