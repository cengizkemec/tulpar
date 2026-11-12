# Tulpar Desktop

**Status:** Phase 0 design. Not implemented.

**Targets:** Windows 10/11 and Linux. macOS is out of scope for v1.

**Stack:** Tauri 2, Rust, the shared Tulpar React/TypeScript client, Vite.

---

## 1. Goals and non-goals

### 1.1 Goals

- A fast, native-feeling desktop client with a small footprint.
- The same core UI and feature set as the web client, without duplicating screens.
- Secure credential storage in the operating system keychain.
- Native notifications, system tray, single-instance behavior, deep links, and window-state persistence.
- Global push-to-talk where the OS and desktop environment permit it.
- Capability detection and honest fallbacks for Linux/X11/Wayland differences.
- Signed auto-updates with a clear release channel.
- Privacy-respecting diagnostics with no default telemetry.

### 1.2 Non-goals for v1

- macOS builds.
- A separate mobile app.
- A second implementation of the backend or a local message database.
- Bundling PostgreSQL/Redis/LiveKit inside the desktop app.
- Remote content loading outside the bundled application.
- Silent background screen capture or microphone access.

---

## 2. Architecture

```
┌────────────────────────────────────────────────────────────┐
│ Tauri 2 process                                            │
│                                                            │
│  ┌──────────────────────┐      ┌────────────────────────┐  │
│  │ Rust core            │      │ WebView                │  │
│  │ - keychain           │◄────►│ shared Tulpar SPA      │  │
│  │ - device auth        │ IPC  │ React + TS + Vite      │  │
│  │ - notifications      │      │ platform adapter       │  │
│  │ - tray / shortcuts   │      └────────────────────────┘  │
│  │ - updater / deep link│                                  │
│  └──────────────────────┘                                  │
└────────────────────────────────────────────────────────────┘
                 │ HTTPS / WSS
                 ▼
          Tulpar Server / LiveKit
```

### 2.1 Shared frontend

- `apps/web` is the canonical SPA.
- `apps/desktop` contains the Tauri shell and a small platform adapter.
- The Tauri build consumes the web build output; it does not fork the UI.
- Desktop-only behavior is selected through a `PlatformAdapter` interface:

```ts
interface PlatformAdapter {
  kind: "web" | "desktop";
  login(instanceUrl: string): Promise<AuthResult>;
  refresh(instanceUrl: string): Promise<AccessToken>;
  logout(instanceUrl: string): Promise<void>;
  notify(payload: NotificationPayload): Promise<void>;
  registerPushToTalk(shortcut: string): Promise<PttRegistration>;
  openExternal(url: string): Promise<void>;
  getCapabilities(): Promise<PlatformCapabilities>;
}
```

The web implementation uses cookies and browser APIs. The desktop implementation invokes narrow Tauri commands. Feature code does not call `window.__TAURI__` directly outside the adapter.

### 2.2 Rust core responsibilities

- Device authorization flow network calls and token exchange.
- OS keychain reads/writes/deletes.
- Access-token refresh serialization (one refresh at a time per instance).
- Native notifications.
- System tray and menu.
- Global shortcut registration and PTT events.
- Deep-link parsing and validation.
- Window state and single-instance handling.
- Auto-update check/download/install with signature verification.
- Diagnostics collection and log redaction.
- Opening external URLs in the system browser after allowlist validation.

The Rust core does not contain business logic for communities, messages, permissions, or voice authorization. Those remain on the server.

### 2.3 Frontend responsibilities

- Rendering and interaction.
- REST and WebSocket clients, using access tokens obtained from the platform adapter.
- LiveKit voice/video.
- Local UI state, drafts, device preferences.
- Never persisting refresh tokens, passwords, or private message caches in webview storage.

---

## 3. Authentication and credentials

### 3.1 Device authorization

Production desktop login uses the device authorization flow:

1. The webview asks the Rust core to start login for an instance.
2. Rust calls `POST /auth/device/code`.
3. Rust opens the verification URI in the system browser.
4. The webview displays the user code and a copy button.
5. After browser approval, Rust polls `POST /auth/device/token`.
6. Rust stores the refresh token in the OS keychain and returns only the access token and user summary to the webview.
7. The webview keeps the access token in memory.

The desktop never stores the refresh token in `localStorage`, IndexedDB, cookies, logs, or crash reports.

### 3.2 Refresh

- The webview calls the platform adapter's `refresh(instanceUrl)` when a request returns `401`.
- Rust serializes refreshes per instance with a mutex/single-flight queue.
- Rust reads the refresh token from the keychain, calls `/auth/refresh` with `{refresh_token, client: "desktop"}`, stores the rotated token, and returns the new access token.
- If the refresh fails with `session_expired` or `session_revoked`, Rust deletes the keychain entry and tells the webview to log out.
- If the keychain is unavailable, the app does not persist the token; the user signs in again on each launch.

### 3.3 Keychain

| Platform | Backend | Notes |
|---|---|---|
| Windows | Credential Manager | Per-instance entry, service name `Tulpar`, account = instance URL |
| Linux | Secret Service via libsecret | GNOME Keyring/KWallet; may require a running secret service |
| Linux without Secret Service | No persistence | Show a clear warning; never fall back to a plaintext file |

Multiple instances have separate keychain entries. Removing an instance from the client asks whether to revoke the session server-side and delete the keychain entry.

### 3.4 Instance switching

- The instance registry stores only URLs, display names, and non-sensitive preferences in local storage.
- Switching instances loads that instance's access token from memory or refreshes it via Rust.
- Each instance has an independent keychain entry, session, cache, and gateway connection. Switching does not merge accounts or data.
- Caches and Zustand stores are keyed by instance URL.
- A notification click opens the correct instance and channel.
- Multiple desktop windows, if added later, must not share a renderer or memory cache across instances without explicit isolation.

---

## 4. Native features

### 4.1 System tray

- Tray icon with unread indicator and a menu:
  - Open Tulpar.
  - Mute/unmute notifications.
  - Voice status (if connected): mute, deafen, leave.
  - Quit.
- Closing the window hides to tray by default; this is configurable.
- “Quit” stops voice and exits fully.
- Linux tray support depends on the desktop environment and AppIndicator extension. If unavailable, fall back to a normal window and document the limitation.

### 4.2 Native notifications

- Use the Tauri notification plugin.
- Windows: toast notifications with the app's AppUserModelID; respect Focus Assist.
- Linux: libnotify/desktop notification daemon; respect Do Not Disturb.
- Notification permission is requested in context, not at first launch.
- Clicking a notification opens the channel/message; the route is validated against known IDs.
- Notification content can be minimized in settings (show sender only, hide message preview).
- Do not send notifications for muted channels/communities or for the currently focused channel.

### 4.3 Global push-to-talk

- Use the Tauri global shortcut plugin where supported.
- Registration is explicit; conflicts are reported to the user with the conflicting shortcut if the OS exposes it.
- Windows: works globally for the application.
- Linux X11: generally works.
- Linux Wayland: compositor-dependent; many compositors restrict global shortcuts. The app falls back to an in-app shortcut and explains why.
- PTT only unmutes while held; it does not bypass server mute.
- A global shortcut does not inject keystrokes into other applications.

### 4.4 Deep links

Register `tulpar://`:

- `tulpar://add-instance?url=https://tulpar.example.com`
- `tulpar://invite/<code>?instance=https://tulpar.example.com`
- `tulpar://channel/<instance>/<community>/<channel>`

Rules:

- Parse and validate the scheme, host, and URL.
- Allow only `https` instance URLs (localhost development may be allowed in debug builds).
- Never auto-authenticate from a deep link.
- Show a confirmation before joining a community or adding an instance.
- Reject unknown paths and overly long inputs.
- A second launch with a deep link is forwarded to the existing instance (single-instance plugin).

### 4.5 Window state

- Persist size, position, maximized/fullscreen, and monitor.
- Restore only if the monitor still exists; otherwise center on the primary display.
- Do not restore off-screen windows.
- Respect the OS scale factor.

### 4.6 Autostart

- Optional “Start Tulpar on login”.
- Starts minimized to tray if the user chooses.
- No auto-join to voice and no microphone/camera activation on startup.

### 4.7 Auto-update

- Tauri updater with signed update manifests.
- Stable and beta channels; the channel is a user setting.
- Signature verification is mandatory; an unsigned update is rejected.
- Update checks are explicit/periodic but never install without user consent unless the user enabled automatic installs.
- Release notes are shown before install.
- Downgrades are not offered through the updater.
- The signing private key is held offline; public keys are embedded in the app.

### 4.8 Diagnostics and crash reporting

- Local diagnostic logs with rotation and redaction.
- “Export diagnostics” creates a zip of logs and system info after showing the contents; no automatic upload.
- Crash reporting is opt-in and points to a configurable, self-hostable endpoint. It is disabled by default and not required for the app to function.
- Never include message content, tokens, passwords, email addresses, or full IP addresses in diagnostics.

---

## 5. Voice and video on desktop

The desktop client uses the same LiveKit SDK and voice UI as the web client, with these differences:

- Native global PTT where supported.
- OS keychain for session persistence.
- Native notifications for mentions and calls (voice channel joins).
- Device enumeration through the WebView; verify WebView2/WebKitGTK behavior.
- Screen capture depends on the WebView. See [VIDEO.md](VIDEO.md#9-desktop-specifics).
- Audio processing is provided by the WebView's WebRTC stack; it may differ from Chrome.
- No background media service. Closing the app or quitting stops voice.

The desktop app does not bundle LiveKit or Coturn. It connects to the instance's configured media server.

---

## 6. Security model

### 6.1 Tauri capabilities

- Use Tauri 2 capabilities with the minimum allowlist.
- Do not enable shell execution, arbitrary filesystem access, arbitrary URL opening, or remote content.
- The webview loads only bundled assets from the app's custom protocol.
- `dangerousDisableAssetCspModification` is not used.
- IPC commands validate every argument and are scoped to the calling window.

### 6.1 Desktop origin and CORS

The Tauri webview uses a custom scheme; the exact `Origin` value depends on the platform (commonly `tauri://localhost` or `http://tauri.localhost`). API calls from the webview are cross-origin relative to the instance.

Phase 7 must choose one of these, verified per platform:

1. Add the exact desktop origins to the instance's CORS allowlist (Bearer tokens are still required; CORS is not an authorization boundary).
2. Route API calls through the Tauri Rust HTTP client via IPC, so the webview does not make cross-origin requests. This adds IPC surface and must validate every request.

The first option is simpler; the second avoids CORS entirely. Either way, the gateway `Origin` validation must explicitly support the chosen desktop origin. No wildcard CORS rule is acceptable.

PWA service-worker registration is disabled in desktop mode; the app is updated through the signed Tauri updater, not a web cache.

### 6.2 Content Security Policy

- Strict CSP with no remote scripts.
- `connect-src` limited to configured instance URLs, LiveKit WSS, and update endpoints.
- `img-src` allows the instance and its user-content origin, plus local blobs for previews.
- `media-src` allows `blob:` for local capture.
- No `unsafe-eval` in production.
- No loading remote HTML in an iframe.

### 6.3 Update and release security

- Code signing for Windows binaries (Authenticode) where a certificate is available.
- Linux packages signed or distributed with checksums; AppImage signature verification where supported.
- Update manifests signed with a Tauri updater key.
- Builds are reproducible enough to compare artifacts; CI uses pinned toolchains and lockfiles.
- No secrets are embedded in the client. LiveKit tokens are short-lived and per-session.

### 6.4 Deep link and external URL safety

- Only `https` instance URLs.
- Allowlist `openExternal` to `https`/`http` and `mailto` (mailto only from explicit UI).
- Never open a URL supplied by a message without user confirmation and scheme validation.
- Prevent command injection by never invoking a shell with URL strings.

### 6.5 Credential safety

- Refresh tokens only in the OS keychain.
- Access tokens only in memory.
- No tokens in logs, analytics, crash reports, or deep links.
- Logout clears keychain, memory, and per-instance caches.
- A compromised renderer can read the access token in memory; CSP, no remote content, and least-privilege IPC reduce the attack surface. Refresh tokens remain outside the renderer.

---

## 7. Build and release

### 7.1 Targets

| Platform | Formats | Notes |
|---|---|---|
| Windows 10/11 x64 | NSIS installer, MSI | WebView2 runtime bootstrapper; min Windows 10 1809 |
| Windows arm64 | Optional | Build only if tested |
| Linux x64 | AppImage, `.deb` | Build on the oldest supported distro for glibc compatibility |
| Linux arm64 | Optional | Build only if tested |

Tauri 2 supports these bundles. Packaging choices are finalized in Phase 7 after test builds.

### 7.2 CI

- Windows build on `windows-latest`.
- Linux build on Ubuntu 22.04 for compatibility, tested on Ubuntu 24.04.
- Rust formatting, clippy, tests.
- Frontend typecheck, lint, unit tests.
- Tauri capability validation.
- Artifact checksum and signature.
- No code signing certificate in PR builds; release builds use protected secrets.

### 7.3 Release channels

- `stable`: tested, signed, recommended.
- `beta`: early access, same signature policy, may include schema-compatible server features only.
- The server advertises a minimum client version; outdated clients show an upgrade prompt but are not silently blocked unless a security issue requires it.

---

## 8. Platform limitations

### Windows

- WebView2 runtime must be present; the installer can bootstrap it.
- Focus Assist and notification settings can suppress toasts.
- Global shortcuts work while the app runs; they do not work if the app is not running.
- Antivirus/EDR may inspect or block installers; code signing reduces warnings.
- ARM64 support depends on WebView2 and native dependencies.

### Linux

- WebKitGTK versions differ across distributions; test Ubuntu 24.04, Fedora, and a Debian stable release.
- Wayland restricts global shortcuts and screen capture; X11 behavior differs.
- Tray icons require AppIndicator support in many environments.
- Secret Service may be unavailable in minimal window managers; no insecure fallback.
- Audio device names and defaults vary by PipeWire/PulseAudio configuration.
- AppImage sandboxing and FUSE requirements vary.
- Packaging for every distribution is out of scope; provide AppImage and deb initially.

### Both

- No background voice when the app is fully quit.
- No native mobile app.
- Screen capture depends on the WebView, not the Tauri shell.
- Auto-update requires network access to the update endpoint and a valid signature.
- Notifications require OS permission and may be delayed by power saving.

---

## 9. Test requirements

- Device authorization: approve, deny, expire, slow poll, keychain unavailable.
- Refresh rotation and single-flight behavior; revoked session clears credentials.
- Multi-instance switching and cache isolation.
- Tray behavior, close-to-tray, quit, and unread indicator.
- Notification permission, click routing, mute settings, Focus Assist/DND.
- Global PTT registration, conflict, release, and fallback on Wayland.
- Deep-link parsing: valid, invalid scheme, private IP, overly long, duplicate instance.
- Window state restore across monitor changes.
- Auto-update signature rejection and successful update on a test channel.
- Diagnostics redaction.
- Voice join/leave, device switch, camera, screen-share capability detection.
- Windows 10/11 and Ubuntu 24.04 (X11 and Wayland) manual smoke tests.
- Upgrade from the previous release without losing local preferences.

---

## 10. Open desktop decisions

| Decision | Current position | Needed by |
|---|---|---|
| Code signing certificate | Not acquired; required for a polished Windows release | Phase 7 |
| Linux packaging | AppImage + deb first | Phase 7 |
| Updater hosting | Instance-independent static endpoint or GitHub Releases; decide | Phase 7 |
| Crash reporting backend | Opt-in, self-hostable; no default vendor | Phase 7 |
| Native screen capture | Not in v1; browser fallback | Phase 7 |
| Tray on Linux | Best-effort with fallback | Phase 7 |
| macOS | Out of scope v1 | Future |
| Auto-start default | Off | Phase 7 |
