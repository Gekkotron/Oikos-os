---
title: Getting started
sidebar_position: 1
description: Run the hub from source, understand its configuration and open the web UI.
---

You need a Rust toolchain (the repo's `rust-toolchain.toml` sets the version) and an MQTT broker the hub can reach, normally Mosquitto with Zigbee2MQTT publishing to it.

Copy the example configuration and run the hub from the repository root:

```sh
cp oikos.example.toml oikos.toml   # then edit the [mqtt] credentials
cargo run -p oikos-os -- run
```

`oikos.toml` is git-ignored and read from the working directory (`run --config <path>` reads another file). Every key can be overridden from the environment as `OIKOS_<SECTION>__<KEY>`, for example `OIKOS_MQTT__PASSWORD=… cargo run -p oikos-os -- run`; `RUST_LOG` wins over `[log] level`. The full list of keys is in [Configuration](../reference/configuration.md).

The hub keeps a persistent MQTT session under `[mqtt] client_id` (default `oikos-os`), so a write queued just before a connection blip is retransmitted on reconnect. Two hubs on the same broker must not share a `client_id`: the broker would bounce them off each other and hand one the other's session.

Until Zigbee2MQTT publishes under `oikos`, list the Home Assistant prefix next to it so the discovery topics it still retains under `homeassistant/` are picked up too:

```toml
[topics]
discovery_prefixes = ["oikos", "homeassistant"]
```

## Open the web UI

Open `http://<host>:8081` in a browser. The hub serves the web UI on the same port as its API.

Release builds embed the web UI, so build the UI before the hub. A binary built before the UI was built serves a 503 page that says the UI has not been built. The build steps are in [Development](../contributors/development.md).

## Next steps

- [Zigbee2MQTT](zigbee2mqtt.md): point Zigbee2MQTT's discovery at the hub.
- [The web UI](web-ui.md): the screens the hub serves.
- [Install on Linux](install-on-linux.md): run the hub in Docker on a Linux host.
- [Talking to the house (MCP)](assistants-mcp.md): connect an AI assistant.
- [Cameras (Frigate)](cameras.md), [Push notifications](push-notifications.md) and [History](history.md): the optional parts.
- [Security](security.md): read this before the hub is reachable from anywhere but your LAN.
