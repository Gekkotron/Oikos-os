# secrets/

Keys the hub reads at start. Git and the Docker build ignore everything in
this folder except this file; `compose.yml` mounts it read-only at
`/etc/oikos/secrets`.

- `firebase.json`: the Firebase service-account key. With it the hub sends
  push notifications; without it it keeps the alert history only. See
  `docs/users/push-notifications.md`.
- `solcast.key`: the Solcast API key, one line. See `docs/users/solar-forecast.md`.
