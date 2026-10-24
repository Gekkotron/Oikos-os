---
title: Push notifications
sidebar_position: 8
description: How the hub sends alerts to the Android app through Firebase Cloud Messaging, the topics it uses, its configuration and how to switch over from OikosPush.
---

The hub sends the house's push notifications to the Android app through Firebase Cloud Messaging. Without a Firebase key it keeps the alert history and sends nothing, with one warning at start. The hub starts either way.

## Turn it on

Copy the Firebase service-account key to `secrets/firebase.json` in the clone and restart the hub:

```bash
cp ~/firebase-service-account.json secrets/firebase.json
docker compose restart hub
```

Nothing tracked needs editing: `compose.yml` always mounts `secrets/` read-only at `/etc/oikos/secrets`, and git and the Docker build ignore everything in it. The hub looks for `secrets/firebase.json` next to its config file, so the same folder works when it runs straight from the repo. Remove the file and restart to turn push off.

To keep the key elsewhere, point `[push] credentials` (or `OIKOS_PUSH__CREDENTIALS`, for example in the git-ignored `.env`) at it; an explicit path wins over `secrets/`. A key file that is unreadable turns push off the same way, and the warning names its path. Only the path is ever logged.

## Configuration

```toml
[push]
prefix = "oikos/push"            # <prefix>/notification/<source>, /tokens/<installId>, /responses, /history
legacy_prefix = "oikos-push"     # OikosPush's topics, still read during the switch-over; "" turns them off
# credentials = "/path/to/firebase-service-account.json"  # leave out to use secrets/firebase.json
```

`retry_interval` (60 s), `max_retries` (10), `history_size` (100 alerts) and `history_days` (7) keep OikosPush's values. The old `history_topic` and `responses_topic` keys are refused at start: the error names the keys `[push]` takes.

## How it works

Producers publish an alert (OikosPush's v1 or legacy payload) on `oikos/push/notification/<source>`; the hub normalises it, adds it to its history and sends it through FCM to every app install registered on `oikos/push/tokens/<installId>`. Sends that fail for a transient reason are retried from memory every `retry_interval` seconds, up to `max_retries` times; a dead token is cleared on the broker. Parts of the hub that raise alerts themselves hand them to the same path in process, without the broker. Camera alerts from Frigate are one such part; see [Cameras (Frigate)](cameras.md).

| Topic | Direction | Retained | Content |
| --- | --- | --- | --- |
| `oikos/push/notification/<source>` | producer → hub | no | v1 or legacy payload |
| `oikos/push/tokens/<installId>` | app → hub | yes | `{token, device_name, lang, updated_at}`; an empty payload unregisters |
| `oikos/push/responses` | app / web → producer | no | `{id, action, install_id, ts}` |
| `oikos/push/history` | hub → app, web | yes | JSON array of the normalised alerts, newest first |

After each (re)connect the hub waits for its subscription to be acknowledged, then 2 s more for the retained tokens, before it sends; alerts arriving meanwhile wait in the queue. The history is the retained `oikos/push/history` itself: the hub adopts it at its first sync, and only when that topic holds no list yet does it seed it once from OikosPush's `oikos-push/history`.

While `legacy_prefix` is set the hub also reads `oikos-push/notification/+`, `oikos-push/tokens/+` and `oikos-push/history`, so producers outside this repo and an app build that is not updated yet keep working; a token registered on both topics is sent to once. The hub never publishes there, except an empty retained payload clearing a dead token that came from `oikos-push/tokens/<id>`.

## Alarm pushes

The native alarm sends on its own channel, `alarm`, from source `alarm`. A triggered alarm is a critical alert with a **Disarm** button: tapping it sends the reply on `oikos/push/responses`, and the hub disarms the alarm. That reply topic is subscribed while the alarm is on. A plain arm or disarm sends no push, and an armed house with an open window sends a quiet note. See [Alarm](alarm.md#phone-push).

## Switching over from OikosPush

0. **Broker ACL.** The hub connects as the `[mqtt] user` of `oikos.toml`. That user needs:
   - [ ] read and write on `oikos/push/#`;
   - [ ] read on `oikos-push/notification/+` and `oikos-push/history`;
   - [ ] read and write on `oikos-push/tokens/+` (the hub clears dead tokens there);
   - [ ] read on `<frigate prefix>/events` and `encryption-gateway/health/+`.

   Mosquitto denies silently, but the Python ACL watchdog keeps running throughout, so a missing grant shows up on the phone as a system alert. The ACL file lives on the broker host, outside this repo.
1. Ship the hub with push on but **no key** (no `secrets/firebase.json`, no `[push] credentials`), and the Frigate bridge and the gateway watchdog **off**: it builds its history from the producers' notifications and shows alerts, and sends nothing. OikosPush is still the sender. Check that the hub's history matches OikosPush's: for alerts received while both run, the same ids in the same order with the same `cancel` state. A mismatch is a bug to fix before step 2.
2. Stop the OikosPush core, the Frigate bridge container and the gateway watchdog container (`docker compose down` on the broker host). Leave the ACL watchdog container running: it stays the Python container until the hub runs next to Mosquitto (or can read its log or `$SYS` topic), and `[watchdog.acl]` is left off until then.
3. Copy the service-account key from the OikosPush deploy directory to `secrets/firebase.json` (git-ignored, never printed), turn `[frigate] bridge` and `[watchdog.gateway]` on (`[watchdog.acl]` stays off, see above), and restart the hub.
4. Send a test notification over MCP and check that it arrives once. Trigger a camera event and check the snapshot, then the clip.

Until step 3 both the hub and OikosPush publish a retained history, on different topics (`oikos/push/history` and `oikos-push/history`); the current app reads the old one, so nothing changes for the phone.
