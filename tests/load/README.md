# Load smoke test

Two harnesses are provided:

- `http-smoke.mjs` — dependency-free Node.js smoke test that registers users, joins a
  public community, sends messages, and reports latency percentiles.
- `k6-http.js` — k6 scenario with thresholds for failed requests and p95 latency.

## Node smoke test

```bash
TULPAR_API_URL=http://127.0.0.1:18080 \
TULPAR_LOAD_USERS=20 \
TULPAR_LOAD_MESSAGES=5 \
node tests/load/http-smoke.mjs
```

## k6

```bash
k6 run tests/load/k6-http.js \
  -e TULPAR_API_URL=http://127.0.0.1:18080 \
  -e TULPAR_VUS=20 \
  -e TULPAR_DURATION=60s
```

Both create disposable users and a community. Flush the development Redis database if
registration rate limits are hit.

For real load tests, run against a staging instance with production-like limits and
record the results before changing timeouts, pool sizes, or rate limits. WebSocket
fan-out and LiveKit media load need a separate harness.
