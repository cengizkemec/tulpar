# Tulpar Voice

**Status:** Phase 4 backend and transport verification complete: token issuance, TURN credentials, room policy, signed webhooks, Redis-backed projection with reconciliation, moderation, and revocation. Two real Edge clients hear each other directly and through forced TURN relay; client UI/device foundations remain Phase 6 work.

**Media server:** LiveKit SFU, deployed independently from the Rust backend.

**NAT traversal:** Coturn for STUN/TURN, with ephemeral credentials.

---

## 1. Goals and non-goals

### 1.1 Goals

- Low-latency voice channels with join/leave, mute, deafen, push-to-talk, and voice activity detection.
- Multiple voice channels per community, with capacity and optional room passwords.
- Backend-enforced join/speak/video/screen permissions.
- Short-lived LiveKit tokens that never expose the API secret.
- Server-side moderation: mute, deafen, move, disconnect.
- Clear reconnection and network-quality behavior.
- Real TURN fallback for restrictive networks.
- Honest platform limitations for browser and desktop clients.

### 1.2 Non-goals for v1

- Recording, transcription, or streaming to external services.
- End-to-end encrypted voice.
- Custom SFU, custom codec, or custom jitter buffer.
- Telephone/PSTN integration.
- DM voice calls. Voice is scoped to community channels in v1; DM calls are a later feature.
- Stage/audience channels with speaker queues.
- Server-side audio mixing or music bots.
- Guaranteed connectivity on every corporate network.

---

## 2. Architecture

```
┌──────────────┐        WebRTC media         ┌──────────────────┐
│ Web / Desktop│◄───────────────────────────►│  LiveKit SFU     │
│ LiveKit SDK  │                             │  room per channel│
└──────┬───────┘                             └────────┬─────────┘
       │ REST: join, token, moderation                │ signed webhooks
       ▼                                              ▼
┌───────────────────────────────────────────────────────────────┐
│ Tulpar Server                                                 │
│ permissions, room policy, token minting, projection, audit    │
└──────────────┬────────────────────────────────────────────────┘
               │
      ┌────────▼────────┐        ┌──────────────────┐
      │ Redis           │        │ Coturn           │
      │ presence/voice  │        │ STUN/TURN relay  │
      │ projection      │        └──────────────────┘
      └─────────────────┘
```

Tulpar backend responsibilities:

- Authenticate the user.
- Check community membership, bans, timeouts, and voice permissions.
- Verify room password and capacity.
- Mint a short-lived LiveKit token with least privilege.
- Generate ephemeral TURN credentials.
- Project participant state from signed LiveKit webhooks.
- Enforce moderation by calling the LiveKit server API.
- Revoke access when permissions or membership change.

LiveKit responsibilities:

- WebRTC signaling.
- Media routing, simulcast, jitter/packet-loss handling.
- Room lifecycle.
- Participant connection state.

LiveKit is never exposed with its API secret. The secret stays on the Tulpar server and the LiveKit server configuration.

---

## 3. Room model

- One Tulpar voice channel maps to one LiveKit room: `tulpar_channel_{channel_id}`.
- The room name is derived server-side; clients cannot choose an arbitrary room.
- Channel IDs are UUIDs; a room name is safe to expose in a token but is not an authorization mechanism.
- A voice channel belongs to exactly one community.
- Room metadata is minimal: channel ID, community ID, creation time. No roles or permissions are trusted from metadata.
- The LiveKit participant identity is the Tulpar user UUID. Display name is the current community nickname or display name, resolved at token issuance.
- Room capacity is `channels.user_limit` (0 or null = no explicit limit). The backend also configures a hard maximum to protect the SFU.

### 3.1 Room password

- Optional per voice channel; stored as an Argon2id hash in `channels.voice_password_hash`.
- The password is verified by Tulpar, never by LiveKit.
- The token endpoint requires the password when set.
- Password attempts are rate limited per user/channel and per IP.
- A wrong password and a missing channel both return a generic `403`/`404` response designed not to leak private channel existence.
- `MANAGE_CHANNELS` can set, rotate, or remove the password. The password is never returned by the API.

---

## 4. Join flow

```
Client → POST /api/v1/channels/{channel_id}/voice/token
          { "password": "optional" }

Tulpar:
  1. authenticate
  2. load channel, community, membership
  3. reject if not a voice channel / not visible
  4. check CONNECT_VOICE
  5. reject if banned or community membership missing
  6. check timeout: may join as listener, cannot SPEAK
  7. check room capacity projection
  8. verify password if set (rate limited)
  9. check LiveKit configuration is present
 10. mint a short-lived LiveKit JWT + ephemeral TURN credentials
 11. audit the join intent, return token
Client → LiveKit connect
LiveKit → Tulpar signed webhook (participant_joined)
Tulpar → Redis projection + VOICE_STATE_UPDATE
```

The REST response is:

```json
{
  "url": "wss://rtc.example.com",
  "room": "tulpar_channel_0192f...",
  "token": "eyJ...",
  "expires_at": "2026-11-12T12:10:00Z",
  "ice_servers": [
    {
      "urls": ["stun:turn.example.com:3478"],
      "username": null,
      "credential": null
    },
    {
      "urls": ["turns:turn.example.com:5349"],
      "username": "1760003600:0192f...",
      "credential": "base64-hmac"
    }
  ]
}
```

The token is valid for 10 minutes by default. It is a connection credential, not a session. If the client reconnects after expiry, it requests a new token through the same endpoint, which re-checks permissions.

### 4.1 Failure responses

| Condition | Response |
|---|---|
| Channel not found or not visible | `404 not_found` |
| Not a member | `404 not_found` (do not leak private community structure) |
| Banned | `403 permission_denied` |
| Missing `CONNECT_VOICE` | `403 permission_denied` |
| Timed out for speaking | Token issued as listener; `SPEAK` omitted |
| Wrong/missing room password | `403 permission_denied`, rate limited |
| Room full | `409 room_full` |
| LiveKit not configured/down | `503 voice_unavailable` |
| Rate limited | `429 rate_limited` |

---

## 5. LiveKit token

### 5.1 Claims

```json
{
  "iss": "<livekit-api-key>",
  "sub": "<user-uuid>",
  "name": "Ada",
  "nbf": 1760000000,
  "exp": 1760000600,
  "video": {
    "room": "tulpar_channel_0192f...",
    "roomJoin": true,
    "canPublish": true,
    "canSubscribe": true,
    "canPublishData": false,
    "canPublishSources": ["microphone", "camera", "screen_share", "screen_share_audio"],
    "roomAdmin": false,
    "roomCreate": false,
    "hidden": false
  },
  "metadata": "{\"userId\":\"0192f...\",\"sessionId\":\"0192f...\"}"
}
```

Rules:

- `canPublishSources` is built from permissions:
  - `SPEAK` → `microphone`
  - `STREAM_VIDEO` → `camera`
  - `SHARE_SCREEN` → `screen_share`, `screen_share_audio`
- If `SPEAK` is missing, the microphone source is omitted. The client can still subscribe.
- `canPublish` is true if at least one source is allowed; `canSubscribe` is true for a listener.
- `canPublishData` is false in v1. Tulpar does not use LiveKit data channels for chat.
- `roomAdmin` is false. Moderation is performed by the Tulpar backend through the LiveKit server API, never by granting admin to a client.
- `roomCreate` is false.
- Metadata contains only IDs needed for correlation; it is not an authorization source.
- The token is signed with the LiveKit API secret, which never reaches the client.
- The token is never logged. Access logs redact authorization headers and token fields.

### 5.2 Token lifetime and revocation

- Default TTL: 10 minutes. Configurable between 5 and 15.
- The token only authorizes the initial connection/reconnection. LiveKit does not re-check Tulpar permissions on every packet.
- On kick, ban, role removal, channel deletion, or permission loss, Tulpar:
  1. Calls the LiveKit server API to remove the participant or mute tracks.
  2. Publishes `VOICE_SESSION_ENDED` to the affected gateway connections.
  3. Refuses to issue new tokens.
- If the LiveKit API is unreachable, the participant may remain until the media connection ends, but no new token is issued. This is a bounded window and is documented in the admin UI.
- Deleting a voice channel removes the LiveKit room.
- A user who loses `CONNECT_VOICE` is removed from the room.

### 5.3 Room creation

- LiveKit rooms are created automatically on first join, or explicitly by the backend if configuration requires it.
- The backend sets a hard participant limit and an empty-room timeout.
- Empty rooms are closed by LiveKit after a configured delay; the projection is cleaned up.
- Rooms are not persisted in PostgreSQL. If Redis and LiveKit state are lost, the projection is rebuilt from LiveKit or cleared.

---

## 6. Coturn and TURN

### 6.1 Why Coturn

LiveKit can provide its own TURN, but Tulpar standardizes on Coturn:

- It is a mature, widely deployed TURN server.
- It can run on a separate host/port and be shared by multiple services.
- It gives operators explicit control over relay ranges and TLS.
- It avoids coupling NAT traversal to the SFU vendor.

LiveKit's built-in TURN is disabled in the reference deployment to avoid two competing TURN configurations. Operators may enable it as a fallback, but the Tulpar-issued ICE server list remains authoritative.

### 6.2 Ephemeral credentials

Tulpar and Coturn share a secret (`TULPAR_TURN_SECRET`). The backend generates time-limited credentials:

```
username = "<unix_expiry>:<user_id>"
credential = base64(HMAC-SHA1(secret, username))
```

This is the standard Coturn REST API mechanism. It is not a custom protocol. SHA-1 is used here only because Coturn's REST credential format requires HMAC-SHA1; it is not used for password storage or signatures elsewhere.

Credentials are:

- Valid for 1 hour by default.
- Scoped to the user ID in the username.
- Returned only in the voice token response.
- Never stored in PostgreSQL.
- Regenerated on every join/reconnect.

### 6.3 Ports and TLS

Reference values (adjust to the host):

| Purpose | Port |
|---|---|
| STUN/TURN UDP/TCP | 3478 |
| TURN over TLS | 5349 |
| TURN relay UDP range | 51000–51999 |

- Coturn must know the public IP (`external-ip`) behind NAT.
- TLS certificates are mounted read-only; automatic renewal is handled by the deployment.
- The relay range must be open in the firewall and sized for expected concurrent relayed participants.
- `no-cli`, `fingerprint`, and a restricted realm are enabled.
- TURN credentials are short-lived; static long-term users are not used.

### 6.4 ICE server list

The client receives STUN and TURN entries. The order encourages direct connectivity, then TURN/UDP, then TURN/TCP, then TURN/TLS. The LiveKit SDK/WebRTC stack handles ICE candidate selection; Tulpar does not implement ICE.

---

## 7. Audio controls

### 7.1 Microphone mute

- Local mute stops publishing the microphone track (or mutes it at the sender).
- The UI reflects the local state immediately.
- Server-side mute is separate and is projected from LiveKit/moderation state.
- A server-muted user cannot unmute themselves until a moderator unmutes them.

### 7.2 Deafen

- Local deafen stops playback of all remote audio and usually also mutes the microphone (user preference).
- Server deafen is enforced by the backend via LiveKit; the client shows the state and cannot override it.
- Deafen does not leave the room.

### 7.3 Push-to-talk

- Desktop: configurable global shortcut via Tauri where the OS permits it.
- Web: keyboard shortcut while the app has focus. Browsers cannot register reliable global shortcuts.
- PTT unmutes the microphone only while held; releasing mutes it.
- If `USE_VOICE_ACTIVITY` is denied, the client must use PTT. This is a client policy; the server cannot distinguish PTT from VAD once the microphone track is published. The permission is therefore a UX/policy control, not a hard anti-cheat boundary.
- PTT key conflicts and accessibility alternatives are documented in the UI.

### 7.4 Voice activity detection

- The client uses the LiveKit/WebRTC VAD or an energy threshold consistent with the SDK.
- Speaking indicators come from LiveKit active-speaker events, not from Tulpar WebSocket traffic.
- Users can disable VAD and use PTT.
- Background noise, music, and mechanical keyboards can trigger VAD; sensitivity is configurable.

### 7.5 Devices

- Enumerate input/output devices with `enumerateDevices`.
- Request permission only when the user joins voice or opens device settings.
- Persist the selected device ID locally (non-sensitive).
- Handle device removal and fall back to the system default with a visible notice.
- Output device selection uses `setSinkId` where supported; otherwise the system default is used.
- Per-user volume is a local gain setting, stored locally, never sent to the server.

### 7.6 Audio processing

Enable WebRTC audio processing where supported:

- Echo cancellation.
- Noise suppression.
- Automatic gain control.

The user can disable each. Browser/desktop support differs. Opus is the audio codec. LiveKit handles adaptive bitrate, forward error correction, packet loss, and jitter; Tulpar does not implement these.

---

## 8. Participant state projection

PostgreSQL does not store live voice participants. Redis holds a rebuildable projection:

```
voice:room:{channel_id}          hash: started_at, participant_count
voice:participant:{channel_id}   hash: user_id -> JSON projection
voice:session:{user_id}          hash: channel_id, joined_at
```

Projection fields:

- `user_id`, `identity`, `display_name`, `joined_at`.
- `self_mute`, `self_deaf`.
- `server_mute`, `server_deaf`.
- Published tracks (`microphone`, `camera`, `screen_share`, `screen_share_audio`).
- Connection quality.

Sources of truth:

- LiveKit webhooks update the projection.
- A reconciler periodically lists LiveKit rooms/participants and repairs drift.
- Redis TTLs prevent permanently stale participants if a webhook is lost.
- The client fetches the projection from the REST endpoint or `/sync`; the gateway pushes `VOICE_STATE_UPDATE`.

Webhooks may arrive out of order. Each update includes a timestamp/sequence from LiveKit or is reconciled against the current participant list. A late `participant_left` must not remove a participant who has rejoined; the reconciler resolves conflicts.

---

## 9. LiveKit webhooks

Endpoint:

```
POST /webhooks/livekit
Authorization: <signed LiveKit webhook JWT>
```

Rules:

- Verify the JWT signature with the LiveKit API secret, `iss`, and `exp`.
- Reject replayed event IDs (bounded Redis set or database dedupe).
- Accept only known event types and rooms that map to an existing voice channel.
- Do not trust participant metadata for authorization; look up the user and membership.
- Process idempotently.
- Return `200` quickly; heavy work is queued if needed.
- Rate limit and monitor failed signature attempts.

Events:

- `room_started`, `room_finished`
- `participant_joined`, `participant_left`
- `track_published`, `track_unpublished`
- `participant_connection_aborted`
- `egress_*` (ignored in v1; recording is disabled)

Webhook processing updates Redis and emits `VOICE_STATE_UPDATE` through the outbox/gateway path.

---

## 10. Moderation

| Action | Permission | Implementation |
|---|---|---|
| Server mute | `MUTE_MEMBERS` | Mute the participant's microphone track via LiveKit; project `server_mute` |
| Server unmute | `MUTE_MEMBERS` | Unmute via LiveKit; audit |
| Server deafen | `DEAFEN_MEMBERS` | Prevent audio subscription/playback via LiveKit permissions; project `server_deaf` |
| Move participant | `MOVE_MEMBERS` | Remove from old room and issue a new token for the target channel, or use LiveKit's move API after verification |
| Disconnect | `MOVE_MEMBERS` | Remove participant from the room; close the voice session |
| Kick/ban | `KICK_MEMBERS` / `BAN_MEMBERS` | Remove membership/ban, then remove from voice and close gateway access |

All moderation actions:

- Check hierarchy and permissions on the backend.
- Are rate limited.
- Are audited.
- Emit a visible event to the affected user.
- Cannot target the owner or a member at/above the actor's highest role.

The actor cannot use LiveKit directly to moderate; the API secret is server-side only.

---

## 11. Reconnection and network quality

- LiveKit's SDK handles transient ICE/WebRTC reconnection.
- The client shows `connecting`, `reconnecting`, `connected`, and `failed` states.
- If the LiveKit token expired during a disconnect, the client calls the Tulpar token endpoint again. The backend re-checks permissions before issuing a new token.
- If the user lost access while disconnected, the token request fails and the client exits the room state.
- Network quality is read from LiveKit (excellent/good/poor/lost). The UI shows an indicator and a short explanation; it does not expose raw IP addresses.
- `MUTE_MEMBERS`/`DEAFEN_MEMBERS` state survives reconnection in the projection.
- A full gateway resync does not affect the media connection; voice state is fetched from the projection.

---

## 12. Capacity and performance

Initial reference targets (validate with real media, not mocks):

| Metric | Target |
|---|---|
| Voice participants per LiveKit node | 100 concurrent |
| Video publishers per node | 20 at 720p/30fps |
| Join token issuance p95 | < 150 ms |
| Voice join to first audio | < 2 s on a good network |
| Reconnect after brief loss | < 5 s |
| TURN relay capacity | Sized to expected restrictive-network users |

Bandwidth is dominated by media, not Tulpar:

- Opus voice: roughly 24–64 kbps per stream depending on settings.
- Video: 720p/30 ≈ 1.5–3 Mbps per publisher; subscribers multiply.
- Screen share: content-dependent; cap bitrate and frame rate based on motion.

LiveKit and TURN should run on a host with sufficient network capacity and CPU. Do not run heavy video loads on a small VPS shared with PostgreSQL without measuring.

Scaling:

- Vertical first.
- Separate LiveKit onto its own host when media load grows.
- Use LiveKit's distributed mode with Redis for multiple SFU nodes when needed.
- Coturn can be scaled independently.
- Tulpar backend token issuance is stateless and scales horizontally.

---

## 13. Failure behavior

| Failure | Impact | Behavior |
|---|---|---|
| LiveKit down | No new voice/video; text unaffected | `503 voice_unavailable`; existing media may drop; clear UI |
| Coturn down | Direct connections may work; restrictive networks fail | Client shows a network error; retry; no fake success |
| Redis down | Voice projection and cross-node events degrade | Media continues; local projection only; REST token issuance still works if DB is up |
| PostgreSQL down | No new tokens; existing media continues | 503; no membership changes |
| LiveKit webhook secret misconfigured | Projection stale; security risk | Reject webhooks; reconcile; readiness alert |
| LiveKit API unreachable during moderation | Participant may remain briefly | Retry; token expiry bounds exposure; audit |
| TURN credentials expired | Reconnect may fail until a new token | Client requests a new token |

---

## 14. Security checklist

- [x] LiveKit API secret only on the server and LiveKit config.
- [x] Short-lived tokens with least-privilege sources.
- [x] Room names derived server-side.
- [x] Backend checks membership, ban, timeout, permission, capacity, password.
- [x] Ephemeral TURN credentials; no static TURN password in clients.
- [x] Token revocation path via LiveKit removal + refusal to reissue.
- [x] Webhook signature verification and replay protection.
- [x] No recording in v1.
- [x] No E2EE claim; server/SFU can access media metadata and content.
- [x] Per-user volume and mute state are local.
- [x] Rate limits on token issuance and password attempts.
- [ ] Real network/TURN tests must pass in Phase 4.
- [ ] LiveKit version upgrade and webhook compatibility must be tested.

---

## 15. Test requirements

### 15.1 Automated and semi-automated

- Token issuance with each permission combination.
- Token contains no excessive grants.
- Expired/revoked token rejection.
- Room password success/failure/rate limit.
- Capacity enforcement.
- Ban/kick/timeout removes or restricts participants.
- Webhook signature, replay, out-of-order events.
- Redis projection rebuild after loss.
- TURN fallback with UDP blocked (real Coturn).
- Reconnect after network interruption.
- Multiple devices for the same user.
- Voice moderation audit records.

### 15.2 Manual matrix

- Chrome, Edge, Firefox, Safari (latest two versions) on Windows/Linux/macOS where available.
- Tulpar Desktop on Windows 10/11 and Linux (X11, Wayland).
- Bluetooth, USB, and built-in microphones/headsets.
- Device hot-plug and default-device changes.
- Push-to-talk with focus and global shortcut limitations.
- Corporate firewall/NAT scenarios.

Mocked voice tests do not count as completion. Phase 4 must validate actual audio through LiveKit and Coturn.

---

## 16. Platform limitations

- Browsers require a secure context and user gesture for microphone access.
- Background tabs may throttle or suspend audio; mobile browsers can interrupt voice when backgrounded.
- iOS Safari has stricter WebRTC and background behavior; the mobile web client is best-effort for voice.
- Global push-to-talk is not possible in a normal browser tab; it is desktop-only where the OS permits.
- Wayland compositors may block global shortcuts; the desktop app must fall back to an in-app shortcut.
- Corporate firewalls can block UDP and TURN/TLS; no client can guarantee connectivity.
- Audio processing quality varies by browser, OS, and device.

See [WEB.md](WEB.md), [DESKTOP.md](DESKTOP.md), and [VIDEO.md](VIDEO.md).

---

## 17. Open voice decisions

| Decision | Current position | Needed by |
|---|---|---|
| E2EE | Not in v1; would require key management and would break server-side moderation/recording | Future |
| Recording | Not in v1; requires consent and storage policy | Future |
| DM calls | Not in v1; additive room type | Post-v1 |
| Stage channels | Not in v1; permission bits reserved | Post-v1 |
| Region selection | Single LiveKit endpoint initially; multi-region later | Scale phase |
| LiveKit built-in TURN | Disabled in reference deployment; Coturn authoritative | Phase 4 |
| Noise suppression model | Browser/WebRTC built-in; no ML model in v1 | Phase 4 |
| Max voice participants default | 100 per node target; channel limit configurable | Phase 4 |
| Push-to-talk global shortcut | Desktop best-effort; documented OS limitations | Phase 7 |
