# Phase 3 Report — Real-Time Messaging

**Status:** Complete for the v1 backend scope. Messaging persistence, WebSocket delivery, presence, typing, sync recovery, attachments, mentions, notifications, and the outbox dispatcher are implemented, tested, and CI-green.

**Date:** 2026-11-12

---

## 1. Implemented

### Database

- `0003_messaging.sql`: messages, reactions, mentions, pins, read states, full-text search, and idempotency indexes.
- `0004_attachments_notifications.sql`: attachments, notifications, notification preferences, and community/channel notification settings.

### Messaging

- Idempotent sends through `(channel_id, author_id, client_nonce)`.
- Text or attachment-only messages, with at most 10 attachments per message.
- Same-channel reply validation, keyset pagination (`before` / `after`), edits, tombstones, and channel last-message repair.
- Reactions, pins with a 50-pin cap, read states, unread counts, and channel-scoped full-text search.
- Mention syntax: `<@user_id>`, `<@&role_id>`, and `@everyone`, with membership, `MENTION_ROLES`, and `MENTION_EVERYONE` checks.

### WebSocket gateway

- `POST /api/v1/gateway/ticket` issues single-use, 60-second tickets bound to the user and session.
- Tickets are hashed in Redis when available, with an in-process single-node fallback.
- `GET /gateway` implements the documented HELLO → IDENTIFY → READY flow, heartbeat ACK, dispatch envelopes, per-connection sequence numbers, and event IDs.
- Message create/update/delete, reaction add/remove, pin updates, read-state updates, typing, presence, and notification events are published.
- Every channel-scoped event is authorized against current permissions per recipient, so channel overwrite changes take effect on existing sockets.
- Presence aggregates multiple sessions (`dnd` > `online` > `idle`), hides `invisible` from other users, respects blocks and `presence_visibility`, and disappears on disconnect.
- Typing requires `SEND_MESSAGES`, is blocked across blocked pairs, and is limited to one signal per user/channel per 5 seconds.
- Slow consumers receive `RESYNC_REQUIRED` and close code `4008`; broadcast lag is covered by a unit test.
- `GET /api/v1/sync` returns the bootstrap snapshot: user, settings, communities, roles, visible channels, read states, unread counts, friends, requests, blocks, DMs, notification count, and gateway metadata.
- `GET /api/v1/sync/instance` exposes public instance metadata.

### Attachments

- Local storage provider with an explicit content-type allowlist, filename sanitization, upload size limit, path traversal protection, and SHA-256 hashing.
- `POST /api/v1/channels/{id}/attachments` multipart upload with `SEND_MESSAGES` + `ATTACH_FILES` checks.
- Attachment metadata is persisted in `files` and `attachments`; message sends bind pre-uploaded files transactionally.
- `GET /api/v1/messages/{id}/attachments/{attachment_id}` streams bytes with permission checks, `nosniff`, and private cache headers.
- Docker development stack uses a persistent uploads volume and a root entrypoint that fixes volume ownership before dropping to the `tulpar` user.

### Notifications and outbox

- Notifications are created for direct user mentions unless the target has muted the channel/community or disabled notifications.
- `GET /api/v1/notifications`, `POST /api/v1/notifications/read`, and `GET/PUT /api/v1/users/me/notification-preferences`.
- Notification creation writes a `notification.created` outbox event in the same transaction.
- The server outbox worker dispatches notification events to the gateway and marks them processed; failures retry with a delay.
- `NOTIFICATION_CREATE` is delivered only to the target user's connections.

### API

- Message, reaction, pin, read-state, search, attachment, notification, sync, and gateway-ticket endpoints.
- OpenAPI now contains 73 paths; TypeScript types were regenerated and typecheck.

---

## 2. Verification

```text
cargo fmt --all -- --check                         -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace                             -> 37 tests passed
pnpm generate:types                                -> pass
pnpm typecheck                                     -> pass
```

Test breakdown:

| Suite | Result |
|---|---|
| API attachments/notifications/outbox integration | 1 passed |
| API auth integration | 4 passed |
| API communities/social integration | 3 passed |
| API gateway integration (real WebSocket) | 1 passed |
| API messaging integration | 1 passed |
| Auth password unit | 3 passed |
| Common utilities | 4 passed |
| Config validation | 4 passed |
| Gateway tickets, presence, slow-consumer unit | 4 passed |
| Permission evaluator | 7 passed |
| Realtime rate limiter | 2 passed |
| Storage validation | 3 passed |

The gateway integration test covers real HELLO/IDENTIFY/READY, heartbeat ACK, message dispatch, typing, presence, channel overwrite enforcement on an existing socket, `/sync`, and single-use tickets.

The attachments/notifications test covers upload validation, attachment-only messages, download bytes, cross-community denial, mention notifications, outbox dispatch, retry scheduling, preferences, mark-read, and sync unread counts.

### Docker verification

The rebuilt Docker stack returned:

- healthy database and Redis checks;
- `200` for community creation, message send, `/sync`, gateway ticket, attachment upload, and attachment-only message creation;
- a real WebSocket READY and heartbeat ACK against `ws://localhost:18080/gateway`;
- 73 OpenAPI paths.

---

## 3. Known gaps

1. **Multi-node gateway fan-out is not implemented.** The hub is process-local; horizontal scaling needs Redis pub/sub or a message bus while keeping REST as the source of truth.
2. **Role and `@everyone` mentions are stored but do not fan out notifications.** Per-user mention notifications are implemented; role/`@everyone` delivery policy is later work.
3. **Notification settings APIs for per-community and per-channel defaults are not exposed.** The tables and mute enforcement exist; the REST surface is future work.
4. **Attachments use local disk only.** S3/MinIO providers, virus scanning, image variants, and orphan cleanup are later work.
5. **Gateway event authorization performs database checks per socket per event.** Correct but not yet optimized with cached visibility sets.
6. **Presence is process-local.** Multi-node presence aggregation needs Redis.
7. **No gateway load test yet.** Backpressure/lag behavior is unit-tested; a multi-client load harness is future work.
8. **Outbox processing is single-process.** `SKIP LOCKED` claiming and per-topic consumers are required for horizontal workers.
9. **Message search uses the `simple` text-search configuration.** Ranking, highlighting, and cross-channel search are later work.
10. **No web/mobile client consumes the gateway yet.** The protocol is API-level verified.

---

## 4. Recommended next steps

1. Add Redis pub/sub fan-out and shared presence/ticket state for multi-node gateways.
2. Add gateway load tests with slow-consumer and reconnect scenarios.
3. Add S3/MinIO storage, attachment scanning, quotas, and cleanup workers.
4. Add channel/community notification setting endpoints and role/`@everyone` notification fan-out.
5. Add message search ranking and cross-channel search.
6. Build the web client gateway integration in Phase 6.
7. Start Phase 4 voice infrastructure.
