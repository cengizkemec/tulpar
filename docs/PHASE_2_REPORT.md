# Phase 2 Report â€” Communities, Permissions, and Social Foundation

**Status:** Phase 2 backend scope is implemented, locally verified, and CI-green. Remaining items are Phase 3 messaging, the admin UI, an internal transaction refactor, and operational/license decisions.

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
- Channel permission overwrite endpoints for roles and members, with category inheritance, member-overwrite-last ordering, and channel visibility filtering through the central evaluator.
- Public community discovery/search, member search, ban listing, bulk role reordering, and ownership transfer.
- Append-only audit records for community, member, role, invite, channel-overwrite, friend, block, and DM metadata actions, plus a `VIEW_AUDIT_LOG`-protected audit-log endpoint.
- Per-user rate limits for community creation, joins, invite creation, friend requests, and DM creation.

### Social

- Friend requests, accept/decline, remove.
- Friend list and pending request list with direction.
- Block/unblock; blocking removes the friendship.
- Privacy checks for friend requests and DMs (`everyone`, `friends`, `mutual_communities`, `nobody`).
- DM conversation metadata creation with block and privacy enforcement.
- DM listing; message storage/delivery is Phase 3.

### API

New endpoints include:

- `GET/POST /api/v1/communities` (public discovery and creation)
- `GET /api/v1/users/me/communities`
- `GET/PATCH/DELETE /api/v1/communities/{id}`
- `POST /api/v1/communities/{id}/join`
- `POST /api/v1/communities/{id}/transfer-ownership`
- `GET /api/v1/communities/{id}/permissions/me`
- `GET /api/v1/communities/{id}/members?query=...`
- `PATCH/DELETE /api/v1/communities/{id}/members/{user_id}`
- `GET /api/v1/communities/{id}/bans`
- `PUT/DELETE /api/v1/communities/{id}/bans/{user_id}`
- `GET /api/v1/communities/{id}/audit-logs`
- `GET/POST /api/v1/communities/{id}/roles`
- `PATCH /api/v1/communities/{id}/roles/reorder`
- `PATCH/DELETE /api/v1/communities/{id}/roles/{role_id}`
- `PUT/DELETE /api/v1/communities/{id}/members/{user_id}/roles/{role_id}`
- `GET/POST /api/v1/communities/{id}/channels`
- `GET /api/v1/channels/{id}`
- `GET /api/v1/channels/{id}/permissions/me`
- `GET/PUT/DELETE /api/v1/channels/{id}/permissions/...`
- `GET/POST /api/v1/communities/{id}/invites`
- `GET/POST/DELETE /api/v1/invites/{code}`
- Friends, blocks, and DM endpoints under `/api/v1/friends`, `/api/v1/blocks`, `/api/v1/dms`

OpenAPI now contains 56 paths and TypeScript types were regenerated from the running instance.

---

## 2. Verification

### Automated

```text
cargo fmt --all -- --check                         -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace                             -> 27 tests passed
pnpm generate:types                                -> pass
pnpm typecheck                                     -> pass
GitHub Actions (commit c24433a)                    -> all jobs green
```

Test breakdown:

| Suite | Result |
|---|---|
| API auth integration | 4 passed |
| API communities/social integration | 3 passed |
| Auth password unit | 3 passed |
| Common utilities | 4 passed |
| Config validation | 4 passed |
| Permission evaluator | 7 passed |
| Realtime rate limiter | 2 passed |

The second integration test is an authorization matrix covering private-community IDOR, invite exhaustion, administrator-grant rejection, kick/overwrite hierarchy checks, community/channel management denials, cross-community role IDs, timeout permission removal, ban-blocked rejoin, and ownership transfer in both directions.

The third integration test asserts Phase 2 database constraints: visibility enums, duplicate active slugs, direct-channel scope fields, role color bounds, overlapping overwrite allow/deny, missing overwrite targets, self direct conversations, and self blocks.

The community/social integration test covers:

1. Two users register and log in.
2. Owner creates a private community with default roles/channels.
3. A non-member receives `404` for the private community.
4. Owner creates an invite; member joins.
5. Owner lists two members.
6. Owner creates a role and assigns it; memberâ€™s effective permissions include the role.
7. Owner denies `VIEW_CHANNEL` to the member on `#general`; the channel disappears from the memberâ€™s channel list and direct lookup returns `404`.
8. Owner removes the overwrite; access is restored.
9. Member sends a friend request; owner accepts.
10. Owner creates a DM with the member.
11. Owner blocks the member; a new DM attempt is rejected with `403`.
12. Public discovery returns the public community and hides the private one; member search finds the invited member.
13. Role reordering works; ban listing returns the banned user; audit logs contain the expected moderation and hierarchy actions.

### Docker stack

`deployments/development/docker-compose.yml` was built and verified with:

- PostgreSQL 16 (pinned digest)
- Redis 7 (pinned digest)
- LiveKit 1.13.11 (pinned digest)
- Coturn 4.6.3 (pinned digest)
- Tulpar API health check and a real community creation through the Docker API
- A rebuilt Phase 2 API container exposing 56 OpenAPI paths, public discovery, and a `community.create` audit record

Docker Desktop cannot read the current OneDrive source path directly. The verified run used a local copy at `%LOCALAPPDATA%\Tulpar\tulpar-docker`. This is an environment limitation, not a Compose error; the repository should be moved off OneDrive for direct builds.

---

## 3. Known gaps

1. **DM messages** are Phase 3; only DM conversation metadata and privacy checks exist.
2. **Owner account deletion with communities** is intentionally blocked with a clear error; ownership transfer or community deletion must happen first.
3. **Community creation + default channel creation is not one transaction.** If default channel creation fails, the API soft-deletes the community; a database-level unit of work would be cleaner.
4. **No community/member admin UI** exists; web work is Phase 6.
5. **Two optional-dependency audit advisories are explicitly ignored.** `rsa` is present because of jsonwebtokenâ€™s optional RSA support (Tulpar uses HS256 only), and `libsqlite3-sys` is present because of SQLxâ€™s optional SQLite support (Tulpar enables PostgreSQL only). The CI comments document both; the lockfile should be cleaned if upstream features allow.
6. **Channel scope is enforced for channel lookup/listing/overwrites; future message and voice paths must reuse the same evaluator.**
7. **Community discovery is a relevance-free substring search.** Ranking, categories, and pagination cursors are later work.
8. **Audit records are best-effort writes.** A failed audit insert is logged but does not roll back the action. A transactional outbox is the production-grade follow-up.

---

## 4. Recommended next steps

1. Move DM messages and read states into Phase 3 with the messaging tables, gateway, and recovery.
2. Build the community/member/role admin UI in Phase 6.
3. Refactor community creation and default-channel creation into one transaction.
4. Add ranking/cursor pagination for discovery and audit queries.
5. Move the repository off OneDrive or use a local build context for Docker.
6. Close the license decision before any public distribution.
7. Remove the documented audit exceptions by upgrading/replacing the optional RSA/SQLite-bearing dependencies when upstream support allows.
