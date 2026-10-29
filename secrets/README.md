# secrets/

This folder is the old location for the hub's keys and is kept only so upgrades can find them.

- `tools/update.sh` moves every key found here (`firebase.json`, `solcast.key`) into `data/secrets/` and never overwrites a file already there.
- New installs have nothing to put here: upload the push key and the Solcast key in the setup wizard, or under Settings. See `docs/users/push-notifications.md` and `docs/users/solar-forecast.md`.

Git and the Docker build ignore everything in this folder except this file.
