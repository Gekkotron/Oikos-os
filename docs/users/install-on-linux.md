---
title: Install on Linux (Docker)
sidebar_position: 2
description: Install Oikos-os on a Linux host from a git clone with Docker Compose, then configure, upgrade and run it.
---

Oikos-os is published only as source. On the Linux box that will run it,
`git clone` the repo and build the image locally with the `Dockerfile`
and `compose.yml` at the repo root.

## Prerequisites on the host

- Docker Engine (24+) with the Compose plugin (2.24+).
- `git`.
- An MQTT broker on the host (Mosquitto is the default pairing).
- Zigbee2MQTT (or any other producer publishing Home Assistant MQTT
  discovery) on the host.
- Optional: TimescaleDB on the host for the history feature. Leave the
  history URL empty to disable it.
- Optional: Frigate on the host for camera alerts.

## Install

```bash
git clone https://github.com/Gekkotron/Oikos-os.git
cd Oikos-os
cp oikos.example.toml oikos.toml
# Edit oikos.toml: set [mqtt] credentials and the broker host (127.0.0.1
# when the broker runs on the same host), the [http] bind address
# (0.0.0.0:8081 to reach the UI from other devices on the LAN), and
# optionally [history] url, [frigate] and [push].
docker compose up -d --build
```

The hub listens on `:8081`. Open `http://<host-ip>:8081` from any device
on the LAN.

## Upgrade

```bash
cd Oikos-os
tools/update.sh
```

The script runs `git pull --rebase`, then `docker compose up -d --build`.

## Install profiles

The two tiers are controlled from `oikos.toml`, not from compose.

### Full (with history)

Set `[history] url` to the TimescaleDB connection string, for example:

```toml
[history]
url = "postgres://oikos:change-me@127.0.0.1:5432/oikos"
```

The hub starts the history writer; the web `/history` views come alive.

### Minimal (no history)

Leave `[history] url` empty (or omit the section). The hub runs; the
history routes answer `503`.

## Logs and status

```bash
docker compose logs -f hub          # follow the hub's logs
docker compose ps                   # container status
docker compose restart hub          # restart in place
docker compose down                 # stop and remove the container
```

## Networking

The compose file uses `network_mode: host`. The hub reaches Mosquitto,
TimescaleDB and Frigate on the host via `127.0.0.1`, and the host's
firewall decides who can reach port `8081`. Allow it inbound on your
LAN interface before pointing the Android app at the hub.

## Push notifications (optional)

The hub sends Firebase Cloud Messaging alerts with a service-account
key file (see [Push notifications](push-notifications.md)). Copy the key
to `secrets/firebase.json` in the clone and restart the hub:

```bash
cp ~/firebase-service-account.json secrets/firebase.json
docker compose restart hub
```

`compose.yml` always mounts `secrets/` read-only at
`/etc/oikos/secrets`, and the hub picks the key up there on its own, so
no tracked file needs editing. Git and the Docker build ignore
everything in `secrets/`. Remove the file and restart to turn push off.

## Overrides without editing tracked files

Any setting can also come from an `OIKOS_*` environment variable (`__`
between levels, for example `OIKOS_MQTT__PASSWORD` or
`OIKOS_PUSH__CREDENTIALS`). Put them in a `.env` file next to
`compose.yml`: compose hands it to the hub when it exists, and git
ignores it.
