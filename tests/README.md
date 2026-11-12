# Tests

Cross-cutting test suites:

- `integration` — API, database, Redis, WebSocket, and voice integration against real services.
- `e2e` — Playwright browser flows and desktop smoke tests.
- `load` — k6 and protocol-specific load tests.
- `security` — authorization, upload, SSRF, rate-limit, embed, and session abuse tests.

Unit tests live next to the code they test. Voice and desktop tests require real media/OS environments; mocked media does not count as completion. See [docs/TESTING.md](../docs/TESTING.md).
