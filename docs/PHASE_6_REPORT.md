# Phase 5/6 Report — Web Client Foundation and Media Controls

**Status:** In progress. The authenticated SPA foundation, text/DM navigation, gateway updates, and voice/video/screen controls are implemented and browser-tested. UI polish, friends/settings/admin surfaces, PWA, accessibility, and the full browser matrix remain open.

**Date:** 2026-11-13

---

## 1. Implemented

### Web application

- `apps/web` React 19 + TypeScript + Vite + Tailwind 4.
- Login and registration flows.
- In-memory access token with HttpOnly refresh-cookie bootstrap and single-flight refresh.
- TanStack Query for server state and Zustand for session/voice state.
- `/sync` snapshot loading.
- Community switcher, category/channel sidebar, DM list, unread badges.
- Channel and DM routes.
- Text message history and sending.
- Gateway WebSocket: HELLO/IDENTIFY/READY, heartbeats, reconnect, message invalidation, voice-state updates, notification invalidation.
- English/Turkish localization with i18next.
- LiveKit lazy-loaded media chunk.
- Friends UI: search, requests, accept/decline, add/remove, block/unblock, and DM creation.
- Settings UI: profile, presence, friend/DM privacy, read receipts, and notification preferences.
- In-app notification center with unread badge, mark-one, and mark-all-read; gateway events refresh the badge.

### Media controls

- Join/leave voice.
- Microphone enable/mute.
- Camera enable/disable.
- Screen-share enable/disable.
- Voice participant tiles from the server projection.
- Gateway-driven voice state updates with polling fallback.

### CI and verification

- Root `pnpm typecheck` now includes the web app.
- TypeScript contract CI job runs the production web build.
- Docker development stack now serves the built web app through nginx at `http://localhost:15173`, proxying `/api`, `/gateway`, and `/webhooks`.
- `tests/web-e2e` logs in, loads `/sync`, sends a browser message, receives an API-created message over the gateway socket, joins a LiveKit voice channel, reaches `Connected`, enables the fake camera, starts screen sharing with the Edge desktop-capture source, leaves voice, and confirms media state resets. It can run against Vite or the Docker web container.

---

## 2. Verification

```text
pnpm --filter @tulpar/web typecheck -> pass
pnpm --filter @tulpar/web build     -> pass (LiveKit split into a lazy chunk)
node tests/web-e2e/run.mjs          -> login/text/gateway/voice/camera/screen/leave pass
TULPAR_WEB_URL=http://localhost:15173 node tests/web-e2e/run.mjs -> Docker nginx path pass
node tests/voice-e2e/run.mjs        -> two-client direct audio pass
TULPAR_VOICE_RELAY=1 node tests/voice-e2e/run.mjs -> TURN relay pass
```

---

## 3. Known gaps

1. Admin UI and community-settings UI are not implemented.
2. Browser notifications and desktop toasts are not implemented; in-app notification center and preferences exist.
3. PWA manifest, service worker, and offline shell are not implemented.
4. Responsive/mobile layouts and accessibility audit are incomplete.
5. Device selection, PTT/VAD, per-user volume, and quality controls are incomplete.
6. Camera/screen video tiles and layout modes (grid/focus/pin/fullscreen) are incomplete.
7. Browser matrix, capability detection, and desktop WebView tests are not recorded.
8. Web e2e and voice e2e require a local Docker media stack and are not in CI.

---

## 4. Recommended next steps

1. Add friends/settings/notification UI and admin surfaces.
2. Add video tiles, layout modes, device selection, and capability detection.
3. Add PWA/offline shell and accessibility pass.
4. Add browser matrix and desktop WebView verification.
5. Add web e2e to a scheduled/manual CI media job.
