# Voice end-to-end harness

Two real Chromium/Edge clients join a Tulpar voice channel through LiveKit, publish
fake microphone audio, and verify that each receives the other participant's audio.

## Prerequisites

- The development Docker stack is running (`postgres`, `redis`, `livekit`, `coturn`, `api`).
- Microsoft Edge is installed at the default Windows path, or `EDGE_PATH` points to it.
- The API is reachable at `TULPAR_API_URL` (default `http://127.0.0.1:18080`).

## Install

```bash
cd tests/voice-e2e
npm install
```

## Direct connection

```bash
npm test
```

## TURN relay only

`iceTransportPolicy=relay` forces both clients through Coturn and fails if the TURN path
is unavailable.

```bash
TULPAR_VOICE_RELAY=1 npm test
```

Expected output is a JSON object where both `owner` and `member` report
`connected: true`, `localAudio: true`, and `remoteAudio: 1`.

## Development notes

The harness reuses fixed `voicee2eowner` and `voicee2emember` accounts. If repeated runs
hit development rate limits, flush the disposable development Redis database:

```bash
docker exec tulpar-dev-redis-1 redis-cli FLUSHDB
```

Never run that command against a production instance.
