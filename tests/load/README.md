# Load smoke test

A dependency-free Node.js HTTP load smoke test for the development or staging API. It
registers users, joins a public community, sends messages, and reports latency
percentiles. It does not exercise WebSocket fan-out or LiveKit media.

## Run

```bash
TULPAR_API_URL=http://127.0.0.1:18080 \
TULPAR_LOAD_USERS=20 \
TULPAR_LOAD_MESSAGES=5 \
node tests/load/http-smoke.mjs
```

The script creates disposable users and a community. Flush the development Redis
database if registration rate limits are hit.

For real load tests, add k6 or a WebSocket-capable harness and run it against a staging
instance with production-like limits. Record results before changing timeouts, pool
sizes, or rate limits.
