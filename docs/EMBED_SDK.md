# Tulpar Embed SDK

**Status:** Embed MVP implemented. Public community widgets, one-time authenticated member authorization with a popup flow, public message previews, embed app management, the hosted widget page, the SDK loader, and a TypeScript wrapper are in place. API keys/bot accounts, outgoing webhooks, and the Rust SDK remain follow-ups.

Tulpar Embed lets a third-party website show a small, secure, read-only view of a Tulpar community: a community card, online member count, public channel preview, voice channel preview, and join/login links. Authenticated member widgets are supported through a scoped popup authorization flow.

The embed never receives a general API token, a LiveKit secret, or another user's session.

---

## 1. Goals and non-goals

### 1.1 Goals

- One small JavaScript SDK that mounts an iframe hosted by the Tulpar instance.
- Strict origin allowlisting and short-lived, audience-scoped tokens.
- Public widgets that work without an account.
- Optional authenticated member widgets using a first-party popup login.
- No third-party cookies required for the token flow.
- A working demo site and integration documentation.
- Predictable rate limits and clear error states.

### 1.2 Non-goals for v1

- Full chat or voice inside an embed.
- Guest accounts.
- Arbitrary themes or custom HTML injection by the host page.
- Server-side rendering inside the host page.
- Embedding private channels or DMs.
- A general-purpose OAuth provider for third-party apps.
- WebSocket streaming to embeds (public widgets poll or refresh on demand).

---

## 2. Architecture

```
Host website                          Tulpar instance
┌─────────────────────┐               ┌──────────────────────────┐
│ Tulpar Embed SDK    │               │ /embed/v1/tulpar-embed.js│
│  - creates iframe   │──────────────►│ /embed/widget/...        │
│  - postMessage      │               │ /api/v1/embed/*          │
└─────────┬───────────┘               └────────────┬─────────────┘
          │ iframe (instance origin)                │
          ▼                                         │
┌─────────────────────┐                             │
│ Tulpar widget page  │──── same-origin REST ───────┘
│ (minimal bundle)    │
└─────────────────────┘
```

- The widget HTML/JS is served by the Tulpar instance, not by the host site.
- The iframe origin is the instance origin. The host page cannot read the iframe DOM or cookies.
- The SDK loader is served by the instance and versioned; no secrets are embedded.
- The public embed key (`emb_...`) is an identifier, not a secret. Security comes from origin validation, app configuration, scopes, and short token lifetimes.

---

## 3. Embed application configuration

An instance administrator creates an embed application for a community:

- Name and description.
- Community ID.
- Public key (`emb_...`), unique and non-secret.
- Allowed origins (exact scheme + host + optional port).
- Enabled widgets (`community`, `members`, `channel_preview`, `voice_preview`, `join`).
- Whether authenticated member widgets are allowed.
- Whether public message previews are allowed, plus the single public channel that may be previewed (if any).
- Token lifetime (default 15 minutes, maximum 60).
- Rate limits.
- Enabled/revoked status and a token version for revocation.

Rules:

- Allowed origins are exact. Wildcards are not accepted by default. An operator may add explicit subdomain entries.
- `http` origins are allowed only for localhost development.
- Changing the allowlist or disabling the app invalidates future token issuance. Existing tokens expire within 15 minutes; bumping `token_version` invalidates them immediately.
- The embed key can be public in HTML; it cannot be used from an unlisted origin.

### 3.1 Data model

`embed_apps`:

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | Primary key |
| `community_id` | `uuid` | FK communities, cascade |
| `name` | `text` | Display name |
| `public_key` | `text` | Unique, `emb_...` |
| `allowed_origins` | `text[]` | Exact origins |
| `widgets` | `text[]` | Allowlisted widget names |
| `allow_authenticated` | `boolean` | Default false |
| `allow_message_preview` | `boolean` | Default false |
| `token_version` | `integer` | Default 0; bump to revoke |
| `created_by` | `uuid` | FK users, set null |
| `created_at` | `timestamptz` | |
| `updated_at` | `timestamptz` | |
| `revoked_at` | `timestamptz` | Nullable |

`embed_authorization_codes`:

| Column | Type | Notes |
|---|---|---|
| `id` | `uuid` | Primary key |
| `embed_app_id` | `uuid` | FK embed_apps, cascade |
| `user_id` | `uuid` | FK users, cascade |
| `code_hash` | `bytea` | SHA-256, unique |
| `origin` | `text` | Host origin that requested it |
| `expires_at` | `timestamptz` | 60 seconds |
| `consumed_at` | `timestamptz` | Single use |
| `created_at` | `timestamptz` | |

Embed access tokens are stateless JWTs; no session table is needed. Revocation is app-level (`revoked_at`, `token_version`) plus short expiry. The embed API re-checks user membership and community permissions on every authenticated request.

---

## 4. Token model

### 4.1 Public token

```json
{
  "iss": "https://tulpar.example.com",
  "aud": "tulpar-embed",
  "sub": "embed_app_uuid",
  "app_id": "embed_app_uuid",
  "community_id": "community_uuid",
  "scope": "public",
  "origin": "https://example.com",
  "ver": 3,
  "iat": 1760000000,
  "exp": 1760000900,
  "jti": "0192f..."
}
```

- Signed with the instance's embed signing key (separate from the user access-token key, or the same key with a distinct audience; a separate key is preferred).
- Audience `tulpar-embed`, never `tulpar-api`.
- Contains no user identity.
- Public data only: community summary, public channel list, online member count, optional public message preview.
- Rejected if the app is disabled, the origin is not in the allowlist, the token version is stale, or the community is private and the widget is not allowed.

### 4.2 Member token

Same shape plus:

```json
{
  "scope": "member",
  "user_id": "user_uuid",
  "session_id": "session_uuid"
}
```

- Issued only after the user logs in on the instance origin and approves the embed request.
- The token is audience-scoped. It cannot call the normal `/api/v1` endpoints.
- Every embed API request re-checks the user's current community membership and permissions. A token does not freeze authorization.
- If the user is banned, leaves, or loses access, requests fail immediately even if the token has not expired.
- The widget can show only the data its scopes and permissions allow.

### 4.3 Lifetime and revocation

- Default 15 minutes; maximum 60.
- The widget refreshes via the SDK when needed; public widgets can request a new token with the embed key and origin.
- Member refresh requires a valid first-party instance session in the popup/iframe context; if unavailable, the widget shows “Sign in again”.
- Disabling the app or incrementing `token_version` invalidates tokens on the next request.
- Tokens are never placed in a URL, query string, or referrer.

---

## 5. Flows

### 5.1 Public widget

1. Host page includes:
   ```html
   <script src="https://tulpar.example.com/embed/v1/tulpar-embed.js" defer></script>
   ```
2. Host calls `TulparEmbed.mount({...})`.
3. SDK creates an iframe pointing at the instance widget URL with the public key and widget type; no token in the URL.
4. The iframe sends `tulpar.embed.ready` to the host.
5. The SDK calls `POST /api/v1/embed/session` from the host page with `key`, `community_id`, and `Origin: https://example.com`. The server validates the origin allowlist and returns a public token.
6. The SDK sends the token to the iframe via `postMessage` with `targetOrigin` set to the instance origin.
7. The iframe validates the message origin, stores the token in memory, and calls the embed API same-origin.

Why the host calls `/embed/session`: the iframe's own `Origin` is the instance, not the host site. The host origin must be validated by the server, and CORS must echo that exact allowed origin. If the host page cannot make the request (CSP, offline), the widget shows a clear error.

### 5.2 Authenticated member widget

1. Host calls `TulparEmbed.login()` from a user gesture.
2. SDK opens a popup at:
   ```
   https://tulpar.example.com/embed/authorize?key=...&origin=https://example.com&state=<random>
   ```
3. The popup is first-party instance content. It requires a normal instance login if the user is not signed in.
4. The user sees the requesting site origin, community, and requested scopes, then approves or denies.
5. On approval, the server creates a one-time authorization code (60 seconds, hashed at rest, bound to app, user, and origin).
6. The popup sends `{ type: "tulpar.embed.authorization", code, state }` to `window.opener` with `targetOrigin: https://example.com`.
7. The SDK validates `event.origin === instanceOrigin`, validates `state`, and calls `POST /api/v1/embed/token` with `{ key, code, origin }` from the host page.
8. The server verifies the code hash, app, origin, user, and expiry, marks it consumed, and returns a member token.
9. The SDK passes the token to the iframe via `postMessage`.

No third-party cookie is required for the widget after the token is issued. The popup uses the instance's first-party cookie only during login/approval.

The authorize page is a narrowly scoped first-party page. It may omit `Cross-Origin-Opener-Policy` (or use `unsafe-none`) so `window.opener` remains available for the one-time handshake. It must still have a strict CSP, no third-party scripts, no remote frames, and only the approval UI. All other application pages keep a strict COOP. If a browser severs the opener anyway, the flow falls back to a visible “return to the site and retry” message; it never guesses a token.

### 5.3 Denial and expiry

- Denial closes the popup and sends a `tulpar.embed.auth_denied` message.
- Expired/consumed codes return `400 invalid_code`.
- The SDK never retries a consumed code; it restarts the login flow.
- The widget shows “Sign in to view” rather than failing silently.

---

## 6. postMessage protocol

All messages are JSON objects with a `type`, `version`, and payload. Both sides validate:

- `event.source` is the expected window.
- `event.origin` is exactly the expected origin (instance or host).
- The message type is known.
- The payload shape and sizes are valid.
- No HTML or script is accepted.

Host → iframe:

```json
{ "type": "tulpar.embed.init", "version": 1, "token": "...", "theme": "dark", "locale": "tr", "parentOrigin": "https://example.com" }
{ "type": "tulpar.embed.update", "version": 1, "token": "..." }
{ "type": "tulpar.embed.destroy", "version": 1 }
```

Iframe → host:

```json
{ "type": "tulpar.embed.ready", "version": 1 }
{ "type": "tulpar.embed.resized", "version": 1, "height": 420 }
{ "type": "tulpar.embed.auth_required", "version": 1 }
{ "type": "tulpar.embed.navigate", "version": 1, "url": "https://tulpar.example.com/invite/..." }
{ "type": "tulpar.embed.error", "version": 1, "code": "origin_not_allowed", "message": "..." }
```

Rules:

- The iframe never navigates the top window. “Join” opens a new tab after validating the URL is on the instance origin.
- Resize messages are throttled and clamped to a maximum height.
- `navigate` URLs are constructed by the instance, not by the host.
- The SDK exposes typed callbacks, not raw message handlers.
- Message schemas are versioned; breaking changes use a new version and the SDK negotiates.

---

## 7. SDK API

```js
const widget = TulparEmbed.mount({
  instance: "https://tulpar.example.com",
  embedKey: "emb_public_key",
  communityId: "0192f...",
  widget: "community",          // community | members | channel_preview | voice_preview
  target: "#tulpar-widget",     // selector or HTMLElement
  locale: "tr",
  theme: "dark",
  height: 420,
  channelId: null,              // required for channel/voice previews
  on: {
    ready: () => {},
    authRequired: () => {},
    error: (err) => {},
    navigate: (url) => {}
  }
});

await widget.login();           // optional authenticated widget
widget.update({ theme: "light" });
widget.destroy();
```

Rules:

- `mount` throws on missing/invalid required options.
- The SDK validates that `instance` is HTTPS (localhost allowed in development).
- The SDK never injects untrusted HTML into the host page.
- The iframe has a `title` for accessibility and a fixed/resizable height.
- The SDK does not read host page cookies or storage.
- `login()` must be called from a user gesture to avoid popup blocking.

### 7.1 Loader

- Served at `/embed/v1/tulpar-embed.js`.
- Versioned path; `v1` is stable within the major version.
- Small, dependency-free bundle, under 15 KB gzipped.
- No eval, no remote imports, no source maps in production (or source maps with restricted access).
- Integrity hashes are published per release for hosts that want SRI.

---

## 8. Widgets

| Widget | Public | Member | Data |
|---|---|---|---|
| Community card | Yes | Yes | Name, icon, description, member count, join button |
| Online members | Yes | Yes | Privacy-filtered online count/list; no emails/IPs |
| Public channel preview | Optional | Yes | Latest messages from a channel explicitly marked public/preview-enabled |
| Voice preview | Yes | Yes | Voice channel name, participant count, join link; no media token |
| Join/login | Yes | Yes | Invite URL or login prompt |

Rules:

- Private communities are not embeddable publicly.
- Public widgets do not grant community membership or a normal `@everyone` role; they use a separate read-only public view policy.
- A public channel preview is opt-in per embed app and limited to that app's configured `preview_channel_id`. Message content is truncated, sanitized, and never includes attachments by default.
- Member lists respect `presence_visibility`, blocks, and privacy settings.
- Embeds never expose DMs, audit logs, member emails, IPs, moderation data, or LiveKit tokens.
- Guest chat is not available in v1.

---

## 9. Security

### 9.1 Threats

- A malicious site steals an embed key and tries to use it from another origin.
- A host page tampers with postMessage to impersonate the widget.
- An attacker injects script into the host page or iframe.
- A token is leaked via URL, referrer, logs, or browser history.
- An embed widget is used to enumerate private communities or members.
- A compromised host page tries to escalate to the user's full session.
- Clickjacking or UI redressing.
- Abuse via high request volume.

### 9.2 Controls

- **Origin allowlist** enforced server-side on every session/token request. `Origin` must match exactly; no wildcard.
- **Public key is not a secret.** It identifies the app; it cannot bypass origin checks.
- **Signed, short-lived, audience-scoped tokens.** `aud = tulpar-embed`; normal API rejects them.
- **No general session exposure.** The widget never receives the user's access/refresh token.
- **Popup login on the instance origin.** The host cannot read the popup DOM or cookies; approval is explicit.
- **postMessage validation** of source, origin, type, version, and payload; `targetOrigin` is never `*`.
- **iframe sandbox** with only required capabilities; no `allow-top-navigation`, no `allow-modals` unless needed, no `allow-popups` except the login flow.
- **CSP**:
  - App: `frame-ancestors 'none'`.
  - Embed pages: `frame-ancestors` set to the app's exact allowed origins.
  - `default-src 'self'`; no remote scripts; `connect-src` limited to the instance; `img-src` limited.
- **No HTML injection**; widget data rendered as text.
- **Token transport**: postMessage only, never query strings or fragments that reach server logs. Tokens live in memory.
- **Rate limits** per app, origin, IP, and token.
- **Revocation**: disable app or bump token version; short TTL bounds exposure.
- **Permission re-check** on every member request.
- **No third-party scripts** in the widget.
- **Logging** excludes tokens, origins beyond a normalized host, and message content.
- **Clickjacking**: `frame-ancestors` on all non-embed pages; embed pages only frameable by allowlisted origins.

### 9.3 What is not trusted

- The host page's JavaScript.
- The `Origin` header alone without a configured app match.
- Any client-supplied scope, user ID, community ID, or role.
- Any data in the iframe URL.
- Any postMessage payload without source/origin validation.

---

## 10. CSP examples

Host page must allow the iframe and script:

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self' https://tulpar.example.com;
  frame-src https://tulpar.example.com;
  connect-src 'self' https://tulpar.example.com;
```

Instance embed page:

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self';
  style-src 'self';
  img-src 'self' data: blob: https://usercontent.example.com;
  connect-src 'self';
  frame-ancestors https://example.com https://www.example.com;
  base-uri 'none';
  object-src 'none';
```

The exact `frame-ancestors` value is generated per request from the app's allowlist. If no origin matches, the embed responds `403` and never renders.

---

## 11. API endpoints

| Method | Path | Access | Notes |
|---|---|---|---|
| POST | `/api/v1/embed/session` | Embed key + allowed Origin | Public token |
| POST | `/api/v1/embed/authorize` | Instance user session + key | Create one-time code |
| POST | `/api/v1/embed/token` | Key + code + allowed Origin | Exchange for member token |
| GET | `/api/v1/embed/community/{id}` | Embed token | Community summary |
| GET | `/api/v1/embed/community/{id}/members` | Embed token | Online/member preview |
| GET | `/api/v1/embed/channels/{id}/preview` | Embed token | Public message preview |
| GET | `/api/v1/embed/voice/{id}` | Embed token | Voice preview |
| GET | `/embed/v1/tulpar-embed.js` | Public | SDK loader |
| GET | `/embed/widget/...` | Public | Widget HTML shell |

The `/embed/session` and `/embed/token` endpoints validate `Origin`, not just a referer. The embed API returns only fields required by the widget.

---

## 12. Demo and documentation

`apps/embed-demo` contains:

- A static demo page with a public community card, member count, channel preview, and login button.
- A local development configuration pointing at a dev instance.
- Examples with plain HTML and a React host.
- A security checklist for website owners.
- Troubleshooting for CSP, third-party cookies, popup blockers, and height.

The demo never contains a production embed key for a real community. Keys are generated per environment.

---

## 13. Rate limits

| Action | Default |
|---|---|
| Public session creation | 60/minute per app + origin + IP |
| Member code exchange | 20/minute per app + origin + IP |
| Embed API reads | 120/minute per token |
| Widget HTML | 120/minute per IP |
| Login popup attempts | 10/minute per user |

Exceeded limits return `429` with `Retry-After`. A widget should show a quiet “Try again shortly” state, not an infinite retry loop.

---

## 14. Testing

- Origin allowlist: exact match, wrong scheme, subdomain, port, null Origin, spoofed Referer.
- Public token: valid, expired, wrong audience, wrong app, revoked app, stale token version.
- Member token: login, approval, denial, expired code, replayed code, origin mismatch, user leaves/banned, permission loss.
- postMessage: wrong source, wrong origin, wrong type, oversized payload, malformed JSON, `targetOrigin: *` rejection.
- iframe CSP: allowed host frames, unlisted host blocked, app page not frameable.
- Rate limits and abusive polling.
- Private community and private channel preview denial.
- Accessibility and keyboard use of the widget.
- Browser matrix: Chrome, Firefox, Safari, mobile; third-party cookie blocking; popup blockers.
- XSS attempts in community names, channel topics, message previews.
- SDK teardown removes listeners and iframe.

---

## 15. Limitations

- Third-party cookie blocking and tracking prevention can affect popup/session behavior; the token flow is designed to avoid relying on third-party cookies, but popups can still be blocked.
- Host CSP must allow the instance script and iframe; Tulpar cannot override the host's policy.
- iframe height is controlled by the host; auto-resize is best-effort and clamped.
- Mobile browsers may handle iframes and popups differently.
- Public widgets use REST polling or explicit refresh, not the full gateway; real-time updates are delayed.
- Guest chat, voice, and DMs are not available in v1.
- Embedding is read-only except the login/join navigation; no message sending.
- The embed key is public and can be copied; origin enforcement is the security boundary.

---

## 16. Open embed decisions

| Decision | Current position | Needed by |
|---|---|---|
| Authenticated widget scopes | Minimal member read scopes | Phase 8 |
| Public message preview opt-in | Off by default; per app/channel | Phase 8 |
| Real-time widget updates | Polling initially; scoped SSE/WS later | Phase 8 |
| Multiple communities per app | One community per embed app in v1 | Phase 8 |
| Wildcard subdomains | Not allowed; explicit origins only | Phase 8 |
| SRI for SDK | Publish hashes; not mandatory | Phase 8 |
| Embed login with SSO | Not in v1 | Post-v1 |
