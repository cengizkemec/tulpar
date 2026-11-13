# @tulpar/desktop

Tauri 2 desktop shell for Tulpar on Windows and Linux. It embeds the shared `apps/web`
build and adds native capabilities through a narrow Rust command surface.

## Implemented

- Desktop server address entry and saved-instance registry.
- Device authorization with browser approval at the instance `/device` page.
- Refresh tokens in the OS keychain; access tokens remain in webview memory.
- Serialized refresh and keychain cleanup on logout/revocation.
- System tray, close-to-tray, native notifications, and unread tooltip.
- Global push-to-talk shortcut and `ptt` events.
- `tulpar://add-instance` deep links, single-instance forwarding, and autostart.
- Window-state persistence and capability reporting.

## Prerequisites

- Rust toolchain with the MSVC target on Windows.
- WebView2 runtime (included with current Windows 10/11).
- Web dependencies installed with `pnpm install` at the repository root.

## Commands

```bash
pnpm --filter @tulpar/web build
pnpm --filter @tulpar/desktop exec tauri build --debug --no-bundle
pnpm --filter @tulpar/desktop dev
```

The debug executable is written to
`apps/desktop/src-tauri/target/debug/tulpar-desktop.exe`.

## Verification

`tests/web-e2e/desktop.mjs` drives the Windows debug build through WebView2 remote
debugging and verifies device approval, keychain refresh, PTT registration, capabilities,
and deep-link forwarding.

## Release follow-ups

Linux packaging, code signing, updater signing keys/endpoints, diagnostics export, and
notification-click routing are not configured yet. See `docs/DESKTOP.md` and
`docs/PHASE_7_REPORT.md`.
