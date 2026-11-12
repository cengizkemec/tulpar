# Tulpar Video and Screen Sharing

**Status:** Phase 0 design. Not implemented.

Video and screen sharing use LiveKit tracks over WebRTC. Tulpar authorizes the tracks, observes participant state through signed LiveKit webhooks, and never proxies media through the Rust backend.

---

## 1. Goals and non-goals

### 1.1 Goals

- Camera video in voice channels, subject to `STREAM_VIDEO`.
- Screen and window sharing, subject to `SHARE_SCREEN`.
- Optional screen-share audio where the platform supports it.
- Simulcast and adaptive bitrate for varied networks.
- Device selection, mute, resolution/frame-rate controls, and network indicators.
- Multiple layouts: grid, focused speaker, pinned participant, fullscreen.
- Graceful capability detection and honest fallbacks.

### 1.2 Non-goals for v1

- Server-side recording, transcoding, or compositing.
- Virtual backgrounds or background blur (may be added as a client feature later).
- 4K or 60fps defaults.
- Broadcasting to external platforms.
- Video in DMs.
- End-to-end encrypted video.
- Guaranteed screen capture on every Linux desktop environment.
- Native capture pipelines that bypass the platform WebView unless a platform requires it.

---

## 2. Architecture

```
Camera / screen ──► WebRTC sender ──► LiveKit SFU ──► subscribers
        ▲                                  │
        │                                  │ signed webhooks
   local permissions,                     ▼
   device selection                 Tulpar projection
```

- The client captures media with `getUserMedia` (camera/microphone) or `getDisplayMedia` (screen/window).
- LiveKit publishes each source as a separate track.
- Subscribers receive the tracks they are allowed to see.
- Tulpar issues the token with `canPublishSources` restricted by permissions.
- Tulpar does not see or store media frames.
- LiveKit handles simulcast layers, bandwidth estimation, packet loss, and jitter.

No media is routed through the Tulpar API server, PostgreSQL, Redis, or object storage.

---

## 3. Permission mapping

| Source | Permission | LiveKit grant |
|---|---|---|
| Microphone | `SPEAK` | `microphone` |
| Camera | `STREAM_VIDEO` | `camera` |
| Screen/window | `SHARE_SCREEN` | `screen_share` |
| Screen audio | `SHARE_SCREEN` | `screen_share_audio` |

A user with `CONNECT_VOICE` but no `STREAM_VIDEO` can join and watch but cannot publish camera. The UI hides the camera control and the token omits the source.

Server mute/deafen affects audio only. Camera and screen tracks are controlled by the user's own publish permission and by moderation actions that remove the participant from the room.

---

## 4. Camera video

### 4.1 Defaults

| Setting | Default | Allowed range (UI) |
|---|---|---|
| Resolution | 720p (1280×720) | 360p, 480p, 720p, 1080p |
| Frame rate | 30 fps | 15, 24, 30 |
| Max bitrate | 2.5 Mbps at 720p/30 | 0.5–4 Mbps |
| Simulcast | Enabled | On by default |
| Codec | Negotiated (VP8/H.264 baseline preferred) | No manual codec picker in v1 |

The actual resolution can be lower due to camera capabilities, CPU, and bandwidth. The UI shows the effective resolution and bitrate.

### 4.2 Camera switching

- Enumerate video inputs with `enumerateDevices`.
- Switch by replacing the track; avoid reconnecting the room.
- If the selected camera disappears, fall back to the default and show a notice.
- Permission denial is a first-class state with instructions, not a silent failure.

### 4.3 Camera privacy

- Camera publishing never starts automatically on joining a voice channel.
- A visible indicator shows when the camera is on.
- Leaving the channel stops all local tracks.
- The app does not capture when the window is closed or the user is disconnected.

---

## 5. Screen and window sharing

### 5.1 Browser flow

1. User clicks “Share screen”.
2. Client checks `SHARE_SCREEN`; the server also enforces it at token issuance.
3. `getDisplayMedia({ video, audio })` is called from a user gesture.
4. The browser shows its own picker (screen, window, tab).
5. The selected track is published as `screen_share`.
6. If the browser provides system audio, it is published as `screen_share_audio`.
7. The user can stop sharing at any time; the track ends and the participant updates.

The app cannot choose the surface on the user's behalf; that is a browser security rule. The app may pass hints (`displaySurface: "monitor"`), but the user decides.

### 5.2 Quality controls

| Setting | Default | Notes |
|---|---|---|
| Resolution | 1080p | Content-dependent; text needs resolution, motion needs frame rate |
| Frame rate | 30 fps | 15 fps for static documents, 30 for video |
| Content hint | `detail` | `motion` when sharing video |
| Max bitrate | 3 Mbps | Capped to protect the SFU and network |
| Simulcast | Enabled | Improves viewer experience on weak networks |

The user can switch between “Text/detail” and “Motion” presets. The UI does not expose raw SDP controls.

### 5.3 Screen-share audio

- Windows Chrome/Edge commonly support system/tab audio; Firefox and Safari vary.
- Linux/Wayland depends on the PipeWire portal and browser; system audio may be unavailable.
- If screen audio is not available, the app shares video only and says so.
- Screen audio is published only when `SHARE_SCREEN` is granted.

### 5.4 Stopping

- The browser “Stop sharing” control ends the track; the client must handle the `ended` event and update the UI.
- Leaving the channel, closing the app, or losing the token stops publishing.
- A moderator disconnecting the participant stops the stream.

---

## 6. Layouts

| Layout | Behavior |
|---|---|
| Grid | Equal tiles up to a limit; overflow shows the most active/recent participants |
| Focus | One large participant/stream, others as thumbnails |
| Pinned | A user pins one participant or screen for themselves; local preference |
| Fullscreen | Fullscreen the focused tile; `Esc` exits |
| Screen-share focus | Screen share becomes primary; camera tiles move to a strip |

Rules:

- Layout preference is local and persisted per device.
- Pinning is local; it does not force other users' layouts.
- The grid virtualizes above 12 tiles to protect rendering performance.
- Speaking indicators come from LiveKit active speakers.
- Video is not rendered for off-screen tiles; subscriptions may be paused to save bandwidth.

---

## 7. Adaptation and network quality

- LiveKit simulcast publishes multiple spatial layers; subscribers receive a layer appropriate to their bandwidth.
- Dynacast can pause unused layers.
- The client displays a connection-quality indicator from LiveKit: excellent, good, poor, lost.
- On poor quality, the client can:
  - reduce the number of subscribed video tiles;
  - lower its own publish resolution/fps;
  - disable camera while keeping screen share, or vice versa.
- The app does not silently turn off a user's camera without a visible state change.
- Audio takes priority over video. Video may be paused to protect voice quality.

---

## 8. Device and capability detection

At startup and before publishing, detect:

- `navigator.mediaDevices.getUserMedia` availability.
- `navigator.mediaDevices.getDisplayMedia` availability.
- `navigator.mediaDevices.enumerateDevices`.
- `HTMLMediaElement.setSinkId` for output selection.
- Secure context (`https` or localhost).
- Permission state where the browser exposes it.
- Available cameras/microphones.
- Screen-capture capability on the current platform.

The UI shows a specific message for each unsupported feature. It never shows a control that is known not to work.

### 8.1 Capability matrix (to be verified in Phases 5 and 7)

| Capability | Chrome/Edge | Firefox | Safari | Tauri Windows (WebView2) | Tauri Linux (WebKitGTK) |
|---|---|---|---|---|---|
| Camera | Yes | Yes | Yes | Verify | Verify |
| Microphone | Yes | Yes | Yes | Verify | Verify |
| Screen share | Yes | Yes | Varies | Verify `getDisplayMedia` | Often limited; verify by distro |
| Screen audio | Common | Varies | Varies | Verify | Usually unavailable |
| Window share | Yes | Yes | Varies | Verify | Verify |
| Output device selection | Yes | Partial | Partial | Verify | Verify |
| Background voice | Limited | Limited | Limited | Native window | Native window |

This table is a planning aid, not a promise. Phase 5/7 must test real devices and record the results. Where a WebView cannot capture, the desktop app may open the instance in the system browser for screen sharing; this is an explicit, documented fallback.

---

## 9. Desktop specifics

- Windows uses WebView2; its `getDisplayMedia` behavior and permission prompts must be tested with the target WebView2 runtime.
- Linux uses WebKitGTK; screen capture support varies by version, distribution, X11 vs Wayland, and the PipeWire portal.
- Tauri's native capabilities do not automatically grant WebRTC capture. Do not promise native capture unless a tested plugin/native pipeline is implemented.
- If the WebView cannot capture:
  - show a clear explanation;
  - offer “Open in browser” for screen sharing;
  - keep camera/microphone working if supported.
- Global push-to-talk is handled by Tauri; video is not affected.
- Native notifications for incoming calls are not applicable in v1 (no DM calls).

A future native capture pipeline would need to feed frames into LiveKit without bypassing Tulpar's authorization. That is a separate project and not part of v1.

---

## 10. Security

- Video publishing requires a valid short-lived LiveKit token with the correct source grant.
- A user cannot publish a source omitted from the token; LiveKit enforces this.
- Screen sharing always requires a user gesture and a visible indicator.
- No automatic screen capture on join, reconnect, or app start.
- A moderator can disconnect a participant; this stops all tracks.
- No recording in v1. If recording is added, it requires explicit consent, a visible indicator, an audit record, and a retention policy.
- Streams are visible only to participants in the same LiveKit room who can subscribe. Tulpar does not create public stream URLs.
- The LiveKit API secret is never exposed.
- Screen-share content may contain sensitive information; the UI warns before sharing and shows a persistent indicator.
- No E2EE in v1.

---

## 11. Performance

- Virtualize participant tiles and pause off-screen video subscriptions.
- Default to 720p/30 for camera, 1080p/30 for screen share with a bitrate cap.
- Prefer simulcast; do not publish a single high-bitrate layer by default.
- On low-end devices, reduce the grid size and default camera to 480p/24.
- Avoid React re-renders on every audio level update; speaking state is throttled.
- Use `requestVideoFrameCallback` or LiveKit stats sparingly; do not poll at high frequency.
- Test with 20 publishers and 50 subscribers on the reference hardware; record CPU, bandwidth, and dropped frames.

Video is bandwidth-heavy. A 20-publisher room at 720p can consume tens of Mbps per subscriber without simulcast limits. The SFU and network must be sized accordingly.

---

## 12. Test matrix

### 12.1 Functional

- Camera on/off, device switch, permission denial.
- Screen share start/stop via app and browser UI.
- Window vs full-screen share.
- Screen audio where supported; graceful absence elsewhere.
- Layout switch, pin, fullscreen, virtualized grid.
- Network quality indicator and adaptation.
- Moderator disconnect stops tracks.
- Token without `STREAM_VIDEO` cannot publish camera.
- Token without `SHARE_SCREEN` cannot publish screen.

### 12.2 Platforms

- Windows 10/11: Chrome, Edge, Firefox, Tulpar Desktop.
- Linux: Chrome, Firefox, Tulpar Desktop on X11 and Wayland (Ubuntu 24.04, Fedora current).
- Safari on macOS/iOS for the web client where available (screen share limitations expected).
- Mobile browsers: camera works where permitted; screen share is usually unavailable; document.

### 12.3 Network

- UDP blocked → TURN/TCP or TURN/TLS.
- Packet loss and jitter (network emulation).
- Bandwidth drop while publishing.
- Reconnect during a screen share.
- SFU restart.

Real media tests are required. Mocked tracks and unit tests are not sufficient for Phase 5 completion.

---

## 13. Limitations

- Browser screen capture requires a secure context and a user gesture.
- The browser, not the app, chooses the shared surface.
- System audio capture is inconsistent across OS/browser combinations.
- Wayland screen capture depends on the portal and compositor.
- WebView2/WebKitGTK screen capture support must be verified; do not assume it matches Chrome.
- iOS/iPadOS screen sharing and background video are restricted.
- No E2EE, recording, or server-side transcoding in v1.
- Video quality is constrained by the weakest link: camera, CPU, uplink, SFU, or viewer downlink.

---

## 14. Open video decisions

| Decision | Current position | Needed by |
|---|---|---|
| Desktop screen capture implementation | Capability-gated WebView capture; browser fallback | Phase 7 |
| Codec preference | Let LiveKit negotiate; prefer VP8/H.264 | Phase 5 |
| Max video publishers per channel | Configurable; default 20 | Phase 5 |
| Virtual backgrounds | Client-side, later | Post-v1 |
| Recording | Not in v1 | Future |
| E2EE | Not in v1 | Future |
| Screen-share audio on Linux | Best-effort; document per distro | Phase 5 |
| Mobile web video | Camera best-effort; screen share generally unavailable | Phase 6 |
