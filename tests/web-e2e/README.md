# Web client end-to-end smoke test

Launches the Vite development server and a headless Microsoft Edge client against the
running development stack. It can also run against the Docker nginx web container.

Verified flow:

1. Log in with a real API account.
2. Load `/sync` and the first community/channel.
3. Read an API-created message.
4. Send a message through the browser UI.
5. Open friends, settings, notifications, community settings, and the admin access-denied state.
6. Switch between English and Turkish.
7. Check the skip link, mobile navigation, Escape-to-close, and offline banner.
8. Join a voice channel through the browser and reach the LiveKit `Connected` state.
9. Publish camera and screen-share tracks, focus/pin layouts, and leave cleanup.
10. In Docker mode, verify the service worker and an offline shell reload.

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

Run against the Docker nginx web container:

```bash
TULPAR_WEB_URL=http://localhost:15173 node run.mjs
```

## Desktop smoke test

`desktop.mjs` drives the Windows Tauri debug build through WebView2 remote debugging:

- server address entry and instance probe;
- device authorization approved in a headless Edge browser;
- keychain session storage and refresh after reload;
- capability reporting, global PTT registration, and deep-link forwarding.

Build the desktop app first, then run:

```bash
pnpm --filter @tulpar/desktop exec tauri build --debug --no-bundle
cd tests/web-e2e
node desktop.mjs
```

The script deletes `%LOCALAPPDATA%\com.tulpar.desktop` to start from a clean WebView
profile, clears the stored desktop session, and kills the app when finished. It requires
the Docker stack and the demo account used by the web smoke test.

Each run creates a fresh account. If repeated runs hit development registration rate
limits, flush the disposable development Redis database once:

```bash
docker exec tulpar-dev-redis-1 redis-cli FLUSHDB
```

Never run that against production.
