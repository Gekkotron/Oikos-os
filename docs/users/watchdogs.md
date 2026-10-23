---
title: Watchdogs
sidebar_position: 12
description: Two opt-in watchdogs that raise system alerts when the encryption gateway changes state or the broker denies an MQTT action.
---

Two opt-in watchdogs raise `system` alerts (ported from OikosPush's `gateway_health_watchdog` and `broker_acl_watchdog`).

## Gateway health

`[watchdog.gateway] enabled = true` subscribes to `<prefix>/+` (default `encryption-gateway/health`), where the gateway publishes one retained state per component (`{"status": "ok" | "fail" | "config_error", ...}`). The hub alerts on a change only: `ok` to non-ok ("<component> down", `warning`), non-ok to `ok` ("<component> recovered", `info`), `fail` to `config_error`, and a first message that is not `ok`. A first `ok`, and a retained state replayed unchanged on reconnect, stay silent.

## Broker ACL

Mosquitto acknowledges an ACL-denied PUBLISH and drops it with only a log line. `[watchdog.acl] enabled = true` turns each `Denied PUBLISH/SUBSCRIBE/UNSUBSCRIBE` line (and the older `Client X is not authorised to ...` wording) into a `warning` alert "MQTT ACL denial". A client, topic and action alert once per `cooldown` seconds (default 300); later denials are counted and named in the next alert (`+N more in last 300s`). Two sources:

- `source = "topic"` (default) reads `log_topic` (`$SYS/broker/log/#`; needs `log_dest topic` in `mosquitto.conf`, and a broker that forwards ACL denials there);
- `source = "file"` tails `log_file` from its end, follows rotation and truncation, and needs the hub to read the Mosquitto log; a file source without `log_file` is a config error.

```toml
[watchdog.gateway]
enabled = true
prefix = "encryption-gateway/health"

[watchdog.acl]
enabled = true
source = "topic"
cooldown = 300
```

The broker user needs read on `<prefix>/+` and, for the topic source, on `$SYS/broker/log/#`.

The switch-over from OikosPush turns these on step by step; see [Push notifications](push-notifications.md#switching-over-from-oikospush).
