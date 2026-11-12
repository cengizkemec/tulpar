# Phase 3 Report — Real-Time Messaging (first slice)

**Status:** In progress. Message persistence, message APIs, reactions, pins, read states, search, and DM messages are implemented and tested. WebSocket delivery, presence, typing, attachments, notifications, sync, and outbox dispatch remain open.

**Date:** 2026-11-12

---

## 1. Implemented

### Database

Migration `0003_messaging.sql` adds:

- `messages` with UUIDv7 IDs, soft deletion, reply targets, client nonces, edit timestamps, and a generated full-text search vector.
- `message_reactions` with one reaction per user/emoji/message.
- `message_mentions` for user, role, and `@everyone` targets.
- `pinned_messages` with a service-enforced 50-pin limit.
- `read_states` with a message-ID cursor and computed unread counts.

Indexes cover channel history, authored messages, search, replies, deletion cleanup, nonce idempotency, reactions, mentions, pins, and read-state lookups.

### Messaging service

- Idempotent sends through `(channel_id, author_id, client_nonce)`.
- Same-channel reply validation.
- Newest-first keyset pagination with `before` and `after`.
- Author edits with `edited_at`.
- Soft deletion that clears content, records the deleter, and repairs the channel last-message projection.
- Reaction add/remove/list.
- Read-state upsert and unread count.
- Pin/unpin/list with a cap.
- Permission-scoped full-text search.

### API

- `POST/GET /api/v1/channels/{id}/messages`
- `GET /api/v1/channels/{id}/messages/search`
- `GET/PATCH/DELETE /api/v1/messages/{id}`
- `GET/PUT/DELETE /api/v1/messages/{id}/reactions/...`
- `GET/PUT /api/v1/channels/{id}/read-state`
- `GET/PUT/DELETE /api/v1/channels/{id}/pins/...`
- Community channels reuse the centralized permission evaluator: `SEND_MESSAGES`, `READ_MESSAGE_HISTORY`, `ADD_REACTIONS`, `PIN_MESSAGES`, `MANAGE_MESSAGES`, `BYPASS_SLOWMODE`, and `VIEW_CHANNEL`.
- Direct-message channels are participant-only and bypass community permission checks.
- Slowmode returns `429` with `Retry-After`.
- Moderator deletes and pin changes write audit records.

OpenAPI now contains 64 paths; TypeScript types were regenerated and typecheck.

---

## 2. Verification

```text
cargo fmt --all -- --check                         -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace                             -> 28 tests passed
pnpm generate:types                                -> pass
pnpm typecheck                                     -> pass
```

The messaging integration test covers:

1. Two community members and one outsider register and log in.
2. A member sends a message with a client nonce; replaying the nonce returns the same message.
3. Keyset listing returns the message once.
4. A reply is created and linked.
5. The author edits the message; a non-author edit is rejected with `403`.
6. A reaction is added, listed, and removed.
7. A moderator pins the message; the pin list contains it.
8. A read cursor clears unread count; a newer message produces unread count 1.
9. Full-text search finds the expected message.
10. An outsider cannot read community messages (`404`).
11. Slowmode rejects a second member message with `429`.
12. Timeout removes `SEND_MESSAGES` and rejects the send with `403`.
13. Moderator deletion produces a tombstone and removes the message from lists.
14. A friend DM accepts and lists messages; an outsider gets `404`.

---

## 3. Known gaps

1. **WebSocket gateway** delivery, tickets, heartbeat, sequencing, filtering, backpressure, and reconnect/resync are not implemented.
2. **Presence and typing** are not implemented.
3. **Attachments and file upload** are not implemented; `message_mentions` is written by schema but mention parsing is not implemented.
4. **Notifications and preferences** are not implemented.
5. **`/sync` catch-up and event contracts** are not implemented.
6. **Outbox dispatch and idempotency integration** are not implemented.
7. **Slowmode is checked per author in the API, not in the gateway.** The gateway must reuse the same rule.
8. **Search uses the `simple` text-search configuration.** Ranking, highlighting, and channel-scoped global search are later work.
9. **Messages are not yet included in account deletion/export policy.** The database anonymizes authors on account deletion, but content purge policy is not implemented.
10. **No messaging rate limit** beyond slowmode and auth rate limits.
11. **No attachment virus scanning, quota enforcement, or storage provider implementation.**

---

## 4. Recommended next steps

1. Build the WebSocket gateway with authenticated tickets, heartbeat, sequence numbers, and slow-consumer disconnection.
2. Add presence, typing, and read-state fan-out over the gateway.
3. Implement `/sync` recovery with `after` cursors and no duplicates.
4. Add attachment upload, validation, quota enforcement, and message attachment DTOs.
5. Add mention parsing and permission checks for role/`@everyone` mentions.
6. Add notifications and user preferences.
7. Add messaging rate limits and gateway-side slowmode enforcement.
8. Add message search ranking and pagination.
