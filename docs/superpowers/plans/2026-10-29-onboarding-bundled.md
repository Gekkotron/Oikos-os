# Onboarding plan 3: bundled stack, update script and docs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `git clone` then `docker compose up` starts Mosquitto, TimescaleDB and the hub with generated credentials; `tools/update.sh` upgrades any install (including one still on `oikos.toml`); the docs describe the wizard instead of a hand-edited file.

**Architecture:** A new `oikos-os init-bundled` subcommand (run by a one-shot `init` compose service from the hub image) writes `data/bundled.json` in plan 1's `Bundled` shape, a Mosquitto password file and config, and a Postgres password file. `compose.yml` gains `init`, `mosquitto` and `timescaledb`; the hub has no `depends_on`, so `docker compose up -d hub` still starts it alone. `tools/update.sh` moves `oikos.toml` and `secrets/` into `./data/`, re-executes itself after `git pull`, and picks hub-only or the full stack.

**Tech Stack:** Rust (clap, `pbkdf2` + `sha2` + `getrandom`), Docker Compose, bash, Docusaurus docs.

**Spec:** `docs/superpowers/specs/2026-10-28-onboarding-design.md` (sections "Install and compose", "Import from `oikos.toml`", "Docs"). Plan 1: `docs/superpowers/plans/2026-10-28-onboarding-hub.md` (Task 7 fixes the `Bundled` shape). Plan 2 (web wizard) is PR #86, not merged yet: the docs describe its pages anyway.

## Global Constraints

- Generic project: never name a real host, IP, room, person or machine in tracked files (use "the home server", `192.168.1.20`, "a bedroom").
- `bundled.json`, the Postgres password file and the Mosquitto password file are written with mode `0600`, atomically, through `setup::fs::write_private_atomic`.
- `init-bundled` is idempotent: when `data/bundled.json` exists it changes nothing.
- Service names `mosquitto` and `timescaledb`, container names `oikos-mosquitto` and `oikos-timescaledb`, and mount targets `/mosquitto/config`, `/mosquitto/data`, `/mosquitto/log`, `/var/lib/postgresql/data` stay exactly these: a `compose.override.yml` from the migration guide (its own image tags, its own folders on those targets, `command: postgres -c port=5433`) must work on top of `compose.yml`.
- Pinned images: `eclipse-mosquitto:2.0.22`, `timescale/timescaledb:2.30.2-pg17`.
- Bundled names: MQTT user `oikos`; database user `oikos`, database `oikos`, port 5432 (plan 1's `BUNDLED_DB_PORT`).
- Mosquitto password lines use the `$7$` format: `user:$7$101$<base64 12-byte salt>$<base64 PBKDF2-HMAC-SHA512, 101 iterations, 64 bytes>` (standard base64 with padding).
- Every Rust build uses this prefix, three UNQUOTED assignments: `CARGO_TARGET_DIR=$HOME/dev-scratch/cargo-target-wizard CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo …`. Run `df -h ~` first; with under 5 GB free, run per-crate or per-test only.
- Commits: `git -c user.name=Gekkotron -c user.email=60887050+Gekkotron@users.noreply.github.com commit …`, explicit paths only (no `git add -A`).
- CI runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## Review Focus

1. A host that already runs a broker on 1883 or Postgres on 5432 runs `docker compose up`: the bundled services fail to bind, the hub still starts; the docs say to use `docker compose up -d hub`. Pinned by the compose check in Task 2 (hub has no `depends_on`).
2. The live-host case: `compose.override.yml` replaces the Mosquitto and Timescale volumes and sets `command: postgres -c port=5433`, and `data/hub.json` already exists. `init` must exit 0 without writing credentials, and `timescaledb` must start with no password file present. Pinned by `init_skips_a_configured_hub` (Task 1) and the port-5433 override in the CI stack job (Task 2).
3. An install still on `oikos.toml` runs `tools/update.sh`: `oikos.toml` and every key in `secrets/` (not its tracked `README.md`) move into `./data/`, nothing in `./data/` is overwritten, and only the hub is rebuilt. Pinned by `test_moves_legacy_files` and `test_never_overwrites_data` (Task 3).
4. `git pull` rewrites `tools/update.sh` while it runs: the new version must run the rest. Pinned by `test_reexecs_after_pull` (Task 3).
5. `init-bundled` re-runs after a crash that left a `passwd` but no `bundled.json`, or after `bundled.json` was deleted once Postgres initialised `data/timescaledb`: the first regenerates all files consistently, the second refuses with a clear message instead of writing a password the database never had. Pinned by `regenerates_after_a_partial_run` and `refuses_when_the_database_exists_without_credentials` (Task 1).

---

### Task 1: `init-bundled` subcommand

**Files:**
- Create: `crates/oikos-os/assets/mosquitto.conf`
- Modify: `crates/oikos-os/src/setup/bundled.rs`, `crates/oikos-os/src/cli.rs`, `crates/oikos-os/src/main.rs`, `crates/oikos-os/Cargo.toml` (add `pbkdf2 = { version = "0.12", default-features = false, features = ["hmac"] }`, `sha2 = "0.10"`, `getrandom = "0.2"`; these match versions already in `Cargo.lock`)
- Test: unit tests in `bundled.rs` and `cli.rs`; `crates/oikos-os/tests/init_bundled.rs` (assert_cmd)

**Interfaces:**
- Consumes: `setup::fs::write_private_atomic(path, bytes)`, `setup::HUB_FILE` (`"hub.json"`), existing `bundled::{Bundled, BUNDLED_FILE, load}`.
- Produces (Task 2 and the docs rely on these paths, relative to the data dir):
  - `bundled.json` (0600): `{"mqtt_user","mqtt_password","db_user","db_password","db_name"}`.
  - `bundled/db-password` (0600): the database password, no trailing newline. Compose mounts `./data/bundled` at `/run/oikos-bundled`.
  - `mosquitto/config/mosquitto.conf` (0644, written only when missing) and `mosquitto/config/passwd` (0600); empty dirs `mosquitto/data` and `mosquitto/log`.
  - `pub fn mosquitto_password_line(user: &str, password: &str, salt: &[u8; 12]) -> String`
  - `pub fn random_secret() -> String` — 24 chars from `[A-Za-z0-9]` (safe in a URL, unbiased: reject bytes ≥ 248).
  - `pub enum InitOutcome { Created, Kept, HubConfigured, DatabaseWithoutCredentials }` and `pub fn init_bundled(dir: &Path) -> anyhow::Result<InitOutcome>`
  - `Cli::InitBundled { data_dir: PathBuf }`, parsed from `oikos-os init-bundled` (honours `--data` / `OIKOS_DATA_DIR`).

Decision order in `init_bundled`:
1. `bundled.json` exists → `Kept`, touch nothing.
2. `hub.json` or `oikos.toml` exists in `dir` → `HubConfigured`, touch nothing (an existing hub; its broker and database are its own, as on a host using the migration guide's override).
3. `timescaledb/PG_VERSION` exists → `DatabaseWithoutCredentials`, touch nothing (the database was created with a password nobody can recover; `main` exits 1 with: `data/timescaledb already holds a database but data/bundled.json is gone; restore bundled.json or delete data/timescaledb to start over`).
4. Otherwise generate both passwords, write `mosquitto.conf` if missing, then `passwd`, `bundled/db-password`, create the empty dirs, and write `bundled.json` **last** (it is the commit point) → `Created`.

`main` prints one line per outcome on stdout and exits 0 for `Created` (`bundled credentials created in <dir>/bundled.json`), `Kept` (`bundled credentials already exist; nothing changed`) and `HubConfigured` (`this hub is already set up; no bundled credentials created`). It initialises logging with `LogConfig::default()` only.

`assets/mosquitto.conf` (embedded with `include_str!`):

```
# Written once by `oikos-os init-bundled`; edit freely, it is never overwritten.
persistence true
persistence_location /mosquitto/data/
log_dest stdout
listener 1883
allow_anonymous false
password_file /mosquitto/config/passwd
```

- [ ] **Step 1: Write the failing tests** in `bundled.rs` (`#[cfg(test)]`, `tempfile` dirs):
  - `password_line_matches_mosquitto_passwd`: salt = base64-decode `8ByG83gjOOd8TqHW`; `mosquitto_password_line("oikos", "correct horse", &salt)` == `oikos:$7$101$8ByG83gjOOd8TqHW$pYBnuy3hWyF7SI1EMLZE3/NTahEnAorkGhzthwFEyTulx8mn9VkqQdIM/2/6a0mrHH7T5QDY+SgGWTmJr899ZA==` (vector from `mosquitto_passwd` 2.0.22).
  - `random_secret_is_24_url_safe_chars`: length 24, all ASCII alphanumeric, two calls differ.
  - `creates_everything_in_an_empty_dir`: outcome `Created`; `load(dir)` gives `mqtt_user == "oikos"`, `db_user == "oikos"`, `db_name == "oikos"`; `bundled/db-password` == `db_password`; `passwd` line starts with `oikos:$7$101$` and recomputing with its salt and `mqtt_password` gives the same line; `mosquitto.conf` contains `allow_anonymous false`; `mosquitto/data` and `mosquitto/log` are dirs; on unix `bundled.json`, `db-password` and `passwd` have mode `0o600`.
  - `second_run_changes_nothing`: run twice; second outcome `Kept`; `bundled.json` bytes identical.
  - `init_skips_a_configured_hub`: dir holding `hub.json` → `HubConfigured`, no `bundled.json`, no `mosquitto/`; same with `oikos.toml` instead.
  - `refuses_when_the_database_exists_without_credentials`: `timescaledb/PG_VERSION` present → `DatabaseWithoutCredentials`, no `bundled.json`.
  - `regenerates_after_a_partial_run`: stale `mosquitto/config/passwd` and `bundled/db-password` but no `bundled.json` → `Created`, both files now match the new `bundled.json`.
  - `keeps_an_edited_mosquitto_conf`: existing `mosquitto.conf` with `# mine` → still `# mine` after `Created`.
- In `cli.rs` tests: `init_bundled_takes_data`: `["oikos-os", "init-bundled"]` → `Cli::InitBundled { data_dir: "./data" }`; `["oikos-os", "--data", "/data", "init-bundled"]` → `/data`.
- In `tests/init_bundled.rs`: `cli_creates_then_keeps`: run `oikos-os --data <tmp> init-bundled` twice with `assert_cmd`; both succeed; stdout contains `created` then `nothing changed`. `cli_fails_on_orphan_database`: `PG_VERSION` present → exit code 1, stderr or stdout mentions `bundled.json is gone`.

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=$HOME/dev-scratch/cargo-target-wizard CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p oikos-os --lib setup::bundled cli` then `… cargo test -p oikos-os --test init_bundled`
Expected: compile errors (functions missing).

- [ ] **Step 3: Implement** the interfaces above. `Bundled` gains `Serialize`. Use `pbkdf2::pbkdf2_hmac::<sha2::Sha512>`, `base64::engine::general_purpose::STANDARD`, `getrandom::getrandom` for salts and passwords. Add the `InitBundled` variant to `RawCommand` (doc comment: "Create the bundled broker and database credentials, then exit.") and to `Cli`, and its arm in `main`.

- [ ] **Step 4: Run the tests, fmt and clippy**

Run: the two test commands above, then `… cargo fmt --all --check` and `… cargo clippy -p oikos-os --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/oikos-os/assets/mosquitto.conf crates/oikos-os/src/setup/bundled.rs crates/oikos-os/src/cli.rs crates/oikos-os/src/main.rs crates/oikos-os/Cargo.toml Cargo.lock crates/oikos-os/tests/init_bundled.rs
git commit -m "feat(hub): init-bundled writes the bundled broker and database credentials"
```

### Task 2: compose stack, image command and CI stack check

**Files:**
- Modify: `compose.yml`, `Dockerfile` (last line), `.dockerignore`, `.github/workflows/ci.yml`
- Create: `tools/ci/stack-check.sh`

**Interfaces:**
- Consumes: Task 1's files and the `init-bundled` subcommand.
- Produces: services `init`, `mosquitto`, `timescaledb`, `hub` (Task 3 starts `hub` alone or all four; the docs name them).

`compose.yml`:
- A YAML anchor for the build (`context: .`, `dockerfile: Dockerfile`) used by both `init` and `hub`, both `image: oikos-os:local` (so a fresh `docker compose up` builds instead of pulling it).
- `init`: `command: ["init-bundled"]`, `restart: "no"`, volume `./data:/data`, no network needed.
- `mosquitto`: `image: eclipse-mosquitto:2.0.22`, `container_name: oikos-mosquitto`, `restart: unless-stopped`, `network_mode: host`, `depends_on: {init: {condition: service_completed_successfully}}`, volumes `./data/mosquitto/config:/mosquitto/config`, `./data/mosquitto/data:/mosquitto/data`, `./data/mosquitto/log:/mosquitto/log`.
- `timescaledb`: `image: timescale/timescaledb:2.30.2-pg17`, `container_name: oikos-timescaledb`, `restart: unless-stopped`, `network_mode: host`, same `depends_on`, `environment: POSTGRES_USER: oikos, POSTGRES_DB: oikos`, volumes `./data/timescaledb:/var/lib/postgresql/data` and `./data/bundled:/run/oikos-bundled:ro`, and `command: ["postgres"]` with this entrypoint, so a missing password file (a configured hub, Review Focus 2) never reaches the image's `file_env`, and an override's `command: postgres -c port=5433` still passes through:

```yaml
    entrypoint:
      - sh
      - -c
      - |
        [ -f /run/oikos-bundled/db-password ] && export POSTGRES_PASSWORD_FILE=/run/oikos-bundled/db-password
        exec docker-entrypoint.sh "$$0" "$$@"
```

- `hub`: unchanged except: no `depends_on`, the `oikos.toml` and `secrets/` mounts are gone (only `./data:/data`), and the header comment explains the four services, `docker compose up -d hub` for people with their own broker and database, and points at `docs/users/install-on-linux.md`. Drop every mention of `oikos.example.toml`.
- `Dockerfile`: `CMD ["run"]` (the hub imports `/data/oikos.toml` by itself).
- `.dockerignore`: add `data/`; drop `oikos.toml` and `secrets/` only if nothing else needs them excluded (keep them excluded: they hold secrets).

`tools/ci/stack-check.sh` (bash, `set -euo pipefail`, run from the repo root on a Linux runner with Docker) proves the chain with the real images:
1. `docker compose config -q`; assert with `docker compose config --format json | jq` that `.services.hub.depends_on` is null and `init`, `mosquitto`, `timescaledb` exist.
2. Build the binary natively (`cargo build -p oikos-os`), run `target/debug/oikos-os --data ./data init-bundled`. (Don't build the image: the Dockerfile build takes too long for CI.)
3. Write `compose.override.yml` with `services: {timescaledb: {command: postgres -c port=5433}}` (the migration guide's override shape) and `docker compose up -d --no-deps mosquitto timescaledb` (`--no-deps`: the init already ran natively, and starting it would build the hub image).
4. Read the passwords from `data/bundled.json` with `jq`. Wait up to 60 s for `pg_isready -h localhost -p 5433` inside `oikos-timescaledb`.
5. `mosquitto_pub -h localhost -u oikos -P <mqtt> -t oikos/ci -m ok` inside `oikos-mosquitto` succeeds; the same with `-P wrong` fails.
6. `psql "postgresql://oikos:<db>@localhost:5433/oikos" -tAc "select extname from pg_extension where extname='timescaledb'"` inside `oikos-timescaledb` prints `timescaledb`.
7. On exit (trap): `docker compose logs mosquitto timescaledb` on failure, then `docker compose down -v`, `rm -f compose.override.yml`.

`ci.yml`: new job `stack` (`name: bundled stack`, ubuntu-latest, checkout, rust-toolchain stable, rust-cache, `run: tools/ci/stack-check.sh`).

- [ ] **Step 1: Write `tools/ci/stack-check.sh`** as above (`chmod +x`); `shellcheck tools/ci/stack-check.sh` clean.
- [ ] **Step 2: Rewrite `compose.yml`**, the Dockerfile `CMD`, `.dockerignore`, and add the CI job.
- [ ] **Step 3: Validate locally** (no Docker daemon or compose plugin on the dev Mac): `uv run --with pyyaml python -I -c "import yaml,sys; d=yaml.safe_load(open('compose.yml')); s=d['services']; assert set(s)=={'init','mosquitto','timescaledb','hub'}; assert 'depends_on' not in s['hub']; print('ok')"` → `ok`; same check for `.github/workflows/ci.yml` parsing. The real proof is the `stack` job on the PR.
- [ ] **Step 4: Commit**

```bash
git add compose.yml Dockerfile .dockerignore .github/workflows/ci.yml tools/ci/stack-check.sh
git commit -m "feat(deploy): compose runs init, Mosquitto and TimescaleDB next to the hub"
```

### Task 3: `tools/update.sh` and `tools/restart.sh`

**Files:**
- Modify: `tools/update.sh`, `tools/restart.sh` (only the `BIND` lookup), `.github/workflows/ci.yml` (one step in the `stack` job)
- Create: `tools/tests/update_test.sh`

**Interfaces:**
- Consumes: Task 2's service names; plan 1's `data/hub.json` (`"http": {"bind": …}`).
- Produces: `tools/update.sh [--hub-only|--all]`.

`update.sh` behaviour, in order:
1. Whole body inside `main() { …; }` with `main "$@"` as the last line, so bash has read all of it before `git pull` rewrites the file.
2. Unless the first argument is `--pulled`: `git pull --rebase`, then `exec "$ROOT/tools/update.sh" --pulled "$@"` (the new version runs the rest).
3. `./oikos.toml` → `./data/oikos.toml`, unless the target exists (then keep both and say `data/oikos.toml already exists; ./oikos.toml left in place`). Each file in `./secrets/` except `README.md` → `./data/secrets/<name>`, never overwriting. Print one `moved …` line per move.
4. If `data/oikos.toml` contains `/etc/oikos/secrets`, warn: `data/oikos.toml names key files under /etc/oikos/secrets; change them to /data/secrets/… or delete those lines (the defaults find data/secrets/)`.
5. Choose: `--hub-only` / `--all` win; else full stack when `data/bundled.json` or `compose.override.yml` exists; else hub-only when `data/oikos.toml` or `data/hub.json` exists; else full stack. Print which and why (`own broker and database: rebuilding the hub only`).
6. `docker compose up -d --build` or `docker compose up -d --build hub`, then `docker compose logs --tail 20 hub`.

`restart.sh`: read `BIND` from `data/hub.json` (`sed -n 's/.*"bind": *"\([^"]*\)".*/\1/p'`), falling back to `./oikos.toml` as today, then `127.0.0.1:8081`.

`tools/tests/update_test.sh`: plain bash, no framework. Each test copies `tools/update.sh` into a fresh temp repo layout, puts stub `git` and `docker` scripts first on `PATH` that append their arguments to a log, runs the script, and asserts on files and the log. Tests:
- `test_moves_legacy_files`: `oikos.toml`, `secrets/firebase.json`, `secrets/solcast.key`, `secrets/README.md` → the three keys are in `data/`, `secrets/README.md` stays, docker log is `compose up -d --build hub`.
- `test_never_overwrites_data`: `data/oikos.toml` and `data/secrets/firebase.json` exist with other content → both unchanged, the root files still there.
- `test_bundled_runs_everything`: `data/bundled.json` → `compose up -d --build` without `hub`.
- `test_override_runs_everything`: `compose.override.yml` + `data/hub.json`, no bundled → full stack.
- `test_flags_win`: `--all` with `data/hub.json` → full; `--hub-only` with `data/bundled.json` → hub.
- `test_warns_about_old_key_paths`: `oikos.toml` naming `/etc/oikos/secrets/firebase.json` → output contains `/etc/oikos/secrets`.
- `test_reexecs_after_pull`: the stub `git` overwrites the temp `tools/update.sh` with a script that prints `NEW VERSION` → output contains `NEW VERSION` and the log shows `pull --rebase` once.

The script exits non-zero on the first failing test and prints `all update.sh tests passed` at the end. CI: add `- run: tools/tests/update_test.sh` and `- run: shellcheck tools/*.sh tools/ci/*.sh tools/tests/*.sh` to the `stack` job.

- [ ] **Step 1: Write `tools/tests/update_test.sh`** (`chmod +x`).
- [ ] **Step 2: Run it**: `tools/tests/update_test.sh` → fails (old script never moves anything).
- [ ] **Step 3: Rewrite `tools/update.sh`** and the `restart.sh` lookup.
- [ ] **Step 4: Run** `tools/tests/update_test.sh` → `all update.sh tests passed`; `shellcheck tools/*.sh tools/ci/*.sh tools/tests/*.sh` → clean (macOS bash 3.2 is the floor: no `mapfile`, no `${arr[@]}` on possibly empty arrays under `set -u`).
- [ ] **Step 5: Commit**

```bash
git add tools/update.sh tools/restart.sh tools/tests/update_test.sh .github/workflows/ci.yml
git commit -m "feat(deploy): update.sh moves oikos.toml and secrets into data and picks hub or stack"
```

### Task 4: install, getting started and the move guide

**Files:**
- Modify: `docs/users/install-on-linux.md`, `docs/users/getting-started.md`, `README.md`, `secrets/README.md`, `docs/contributors/development.md`
- Create: `docs/users/move-into-the-stack.md`

**Interfaces:**
- Consumes: Task 2's services and paths, Task 3's `update.sh` behaviour, Task 1's outcomes and messages. Wizard steps and Settings page names come from plan 2: read them from `git show origin/feat/onboarding-wizard:crates/oikos-web/src/i18n/…` (en strings) rather than guessing.

Content (spec "Docs" and "Install and compose"):
- `install-on-linux.md`: prerequisites (Docker with the compose plugin; Zigbee2MQTT runs elsewhere or on the same host); install = `git clone`, `cd`, `docker compose up -d --build`, then `docker compose logs hub` for the `Open http://<ip>:8081 to set up your home` line; what the four services are and what lands in `./data/` (`bundled.json` holds the generated passwords, mode 0600; back up `./data/`); pointing Zigbee2MQTT at the bundled broker (user `oikos`, password from `data/bundled.json`, or a second user added with `mosquitto_passwd` inside `oikos-mosquitto`); **Bring your own broker and database** (`docker compose up -d --build hub`, the port-clash note for 1883 and 5432, the wizard takes your addresses); **Upgrading** (`tools/update.sh`, what it decides); **Upgrading from `oikos.toml`** (what `update.sh` moves, the two manual `mv` lines, the `/etc/oikos/secrets` path note, the import log line `oikos.toml imported into the database; it is no longer read and can be deleted`, and that own-broker installs keep `docker compose up -d --build hub`); logs and status commands. Keep the existing firewall and push sections that still apply.
- `getting-started.md`: install in one command, then the wizard (Welcome → MQTT → Database → Devices → Services → Done), "Run setup again" in Settings.
- `move-into-the-stack.md` ("Move an existing broker and database into the stack"): the generic version of the migration guide: note the five facts (tags, folders, ports, how the old containers start), check `persistence true`, back up (`pg_dump`, config tarball), the `compose.override.yml` (same tags, existing folders on the four mount targets, `command: postgres -c port=<port>` when it isn't 5432, no password variables), the switch (stop hub, stop old containers with long timeouts and `--restart=no`, cold copies, `docker compose up -d`), checks, rollback. Explain that with `data/hub.json` present `init` creates no bundled credentials. No dates, sizes or anything from one particular house.
- `README.md`: the install line becomes `docker compose up -d --build`, then open the URL from the log.
- `secrets/README.md`: this folder is the old location; `tools/update.sh` moves its keys into `data/secrets/`; new installs upload keys in the wizard.
- `development.md`: running from source is `cargo run -p oikos-os` (data in `./data`, wizard at `http://localhost:8081/setup`); `--data <dir>`; local broker and database through `docker compose up -d mosquitto timescaledb`; drop `oikos.example.toml`.

- [ ] **Step 1: Write the pages.**
- [ ] **Step 2: Build the site**: `cd website && npm ci && npm run build` → succeeds (broken links throw). Add the new page to the sidebar only if `website/sidebars.*` lists pages explicitly.
- [ ] **Step 3: Commit**

```bash
git add docs/users/install-on-linux.md docs/users/getting-started.md docs/users/move-into-the-stack.md README.md secrets/README.md docs/contributors/development.md
git commit -m "docs: install with docker compose up and the setup wizard"
```

### Task 5: feature pages, configuration reference, and `oikos.example.toml` removed

**Files:**
- Modify: `docs/users/{weather,solar-forecast,cameras,push-notifications,climate,history,watchdogs,flows,security,web-ui,assistants-mcp}.md` where they show TOML, `docs/reference/configuration.md`, `docs/reference/http-api.md`, `crates/oikos-os/tests/config.rs`
- Move: `oikos.example.toml` → `crates/oikos-os/tests/fixtures/legacy-oikos.toml` (`git mv`)

**Interfaces:**
- Consumes: plan 1's sections (`crates/oikos-os/src/setup/sections.rs`), `hub.json` fields (`setup/hub_file.rs`), routes (`setup/routes.rs`, `http/` router), MCP `get_settings`/`set_settings` (already in `mcp-tools.md`); plan 2's Settings page names (as in Task 4).

Content:
- Each feature page: replace its `[section]` TOML block with "Set it up in **Settings → <page>** (or the Services step of the setup wizard)", list the fields as the form shows them, and keep any facts that still hold (API budgets, label lists). Low-level knobs with no screen (topic prefixes, timeouts, `runs_kept`, push retry, watchdogs, `send_ha_birth`) are changed through `PUT /api/v1/settings/advanced` or the `set_settings` MCP tool: say so on `watchdogs.md` and in the reference.
- `configuration.md` becomes "Settings reference": where settings live (`data/hub.json`, the `settings` table, `data/settings-cache.json`, `data/bundled.json`), each section and its fields with defaults (read them from `sections.rs` and `config.rs`, not from memory), secrets are write-only (`{"set": true}`), the `OIKOS_*` variables that still apply (HTTP bind, log level, MQTT, database URL) and `--data` / `OIKOS_DATA_DIR`, and the one-time import of an old `oikos.toml`.
- `http-api.md`: add the setup and settings routes if they are missing (`GET /api/v1/setup`, `POST /api/v1/setup/test/{mqtt,database,solcast,frigate}`, `POST /api/v1/setup/finish`, `GET`/`PUT /api/v1/settings/{section}`, plus any other route plan 1 added: check the router), with status codes from the code.
- `tests/config.rs`: `example_config_loads` reads `tests/fixtures/legacy-oikos.toml` and is renamed `legacy_config_still_loads`; its header comment in the fixture says it is an old-format file kept to test the import.
- `grep -rn "oikos.example.toml\|/etc/oikos" --exclude-dir=superpowers --exclude-dir=node_modules --exclude-dir=build .` returns nothing outside the fixture and the upgrade notes.

- [ ] **Step 1: Move the example file and fix the test**; run `CARGO_TARGET_DIR=$HOME/dev-scratch/cargo-target-wizard CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p oikos-os --test config` → passes.
- [ ] **Step 2: Rewrite the pages.**
- [ ] **Step 3: Check**: the grep above is clean; `cd website && npm run build` succeeds.
- [ ] **Step 4: Commit**

```bash
git add -u docs crates/oikos-os/tests/config.rs oikos.example.toml
git add crates/oikos-os/tests/fixtures/legacy-oikos.toml
git commit -m "docs: settings pages replace TOML sections; oikos.example.toml becomes a test fixture"
```

### Task 6: whole-branch check, PR and CI (controller)

- [ ] **Step 1:** `df -h ~`, then `… cargo fmt --all --check`, `… cargo clippy --workspace --all-targets -- -D warnings`, `… cargo test --workspace` (per crate if under 5 GB free), `tools/tests/update_test.sh`, `shellcheck tools/*.sh tools/ci/*.sh tools/tests/*.sh`.
- [ ] **Step 2:** Final whole-branch review against the spec sections and the Review Focus list; fix wave if needed.
- [ ] **Step 3:** Push `feat/onboarding-bundled`, open the PR against `main`, watch CI (`lint`, `test`, `web`, `stack`) until green; fix on the branch if not.
- [ ] **Step 4:** Delete `~/dev-scratch/cargo-target-wizard`.
