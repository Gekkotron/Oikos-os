# Onboarding: set up the hub from the browser

Status: design, awaiting approval. Date: 2026-10-28.

## Intent

Installing Oikos-os today means copying `oikos.example.toml`, editing it by hand, dropping key
files in `secrets/`, and knowing which sections matter. The goal is an install where the user runs
one command, opens a URL, and sets the house up in the browser. `oikos.toml` stops being something
a user edits.

Two facts changed since the file was designed:

- TimescaleDB is no longer optional. History, climate learning and the solar forecast all need it,
  so it can also hold the hub's settings.
- Most settings that were added later (home, alarm, energy roles, off-peak hours, flows, groups)
  are already edited in the web UI. What is left in the file is the connection plumbing and a few
  services: MQTT, the database, weather, Solcast, Frigate, push, the climate switch, time zone and
  location.

Onboarding is for **every** hub, not only new ones. An existing hub with an `oikos.toml` imports
it once and carries on without the user noticing.

## Goals

- `git clone`, then `docker compose up`, starts Mosquitto, TimescaleDB and the hub. The hub's log
  prints the URL to open, like `Open http://192.168.1.20:8081 to set up your home`.
- The first visit opens a setup wizard: **Welcome → MQTT → Database → Devices → Services → Done**.
- Every value the wizard sets can be changed later from a Settings page, using the same form.
- People who already run their own broker and database start only the hub
  (`docker compose up -d hub`) and point the wizard at them. Nothing gets recreated.
- An existing hub with an `oikos.toml` imports it on first start, skips the wizard, and keeps
  running with the same behaviour.
- Zigbee2MQTT stays external. It needs the USB Zigbee dongle on whatever machine has it.

## Non-goals

- Authentication and TLS. The hub still has no login (see `docs/users/security.md`); this design
  doesn't change that, but it doesn't make it worse either (see "Secrets").
- Bundling Zigbee2MQTT or Frigate.
- Importing from Jeedom or Home Assistant.
- A UI for the low-level knobs: topic prefixes, command timeouts, `runs_kept`, push retry timing,
  watchdogs, `send_ha_birth`. They keep their defaults. Imported values are kept, and they can be
  changed through the settings API (and MCP), but no screen shows them.
- Backup and restore from the UI.

## Anti-goals

These would make it a failure even if it technically works:

- The live house goes down, or loses flows, alarm or climate settings, when an existing hub
  upgrades.
- A secret (MQTT password, database password, Solcast key, Frigate password, Firebase key) can be
  read back over the HTTP API or MCP.
- Closing the browser halfway through setup loses what was entered, or leaves a hub that won't
  start.
- The user has to open a terminal again after `docker compose up`, except to read the URL.

## Install and compose

`compose.yml` gets four services. *(The user chose: by default, everything runs.)*

| Service | Image | Notes |
| --- | --- | --- |
| `init` | the hub image, command `oikos-os init-bundled --data /data` | One-shot. Creates the bundled credentials if missing, then exits. |
| `mosquitto` | `eclipse-mosquitto:<pinned 2.x tag>` | Host network, `depends_on: init` (completed). Config shipped in the repo. |
| `timescaledb` | `timescale/timescaledb:<pinned tag>-pg17` | Host network, `depends_on: init` (completed). |
| `hub` | built from the `Dockerfile` | Host network, **no** `depends_on`, so `docker compose up -d hub` starts it alone. |

- Volumes: everything the hub writes lives in `./data` (git-ignored), mounted at `/data`.
  Mosquitto uses `./data/mosquitto/{config,data,log}`; TimescaleDB uses `./data/timescaledb`.
  `compose.yml` no longer mounts `oikos.toml` or `secrets/`. Mounting a missing file makes Docker
  create a directory in its place, which would break new installs.
- Service names and mount targets match the ones in the migration guide the user is following
  (`mosquitto`: `/mosquitto/config`, `/mosquitto/data`, `/mosquitto/log`; `timescaledb`:
  `/var/lib/postgresql/data`). That way a `compose.override.yml` written for that migration keeps
  working on top of the new `compose.yml`, because Compose merges volumes by target path.
- **Bundled credentials** *(my call)*: `init-bundled` is idempotent. When `./data/bundled.json` is
  missing, it generates a random MQTT password and a random database password. It writes:
  - the Mosquitto password file (`mosquitto_passwd` hash format) for user `oikos`;
  - a file that the TimescaleDB image reads through `POSTGRES_PASSWORD_FILE` on its first start;
  - `./data/bundled.json` with both credentials in clear text (file mode `0600`), so the wizard can
    pre-fill them.

  When the file exists, `init-bundled` changes nothing. The bundled Mosquitto config has
  `persistence true` and `allow_anonymous false`, and listens on 1883 on all interfaces, so
  Zigbee2MQTT on another machine can reach it.
- **Port clashes.** With host networking, a bundled service fails to start if the host already
  runs a broker on 1883 or Postgres on that port. The install docs say so and give the hub-only
  command. The wizard doesn't depend on the bundled services being up.
- **The URL banner** *(my call)*: at startup the hub logs one line per non-loopback IPv4 address of
  the host (skipping Docker bridge interfaces), for example `Open http://192.168.1.20:8081 to set up
  your home`, or `… to open your home` once setup is done. With `docker compose up` in the
  foreground the user sees it straight away. The docs give `docker compose logs hub` for the `-d`
  case.

## Where settings live

Two places, split by what the hub needs before it can reach anything:

1. **`/data/hub.json`**, the bootstrap file. It holds the MQTT connection (host, port, user,
   password, client id, Zigbee2MQTT base topic), the database URL, the **location** (city,
   country, latitude, longitude, IANA time zone), the HTTP bind address, the log level, the
   wizard's progress and the "setup finished" flag. The hub writes it, one step at a time, and the
   user never edits it. *(My call: the location lives here and not in the table, because the
   Welcome step runs before any database exists and `Hub::new` needs the time zone.)*
2. **A `settings` table in TimescaleDB** for everything else: one row per section (`weather`,
   `solcast`, `frigate`, `push`, `climate`, `advanced`), with
   `key text primary key, value jsonb, updated_at timestamptz`, added by a new embedded migration.
   Secrets are stored in the same rows.

**Location replaces two older fields.** `[flows] timezone/latitude/longitude` and
`[weather] city/country` all move into the one location. Weather uses its coordinates and city
name, and stops geocoding a city of its own. The weather row keeps only on/off and the refresh
interval. *(My call.)*

**Language** stays per browser, as the web UI's FR/EN choice works today. The Welcome step sets it
for that browser. *(My call.)*

*(My call)* A copy of the last settings read successfully is kept in `/data/settings-cache.json`
(`0600`). If the database is down at boot, the hub starts from that copy and logs a warning,
because the database being down must not stop the house. Today a down database only turns history
off; that stays true.

The existing retained-MQTT documents (home settings, alarm, climate schedules, flows, groups,
dashboards) **don't move**. They already work and are already editable. This design only replaces
`oikos.toml`.

`OIKOS_*` environment variables keep overriding values *(my call)*, but only bootstrap values (HTTP
bind, log level, MQTT, database URL). That keeps the Dockerfile's `OIKOS_HTTP__BIND` and lets
someone script an unattended install.

The `mcp-stdio` subcommand only uses config for logging. It reads the log level from
`/data/hub.json` when it exists, and uses defaults otherwise. `--config` stays, only to name a file
to import. A new `--data <dir>` flag (default `./data` outside Docker, `/data` in the image) says
where `hub.json` and the cache live. *(My call.)*

Internally, the existing `Config` structs stay as the in-memory shape. They are loaded from
`hub.json`, the settings rows and the env layer instead of the TOML file. The validators
(`weather.validate()`, `solcast.validate()`, …) run when settings are saved, and their messages are
shown in the form.

## Hub states

| State | When | What runs |
| --- | --- | --- |
| Import | no `hub.json`, but `/data/oikos.toml` exists (or `--config <path>` is given) | Import (below), then Normal. |
| Setup mode | `hub.json` is missing, or lacks a tested MQTT connection or database URL | HTTP only: the web UI, the setup API, the banner. No MQTT, no engine. `/` redirects to `/setup`. |
| Normal | `hub.json` holds both MQTT and the database | Everything, as today. If setup was never finished, `/` redirects to `/setup` at the step where it stopped. |

Each wizard step writes its part of `hub.json` when the user presses Next, so a half-done setup
resumes where it stopped. Once MQTT and the database are both saved, the hub starts its MQTT link,
the database writer and the engine, exactly as on a normal boot. From that point on the hub **is**
running; the remaining steps (Devices, Services) are ordinary settings shown inside the wizard.
"Setup finished" only decides where `/` lands.

**Applying a change** *(my call)*. The simplest acceptable mechanism is for the hub to re-execute
itself after any settings change that needs it, including leaving setup mode. That way the existing
start-once structure of `main.rs` and `Hub` doesn't need refactoring into something reconfigurable.
Requirements:
- It works without Docker (`cargo run`), so it can't rely on Docker's restart policy. The process
  replaces itself (`exec` of its own binary with the same arguments), it doesn't just exit.
- The wizard and Settings pages tolerate the few seconds the HTTP server is gone. They show
  "Applying…" and reconnect when the hub answers again.
- The builder may restart a single task in place instead, where that's cheap (weather, Solcast,
  Frigate bridge). That's an optimisation, not a requirement.

## Import from `oikos.toml`

For existing hubs. `compose.yml` no longer mounts `oikos.toml` and `secrets/`, so they have to move
into `./data/` before the new hub starts. *(My call)* `tools/update.sh`, the documented upgrade
command, does it: when it finds `./oikos.toml` or `./secrets/` at the repo root, it moves them into
`./data/` before rebuilding and says so. The upgrade notes give the same two `mv` lines for anyone
upgrading by hand. They also say that someone who runs their own broker and database (and hasn't
moved them into the stack) must keep using `docker compose up -d --build hub`. `update.sh` builds
and starts only the hub when `./data/bundled.json` doesn't exist and an `oikos.toml` was found.

On a start with no `hub.json`:

1. If `/data/oikos.toml` exists (or `--config` names a file), load it with today's loader,
   including today's `OIKOS_*` env layer and today's validation. If it's invalid, refuse to start
   with today's error, as now.
2. Write `hub.json` from `[mqtt]`, `[topics] z2m_base`, `[history] url`, `[http]`, `[log]` and the
   location (`[flows] timezone/latitude/longitude`, plus `[weather] city/country` as the location's
   name). Mark setup finished. If `[flows]` has no coordinates but weather has a city, the
   coordinates come from geocoding that city, retried in the background; until then the old
   defaults (Paris) apply, as today.
3. Boot **Normal** straight away, with the remaining sections taken from the TOML values in memory
   and written to `settings-cache.json`. The house never waits on the database.
4. In the background, once the database answers (retrying, as the history writer does today), run
   the migrations and write each remaining section as a `settings` row. Values the user never set
   are written as their defaults, so behaviour doesn't change. Read `secrets/firebase.json` and
   `secrets/solcast.key` (or the paths named in the TOML) into the `push` and `solcast` rows. A
   `settings_imported` flag in `hub.json` records that this copy is done; until it is set, every
   start repeats it from the cache.
5. When the copy is done, log
   `oikos.toml imported into the database; it is no longer read and can be deleted`.

A `[history]` URL that's empty can't be fully imported. The hub writes MQTT and the location to
`hub.json` and starts in setup mode at the Database step. The file is never modified.

## The wizard

Route `/setup` in the web UI, in the existing look (theme and FR/EN). Each step saves when the user
presses Next, so closing the tab loses nothing. Back is always allowed. Steps after Database are
skippable.

1. **Welcome.** Language (FR/EN, pre-picked from the browser), then "Where is your home?": a city
   and country, resolved through Open-Meteo's geocoding (the same lookup the weather feature already
   uses) into a latitude, longitude and IANA time zone that the user can correct. *(My call: the sun
   trigger needs the location even when weather is off, so it's asked here rather than in the
   Weather step.)*
2. **MQTT.** Host, port, user, password. Pre-filled from `bundled.json` when it exists, otherwise
   `localhost:1883`. A collapsed "Advanced" part holds the client id and the Zigbee2MQTT base topic.
   **Test connection** connects with those values and reports success or the broker's reason
   (refused, bad credentials, timed out). Next is enabled only after a successful test.
3. **Database.** Host, port, database, user, password (shown as fields, stored as a URL).
   Pre-filled from `bundled.json` when it exists. The test connects, checks that the `timescaledb`
   extension is available, and runs the migrations. If the database already holds Oikos history (an
   existing hub reached through the wizard), say so and how far back it goes. Required: there's no
   Skip.
4. **Devices.** What the hub has found through MQTT discovery, grouped by room, with a live count.
   *(My call: a room can be set and a device renamed inline, reusing the device-room control from
   the device sheet, but none of it is required.)* With nothing found after a few seconds: "No
   devices yet. Check that Zigbee2MQTT publishes to this broker under `<base topic>`", with a link
   back to the MQTT step.
5. **Services.** One screen listing the optional services as cards. Each card shows "Not set up",
   "Set up" or "Skipped" and opens its form in place. *(My call: one checklist screen, not one step
   per service, so a user can do just the one they care about.)*
   - **Weather:** on/off; uses the Welcome location; refresh minutes.
   - **Solar forecast (Solcast):** API key, rooftop site ids, daily limit; the test makes one call.
   - **Energy:** which detected sensors are the grid meter (bought/sold) and solar production,
     using the existing energy roles, plus off-peak hours (the existing home settings).
   - **Cameras (Frigate):** URL, user, password, the media URL for the phone, alerts on/off with
     labels and cameras, live view on/off. The test fetches `/api/config` and lists the cameras.
   - **Push notifications:** upload the Firebase service-account JSON; the card shows its project
     id once it's stored.
   - **Climate engine:** on/off, with the existing "beta" warning.
   - **Alarm:** on/off, linking to the existing alarm settings page.
6. **Done.** A summary of what's set up and what was skipped, and a button to the house.

## Settings afterwards

Each wizard step has a matching Settings page that uses the same form component. The Settings list
gains **Location**, **Database**, **Weather**, **Solar forecast**, **Cameras** and
**Push notifications**. The existing **Broker** page becomes editable (it's the MQTT step), and
**Energy** settings join the existing **Home** page. Settings has a "Run setup again" link to
`/setup`, which never wipes anything.

## API

The REST API grows a settings resource *(shape is my call; details left to the builder)*:

- `GET /api/v1/setup`: the state (setup mode or not, finished or not, the step to resume, whether
  bundled credentials exist).
- `POST /api/v1/setup/test/{mqtt|database|solcast|frigate}`: tests values without saving them.
- `GET /api/v1/settings/{section}` / `PUT /api/v1/settings/{section}`: read and replace a section.
  Bootstrap sections (`mqtt`, `database`) go to `hub.json`; the rest go to the table.
- `POST /api/v1/setup/finish`.
- MCP gets matching read and write tools for the sections, following the existing tool style.

## Secrets

- A secret is **write-only** over HTTP and MCP. Reads return `{"set": true}` or `{"set": false}`,
  never the value. A form shows "Saved, enter a new one to replace it". In a `PUT`, a secret that's
  left out keeps the stored one, and an explicit `null` clears it. *(My call.)*
- *(My call)* **A new address needs the secret again.** A `PUT` that changes a service's URL or
  host (Frigate, broker, database, Solcast has none) without also sending its password is refused
  with "Enter the password for the new address". Without this rule, anyone on the LAN could point
  the Frigate URL (or the broker host) at their own machine and collect the stored password, which
  isn't possible today because the settings live in a file.
- Logs never print a secret. `Debug` impls redact them, as `MqttConfig` already does.
- `bundled.json`, `hub.json` and `settings-cache.json` are written with mode `0600`.

## Docs

- `docs/users/install-on-linux.md` and `getting-started.md` are rewritten around
  `docker compose up` and the wizard, with a "Bring your own broker and database" section (the
  hub-only command and the port-clash note) and an "Upgrading from oikos.toml" section (move
  `oikos.toml` and `secrets/` into `./data/`, then the import).
- Every page that shows a `[section]` of TOML (`weather.md`, `solar-forecast.md`, `cameras.md`,
  `push-notifications.md`, `climate.md`, `history.md`, `watchdogs.md`, `flows.md` for the time
  zone) is changed to point at its Settings page instead.
- `oikos.example.toml` is deleted. The import reads any older file.
- A user doc, "Move an existing broker and database into the stack", carries the generic version of
  the migration guide written on 2026-10-28.

## Left to the builder

- How changes are applied (restarting tasks in place versus re-exec), within the rule above.
- The exact JSON shapes of the setup and settings API, and the MCP tool names.
- How the wizard's step progress is stored (in `hub.json` or a settings row), as long as resuming
  works.
- The bundled image tags, which are pinned to current stable releases.
- How the IPs in the banner are chosen, as long as Docker bridges and loopback are skipped.
- How to split the work into plans. Hub storage, import and setup mode must land before the wizard
  UI, and compose and docs come last.
