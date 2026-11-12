# Tulpar Authentication and Sessions

**Status:** Phase 1. Registration, Argon2id passwords, login, refresh rotation/reuse detection, logout, session management, email verification, password reset/change, re-authentication, desktop device authorization, SMTP email delivery, account deletion, and data export are implemented. SMTP delivery was verified with Mailpit. MFA, bots, and API keys are not implemented yet.

This document defines how users prove identity, how sessions are created and revoked, and how web, desktop, bot, and API clients authenticate. Authorization (what an authenticated user may do) is defined in [PERMISSIONS.md](PERMISSIONS.md).

---

## 1. Security goals

1. Passwords are never stored or logged in plaintext.
2. A stolen database does not allow immediate account takeover.
3. Refresh tokens rotate and are bound to a server-side session.
4. Browser refresh tokens are inaccessible to JavaScript.
5. Desktop refresh tokens are stored in the operating system keychain, not webview storage.
6. Account enumeration is minimized on login and password reset.
7. Brute-force and credential-stuffing attempts are rate limited.
8. Session revocation is effective quickly and visibly.
9. Authentication events are auditable without logging secrets.
10. The system does not invent cryptography or authentication protocols.

---

## 2. Passwords

### 2.1 Hashing

- Algorithm: **Argon2id** via the Rust `argon2` crate and `password-hash` PHC format.
- Baseline parameters: memory 19 MiB, iterations 2, parallelism 1, output 32 bytes, random 16-byte salt.
- Parameters are versioned in the PHC string. On successful login, if the stored parameters are below the current policy, the password is re-hashed and saved.
- Passwords are normalized to Unicode NFC before hashing. They are not trimmed or case-folded.
- Minimum length: 12 characters. Maximum length: 128 characters. No composition rules (no forced symbols or character classes).
- Reject passwords consisting only of whitespace or containing NUL.
- A local common-password/breach list may be added without an external network call. It is not required for v1.

Argon2id runs on a blocking worker pool, never on the async runtime thread. The server enforces a semaphore to bound concurrent hashing and avoid memory exhaustion from login floods.

### 2.2 Timing and enumeration

- For an unknown username/email, the server verifies a dummy Argon2id hash with the same parameters before returning the generic failure. This reduces timing differences.
- Login always returns `401 invalid_credentials`, regardless of whether the account exists, is disabled, or the password is wrong. The only exception is `403 email_verification_required`, which is returned only after a correct password for an unverified account; this is an intentional usability tradeoff and is rate limited.
- Password reset always returns `202 Accepted` with “If an account exists, an email has been sent.”
- Registration with an existing email returns a generic `202` and sends a notification to the existing account. It does not reveal whether the address is registered. Username conflicts may return `409` because usernames are public and searchable.
- Error responses do not include timing-sensitive details.

### 2.3 Password changes

- Require the current password unless the account was created without one (future OAuth-only accounts).
- On success:
  - revoke all other sessions;
  - keep the current session and rotate its refresh token;
  - bump `users.auth_version`;
  - send a security notification email;
  - write an audit event.
- If the current password is wrong, rate limit and audit the attempt.

### 2.4 Password reset

1. `POST /auth/password/forgot` with an email or username.
2. Always return `202` after a fixed minimum delay.
3. If the account exists, create a 32-byte random token, store its SHA-256 hash with a 1-hour expiry, invalidate older active reset tokens, and send an email.
4. The email link is `https://<instance>/reset-password#token=<token>`. The token is in the URL fragment so it is not sent to the server in request logs or referrer headers. The page reads it with JavaScript and submits it in a POST body.
5. `POST /auth/password/reset` consumes the token, validates the new password, revokes all sessions, and clears the token.
6. A used, expired, or unknown token returns `400 invalid_token`, with the same response for all three cases.
7. Success sends a security notification.

Password reset does not reveal the account's email address, username, or existence.

---

## 3. Registration and email verification

### 3.1 Registration

Request fields: username, display name, email, password, locale.

Checks:

- Registration enabled by instance policy.
- Username format, reserved names, and reservation table.
- Email syntactic validity and length limit (254 characters).
- Password policy.
- IP and device rate limits.
- Optional CAPTCHA is an operator integration and disabled by default; no third-party dependency is required.

On success:

1. Normalize username/email.
2. Hash the password.
3. Insert `users`, `user_settings`, and `notification_preferences` in one transaction.
4. Create an email verification token if verification is required.
5. Queue a verification email through the outbox.
6. Write an audit event.
7. Return `202` with `verification_required` and `login_required: true`. Registration never issues a session; this keeps the response generic and avoids enumeration.

### 3.2 Email verification

- Token: 32 random bytes, SHA-256 hash stored, 24-hour expiry, single use.
- Link: `https://<instance>/verify-email#token=<token>`.
- Resend is rate limited by account and IP.
- A new token invalidates old unconsumed tokens.
- If verification is required, login after a correct password returns `403 email_verification_required`; no access or refresh token is issued.
- If verification is disabled by the instance, accounts are usable immediately. This is recorded in the audit log.
- Changing an email address requires verifying the new address. The old address receives a notification.

### 3.3 Email delivery

> **Phase 1 status:** the `smtp` driver is implemented with `lettre` and was verified end to end using Mailpit (verification and password-reset messages). A `log` driver remains available for development, and tests use a capture sender. Production configuration requires `email.driver=smtp` and a non-empty SMTP URL.

- SMTP with STARTTLS or implicit TLS; credentials from environment/config, never the database.
- Templates are localized (`en`, `tr`) and contain no secrets other than the one-time token.
- Links use the configured public URL, never the request `Host` header, to prevent host-header injection.
- Bounce handling is minimal in v1: failed deliveries are logged and retried with backoff. Hard-bounce suppression is a later feature.
- Operators must configure SPF, DKIM, and DMARC; this is documented in [DEPLOYMENT.md](DEPLOYMENT.md).
- Email tokens are redacted from logs.

---

## 4. Access tokens

### 4.1 Format

Signed JWT, `HS256`, with a 256-bit random secret from configuration.

Header:

```json
{ "alg": "HS256", "typ": "JWT", "kid": "2026-11" }
```

Claims:

```json
{
  "iss": "https://tulpar.example.com",
  "sub": "0192f...",
  "aud": "tulpar-api",
  "sid": "0192f...",
  "ver": 3,
  "auth_time": 1760000000,
  "iat": 1760000000,
  "exp": 1760000900,
  "jti": "0192f..."
}
```

| Claim | Meaning |
|---|---|
| `iss` | Instance public URL |
| `sub` | User UUID |
| `aud` | `tulpar-api` |
| `sid` | Session UUID |
| `ver` | `users.auth_version` at issue time |
| `auth_time` | Time of the last strong authentication (password/device approval) |
| `iat`, `exp` | Issued/expiry, 15 minutes by default |
| `jti` | Unique token ID for diagnostics |

The token is verified for signature, `alg`, `kid`, `iss`, `aud`, `exp`, and `iat` (60-second clock skew). It is never accepted from a cookie for general API calls.

### 4.2 Why HS256

Tulpar is a modular monolith with one verifier. HS256 with a 256-bit secret is simple, standard, and avoids key-distribution complexity. If the API is split into independent services that must verify tokens without sharing a signing secret, migrate to EdDSA with a published public key and a `kid` rotation process. That migration is additive.

The signing secret:

- Is at least 32 bytes, generated with a CSPRNG.
- Is loaded from an environment variable or secret file, never committed.
- Is never returned by the admin API.
- Supports multiple keys by `kid` for rotation. Old keys verify existing tokens until they expire.
- Is rotated by adding a new key, switching issuance, and removing the old key after the maximum access-token lifetime.

### 4.3 Revocation window

Access tokens are stateless. Revoking a session or bumping `auth_version` takes effect immediately for:

- New token issuance.
- The WebSocket gateway (it receives the revocation and closes the socket).
- Voice: the server can remove the participant from LiveKit and refuses new tokens.

Existing REST access tokens remain valid until expiry, at most 15 minutes by default. This is a documented tradeoff. Sensitive endpoints require a fresh password or a recent `auth_time` check, so a stolen access token cannot change the password or delete the account without the password. Operators can reduce the access-token lifetime to 5 minutes at the cost of more refreshes.

### 4.4 Re-authentication

Sensitive actions (password change, email change, account deletion, instance-admin destructive actions, ownership transfer) require a strong authentication event within the last 5 minutes.

- Login and device approval set `sessions.last_verified_at`.
- `POST /auth/reauth` verifies the current password, updates `last_verified_at`, and issues a new access token whose `auth_time` is now.
- Refresh preserves the existing `auth_time`; it does not silently turn a routine refresh into a re-authentication.
- A protected endpoint returns `403 reauth_required` when `auth_time` is too old.
- Password change and account deletion additionally require the password in the request body; the `auth_time` check is defense in depth, not a replacement for knowing the password.
- Re-authentication is rate limited and audited.

A future per-request revocation cache can close the window further; it is not required for v1.

---

## 5. Refresh tokens and sessions

### 5.1 Token format

- 32 random bytes, base64url encoded (43 characters).
- Stored as SHA-256 in `refresh_tokens.token_hash`; the plaintext is never stored.
- Sent to browsers only as an `HttpOnly` cookie; sent to desktop only in the device-flow response and stored in the OS keychain.
- Lifetime: 30 days sliding, with a 30-day absolute session limit. Both are configurable.

### 5.2 Rotation

Every refresh:

1. Look up the token hash.
2. Reject if expired, revoked, or the session is revoked.
3. If `used_at` is already set, treat it as reuse: revoke the entire session and all tokens, write a security event, and return `401`.
4. Mark the old token `used_at`, insert a new token with the same `token_family_id`, link `replaced_by_id`, and return the new token.
5. Update `sessions.last_seen_at`.
6. Issue a new access token.

Rotation is transactional. Two simultaneous refreshes with the same token: one succeeds; the other is treated as reuse and revokes the session. Clients must serialize refresh calls. The web client uses a single-flight promise; the desktop Rust layer uses a mutex.

### 5.3 Session limits

- Maximum active sessions per user: 20 by default.
- Maximum concurrent gateway connections per user: 5.
- Creating a new session beyond the limit does not silently evict old sessions; the user must revoke one. The admin can revoke sessions.
- Each session records device name, platform, IP, user agent, created/last-seen, and expiry.

### 5.4 Session revocation

Revoked immediately:

- Refresh and access-token issuance.
- Gateway socket (server push + close).
- Voice participant, when the revocation is security-related and LiveKit is reachable.

Not immediate:

- Existing REST access tokens, until expiry (see §4.3).

Triggers:

- Logout current session.
- Logout all sessions.
- Password reset or change.
- Account deletion.
- Instance admin action.
- Reuse detection.
- Email change (security-sensitive).
- User disables an account.

Every revocation writes an audit record with the reason.

---

## 6. Browser flow

### 6.1 Login

```
POST /auth/login
  → verify Origin is an allowed web origin
  → verify credentials
  → create session
  → create refresh token
  → Set-Cookie: __Secure-tulpar_refresh=...
  → return access token + user
```

The browser keeps the access token in memory (Zustand store) and never in `localStorage`. On page reload, it calls `/auth/refresh` using the cookie to obtain a new access token.

### 6.2 Refresh

```
POST /auth/refresh
  Cookie: __Secure-tulpar_refresh=...
  Origin: https://tulpar.example.com
  → validate Origin
  → rotate refresh token
  → Set-Cookie with the new value
  → return new access token
```

Cookie attributes:

```
Path=/api/v1/auth
HttpOnly
Secure
SameSite=Lax
Max-Age=2592000
```

The `__Secure-` prefix requires the `Secure` attribute. The cookie is not scoped to `/`, so it is not sent with normal API requests.

### 6.3 CSRF

- General API mutations use `Authorization: Bearer`, not cookies. CSRF does not apply to them.
- Login, refresh, logout, and device approval validate the `Origin` header against the configured web origins.
- `SameSite=Lax` prevents the refresh cookie from being sent on cross-site POST requests in current browsers.
- If a future endpoint authenticates with a cookie, it must implement a double-submit CSRF token and require a custom header. This is a hard rule, not a convention.

### 6.4 Logout

- Web: revoke the current session, clear the cookie with `Max-Age=0`, return `204`.
- Desktop: send the refresh token in the body, revoke, delete the keychain entry, return `204`.
- Logout is idempotent. An already-revoked token still returns `204`.

---

## 7. Desktop flow: device authorization

Tulpar Desktop never asks the user to type a password into a native login form that stores credentials. It uses the OAuth 2.0 Device Authorization Grant pattern, implemented with Tulpar-specific endpoints and short-lived codes.

### 7.1 Flow

1. Desktop calls `POST /auth/device/code` with a client name and platform.
2. Server returns `device_code`, `user_code`, `verification_uri`, expiry, and poll interval.
3. Desktop opens `verification_uri` in the system browser and displays the user code.
4. The user signs in to the instance in the browser and approves the code.
5. Desktop polls `POST /auth/device/token` with the device code.
6. On approval, the server creates a session and returns an access token and refresh token once.
7. The Tauri Rust layer stores the refresh token in the OS keychain and keeps the access token in memory. The webview never receives the refresh token.

### 7.2 Security rules

- Device code: 32 random bytes, hashed at rest, 10-minute expiry, single use.
- User code: 8 characters from an unambiguous alphabet, formatted `ABCD-EFGH`, hashed or stored case-insensitively, rate limited.
- Approval requires an authenticated browser session, a valid Origin, and a confirmation screen showing the client name and requesting IP.
- Polling is rate limited; too-fast polling returns `slow_down` and increases the interval.
- A denied or expired code cannot be approved later.
- The approval page warns if the user did not initiate the request.
- The token response is sent once; subsequent polls return `expired_token`.
- The desktop never stores the refresh token in `localStorage`, IndexedDB, a log, or a crash report.

### 7.3 Keychain behavior

| Platform | Storage | Fallback |
|---|---|---|
| Windows | Credential Manager via `keyring` | Do not persist; require login each launch |
| Linux | Secret Service (GNOME Keyring / KWallet via libsecret) | Do not persist; require login each launch |

Tulpar does not write an encrypted token file with a hardcoded key. If the OS keychain is unavailable, the user is informed and must re-authenticate.

---

## 8. API keys and bot tokens

### 8.1 API keys

- Format: `tlp_<prefix>_<secret>` where `prefix` is a short lookup value and `secret` is 32 random bytes.
- Store only `prefix` and `SHA-256(secret)`. The full key is shown once.
- Scopes are an explicit allowlist (e.g. `messages:read`, `messages:write`, `communities:read`). An API key cannot be used for instance administration unless the owner is an instance admin and the scope is granted.
- Keys are revocable, optionally expiring, and record `last_used_at`.
- API keys are rate limited per key and per owner.
- A leaked key cannot be recovered from the database.

### 8.2 Bot tokens

- Format: `tlb_<prefix>_<secret>`.
- A bot is a `users` row with `is_bot = true`, a `bot_applications` row, and a token.
- A bot can belong to communities and holds ordinary roles/permissions.
- Bots cannot be instance administrators, cannot own communities, and cannot use the desktop/web auth flows.
- Rotating a bot token revokes the previous token in the same transaction.

### 8.3 Token comparison

All token lookups use a hash lookup by unique index. Signature comparisons use constant-time comparison. No token is compared with `==` on secret material.

---

## 9. Email address changes

1. Require the current password or a fresh re-authentication.
2. Verify the new address with a token.
3. Notify the old address.
4. On confirmation, update `users.email`, set `email_verified_at`, and revoke all other sessions.
5. Rate limit changes (e.g. 3 per 24 hours).
6. Audit the change without logging the full address in metadata.

---

## 10. Account deletion

1. Require a fresh password confirmation and an explicit typed confirmation.
2. Show the consequences: sessions revoked, DMs deleted, community messages anonymized, username reserved. If the user owns a community, they must transfer ownership or delete the community first; the server blocks deletion otherwise.
3. Revoke all sessions and device authorizations.
4. Anonymize the profile and delete private data as described in [DATABASE.md](DATABASE.md#131-account-deletion-sequence).
5. Send a final confirmation email if the address is still available and the user opted in.
6. Keep an audit record with the actor set to null after the user row is purged.

There is no “soft delete login” path: a deleted account cannot authenticate.

---

## 11. Instance administration

- Instance admin membership is a separate table, granted by CLI or an existing admin.
- The first admin is created with `tulpar-server admin create` after migrations. The CLI prompts for a password and never accepts it as a command-line argument (to avoid shell history).
- Admin actions are audited with actor, target, IP, and action.
- Destructive admin actions require `auth_time` within 5 minutes (`POST /auth/reauth`) and explicit confirmation.
- Admins cannot read passwords, refresh tokens, API key secrets, or webhook secrets through the API.
- Admin sessions are ordinary sessions with an `instance_admin` check on each request. There is no separate “super token”.
- An admin cannot demote the last remaining admin. Ownership of the instance is not transferable through the API in v1; use the CLI.

---

## 12. Brute force and abuse controls

| Control | Behavior |
|---|---|
| Login per IP | Token bucket, 5 failures/15 min, then exponential delay |
| Login per account | Same, keyed by normalized username/email hash |
| Password reset | 3/hour per account, 10/hour per IP |
| Registration | 3/hour per IP, plus optional invite requirement |
| Email verification resend | 3/hour per account |
| Device code creation | 10/hour per IP |
| Device approval attempts | 10/hour per user |
| Refresh | 60/hour per session; reuse revokes |
| API key auth failures | 20/minute per IP, then 429 |
| WebSocket tickets | 10/minute per user |

Rate-limit keys are hashed where they contain usernames or emails. Redis stores counters with TTL; no permanent login history is kept in Redis.

After repeated failures, the response remains generic. The server may add a short delay, require a CAPTCHA (if configured), or temporarily block the IP. It does not permanently lock the account, because that enables a denial-of-service attack against a known username.

---

## 13. Security events and audit

Audited authentication events:

- Registration, verification, login success/failure (aggregated), logout, logout-all.
- Refresh reuse detection.
- Password change/reset.
- Email change.
- Session revocation.
- Device authorization approval/denial.
- API key/bot token creation and revocation.
- Instance admin grant/revoke.
- Account disable/enable/delete.

Audit metadata never includes passwords, tokens, full email addresses, or message content. IP addresses are stored for security review and retention-limited.

---

## 14. Failure behavior

| Dependency | Authentication impact |
|---|---|
| PostgreSQL down | No login, refresh, registration, or session validation beyond an already-issued access token |
| Redis down | Login/refresh still work via PostgreSQL; rate limits degrade to per-process memory; gateway tickets fail closed; device polling still works |
| SMTP down | Registration with required verification is delayed; accounts and tokens are preserved; resend is available |
| Clock skew | 60-second tolerance; larger skew causes token rejection; operators should use NTP |
| Signing key missing/invalid | Server refuses to start rather than issuing unsigned or default-key tokens |
| Keychain unavailable (desktop) | Session is not persisted; user must re-authenticate |

---

## 15. Test requirements

Authentication is security-critical. At minimum:

- Argon2id hash/verify, parameter upgrade, dummy verify timing.
- Password policy boundaries and Unicode normalization.
- Login success/failure, generic errors, rate limits, disabled account, unverified email.
- Refresh rotation, concurrent refresh, reuse detection, expiry, revocation, family revocation.
- Cookie attributes, Origin validation, missing/foreign Origin, CSRF assumptions.
- Access-token signature, algorithm confusion, `kid` rotation, issuer/audience/expiry checks.
- Email verification token expiry, replay, resend invalidation.
- Password reset token expiry, replay, session revocation, generic responses.
- Device flow approval, denial, expiry, slow polling, one-time token, keychain mock.
- API key/bot token hashing, scope enforcement, revocation, last-used updates.
- Account deletion removes sessions and private data.
- No secret appears in logs or error bodies.
- Property-based tests for token parsing and permission-adjacent auth checks.
- Load test for Argon2 concurrency limits.

---

## 16. Open authentication decisions

| Decision | Current position | Needed by |
|---|---|---|
| MFA (TOTP/WebAuthn) | Not in v1; schema can add `user_mfa` later | Post-v1 |
| OIDC/SSO | Not in v1; instance-level OIDC is a likely enterprise feature | Post-v1 |
| Email verification default | Required by default; operator can disable | Phase 1 |
| Access token lifetime | 15 minutes; configurable 5–30 | Phase 1 |
| Session absolute lifetime | 30 days | Phase 1 |
| JWT algorithm | HS256 initially; EdDSA if split services | Phase 1 |
| Breach-password list | Local list considered; no external API | Phase 1 |
| CAPTCHA | Optional integration, disabled by default | Phase 2 |
| Account recovery without email | Not supported; operator-assisted only | Phase 2 |
