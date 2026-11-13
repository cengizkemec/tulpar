# Phase 8 Report — Embed and developer platform

**Status:** Phase 8 complete for the v1 developer scope. Public/authenticated embed widgets, message previews, scoped API keys, owner-managed bots, signed outgoing webhooks, TypeScript and Rust SDKs, and their integration tests are implemented. Richer API-key scopes, more webhook event types, and richer typed SDK models remain follow-ups.

**Date:** 2026-11-13

---

## 1. Implemented

### Data model

- Migration `0006_embeds.sql` adds `embed_apps` and `embed_authorization_codes`.
- Embed apps are scoped to a community, have exact allowed origins, enabled widgets, an authentication flag, a public `emb_...` key, and a token version for revocation.
- Authorization codes are SHA-256 hashed, single-use, origin-bound, and expire after 60 seconds.

### API

- `POST /api/v1/communities/{community_id}/embed-apps` — create an embed app (`MANAGE_COMMUNITY`).
- `GET /api/v1/communities/{community_id}/embed-apps` — list apps (`MANAGE_COMMUNITY`).
- `PATCH /api/v1/embed-apps/{app_id}` — update origins/widgets/flags/revocation.
- `DELETE /api/v1/embed-apps/{app_id}` — revoke and bump the token version.
- `POST /api/v1/embed/authorize` — authenticated member starts the popup authorization flow.
- `POST /api/v1/embed/token` — public token or one-time code exchange.
- `GET /api/v1/embed/community` — widget data using a short-lived embed JWT.
- `POST /api/v1/users/me/api-keys` — create a scoped API key; the plaintext `tk_...` value is returned once.
- `GET /api/v1/users/me/api-keys` — list keys without plaintext.
- `DELETE /api/v1/users/me/api-keys/{key_id}` — revoke a key.
- `POST /api/v1/users/me/bots` — create a bot account owned by the current user.
- `GET /api/v1/users/me/bots` — list owned bots.
- `DELETE /api/v1/users/me/bots/{bot_id}` — disable a bot and revoke its API keys.
- `POST /api/v1/users/me/api-keys` accepts an optional `bot_id` so the owner can issue a scoped bot token.
- API keys support `read` and `write` scopes. `read` allows GET/HEAD on non-sensitive routes; `write` additionally allows posting messages. Auth/session, admin, export, audit, API-key-management, and embed-app-management routes are blocked for API keys.
- `GET /api/v1/communities/{community_id}/webhooks` — list outgoing webhooks.
- `POST /api/v1/communities/{community_id}/webhooks` — create a webhook; the signing secret is returned once.
- `DELETE /api/v1/webhooks/{webhook_id}` — delete a webhook.
- Webhook deliveries are queued in `webhook_deliveries`, signed with HMAC-SHA256, retried with exponential backoff, and revalidated against SSRF rules at delivery time.

### Token and origin rules

- Embed JWTs use the instance JWT secret, issuer, `embed` audience, app/community/origin/widget claims, and a 15-minute lifetime.
- Public tokens are only issued for public communities.
- Authenticated tokens require community membership at issue time and re-check membership on every widget request.
- Origins must be exact `http(s)` origins without paths or credentials; plain HTTP is limited to localhost.
- Revocation and origin/widget changes bump `token_version`, invalidating existing tokens.

### Frontend and SDK

- `/embed/widget` renders a minimal community card, posts `tulpar:ready`/`tulpar:resize` messages, and never exposes the token to the host page.
- `/embed/authorize` is the first-party popup that approves a member token and posts the one-time code back to the widget.
- Public message previews use the configured public text/announcement channel and the `allow_message_preview` flag.
- `/embed/v1/tulpar-embed.js` is the hosted loader: it creates the iframe, validates `postMessage` origins, and auto-mounts `data-tulpar-embed` elements.
- `packages/embed-sdk` provides a typed TypeScript `mount` wrapper.
- `apps/embed-demo` is a separate-origin demo host page.
- Community settings now has an Embeds section to create, inspect, and revoke applications.

### Tests

- `crates/api/tests/embed_flow.rs` covers public token issuance, community data, bad origins, disabled widgets, member authorization, code reuse rejection, revocation, private-community rejection, and message previews.
- `crates/api/tests/api_keys_flow.rs` covers scoped read/write keys, blocked key-management routes, message sends, and revocation.
- `crates/api/tests/bots_flow.rs` covers bot creation, owner listing, scoped bot tokens, and bot deletion revoking tokens.
- `crates/api/tests/webhooks_flow.rs` covers webhook creation, one-time secret return, signed delivery, deletion, and private-address rejection.
- `cargo test -p tulpar-api --test <name> --no-run` passes locally; database-backed tests run in CI with PostgreSQL.

---

## 2. Known gaps

1. REST/WebSocket developer documentation is still the existing OpenAPI/WEBSOCKET docs; no new SDK examples beyond the demo and Rust README.
2. API key scopes are intentionally limited to read and message-send; moderation/channel-management scopes need an explicit policy.
3. Bot accounts are owner-managed and cannot log in with a password; they use scoped API keys only.

---

## 3. Next steps

1. Expand API key scopes for moderation/channel management with an explicit policy.
2. Add more webhook event types and a delivery dashboard.
3. Add richer typed SDK models and WebSocket/gateway support.
4. Add developer-facing examples for bot account and webhook management.
