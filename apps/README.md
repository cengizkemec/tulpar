# Applications

- `web` — the canonical React/TypeScript SPA and PWA. It contains the full user and admin experience.
- `desktop` — the Tauri 2 shell for Windows and Linux. It wraps the web build and adds native capabilities through a narrow platform adapter.
- `embed-demo` — a static demo site for the embed SDK and widget documentation.

`apps/web` now contains the React/Vite client foundation: authentication, `/sync`, community/channel navigation, text messaging, the WebSocket gateway, and LiveKit voice/video/screen controls. The desktop shell and embed demo remain future phases. See [docs/WEB.md](../docs/WEB.md), [docs/DESKTOP.md](../docs/DESKTOP.md), and [docs/EMBED_SDK.md](../docs/EMBED_SDK.md).
