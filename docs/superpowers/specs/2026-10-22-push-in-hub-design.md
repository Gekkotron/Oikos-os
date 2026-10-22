# Push notifications in the hub (absorbing OikosPush)

Status: design, 2026-10-22.

## 1. Intent

Phone notifications for the Oikos Android app are sent today by **OikosPush**, a separate Python service (`~/Documents/SourcesPerso/OikosPush`, reference commit `f122031`) running in Docker next to Mosquitto, with three more containers beside it: the Frigate bridge, the broker ACL watchdog and the gateway health watchdog. The hub only reads OikosPush's retained history (web Alerts) and publishes button replies where the app does.

The user wants all of it **inside Oikos-os**, as crates of the workspace, so the OikosPush containers can be switched off for good. This was phase 3 of the hub design (`2026-10-18-oikos-os-design.md` §9). Once the hub owns push, flows and MCP can send a notification too.

The project is generic: nothing below is specific to the user's house (one broker, one Firebase project, any number of phones).

## 2. Goals, non-goals, anti-goals

**Goals**
- The hub sends FCM pushes itself with the behaviour OikosPush has today (§4): same normalisation, same FCM data map, same token handling, same history rules, same retries.
- The Frigate bridge runs in the hub (§5).
- The broker ACL watchdog and the gateway health watchdog run in the hub (§6).
- Topics move from `oikos-push/…` to `oikos/push/…` (§3), and the Android app moves with them in a parallel session (§9).
- A flow can send a notification (`notify` node) and MCP can send one and list alerts (§7).
- The OikosPush containers can be stopped with no lost alert and no double delivery (§8).

**Non-goals**
- Reporting that the hub itself is down. Once the hub is the sender, nothing can push that; OikosPush has the same blind spot today. It belongs to the deployment phase.
- New alert sources beyond today's three bridges.
- New web screens. The Alerts screens stay as they are; they now read the hub's own store.
- OikosPush's heartbeat topic: no consumer exists (the app does not read it), so it is dropped.

**Anti-goals** (failure even if it technically works)
- An alert delivered twice to the phone during or after the switch-over.
- An alert lost silently: a producer's payload that OikosPush would have sent and the hub drops without a log line.
- The hub refusing to start, or the house UI breaking, because Firebase credentials are missing or wrong. Push is an optional part of the hub, like history.

## 3. Topics (decided with the user: "ok for the new topic")

The prefix is `oikos/push` (configurable, `[push] prefix`). It sits under the hub's `oikos` discovery prefix, so `push` is added to the reserved sub-trees (`crates/oikos-core/src/discovery/topic.rs`) and is never read as discovery.

| Topic | Direction | Retained | Content |
|---|---|---|---|
| `oikos/push/notification/<source>` | producer → hub | no | v1 or legacy payload (OikosPush README "v1 payload", "Legacy payload") |
| `oikos/push/tokens/<installId>` | app → hub | yes | `{token, device_name, lang, updated_at}`; empty payload unregisters |
| `oikos/push/responses` | app / web → producer | no | `{id, action, install_id, ts}` |
| `oikos/push/history` | hub → app, web | yes | JSON array of normalised alerts, newest first |

**History topic (Claude's call):** earlier notes said `oikos/history`. Since then the hub gained entity history (Timescale, `/api/v1/history`), and `oikos/history` would read as that. The alert history goes to `oikos/push/history` instead, next to the other push topics.

**Legacy intake during the transition (Claude's call):** the hub also subscribes to `oikos-push/notification/+` and `oikos-push/tokens/+`, and treats them exactly like the new topics, so producers outside this repo (Jeedom, scripts) and an app build that is not updated yet keep working. A token present on both topics is sent to once (dedupe by token, as OikosPush does). The legacy prefix is configurable (`[push] legacy_prefix`, default `oikos-push`, empty disables it). The hub never publishes on the legacy topics, except when it clears a dead token that came from one (§4).

## 4. `oikos-push` crate

Behaviour parity with OikosPush is the rule: its README, `normalize.py`, `history.py`, `tokens.py`, `fcm_sender.py`, `notification_queue.py` and its `tests/` are the specification. Port the tests as Rust tests. In short:

- **Normalisation.** v1 payloads (`v: 1`, `id`, `title`, the optional `body`, `channel`, `severity`, `image_url`, `video_url`, `entity_id`, `actions`, `silent`, `ttl`, `cancel`, `loc`) and legacy Mqtt2FCMPush payloads, with the legacy mapping table. Unknown channel → `misc`. Rejected payloads are logged and dropped.
- **Tokens.** Rebuilt from retained messages on every (re)connect; "synced" once the subscription is acknowledged plus a 2 s settle; alerts received before that are queued and sent once synced. With no token once synced, an alert is kept in the history only.
- **FCM.** HTTP v1 API, data-only messages, signed in with the Firebase service-account key (`[push] credentials`, a file path). Android priority, TTL, the 4 KB data-map limit with `…` body truncation, and the data-map keys (including `loc`) as in OikosPush. `UNREGISTERED` or a token-related `INVALID_ARGUMENT` clears the token: an empty retained payload on every install topic that used it, on the topic it came from (new or legacy). A message-related `INVALID_ARGUMENT` is logged and dropped. Other failures retry for the failed tokens only, every `retry_interval` (60 s), up to `max_retries` (10) or the TTL.
- **History.** Same rules: an existing id is replaced and moved to the front, a cancel marks the stored alert in place, trimmed to `history_size` (100) and `history_days` (7) by `sent_at`, republished retained after every change and on every (re)connect.
- **Persistence (Claude's call).** The broker is the database, as for the rest of the hub: on start the history is loaded from the retained `oikos/push/history` (or, while that is empty, once from the legacy `oikos-push/history`, which seeds it). The retry queue is kept **in memory only**. A hub restart loses pending retries (at most a TTL, 12 h, of undelivered alerts); the alerts are still in the history, which the app catches up from. This drops OikosPush's `data/` directory.
- **Without credentials** the crate still normalises and keeps the history (so web Alerts, flows and MCP work), logs one warning at start, and sends nothing. This is also how the switch-over avoids double delivery (§8).
- **API.** The crate exposes an in-process handle: `send(alert)` (used by the Frigate bridge, the watchdogs, the flow `notify` node and MCP), and a stream of history changes (used by the hub's existing alert store, which today parses the retained topic).

The crate does MQTT through the hub's existing connection (subscriptions and publishes routed by the hub), not a second client, so it shares the hub's connection state and credentials. Today the hub subscribes centrally (`Hub::subscription_topics()`, resubscribing on every ConnAck in `crates/oikos-os/src/mqtt.rs`) and does not surface SUBACK. The plan's first task extends that loop so a subsystem learns when its filters are acknowledged after each (re)connect, which the token "synced" rule needs. That task gates the rest.

**Seeding from the legacy history (Claude's call):** at boot, once the retained burst has settled, if `oikos/push/history` is still empty and `oikos-push/history` is not, the hub copies the legacy list once and republishes it on the new topic. It never seeds again once the new topic holds a list, so restarts during and after the switch-over do not re-import.

## 5. `oikos-frigate` crate (user: Frigate bridge as its own crate)

The behaviour of `bridges/frigate/frigate_bridge.py` and its README: listens on `<frigate topic prefix>/events`, sends the first qualifying event as an audible alert with the snapshot, sends the same id again on `end` with the clip and `silent: true`, label / camera / zone filters, per-camera cooldown, severity, TTL, the `loc` hint `frigate.object`. Id `frigate-<event id>`, channel `camera`, title the camera name.

It hands alerts to `oikos-push` in process; `source` is `frigate`. Settings go in the existing `[frigate]` section: `media_url` (the base URL the phone uses for media, the bridge's `FRIGATE_URL`; defaults to `url`), `labels`, `cameras`, `zones`, `cooldown`, `severity`, `ttl`, `topic_prefix`. The bridge is on only when `[frigate]` has a URL **and** `bridge = true`; `bridge` defaults to `false` (Claude's call), so upgrading a hub that already uses `[frigate]` for the media proxy does not start a second bridge next to the Python one. Frigate publishes its events on the same broker as the hub (the Python bridge used OikosPush's own `MQTT_BROKER`), so the bridge's `<topic_prefix>/events` subscription goes through the hub's connection like every other topic.

Whether the media proxy and the Frigate login code (`crates/oikos-os/src/media.rs`) move into this crate is left to the builder.

## 6. `oikos-watchdog` crate (user: watchdogs in another crate; in-process was Claude's call, user: "ok for all")

Both watchdogs run inside the hub and hand alerts to `oikos-push` in process (`source` `system`, channel `system`). Behaviour as in `bridges/*_watchdog/*.py` and their READMEs.

- **Gateway health.** Subscribes to `<prefix>/+` (default `encryption-gateway/health`), keeps each component's last status, pushes on transitions only, with the README's transition table and severities.
- **Broker ACL.** Turns Mosquitto `Denied PUBLISH/SUBSCRIBE/UNSUBSCRIBE` lines into alerts, with the cooldown per `(client, topic, action)` and the `(+N more in last Xs)` suffix. Two sources, as in Python: `topic` (`$SYS/broker/log/#`) and `file` (tails a log path). **Constraint:** the house broker does not forward denials to `$SYS` (that is why the Python bridge gained the file source), and the hub runs on the Mac while `mosquitto.log` is on the Geekom. Until the hub is deployed on the Geekom, the ACL watchdog is configured off in the house and the Python ACL container keeps running, publishing on `oikos-push/notification/system`, which the hub accepts through the legacy intake (§3).

Config: `[watchdog.gateway]` (`enabled`, `prefix`) and `[watchdog.acl]` (`enabled`, `source`, `log_file`, `log_topic`, `cooldown`). Both are off by default.

## 7. Flows, MCP, HTTP

- **Flow node `notify`.** Parameters: `title` (required), `body`, `channel` (default `misc`), `severity` (default `info`), `entity_id`, `image_url`, `silent`, and an optional `id` (default: `flow-<flow id>-<run id>`; giving a fixed `id` makes each run replace the previous alert). `source` is `flow`. No output ports beyond the default edge. Validation follows the other nodes (`crates/oikos-core/src/flow/validate.rs`). The web flow editor's ＋Add picker and parameter editor get the node.
- **MCP.** `send_notification` (the same fields as the node) and `list_alerts` (the history, newest first, with an optional `limit` and `channel`). The server instructions mention both and the `notify` node.
- **HTTP.** `/api/v1/alerts` and the action route keep their shape, read from the in-process store; the action route publishes on `[push] responses` topic (`oikos/push/responses`). `[push] history_topic` / `responses_topic` are replaced by `prefix` and `legacy_prefix`; a config still setting the old keys is a start-up error naming the new keys (Claude's call: the only config is the user's). Web button replies keep `install_id: "oikos-web"`. `oikos.example.toml` and the README follow this document where they disagree (they still say `oikos/history`).
- `/api/v1/status` (or the existing status payload) reports push as `off` (no credentials), `syncing`, or `ready` with the number of registered installs. No web UI beyond what already reads the status.

## 8. Switch-over (in the house)

0. Broker ACL. The hub connects as the `[mqtt] user` of `oikos.toml`. That user needs: read and write on `oikos/push/#`; read on `oikos-push/notification/+` and `oikos-push/history`; read and write on `oikos-push/tokens/+` (it clears dead tokens there); read on `<frigate prefix>/events` and `encryption-gateway/health/+`. Mosquitto denies silently, but the Python ACL watchdog keeps running throughout (§6), so a missing grant shows up on the phone as a system alert. The README carries this checklist; the ACL file itself lives on the Geekom, outside this repo, and the user edits it.
1. Ship the hub with push on but **no `[push] credentials`**, and the Frigate bridge and the gateway watchdog **off**: it builds its history from the producers' notifications and shows alerts, sends nothing. OikosPush is still the sender. Check the hub's history matches OikosPush's: for alerts received while both run, the same ids in the same order with the same `cancel` state. A mismatch is a bug to fix before step 2.
2. Stop the OikosPush core, the Frigate bridge container and the gateway watchdog container (`docker compose down` on the Geekom). Leave the ACL watchdog container running (§6).
3. Set `[push] credentials` (copy the service-account key from the OikosPush deploy dir to the Mac, git-ignored, never printed), turn `[frigate] bridge` and `[watchdog.gateway]` on, restart the hub.
4. Send a test notification over MCP; check it arrives once. Trigger a camera event; check snapshot then clip.

Until step 3 both the hub and OikosPush publish a retained history on different topics (`oikos/push/history` and `oikos-push/history`); the current app reads the old one, so nothing changes for the phone.

## 9. Android app (user: "launch the change in an app side session")

A separate session in the app repo (`~/Documents/SourcesPerso/Oikos`) moves `PushTopics` (`core/alerts/.../PushTopics.kt`) to `oikos/push/tokens/{installId}`, `oikos/push/responses`, `oikos/push/history`, and on its next registration clears its old retained `oikos-push/tokens/<installId>` (empty retained payload) so it is not counted twice. It is started once this design is approved, with this section and §3 as its contract. The hub's legacy intake means the two sides can land in any order.

## 10. Constraints

- Rust workspace conventions as the other crates (edition 2024, workspace dependencies, Apache-2.0, author Gekkotron). New crates: `crates/oikos-push`, `crates/oikos-frigate`, `crates/oikos-watchdog`, all started by the `oikos-os` binary.
- No per-repo cache or target overrides (`$DEV_SCRATCH` is shared).
- Secrets (Firebase key, Frigate login) are never logged, printed or committed; `oikos.toml` stays git-ignored.
- README sections updated: topics, config, MCP tools, flow node, switch-over.

## 11. Left to the builder

- The FCM client: hand-rolled OAuth2 JWT (RS256) + `reqwest`, or a crate such as `gcp_auth`. Token caching and refresh.
- How the three crates plug into the hub (traits, channels, the MQTT routing), and whether `oikos-push` depends on `oikos-core` or only on `oikos-api` types.
- The exact Rust shape of `AlertView` vs the crate's own alert type.
- Whether the media proxy moves to `oikos-frigate` (§5).
- Task order, as long as the hub stays shippable after each task.
