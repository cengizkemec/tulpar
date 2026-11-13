# @tulpar/web

React 19 + TypeScript + Vite client foundation for Tulpar.

## Implemented

- Login/register and refresh-cookie session bootstrap.
- `/sync`-driven community/channel/DM navigation.
- Text message history and sending.
- WebSocket gateway live updates.
- LiveKit voice join/leave, microphone/camera selection, push-to-talk, connection quality, screen share, video tiles, pin/focus/grid layouts, and fullscreen.
- Friends/search/requests/blocking/DM UI.
- Profile, privacy, and notification settings UI.
- In-app notification center with unread badge.
- Browser notifications with permission control.
- Instance admin panel: overview, user moderation/admin grants, and audit log.
- Community settings: general, members, roles, invites, bans, and audit.
- Full English/Turkish localization with a persisted language switcher.
- PWA install prompt, offline banner, manifest, and shell-only service worker.
- Accessibility pass: skip link, landmarks, labelled controls, keyboard navigation, and WCAG AA contrast.

## Commands

```bash
pnpm --filter @tulpar/web dev
pnpm --filter @tulpar/web typecheck
pnpm --filter @tulpar/web build
```

The Vite dev server proxies `/api` and `/gateway` to `http://127.0.0.1:18080`.
Override the API base with `VITE_API_URL` for non-proxy deployments.

Browser smoke coverage lives in [tests/web-e2e](../../tests/web-e2e/README.md).
