# Phase 8 Report — Embed and developer platform

**Status:** Embed MVP implemented. Public community widgets, one-time authenticated member authorization with a popup flow, public message previews, the embed app management API/UI, the hosted widget page, the SDK loader, a typed TypeScript wrapper, and a demo host page are in place. API keys/bot accounts, outgoing webhooks, and the Rust SDK remain follow-ups.

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
- API keys support `read` and `write` scopes. `read` allows GET/HEAD on non-sensitive routes; `write` additionally allows posting messages. Auth/session, admin, export, audit, API-key-management, and embed-app-management routes are blocked for API keys.

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

- `crates/api/tests/embed_flow.rs` covers public token issuance, community data, bad origins, disabled widgets, member authorization, code reuse rejection, revocation, and private-community rejection.
- `cargo test -p tulpar-api --test embed_flow --no-run` passes locally; the database-backed test runs in CI with PostgreSQL.

---

## 2. Known gaps

1. Bot accounts/dedicated bot profiles are not implemented; API keys are owned by normal user accounts.
2. Outgoing webhooks with signatures, retries, and SSRF protection are not implemented.
3. The Rust SDK is not implemented.
4. REST/WebSocket developer documentation is still the existing OpenAPI/WEBSOCKET docs; no new SDK examples beyond the demo.
5. API key scopes are intentionally limited to read and message-send; moderation/channel-management scopes need an explicit policy.

---

## 3. Next steps

1. Add bot account creation/management and per-bot permissions.
2. Add outgoing webhooks with HMAC signatures, retries, and SSRF checks.
3. Add the Rust SDK and publish versioned SDK packages.
4. Expand API key scopes for moderation/channel management with an explicit policy.
5. Add the Rust SDK and publish versioned SDK packages.
