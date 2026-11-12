# Rust crates

The Tulpar backend is a modular monolith. Crates are grouped by responsibility; dependencies point downward and no cycles are allowed.

## Foundation

- `core` — IDs, domain enums, errors, pagination, clocks.
- `common` — validation, normalization, token generation, time utilities.
- `config` — configuration loading, validation, feature flags, instance settings.
- `database` — pool, transactions, migrations, outbox/idempotency helpers.
- `events` — versioned event contracts shared by the API, gateway, workers, and integrations.
- `permissions` — permission bits and the pure authorization evaluator.

## Identity and social

- `users` — accounts, profiles, settings, privacy, instance admins.
- `auth` — passwords, sessions, refresh rotation, device flow, verification/reset tokens.
- `social` — friends, blocks, DM conversation metadata.
- `communities` — communities, members, roles, invites, bans.
- `channels` — categories, channels, permission overwrites.

## Content and platform

- `messaging` — messages, reactions, mentions, pins, attachments, read states.
- `notifications` — notification records and preferences.
- `voice` — LiveKit authorization and voice projection.
- `storage` — object storage abstraction and upload validation.
- `audit` — append-only audit records.
- `integrations` — webhooks, API keys, bots, embed apps.

## Composition

- `realtime` — Redis pub/sub, presence, tickets, fan-out.
- `api` — Axum REST routes, DTOs, OpenAPI, application services.
- `gateway` — WebSocket protocol and connection lifecycle.
- `server` — binary, wiring, CLI, background workers.

No crate contains application code yet. Phase 1 creates the workspace and verifies the build. See [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) for dependency rules and table ownership.
