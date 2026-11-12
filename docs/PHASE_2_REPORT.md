# Phase 2 Report — Communities, Permissions, and Social Foundation

**Status:** In progress. The community/permission/social foundation is implemented and locally verified. Several Phase 2 endpoints and hardening items remain open.

**Date:** 2026-11-12

---

## 1. Implemented

### Permissions

- 64-bit permission bitfield with API string names.
- Centralized evaluator in `tulpar-permissions`.
- Owner bypass, Administrator bypass, role union, category/channel overwrite ordering, member override last, deny-wins behavior, and timeout removal.
- Unit tests for default permissions, administrator bypass, overwrite precedence, timeout behavior, owner permissions, and name round-tripping.

### Database

Migration `0002_communities_and_social.sql` adds:

- `communities`
- `channels` (community and direct scopes)
- `community_members`
- `roles`, `member_roles`
- `invites`, `bans`
- `channel_permission_overwrites`
- `direct_conversations`
- `friendships`, `user_blocks`

The migration was applied to the Docker PostgreSQL instance and is applied by `sqlx::test` in integration tests.

### Communities

- Create community with owner membership, default `@everyone` role, Moderator role, and Administrator role.
- Default category and `#general` channel creation through the API composition layer.
- Community read/update/soft-delete.
- Public self-join scaffolding for public communities.
- Member listing, nickname/timeout updates, kick, ban, unban.
- Role listing, creation, update, deletion, assignment, and removal with hierarchy and anti-escalation checks.
- Invite creation, public preview, join, listing, and revocation.
- Effective community permissions endpoint.

### Social

- Friend requests, accept/decline, remove.
- Friend list and pending request list with direction.
- Block/unblock; blocking removes the friendship.
- Privacy checks for friend requests and DMs (`everyone`, `friends`, `mutual_communities`, `nobody`).
- DM conversation metadata creation with block and privacy enforcement.
- DM listing; message storage/delivery is Phase 3.

### API

New endpoints include:

- `POST /api/v1/communities`
- `GET /api/v1/users/me/communities`
- `GET/PATCH/DELETE /api/v1/communities/{id}`
- `POST /api/v1/communities/{id}/join`
- `GET /api/v1/communities/{id}/permissions/me`
- `GET /api/v1/communities/{id}/members`
- `PATCH/DELETE /api/v1/communities/{id}/members/{user_id}`
- `PUT/DELETE /api/v1/communities/{id}/bans/{user_id}`
- `GET/POST /api/v1/communities/{id}/roles`
- `PATCH/DELETE /api/v1/communities/{id}/roles/{role_id}`
- `PUT/DELETE /api/v1/communities/{id}/members/{user_id}/roles/{role_id}`
- `GET/POST /api/v1/communities/{id}/channels`
- `GET/POST /api/v1/communities/{id}/invites`
- `GET/POST/DELETE /api/v1/invites/{code}`
- Friends, blocks, and DM endpoints under `/api/v1/friends`, `/api/v1/blocks`, `/api/v1/dms`

OpenAPI now contains 46 paths and TypeScript types were regenerated from the running Docker API.

---

## 2. Verification

### Automated

```text
cargo fmt --all -- --check                         -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace                             -> 24 tests passed
pnpm generate:types                                -> pass
pnpm typecheck                                     -> pass
```

Test breakdown:

| Suite | Result |
|---|---|
| API auth integration | 4 passed |
| API communities/social integration | 1 passed |
| Auth password unit | 3 passed |
| Common utilities | 4 passed |
| Config validation | 4 passed |
| Permission evaluator | 6 passed |
| Realtime rate limiter | 2 passed |

The community/social integration test covers:

1. Two users register and log in.
2. Owner creates a private community with default roles/channels.
3. A non-member receives `404` for the private community.
4. Owner creates an invite; member joins.
5. Owner lists two members.
6. Owner creates a role and assigns it; member’s effective permissions include the role.
7. Member sends a friend request; owner accepts.
8. Owner creates a DM with the member.
9. Owner blocks the member; a new DM attempt is rejected with `403`.

### Docker stack

`deployments/development/docker-compose.yml` was built and verified with:

- PostgreSQL 16 (pinned digest)
- Redis 7 (pinned digest)
- LiveKit 1.13.11 (pinned digest)
- Coturn 4.6.3 (pinned digest)
- Tulpar API health check and a real community creation through the Docker API

Docker Desktop cannot read the current OneDrive source path directly. The verified run used a local copy at `%LOCALAPPDATA%\Tulpar\tulpar-docker`. This is an environment limitation, not a Compose error; the repository should be moved off OneDrive for direct builds.

---

## 3. Known gaps

1. **Channel permission overwrite REST endpoints** are not implemented. The evaluator and database table exist; channel-scoped enforcement in API/message/voice paths is pending.
2. **Channel list is not yet permission-filtered.** All community members see all channels until channel overwrites are enforced.
3. **Audit records are not written for community/social actions.** Auth audit records exist; community moderation, role changes, invites, bans, and blocks need audit entries.
4. **Community discovery/search, member search, full ban listing, role reorder, and community audit-log APIs** are not implemented.
5. **DM messages** are Phase 3; only DM conversation metadata exists.
6. **Full authorization matrix and adversarial escalation tests** are not complete. The current integration test covers the main flow, not every endpoint/hierarchy edge case.
7. **Friend request re-request after decline** currently conflicts rather than reopening the request.
8. **Owner account deletion with communities** is intentionally blocked with a clear error; ownership transfer or community deletion must happen first.
9. **Community creation + default channel creation is not one transaction.** If default channel creation fails, the API soft-deletes the community; a database-level unit of work would be cleaner.
10. **No community/member admin UI** exists; web work is Phase 6.
11. **No rate limits** specific to community/invite/friend actions yet; global authenticated limits apply, but abuse controls need per-action budgets.
12. **No channel permission overwrite tests** despite the pure evaluator tests.
13. **Two optional-dependency audit advisories are explicitly ignored.** `rsa` is present because of jsonwebtoken’s optional RSA support (Tulpar uses HS256 only), and `libsqlite3-sys` is present because of SQLx’s optional SQLite support (Tulpar enables PostgreSQL only). The CI comments document both; the lockfile should be cleaned if upstream features allow.

---

## 4. Recommended next steps

1. Add channel permission overwrite endpoints and enforce channel-scoped permissions in channel listing and future message/voice paths.
2. Add audit records for community, role, invite, ban, and social actions.
3. Add community discovery/search, member search, ban listing, role reorder, and audit-log endpoints.
4. Expand the authorization matrix and escalation tests across every new endpoint.
5. Add action-specific rate limits for invites, friend requests, DMs, and community joins.
6. Move DM messages and read states into Phase 3 with the messaging tables, gateway, and recovery.
7. Move the repository off OneDrive or use a local build context for Docker.
8. Obtain the first green GitHub Actions run and close the license decision before any public distribution.
9. Remove the documented audit exceptions by upgrading/replacing the optional RSA/SQLite-bearing dependencies when upstream support allows.
