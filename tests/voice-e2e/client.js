const { Room, RoomEvent, Track } = window.LivekitClient;

const params = new URLSearchParams(window.location.search);
const state = {
  connected: false,
  localAudio: false,
  remoteAudio: 0,
  error: null,
  events: [],
};
window.__tulparVoice = state;

const room = new Room({
  rtcConfig: params.get("relay") === "1" ? { iceTransportPolicy: "relay" } : undefined,
});

room.on(RoomEvent.Connected, () => {
  state.connected = true;
  state.events.push("connected");
});
room.on(RoomEvent.TrackSubscribed, (track) => {
  if (track.kind === Track.Kind.Audio) {
    state.remoteAudio += 1;
    state.events.push("remote-audio");
  }
});
room.on(RoomEvent.TrackUnsubscribed, (track) => {
  if (track.kind === Track.Kind.Audio) {
    state.remoteAudio = Math.max(0, state.remoteAudio - 1);
  }
});
room.on(RoomEvent.Disconnected, () => {
  state.connected = false;
  state.events.push("disconnected");
});

try {
  await room.connect(params.get("url"), params.get("token"));
  await room.localParticipant.setMicrophoneEnabled(true);
  state.localAudio = Boolean(
    room.localParticipant.getTrackPublication(Track.Source.Microphone)?.track,
  );
  state.events.push("local-audio");
} catch (error) {
  state.error = String(error?.message || error);
  state.events.push("error");
}

window.__tulparDisconnect = () => room.disconnect();
