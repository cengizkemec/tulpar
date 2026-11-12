# Phase 4 Report — Voice Infrastructure

**Status:** Backend infrastructure implemented and tested. Real-audio and TURN-fallback client verification remains open.

**Date:** 2026-11-13

---

## 1. Implemented

### Voice service

- LiveKit JWT minting with permission-derived sources:
  - `SPEAK` → microphone
  - `STREAM_VIDEO` → camera
  - `SHARE_SCREEN` → screen share + screen-share audio
- Tokens are short-lived (300–900 seconds), contain no API secret, and use a private `roomAdmin=false` grant.
- Coturn REST credentials: `<expiry>:<user_id>` plus base64 HMAC-SHA1, with the shared secret remaining server-side.
- STUN, TURN/UDP, and optional TURN/TLS ICE entries.
- Argon2id room-password verification against `channels.voice_password_hash`.
- In-memory voice projection with join/leave/track/server-mute/server-deafen state.
- LiveKit webhook verification:
  - HS256 signature, issuer, expiry, and body SHA-256 claim.
  - Durable event-ID deduplication in `voice_webhook_events`.
  - `participant_joined`, `participant_left`, `track_published`, `track_unpublished`, `participant_connection_aborted`, and `room_finished`.
  - Stale `participant_left` events cannot remove a newer rejoin.
- LiveKit admin client:
  - `RemoveParticipant`, `MutePublishedTrack`, `UpdateParticipant`, and `DeleteRoom` Twirp calls.
  - Admin tokens are minted server-side with a 60-second lifetime.

### API

- `POST /api/v1/channels/{id}/voice/token`
- `GET /api/v1/channels/{id}/voice/states`
- `PUT /api/v1/channels/{id}/voice/password` (set or remove)
- `PUT /api/v1/channels/{id}/voice/participants/{user_id}/server-mute`
- `PUT /api/v1/channels/{id}/voice/participants/{user_id}/server-deafen`
- `DELETE /api/v1/channels/{id}/voice/participants/{user_id}`
- `POST /api/v1/channels/{id}/voice/participants/{user_id}/move`
- `POST /webhooks/livekit`

Token issuance enforces channel visibility, `CONNECT_VOICE`, bans, room password, room capacity, and timeout downgrade-to-listener. Moderators require `MUTE_MEMBERS`, `DEAFEN_MEMBERS`, or `MOVE_MEMBERS`, cannot target the owner or members at/above their role, and actions are audited.

Kick/ban revokes voice participation across community voice channels. A user-level `CONNECT_VOICE` deny removes the participant from that channel. Timeout updates the live LiveKit publish permission.

### Configuration and deployment

- `VoiceConfig` for LiveKit public/admin URLs, API key/secret, token TTL, STUN/TURN URLs, TURN secret, TURN TTL, and hard capacity.
- Development Compose wires LiveKit API credentials and Coturn shared-secret authentication.
- Coturn now uses `--use-auth-secret`/`--static-auth-secret`, matching the ephemeral credential scheme.

---

## 2. Verification

```text
cargo fmt --all -- --check                         -> pass
cargo clippy --workspace --all-targets -- -D warnings -> pass
cargo test --workspace                             -> 42 tests passed
pnpm generate:types                                -> pass
pnpm typecheck                                     -> pass
```

The voice integration test covers:

1. Voice-channel token issue with LiveKit claims and two ICE entries.
2. Non-member denial.
3. Password set, wrong-password rejection, correct-password acceptance.
4. Timeout downgrade to a listener with no microphone source.
5. Capacity rejection with a full projection.
6. Signed participant-join webhook and state projection.
7. Replay deduplication and stale-left protection.
8. Tampered-body rejection.
9. Server mute/deafen calls to a mock LiveKit admin API.
10. Kick revocation and participant removal.

---

## 3. Known gaps

1. **No real browser audio test yet.** Two real clients hearing each other requires the Phase 6 web client.
2. **No UDP-blocked TURN fallback test yet.** Coturn credentials and URLs are issued, but real relay behavior needs network-level testing.
3. **Voice projection is process-local.** Multi-node deployments need Redis-backed projection and fan-out.
4. **Webhook reconciliation with LiveKit RoomService list APIs is not implemented.** Lost webhooks can leave projection drift until reconnect.
5. **Role-level `CONNECT_VOICE` overwrite changes do not eagerly remove all affected users.** User-level overwrites, kick, ban, and timeout are handled.
6. **No recording, transcription, E2EE, or stage channels.** These are out of v1 scope.
7. **LiveKit admin calls are best-effort on revocation.** If LiveKit is unreachable the participant may remain until media disconnect; new tokens are still refused.
8. **No voice event sequence field from LiveKit.** Deduplication is event-ID based; stale-left protection uses timestamps.
9. **No real TURN secret rotation flow.** Operators rotate the shared secret and restart Coturn.

---

## 4. Recommended next steps

1. Build the web client voice UI and run two-client real-audio tests in Phase 6.
2. Test TURN fallback with UDP blocked and TURN/TLS.
3. Add Redis-backed voice projection for multi-node deployments.
4. Add a LiveKit reconciliation worker.
5. Add role-overwrite bulk revocation.
6. Start Phase 5 video/screen-share design against the same token grants.
