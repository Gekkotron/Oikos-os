---
title: Security
sidebar_position: 13
description: What the hub does not protect in v1, what that means for your LAN and how to keep it off the internet.
---

There is no authentication yet, and that is deliberate for v1. The hub binds on `0.0.0.0`, answers any origin (CORS `*`) and its MCP endpoint accepts any `Host` header. Any device on the LAN, and any web page open in a browser on the LAN (through DNS rebinding), can therefore read the whole house and send commands, alarm arming and disarming included. Authentication and TLS arrive in a later phase.

:::danger Keep the hub off the internet
No port forwarding, and no exposure through a reverse proxy without authentication in front of it.
:::

## What else to know

- `get_entity` and the `bindings` it returns hand out the raw discovery config; for a bridge other than Zigbee2MQTT that config could carry credentials in topics or templates.
- With a `[frigate]` section the hub also fetches alert media from Frigate with the configured login. It only ever requests an event's `snapshot.jpg`, `clip.mp4` or `thumbnail.jpg` on that origin and follows no redirect, but any LAN client can request those, so give it a Frigate *viewer* account, not the admin. See [Cameras (Frigate)](cameras.md).
- Flows are retained on the broker in clear, so the hub refuses a flow whose `command` node carries an alarm code. Arm and disarm from a flow only on a panel that needs none. See [Flows](flows.md).
