# Tulpar

**Tulpar is an independent, self-hosted communication platform for communities, teams, and organizations.**

Tulpar combines Discord-like communities and real-time social features with TeamSpeak-like ownership of the server and its data. It provides persistent text messaging, voice and video communication, a complete web client, Windows and Linux desktop clients, an embeddable widget SDK, and a documented developer platform.

Tulpar does not depend on Discord accounts, Discord APIs, or Discord infrastructure. A community owner can run a Tulpar instance on their own server and retain full control of users, messages, files, voice infrastructure, and configuration.

> **Project status: Phase 1 — Core backend foundation.**
>
> The Rust workspace, configuration, PostgreSQL schema, Redis integration, health endpoints, OpenAPI document, authentication, sessions, device authorization, SMTP email delivery, account deletion/export, and user profile/settings endpoints are implemented and tested locally. There is no production-ready release yet. Docker Compose deployment has not been verified, and the CI workflow has not yet been proven on GitHub. See [PHASE_1_REPORT.md](docs/PHASE_1_REPORT.md) for the exact verification status and remaining work.

---

## Product surface

| Product | Purpose | Status |
|---|---|---|
| **Tulpar Server** | Self-hosted Rust backend: auth, users, communities, messaging, permissions, voice authorization, files, administration, APIs | Planned, Phase 1 |
| **Tulpar Web** | Full browser client and PWA | Planned, Phase 6 |
| **Tulpar Desktop** | Tauri 2 desktop client for Windows 10/11 and Linux | Planned, Phase 7 |
| **Tulpar Embed** | Secure iframe widgets and SDK for third-party websites | Planned, Phase 8 |
| **Tulpar Developer Platform** | REST API, WebSocket events, webhooks, API keys, bots, TypeScript and Rust SDKs | Planned, Phase 8 |

## Core principles

1. **Self-hosting first.** Every instance is independently deployable. No central Tulpar account is required.
2. **Data ownership.** The instance owner controls the database, storage, voice infrastructure, and configuration.
3. **Privacy by default.** No mandatory telemetry, no hidden cloud dependencies, and optional integrations are disabled unless configured.
4. **Security is part of the feature.** Authorization is enforced on the backend, secrets never reach clients, and defaults are hardened.
5. **Boring, durable technology.** PostgreSQL is the source of truth. Redis is ephemeral. Standard protocols are preferred over invented ones.
6. **Modular monolith first.** Clear crate boundaries without premature microservices.
7. **Cross-platform.** Windows, Linux, and modern browsers. Capability detection and honest fallbacks where a platform cannot support a feature.
8. **Maintainability over speed.** Small modules, tested changes, migrations, documentation, and incremental delivery.

## Architecture at a glance

```
                         ┌──────────────────────────────┐
                         │  Tulpar Web / Tulpar Desktop │
                         │  React + TypeScript + Vite   │
                         └───────────────┬──────────────┘
                                         │ HTTPS REST + WSS
                         ┌───────────────▼──────────────┐
                         │        Caddy / TLS           │
                         └───────┬──────────────┬───────┘
                                 │              │
                  ┌──────────────▼─────┐   ┌────▼──────────────┐
                  │  Tulpar Server     │   │  LiveKit + Coturn │
                  │  Axum modular      │   │  SFU / TURN       │
                  │  monolith + WS     │   └───────────────────┘
                  └───┬──────────┬─────┘
                      │          │
              ┌───────▼───┐  ┌───▼──────┐        ┌───────────────┐
              │ PostgreSQL│  │  Redis   │        │ Object storage│
              │ source of │  │ presence │        │ local / S3 /  │
              │ truth     │  │ ephemeral│        │ MinIO         │
              └───────────┘  └──────────┘        └───────────────┘
```

One Tulpar deployment is one **instance**. An instance can host many communities and users. Clients can connect to multiple independent instances and keep their sessions strictly separated.

## Technology choices

| Area | Choice | Why |
|---|---|---|
| Backend | Rust, Tokio, Axum, SQLx, Serde, Tower, Tracing, utoipa | Memory safety, predictable performance, strong async ecosystem, one binary |
| Database | PostgreSQL 16+ | Transactions, constraints, indexing, full-text search, mature backup tooling |
| Cache / presence | Redis 7+ | Ephemeral presence, rate limits, tickets, pub/sub; never the message source of truth |
| Voice / video | LiveKit SFU + Coturn | Proven WebRTC SFU; independent media service; standard TURN fallback |
| Desktop | Tauri 2 + Rust + React + TypeScript | Small binaries, native integration, shared frontend, lower footprint than Electron |
| Web | React + TypeScript + Vite + Zustand + TanStack Query + Tailwind CSS | Shared component model with desktop; strong ecosystem; PWA support |
| Storage | Local filesystem, S3-compatible, MinIO behind one abstraction | Self-hosting flexibility without rewriting messaging |
| Deployment | Docker Compose, Caddy, Ubuntu Server 24.04 | Simple single-VPS start; a path to horizontal scaling |

## Repository layout

```
tulpar/
  apps/
    web/                 Full web client and PWA
    desktop/             Tauri 2 shell around the shared client
    embed-demo/          Demo site for the embed SDK
  crates/                Rust workspace (modular monolith)
    core/ common/ config/ database/ events/ permissions/
    users/ auth/ social/ communities/ channels/ messaging/
    notifications/ voice/ storage/ audit/ integrations/
    realtime/ api/ gateway/ server/
  packages/              Shared TypeScript packages
    shared-types/ api-client/ ui/ i18n/ livekit-client/ embed-sdk/
  infrastructure/        Service configuration and deployment assets
    docker/ postgres/ redis/ livekit/ coturn/ caddy/ monitoring/
  deployments/           Compose stacks and environment templates
    docker-compose/ production/ development/
  docs/                  Architecture, contracts, operations, and policies
  tests/                 Integration, end-to-end, load, and security tests
  scripts/               Development and operations scripts
```

The detailed crate dependency rules and ownership model are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). The database is specified in [docs/DATABASE.md](docs/DATABASE.md).

## Documentation

| Document | Contents |
|---|---|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design, crate boundaries, runtime flows, failure modes, risks |
| [DATABASE.md](docs/DATABASE.md) | Schema, constraints, indexes, migrations, retention |
| [API.md](docs/API.md) | REST contract, errors, pagination, rate limits, endpoint catalog |
| [WEBSOCKET.md](docs/WEBSOCKET.md) | Gateway protocol, events, recovery, presence |
| [AUTHENTICATION.md](docs/AUTHENTICATION.md) | Passwords, sessions, refresh rotation, desktop device flow, CSRF |
| [PERMISSIONS.md](docs/PERMISSIONS.md) | Permission bits, roles, hierarchy, channel overwrites, escalation rules |
| [VOICE.md](docs/VOICE.md) | LiveKit integration, token issuance, Coturn, moderation, failure handling |
| [VIDEO.md](docs/VIDEO.md) | Camera, screen sharing, codecs, adaptation, platform limitations |
| [DESKTOP.md](docs/DESKTOP.md) | Tauri architecture, secure storage, tray, notifications, push-to-talk, updates |
| [WEB.md](docs/WEB.md) | Web client architecture, PWA, state, localization, accessibility |
| [EMBED_SDK.md](docs/EMBED_SDK.md) | Embed security model, token flow, postMessage protocol, SDK |
| [SECURITY.md](docs/SECURITY.md) | Threat model, controls, reporting policy, secure defaults |
| [DEPLOYMENT.md](docs/DEPLOYMENT.md) | Topology, ports, services, sizing, installation and upgrade plan |
| [BACKUP.md](docs/BACKUP.md) | Backup, retention, restore, disaster recovery |
| [TESTING.md](docs/TESTING.md) | Test pyramid, CI gates, load and security testing |
| [DECISIONS.md](docs/DECISIONS.md) | Architecture decision records |
| [ROADMAP.md](docs/ROADMAP.md) | Phases 0–9, deliverables, exit criteria |
| [PHASE_0_REPORT.md](docs/PHASE_0_REPORT.md) | Phase 0 review packet, risks, open questions, next steps |
| [PHASE_1_REPORT.md](docs/PHASE_1_REPORT.md) | Phase 1 implementation and verification report |
| [CONTRIBUTING.md](docs/CONTRIBUTING.md) | Development workflow, style, review and security rules |

## Development environment

The project targets:

- Rust stable.
- Node.js 24 LTS and pnpm 10.
- Docker Engine / Docker Desktop with Compose v2.
- PostgreSQL 16+ and Redis 7+ for local development.
- Tauri 2 system dependencies for desktop builds.
- Ubuntu Server 24.04 for production.

The current development machine now has Rust stable and pnpm installed. Docker is still unavailable. Local verification uses a user-local PostgreSQL 16 cluster and a portable Redis-compatible server; see [PHASE_1_REPORT.md](docs/PHASE_1_REPORT.md). The local Node version is 26; the project target remains Node 24 LTS for CI and release builds.

> **Windows development note:** this repository currently lives under a OneDrive-synced directory. Rust incremental compilation has produced access-denied warnings during local builds. Move the repository to a local, non-synced path, or set `CARGO_TARGET_DIR` outside OneDrive. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#17-development-environment-risks).

## Security

Tulpar handles private messages, voice, and user data. Security is not deferred to a later phase.

- All authorization is enforced by the backend.
- Passwords use Argon2id. Refresh tokens rotate and are stored hashed.
- Browser refresh tokens use `HttpOnly`, `Secure`, `SameSite` cookies. Desktop refresh tokens use the OS keychain via a device authorization flow.
- LiveKit secrets and object-storage credentials never reach clients.
- No end-to-end encryption is claimed for v1. The server can read message content and voice metadata. This is documented honestly in [SECURITY.md](docs/SECURITY.md).

To report a vulnerability, follow [SECURITY.md](docs/SECURITY.md). Do not open a public issue for a security problem.

## License

The license has not been selected yet. This is an unresolved product decision and is tracked in [docs/ROADMAP.md](docs/ROADMAP.md#open-questions). No license file is included until that decision is made.

## Name

The project and all products use the name **Tulpar**. The previous project name must not appear in code, documentation, packages, containers, or user-facing text.
