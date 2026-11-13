# Phase 7 Report — Tulpar Desktop

**Status:** Windows foundation complete and smoke-tested. Linux packaging, signed releases, updater endpoints, diagnostics/crash reporting, and the full platform matrix remain release follow-ups.

**Date:** 2026-11-13

---

## 1. Implemented

### Tauri 2 shell

- `apps/desktop` is a pnpm workspace package with the Tauri CLI.
- `apps/desktop/src-tauri` embeds the shared `apps/web/dist` build; there is no forked UI.
- Strict CSP allows bundled assets, IPC, HTTPS/WSS, and localhost development endpoints.
- The web build disables PWA service-worker registration in Tauri mode and removes any previously registered worker/cache.

### Desktop adapter and server selection

- The web app detects Tauri through `@tauri-apps/api/core`.
- Desktop-only screens:
  - server address entry and saved-instance registry;
  - device authorization with a user code, copy button, and browser approval link;
  - desktop settings for global PTT and autostart.
- The web client still connects to its own origin; `/connect` redirects to `/app` outside Tauri.
- Device approval is available to browsers at `/device`, so the desktop can open the instance's normal web origin.

### Authentication and credentials

- Device authorization commands call `/api/v1/auth/device/code` and `/api/v1/auth/device/token`.
- The refresh token is stored only in the OS keychain (`Tulpar` service, per-instance account).
- Access tokens remain in webview memory.
- Refresh is serialized in Rust and rotates the keychain entry.
- Logout revokes the server session with the stored refresh token and deletes the keychain entry.
- A failed refresh with `invalid_token`, `session_expired`, or `session_revoked` deletes the stored credential.

### Native features

- System tray with Open/Quit, close-to-tray, and unread tooltip updates.
- Native notifications through the Tauri notification plugin.
- Global PTT registration with conflict reporting; the Rust core emits `ptt` down/up events and the voice panel applies them only while connected.
- Deep links: `tulpar://add-instance?url=...` is forwarded to the existing instance, prefilled on the connect screen, and requires user confirmation.
- Autostart enable/disable through the autostart plugin.
- Single-instance forwarding for second launches.
- Window-state persistence through the window-state plugin.
- Capability reporting for keychain, global shortcuts, and tray.
- The updater plugin is configured, but endpoints and signing keys are intentionally unset until release engineering.

### Server changes

- CORS validation now accepts the `tauri://` custom-protocol origin.
- Development Compose allows the Windows and Linux desktop origins.
- Desktop REST calls use Bearer tokens; no wildcard CORS rule was added.

---

## 2. Verification

The following was run on Windows 11 with WebView2 and the Docker development stack.

```text
cargo check (apps/desktop/src-tauri)                    -> pass
tauri build --debug --no-bundle                         -> pass
node tests/web-e2e/desktop.mjs                          -> pass
```

`desktop.mjs` verifies:

- server probe and instance selection;
- device code creation;
- browser login and `/device` approval;
- desktop login after approval;
- keychain session presence;
- refresh after a full reload;
- global PTT registration and unregistration;
- capability reporting;
- deep-link forwarding to `/connect?url=...` with the URL prefilled.

Additional manual checks:

- close-to-tray keeps the process alive and hides the window;
- F8 global PTT emits `down` and `up` states;
- native notification command succeeds;
- autostart can be enabled and disabled;
- invalid `javascript:` URLs are rejected by `desktop_open_external`.

---

## 3. Known gaps

1. Linux packaging and WebKitGTK/Secret Service verification are pending.
2. Windows 10 and Ubuntu 24.04 install matrices are not recorded.
3. Code signing, updater signing keys, release channels, and signed manifests are not configured.
4. Local diagnostics export and a crash log are implemented; opt-in crash upload/reporting is not.
5. Notification-click routing to a specific channel/message is not implemented.
6. The updater UI is not exposed until a signed release feed exists.
7. Screen capture and device behavior are verified through the WebView only on Windows.
8. Real-device tray/AppIndicator behavior on Linux is untested.

---

## 4. Next steps

1. Configure the updater public key, release endpoint, and signed artifacts.
2. Add Linux CI packaging (AppImage/deb) and test on Ubuntu 24.04/X11 and Wayland.
3. Add diagnostics export with redaction and opt-in crash reporting.
4. Add notification-click routing and unread tray-icon variants.
5. Run Windows 10/11 install/uninstall and upgrade-preservation tests.
