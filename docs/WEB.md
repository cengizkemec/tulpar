# Tulpar Web Client

**Status:** Phase 0 design. Not implemented.

**Stack:** React 19, TypeScript, Vite, Zustand, TanStack Query, Tailwind CSS, LiveKit JavaScript SDK, i18next.

The web client is a complete Tulpar client, not a landing page. It is also the canonical UI shared with Tulpar Desktop.

---

## 1. Goals and non-goals

### 1.1 Goals

- Full authentication, communities, channels, DMs, friends, voice, video, screen sharing where supported, notifications, settings, and administration.
- Responsive layout for desktop, tablet, and mobile browsers.
- Installable PWA with an offline shell.
- Turkish and English localization with no hardcoded interface strings.
- Keyboard-accessible, screen-reader-friendly UI.
- Strict security posture: no private data in unsafe caches, no unsanitized HTML, no secrets in the bundle.
- One codebase shared with the desktop shell.

### 1.2 Non-goals for v1

- A separate mobile native app.
- Offline message sending or offline access to private history.
- Background voice that survives browser suspension.
- Reliable global push-to-talk (browser limitation).
- Server-side rendering (the app is an authenticated SPA; SEO is not a goal).
- Embedding arbitrary third-party scripts.

---

## 2. Application architecture

```
apps/web
  src/
    app/            providers, router, bootstrap
    routes/         login, app shell, admin, embed, invite
    features/       auth, communities, channels, messaging, friends,
                    dms, voice, video, notifications, settings, admin
    components/     feature-local components
    stores/         Zustand session/UI/voice stores
    lib/            query client, gateway client, platform adapter,
                    formatting, validation
    locales/        en, tr
packages/
  ui                design system, Radix primitives, tokens
  api-client        typed REST client, auth, retries, errors
  shared-types      generated OpenAPI types + WS event types
  i18n              i18next setup and shared formatting
  livekit-client    room lifecycle, devices, voice/video wrappers
  embed-sdk         separate embed bundle (Phase 8)
```

### 2.1 Routing

| Route | Purpose |
|---|---|
| `/login` | Login |
| `/register` | Registration |
| `/verify-email` | Email verification (token in fragment) |
| `/forgot-password` | Request reset |
| `/reset-password` | Reset (token in fragment) |
| `/device` | Device authorization approval |
| `/invite/:code` | Invite preview/join |
| `/app` | Main authenticated shell |
| `/app/c/:communityId/:channelId` | Community channel |
| `/app/dm/:channelId` | Direct message |
| `/app/friends` | Friends and requests |
| `/app/settings/*` | User settings |
| `/app/c/:communityId/settings/*` | Community settings (permission-gated) |
| `/admin/*` | Instance administration (instance-admin-gated) |
| `/embed/*` | Embeddable widget surfaces, separate minimal shell |
| `/offline` | PWA offline fallback |

Routes are lazy-loaded. Admin and embed routes are separate chunks. The route guard checks authentication and shows a permission-denied state, but the backend remains authoritative.

### 2.2 Providers

- `QueryClientProvider` with instance-scoped query keys.
- `I18nProvider`.
- `ThemeProvider` (dark default, light, system; accent color).
- `GatewayProvider` (one socket per active instance).
- `VoiceProvider` (one LiveKit room at a time).
- `NotificationProvider` (in-app toasts and browser/native notifications).
- `PlatformProvider` (web or desktop adapter).
- `ErrorBoundary` per route group.

### 2.3 State ownership

| State | Owner | Persistence |
|---|---|---|
| Access token | Zustand auth store | Memory only |
| Refresh token | Web: HttpOnly cookie; desktop: OS keychain via Rust | Not JS-accessible |
| Server data | TanStack Query | Memory; optional session storage not used for private data |
| UI layout, theme, accent | Zustand + localStorage | Local, non-sensitive |
| Drafts | Zustand, optional IndexedDB | Local, opt-in |
| Voice room state | Zustand voice store | Memory |
| Instance registry | localStorage | URL and display name only |
| Notification preferences | Server + local cache | Server is authoritative |

Query keys always include the instance URL:

```ts
["instance", instanceUrl, "channel", channelId, "messages", cursor]
```

Switching instances must not reuse another instance's cache. Logout clears all query and store data for that instance.

### 2.4 Data fetching

- TanStack Query for REST reads and mutations.
- Mutations optimistically update message lists with a client nonce; reconcile by ID/nonce.
- On `401`, a single-flight refresh is attempted. If it fails, the route logs out.
- Retries: idempotent GETs with exponential backoff; mutations only retry when an `Idempotency-Key` is set.
- Pagination: infinite queries for messages, members, notifications, audit logs.
- Errors map to stable codes, then localized messages.

### 2.5 Gateway integration

- One WebSocket per instance, opened after authentication.
- `READY` triggers `GET /sync` and message catch-up.
- Dispatch events update the query cache and stores:
  - `MESSAGE_CREATE` prepends/appends to the active channel and updates unread counts.
  - `MESSAGE_UPDATE`/`DELETE` patches or tombstones.
  - `CHANNEL_ACCESS_UPDATE` removes access immediately.
  - `VOICE_STATE_UPDATE` updates the voice panel.
  - `NOTIFICATION_CREATE` updates the inbox and may show a notification.
- Sequence gaps trigger resync; the client never assumes the socket is complete.
- Reconnect uses exponential backoff with jitter and stops on session revocation.

---

## 3. Authentication in the browser

1. `POST /auth/login` sets the `__Secure-tulpar_refresh` cookie and returns an access token.
2. The access token is stored in memory.
3. On reload, `POST /auth/refresh` uses the cookie to obtain a new access token.
4. API calls use `Authorization: Bearer`.
5. Logout revokes the session and clears the cookie.
6. Multi-instance: each instance is a different origin and has its own cookie; the client registry tracks URLs, not tokens.

The web client never reads the refresh cookie, never writes tokens to `localStorage`, and never puts tokens in URLs.

### 3.1 Multi-instance behavior

Each Tulpar instance is a separate origin. The instance switcher navigates the browser to the selected instance; cookies and tokens are not shared across origins. Returning to an instance reuses its existing cookie if valid, otherwise it asks for login.

The web client does not attempt to aggregate private data from multiple instances in one page. Doing so would require cross-origin credentials, would be blocked or weakened by browser cookie policy, and would undermine instance isolation. Desktop has a separate native session store and can keep per-instance credentials in the OS keychain.

### 3.1 CSRF

- Mutations use the Bearer token, so they are not cookie-authenticated.
- Login/refresh/logout/device approval send an `Origin` header that the server validates.
- No custom CSRF token is required for the current flow. Any future cookie-authenticated mutation must add double-submit CSRF protection first.

### 3.2 Account recovery links

Verification and reset tokens are read from the URL fragment (`#token=...`), never from the query string, so they are not sent to the server in logs or referrer headers. The page immediately clears the fragment after reading it.

---

## 4. Feature surfaces

### 4.1 Main shell

- Instance switcher.
- Community rail with icons, unread badges, and add/join actions.
- Channel/category sidebar with text and voice channels.
- Main content: message list, thread/forum view, or voice room.
- Member list (permission-gated, responsive drawer on mobile).
- User panel: avatar, presence, mute/deafen, settings.
- Voice panel: connection state, device controls, participants, screen share.
- Search and quick switcher.

On mobile:

- Community rail and channel list become drawers.
- Member list is a sheet.
- Voice controls collapse into a compact bar.
- Message input remains reachable above the keyboard.

### 4.2 Messaging

- Cursor-paginated history with virtualization.
- Replies, mentions, reactions, pins, attachments, edits, deletions.
- Markdown subset rendered safely.
- Link previews only with `EMBED_LINKS`; previews are fetched by the server or a constrained client fetch, never by injecting HTML.
- Typing indicators and unread dividers.
- Jump-to-message and “new messages” marker.
- Slowmode countdown.
- Optimistic sends with retry and duplicate protection.
- Empty, loading, error, permission-denied, and offline states.

### 4.3 Voice and video

- Join/leave, mute, deafen, push-to-talk while focused.
- Input/output device selection.
- Per-user volume.
- Speaking indicators from LiveKit.
- Camera and screen share where supported.
- Participant grid/focus layouts.
- Network quality and reconnection states.
- Clear error messages for permission denial, device busy, unsupported capture, and TURN failure.

Browser limitations are documented in [VOICE.md](VOICE.md) and [VIDEO.md](VIDEO.md). The UI never promises global shortcuts or background voice.

### 4.4 Friends and DMs

- Friend list, requests, blocking, privacy settings.
- DM list with unread counts and last activity.
- Typing and presence.
- No group DMs in v1.

### 4.5 Notifications

- In-app inbox and toasts.
- Browser notifications after explicit permission.
- Per-community/channel mute settings.
- Mention-only mode.
- Do not notify for the focused channel or for muted scopes.
- Notification click routes to the message/channel.
- Service-worker push is optional and disabled unless VAPID is configured; in-app and desktop notifications work without it.

### 4.6 Settings

- Profile, avatar, display name, bio, custom status.
- Account: email, password, sessions/devices, account deletion.
- Privacy: friend requests, DMs, presence, read receipts.
- Notifications: desktop/browser, sounds, mentions, per-community/channel.
- Appearance: dark/light/system, accent color, compact messages.
- Voice/video: devices, PTT, VAD, audio processing.
- Language: English/Turkish.
- Advanced: diagnostics, cache clear, instance info.

### 4.7 Administration

- Instance admin routes under `/admin`, lazy-loaded and permission-gated.
- Community settings under `/app/c/:id/settings`, gated by community permissions.
- Responsive tables/cards; destructive actions require confirmation and recent re-authentication.
- Secrets are never displayed; only configured/not configured and test-connection actions.

---

## 5. PWA

### 5.1 Manifest

- Name: Tulpar.
- Short name: Tulpar.
- Icons: 192, 512, maskable.
- Theme color and background color matching the dark theme.
- `display: standalone`.
- Start URL `/app`.
- Scope `/`.
- No orientation lock.

### 5.2 Service worker

- Precache the application shell and static assets.
- Serve the offline page when navigation fails.
- Do not cache:
  - API responses containing private data.
  - WebSocket traffic.
  - Auth tokens.
  - Message content or attachments.
  - Admin pages with private data.
- A runtime cache may store public static assets with versioned filenames.
- Activation cleans old caches.
- An update prompt asks the user to reload; never force reload during a call or unsent draft.

### 5.3 Installability and limitations

- Installability depends on HTTPS, manifest, icons, and a service worker.
- iOS/iPadOS add-to-home-screen behavior differs; push and background voice are restricted.
- Background tabs may throttle timers and audio; the UI shows a “voice may be interrupted” warning on mobile.
- PWA install is optional; the web app works without it.

### 5.4 Offline behavior

- Offline: show a clear banner and the offline page for navigations.
- Cached shell may load, but API calls fail with a retry state.
- Do not queue messages for automatic send while offline; keep them as local drafts with an explicit retry when online.
- No offline access to private message history.

---

## 6. Localization

- `i18next` + `react-i18next`; resources in `packages/i18n` or `apps/web/src/locales`.
- Languages: `en`, `tr`.
- No user-facing string is hardcoded in components.
- Use `Intl` for dates, times, numbers, relative time, and pluralization.
- Server sends stable codes; the client maps them to localized messages with fallbacks.
- Layout must tolerate longer Turkish strings.
- Missing-key behavior: fall back to English and log a development warning; production shows a safe generic message.
- RTL is not required for v1 but layout primitives should not assume hardcoded left/right where avoidable.

---

## 7. Design system and accessibility

### 7.1 Visual language

- Original identity; do not copy Discord's logo, assets, or exact layout.
- Dark theme default, light theme optional, accent colors configurable.
- Compact but readable typography; clear hierarchy.
- Calm surfaces, limited motion, no decorative animation that blocks work.
- Consistent spacing, radii, and focus rings.
- Status colors meet contrast requirements in both themes.

### 7.2 Accessibility target

- WCAG 2.1 AA for the web client.
- Keyboard navigation for every action.
- Visible focus indicators.
- Semantic landmarks, headings, lists, and buttons.
- Screen-reader labels for icon-only controls.
- `aria-live` for connection status, new messages where appropriate, and errors.
- Respect `prefers-reduced-motion`.
- Minimum contrast 4.5:1 for body text and 3:1 for large text/UI components.
- No color-only status; pair with text/icon.
- Dialogs trap focus and restore it on close.
- Voice controls are operable without a mouse.
- Test with NVDA or Narrator and keyboard-only navigation.

---

## 8. Performance

### 8.1 Budgets

| Metric | Target |
|---|---|
| Initial JS (gzip, app shell, excluding admin/embed) | < 350 KB |
| Route chunk (gzip) | < 120 KB |
| First contentful paint (mid-range mobile) | < 2.5 s |
| Time to interactive (mid-range mobile) | < 4 s |
| Message list scroll | 60 fps with virtualization |
| Memory for 10k messages | Bounded; windowed list, not all DOM nodes |

### 8.2 Techniques

- Route-level code splitting; admin and embed separate.
- Virtualize message lists and large member lists.
- Avoid re-rendering the whole message list on typing/presence events.
- Throttle active-speaker and typing updates.
- Use `React.memo`/selectors only where measured.
- Lazy-load LiveKit only when joining voice.
- Compress images and serve thumbnails.
- Use HTTP caching for immutable assets; no cache for API responses unless explicitly public.
- Prefetch the next message page on scroll.
- Measure with real builds; no performance claims without numbers.

---

## 9. Security

- React escaping by default; never use `dangerouslySetInnerHTML` for user content.
- Markdown renderer with a strict sanitizer and URL scheme allowlist (`https`, `http`, `mailto`). No `javascript:`, `data:`, or `file:` links.
- Attachment previews use sandboxed, separate-origin URLs; HTML/SVG files are downloads, not inline documents.
- CSP headers are set by Caddy, not by client code:
  - `default-src 'self'`
  - `script-src 'self'` (no inline scripts; Vite builds external assets)
  - `style-src 'self' 'unsafe-inline'` only if required by the CSS approach; prefer hashed/nonce styles
  - `img-src 'self' data: blob: https://usercontent.example.com`
  - `media-src 'self' blob:`
  - `connect-src 'self' https://tulpar.example.com wss://tulpar.example.com wss://rtc.example.com`
  - `frame-ancestors 'none'` for the app; embed routes use a per-request allowlist
  - `object-src 'none'`, `base-uri 'self'`, `form-action 'self'`
- No third-party analytics or tracking scripts by default.
- Tokens only in memory (access) or HttpOnly cookie (refresh).
- Instance URLs are validated; no `javascript:` or `file:` navigation.
- External links open with `rel="noopener noreferrer"`.
- Embed widgets use a separate minimal bundle and scoped tokens; never the full app session.
- Clear all instance data on logout.
- Content Security Policy violations are reported to an optional self-hosted endpoint, disabled by default.

---

## 10. Error and state handling

Every feature must implement:

- **Loading:** skeletons or progress, not a blank screen.
- **Empty:** meaningful text and a next action.
- **Error:** stable code, localized message, retry.
- **Reconnecting:** banner and queued-event behavior.
- **Permission denied:** explanation without leaking private resource details.
- **Offline:** banner, disabled network actions, draft preservation.
- **Rate limited:** countdown and retry guidance.
- **Maintenance:** instance message and retry time.

The app must not silently swallow a failed mutation. Optimistic updates roll back on failure and show the error.

---

## 11. Browser support

Target the latest two stable versions of:

- Chrome/Edge (desktop and Android).
- Firefox (desktop and Android).
- Safari (macOS and iOS/iPadOS).

Minimum:

- WebRTC, WebSocket, ES2022, CSS grid/flex, `Intl`, service workers, `getUserMedia`.
- No Internet Explorer.
- Older browsers show an unsupported-browser page instead of a broken app.

Voice/video support is capability-detected, not assumed from the browser name. Safari and mobile browsers have documented limitations.

---

## 12. Testing

- Vitest + React Testing Library for components, stores, and utilities.
- Mock Service Worker for API contract tests.
- Playwright for end-to-end flows:
  - register/login/refresh/logout;
  - create community/channel, send/edit/delete message, reactions, pins;
  - friend request/accept, DM;
  - join voice (with a test LiveKit instance where feasible);
  - notifications and settings;
  - admin permission gating;
  - PWA offline shell.
- Accessibility checks with axe in component tests and key Playwright flows.
- Visual regression for the design system, not every screen.
- Lighthouse CI budgets for the app shell.
- Real browser matrix in CI where feasible; manual smoke tests for Safari/iOS.
- Contract tests against the generated OpenAPI schema and WS event types.

---

## 13. Open web decisions

| Decision | Current position | Needed by |
|---|---|---|
| CSS approach details | Tailwind 4 + CSS variables; confirm build | Phase 6 |
| Component primitive library | Radix UI | Phase 6 |
| Browser push | Optional VAPID, disabled by default | Phase 6 |
| Offline drafts | Optional IndexedDB, user-controlled | Phase 6 |
| Admin bundle | Integrated `/admin` initially | Phase 6 |
| Embed bundle isolation | Separate Vite entry, separate CSP | Phase 8 |
| Mobile voice | Best-effort, documented limitations | Phase 6 |
| Analytics | None by default | Phase 6 |
| Sentry/error reporting | Opt-in, self-hostable, disabled by default | Phase 9 |
