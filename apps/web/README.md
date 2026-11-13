# @tulpar/web

React 19 + TypeScript + Vite client foundation for Tulpar.

## Implemented

- Login/register and refresh-cookie session bootstrap.
- `/sync`-driven community/channel/DM navigation.
- Text message history and sending.
- WebSocket gateway live updates.
- LiveKit voice join/leave, microphone, camera, and screen share.
- Friends/search/requests/blocking/DM UI.
- Profile, privacy, and notification settings UI.
- In-app notification center with unread badge.
- PWA manifest and shell-only service worker.
- English/Turkish localization foundation.

## Commands

```bash
pnpm --filter @tulpar/web dev
pnpm --filter @tulpar/web typecheck
pnpm --filter @tulpar/web build
```

The Vite dev server proxies `/api` and `/gateway` to `http://127.0.0.1:18080`.
Override the API base with `VITE_API_URL` for non-proxy deployments.

Browser smoke coverage lives in [tests/web-e2e](../../tests/web-e2e/README.md).
