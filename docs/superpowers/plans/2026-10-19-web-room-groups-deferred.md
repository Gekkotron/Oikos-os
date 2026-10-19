# Web room groups — deferred findings

Status: written 2026-10-19 after the implementation of `2026-10-19-web-room-groups.md` (commits 87bd79c..1985dfd). Everything here is known and left for a later wave.

## Not built on purpose

- **Group "Retry" merge of failures** (app spec §5): when several members of a group fail, the app merges them into one retry action. The web has no merge; each member reports its own failure.
- **Group tiles outside Home**: the group cards exist only on Home. Energy, Security, the entity list and the search views do not fold entities into groups.
- **Search and hidden views**: the web has no search and no "hidden" view, so there is nowhere a group could be found or restored from the app's search and hidden lists.

## Needs a follow-up in the hub or MCP

- **MCP tool for group names and favourites**: `set_areas` keeps group fields the caller leaves out, but there is no tool to set `groupNames` or `groupFavorites`. The REST route `PATCH /api/v1/areas/{id}/groups/{kind}` is the only writer.
- **`set_areas` cannot clear a field**: an empty value means "keep", so an icon or group field cannot be cleared over MCP. A `null` sentinel, as in `set_override`, would fix it.

## Missing browser tests (`crates/oikos-web/tests`)

- The group sheet has no browser tests for: the "group is gone" render (the group was ungrouped elsewhere while the sheet was open), a blank name saved as `null`, the ungroup flow, and the member checkbox restore after a failed ungroup.
- The light group toggle, the "Opening…" text and the pending dot on the cards are not covered by tests.
- `tests/sheet.rs` `state_echo_keeps_the_override_draft` fails because the test lacks a `ReturnPath` context. This comes from commit 0432365, not from the room groups work.

## Behaviour gaps

- **"Ungroup all" stops at the first failure**: the members after the failed one are not tried, and the sheet does not say so.
- **Stale group name**: the group sheet's name field is seeded once when it opens, so it goes stale if the name changes elsewhere while the sheet is open.
- **Favourite switch has no in-flight guard**: a second tap while the first request is pending sends another request.
- **No pending feedback on the group sheet**: its controls do not show that a command or a save is in flight.

## Performance

- Each group card's members memo is O(groups × entities) per state echo. Not measured; index the entities by id if it shows up in a profile.
