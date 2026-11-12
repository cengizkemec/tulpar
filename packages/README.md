# Shared TypeScript packages

- `shared-types` — generated OpenAPI types plus hand-written WebSocket event contracts.
- `api-client` — typed REST client, auth/refresh handling, retries, error mapping.
- `ui` — design system, tokens, accessible primitives, shared components.
- `i18n` — English/Turkish resources and formatting helpers.
- `livekit-client` — room lifecycle, device selection, voice/video wrappers.
- `embed-sdk` — the small iframe SDK used by third-party websites.

Packages are consumed by `apps/web` and, through the shared build, by `apps/desktop`. Do not create duplicate resource types or fork the UI for desktop. The developer-facing TypeScript SDK will wrap `api-client` and `shared-types` in Phase 8; the Rust SDK is a separate client crate or repository decision recorded then. See [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md#5-frontend-architecture).
