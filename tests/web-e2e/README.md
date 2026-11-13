# Web client end-to-end smoke test

Launches the Vite development server and a headless Microsoft Edge client against the
running development stack.

Verified flow:

1. Log in with a real API account.
2. Load `/sync` and the first community/channel.
3. Read an API-created message.
4. Send a message through the browser UI.
5. Join a voice channel through the browser and reach the LiveKit `Connected` state.

## Prerequisites

- `postgres`, `redis`, `livekit`, `coturn`, and `api` services are running.
- Microsoft Edge is installed at the default Windows path.
- The web dependencies are installed with `pnpm install` at the repository root.

## Run

```bash
cd tests/web-e2e
npm install
npm test
```

Each run creates a fresh account. If repeated runs hit development registration rate
limits, flush the disposable development Redis database once:

```bash
docker exec tulpar-dev-redis-1 redis-cli FLUSHDB
```

Never run that against production.
