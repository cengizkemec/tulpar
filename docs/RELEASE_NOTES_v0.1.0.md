# Tulpar v0.1.0 — Public Preview

Tulpar is an independent, self-hosted communication platform for communities, teams,
and organizations. This is the first public preview of the server, web client, desktop
client, embed platform, and developer tooling.

## Highlights

### Server

- Rust/Axum modular monolith with PostgreSQL and Redis.
- Authentication, refresh rotation, device authorization, sessions, account export and deletion.
- Communities, roles, channel overwrites, invites, bans, timeouts, audit logs.
- Text messaging, attachments, reactions, pins, read states, search, notifications.
- WebSocket gateway with tickets, heartbeats, presence, typing, and reconnect snapshots.
- LiveKit voice/video authorization, TURN credentials, moderation, and webhook reconciliation.

### Web client

- Login/register, `/sync`, community/channel/DM navigation, messaging, friends, settings.
- LiveKit voice/video/screen share with device selection, PTT, quality, grid/focus/pin/fullscreen.
- Notification center, instance admin, community settings, invites, bans, audit.
- English/Turkish localization, PWA install/offline shell, accessibility pass.
- Lighthouse login shell: 100 performance / 100 accessibility / 100 best practices.

### Desktop (Windows preview)

- Tauri 2 shell embedding the shared web client.
- Server address entry, device authorization, OS keychain refresh tokens.
- Tray, native notifications, global PTT, deep links, autostart, single instance, window state.
- Local diagnostics export and crash log.
- Linux compilation is checked in CI; signed Linux/Windows installers are not part of this preview.

### Embed and developer platform

- Embed applications with exact origin allowlists and scoped, short-lived JWTs.
- Public and authenticated member widgets, one-time authorization codes, message previews.
- Hosted `/embed/v1/tulpar-embed.js` loader and typed `@tulpar/embed-sdk`.
- Scoped API keys with hashed `tk_...` tokens, read/message-send scopes, revocation.
- Owner-managed bot accounts and scoped bot tokens.
- Signed outgoing webhooks (`message.created`) with HMAC-SHA256, retries, and SSRF checks.
- TypeScript and Rust SDK foundations.

### Production

- Single-host Docker Compose stack with Caddy TLS/security headers.
- PostgreSQL, Redis, LiveKit, Coturn, API, web, and optional Prometheus/Grafana monitoring.
- Backup and restore scripts, checksums, retention, and an isolated restore-drill script.
- k6 and Node HTTP load harnesses.

## Verification

The repository CI is green for:

- Rust format, clippy, and workspace tests.
- Embed, API key, bot, and webhook integration tests.
- TypeScript contract and web production build.
- Windows desktop shell check and Linux desktop compile check.
- Secret scan and dependency audit.

## Install

See [deployments/production/README.md](../deployments/production/README.md) for the
production stack and [docs/DEPLOYMENT.md](DEPLOYMENT.md) for the broader guide.

```bash
cd deployments/production
cp .env.example .env
# fill every required value
docker compose --env-file .env up -d --build
```

The web client always connects to the origin it is served from. The desktop client asks
for the server address.

## Known limitations

- **License:** no license has been selected yet. The source is public for evaluation;
  redistribution and commercial deployment should wait for a license.
- Windows/Linux desktop installers are **unsigned** in this preview. Signed releases and
  updater feeds require code-signing certificates and Tauri updater keys.
- Clean Ubuntu 24.04 installation, a real restore drill, and the full Windows/Linux
  matrix still need to be recorded.
- Multi-node gateway fan-out, S3 storage, and an outbox `SKIP LOCKED` worker are follow-ups.
- Alertmanager routing, WebSocket/media load tests, and external security review remain open.

## Links

- [CHANGELOG](../CHANGELOG.md)
- [Roadmap](ROADMAP.md)
- [Deployment](DEPLOYMENT.md)
- [Desktop](DESKTOP.md)
- [Embed SDK](EMBED_SDK.md)
- [Webhooks](WEBHOOKS.md)
- [Testing](TESTING.md)
