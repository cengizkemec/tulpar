import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Room, Track as LiveKitTrack } from "livekit-client";

import { request } from "../../lib/api";
import type { Channel, VoiceParticipant, VoiceToken } from "../../lib/types";
import { useVoiceStore } from "../../stores/voice";

const EMPTY_PARTICIPANTS: VoiceParticipant[] = [];

interface RemoteMedia {
  camera?: LiveKitTrack;
  screen?: LiveKitTrack;
}

export function VoicePanel({ channel }: { channel: Channel }) {
  const { t } = useTranslation();
  const channelId = channel.id ?? "";
  const roomRef = useRef<Room | null>(null);
  const audioElements = useRef(new Map<string, HTMLMediaElement>());
  const [connected, setConnected] = useState(false);
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mic, setMic] = useState(false);
  const [camera, setCamera] = useState(false);
  const [screen, setScreen] = useState(false);
  const [localCamera, setLocalCamera] = useState<LiveKitTrack>();
  const [localScreen, setLocalScreen] = useState<LiveKitTrack>();
  const [remoteMedia, setRemoteMedia] = useState<Record<string, RemoteMedia>>(
    {},
  );
  const [focusedTile, setFocusedTile] = useState<string | null>(null);
  const [pinnedTile, setPinnedTile] = useState<string | null>(null);
  const [devices, setDevices] = useState<MediaDeviceInfo[]>([]);
  const [micDevice, setMicDevice] = useState("");
  const [cameraDevice, setCameraDevice] = useState("");
  const [quality, setQuality] = useState("unknown");
  const participants =
    useVoiceStore((state) => state.participants[channelId]) ??
    EMPTY_PARTICIPANTS;
  const setParticipants = useVoiceStore((state) => state.setParticipants);

  const states = useQuery({
    queryKey: ["voice-states", channelId],
    queryFn: () =>
      request<VoiceParticipant[]>(
        `/api/v1/channels/${channelId}/voice/states`,
      ),
    enabled: Boolean(channelId),
    refetchInterval: 5000,
  });

  useEffect(() => {
    if (states.data) {
      setParticipants(channelId, states.data);
    }
  }, [channelId, setParticipants, states.data]);

  const clearMedia = () => {
    for (const element of audioElements.current.values()) {
      element.remove();
    }
    audioElements.current.clear();
    setLocalCamera(undefined);
    setLocalScreen(undefined);
    setRemoteMedia({});
  };

  useEffect(() => {
    return () => {
      roomRef.current?.disconnect();
      roomRef.current = null;
      for (const element of audioElements.current.values()) {
        element.remove();
      }
      audioElements.current.clear();
    };
  }, [channelId]);

  async function refreshDevices() {
    if (!navigator.mediaDevices?.enumerateDevices) {
      return;
    }
    const all = await navigator.mediaDevices.enumerateDevices();
    setDevices(all);
  }

  async function selectDevice(kind: MediaDeviceKind, deviceId: string) {
    if (!roomRef.current) {
      return;
    }
    await roomRef.current.switchActiveDevice(kind, deviceId);
    if (kind === "audioinput") {
      setMicDevice(deviceId);
    } else {
      setCameraDevice(deviceId);
    }
  }

  async function join() {
    setJoining(true);
    setError(null);
    try {
      const voice = await request<VoiceToken>(
        `/api/v1/channels/${channelId}/voice/token`,
        { method: "POST", body: {} },
      );
      const livekit = await import("livekit-client");
      const room = new livekit.Room({
        adaptiveStream: true,
        dynacast: true,
      });
      roomRef.current = room;
      room.on(
        livekit.RoomEvent.TrackSubscribed,
        (track, _publication, participant) => {
          if (track.kind === livekit.Track.Kind.Audio) {
            const element = track.attach();
            element.style.display = "none";
            document.body.appendChild(element);
            audioElements.current.set(
              track.sid ?? `${participant.identity}-audio`,
              element,
            );
          }
          if (track.kind === livekit.Track.Kind.Video) {
            const isScreen = track.source === livekit.Track.Source.ScreenShare;
            setRemoteMedia((current) => ({
              ...current,
              [participant.identity]: {
                ...current[participant.identity],
                ...(isScreen
                  ? { screen: track }
                  : { camera: track }),
              },
            }));
          }
        },
      );
      room.on(
        livekit.RoomEvent.TrackUnsubscribed,
        (track, _publication, participant) => {
          if (track.kind === livekit.Track.Kind.Audio) {
            const key = track.sid ?? `${participant.identity}-audio`;
            audioElements.current.get(key)?.remove();
            audioElements.current.delete(key);
          }
          if (track.kind === livekit.Track.Kind.Video) {
            setRemoteMedia((current) => {
              const existing = current[participant.identity];
              if (!existing) {
                return current;
              }
              return {
                ...current,
                [participant.identity]: {
                  ...existing,
                  ...(track.source === livekit.Track.Source.ScreenShare
                    ? { screen: undefined }
                    : { camera: undefined }),
                },
              };
            });
          }
        },
      );
      room.on(livekit.RoomEvent.ParticipantDisconnected, (participant) => {
        setRemoteMedia((current) => {
          const next = { ...current };
          delete next[participant.identity];
          return next;
        });
      });
      room.on(livekit.RoomEvent.Disconnected, () => {
        setConnected(false);
        setMic(false);
        setCamera(false);
        setScreen(false);
        setQuality("unknown");
        clearMedia();
      });
      room.on(
        livekit.RoomEvent.ConnectionQualityChanged,
        (nextQuality, participant) => {
          if (participant.isLocal) {
            setQuality(String(nextQuality));
          }
        },
      );
      await room.connect(voice.url, voice.token);
      setConnected(true);
      void refreshDevices();
      try {
        await room.startAudio().catch(() => undefined);
        await room.localParticipant.setMicrophoneEnabled(true);
        setMic(true);
      } catch {
        setMic(false);
        setError(t("microphoneAccessWarning"));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
      roomRef.current?.disconnect();
      roomRef.current = null;
      clearMedia();
    } finally {
      setJoining(false);
    }
  }

  async function leave() {
    roomRef.current?.disconnect();
    roomRef.current = null;
    setConnected(false);
    setMic(false);
    setCamera(false);
    setScreen(false);
    clearMedia();
  }

  async function toggleMic() {
    if (!roomRef.current) return;
    const next = !mic;
    await roomRef.current.localParticipant.setMicrophoneEnabled(next);
    setMic(next);
  }

  async function toggleCamera() {
    if (!roomRef.current) return;
    const next = !camera;
    try {
      await roomRef.current.localParticipant.setCameraEnabled(next);
      setCamera(next);
      setLocalCamera(
        next
          ? roomRef.current.localParticipant.getTrackPublication(
              (await import("livekit-client")).Track.Source.Camera,
            )?.videoTrack
          : undefined,
      );
      if (next) {
        await refreshDevices();
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    }
  }

  async function toggleScreen() {
    if (!roomRef.current) return;
    const next = !screen;
    try {
      await roomRef.current.localParticipant.setScreenShareEnabled(next);
      setScreen(next);
      setLocalScreen(
        next
          ? roomRef.current.localParticipant.getTrackPublication(
              (await import("livekit-client")).Track.Source.ScreenShare,
            )?.videoTrack
          : undefined,
      );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    }
  }

  const videoTiles = [
    ...(localCamera ? [{ id: "local-camera", label: t("you"), track: localCamera, local: true, screen: false }] : []),
    ...(localScreen ? [{ id: "local-screen", label: t("yourScreen"), track: localScreen, local: true, screen: true }] : []),
    ...Object.entries(remoteMedia).flatMap(([identity, media]) => [
      ...(media.camera
        ? [{ id: `${identity}-camera`, label: identity.slice(0, 8), track: media.camera, local: false, screen: false }]
        : []),
      ...(media.screen
        ? [{ id: `${identity}-screen`, label: t("screenOf", { name: identity.slice(0, 8) }), track: media.screen, local: false, screen: true }]
        : []),
    ]),
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col p-5">
      <div className="flex flex-wrap items-center gap-2">
        {connected ? (
          <button
            className="rounded-lg bg-red-500/15 px-3 py-2 text-sm font-medium text-red-200 hover:bg-red-500/25"
            onClick={() => void leave()}
          >
            {t("leaveVoice")}
          </button>
        ) : (
          <button
            data-testid="voice-join"
            className="rounded-lg bg-[var(--success)] px-3 py-2 text-sm font-semibold text-slate-950 hover:brightness-110 disabled:opacity-50"
            onClick={() => void join()}
            disabled={joining}
          >
            {joining ? t("connecting") : t("joinVoice")}
          </button>
        )}
        <button
          data-testid="voice-mic"
          data-active={mic}
          aria-pressed={mic}
          className={`rounded-lg px-3 py-2 text-sm ${
            mic
              ? "bg-[var(--accent)] text-white"
              : "bg-[var(--surface-3)] text-slate-300"
          }`}
          onClick={() => void toggleMic()}
          disabled={!connected}
        >
          🎙 {t("microphone")}
        </button>
        <button
          data-testid="voice-ptt"
          className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm text-slate-300 active:bg-[var(--accent)] active:text-white"
          disabled={!connected}
          onPointerDown={async () => {
            await roomRef.current?.localParticipant.setMicrophoneEnabled(true);
            setMic(true);
          }}
          onPointerUp={async () => {
            await roomRef.current?.localParticipant.setMicrophoneEnabled(false);
            setMic(false);
          }}
          onPointerLeave={async () => {
            if (mic) {
              await roomRef.current?.localParticipant.setMicrophoneEnabled(false);
              setMic(false);
            }
          }}
        >
          🎤 {t("pushToTalk")}
        </button>
        <button
          data-testid="voice-camera"
          data-active={camera}
          className={`rounded-lg px-3 py-2 text-sm ${
            camera
              ? "bg-[var(--accent)] text-white"
              : "bg-[var(--surface-3)] text-slate-300"
          }`}
          onClick={() => void toggleCamera()}
          disabled={!connected}
        >
          📷 {t("camera")}
        </button>
        <button
          data-testid="voice-screen"
          data-active={screen}
          className={`rounded-lg px-3 py-2 text-sm ${
            screen
              ? "bg-[var(--accent)] text-white"
              : "bg-[var(--surface-3)] text-slate-300"
          }`}
          onClick={() => void toggleScreen()}
          disabled={!connected}
        >
          🖥 {t("screen")}
        </button>
        <span
          data-testid="voice-status"
          className="ml-auto text-xs text-[var(--muted)]"
        >
          {connected
            ? `${t("connected")}${quality !== "unknown" ? ` · ${quality}` : ""}`
            : ""}
        </span>
      </div>

      {connected ? (
        <div className="mt-3 flex flex-wrap gap-3 text-xs text-[var(--muted)]">
          <label className="flex items-center gap-2">
            {t("microphone")}
            <select
              className="max-w-52 rounded border border-[var(--border)] bg-[var(--surface-2)] px-2 py-1"
              value={micDevice}
              onChange={(event) =>
                void selectDevice("audioinput", event.target.value)
              }
            >
              <option value="">{t("defaultDevice")}</option>
              {devices
                .filter((device) => device.kind === "audioinput")
                .map((device) => (
                  <option key={device.deviceId} value={device.deviceId}>
                    {device.label || t("microphone")}
                  </option>
                ))}
            </select>
          </label>
          <label className="flex items-center gap-2">
            {t("camera")}
            <select
              className="max-w-52 rounded border border-[var(--border)] bg-[var(--surface-2)] px-2 py-1"
              value={cameraDevice}
              onChange={(event) =>
                void selectDevice("videoinput", event.target.value)
              }
            >
              <option value="">{t("defaultDevice")}</option>
              {devices
                .filter((device) => device.kind === "videoinput")
                .map((device) => (
                  <option key={device.deviceId} value={device.deviceId}>
                    {device.label || t("camera")}
                  </option>
                ))}
            </select>
          </label>
        </div>
      ) : null}

      {error ? (
        <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
          {error}
        </p>
      ) : null}

      {videoTiles.length ? (
        <>
          <div className="mt-4 flex items-center gap-2">
            <h2 className="text-sm font-semibold">{t("video")}</h2>
            {focusedTile ? (
              <button
                className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                onClick={() => setFocusedTile(null)}
              >
                {t("backToGrid")}
              </button>
            ) : (
              <span className="text-xs text-[var(--muted)]">
                {t("clickTileToFocus")}
              </span>
            )}
          </div>
          <div
            className={`mt-3 grid gap-3 ${
              focusedTile ? "" : "md:grid-cols-2 xl:grid-cols-3"
            }`}
          >
            {videoTiles
              .filter((tile) => !focusedTile || tile.id === focusedTile)
              .map((tile) => (
                <VideoTile
                  key={tile.id}
                  label={tile.label}
                  track={tile.track}
                  local={tile.local}
                  screen={tile.screen}
                  pinned={pinnedTile === tile.id}
                  onFocus={() => setFocusedTile(tile.id)}
                  onPin={() =>
                    setPinnedTile(pinnedTile === tile.id ? null : tile.id)
                  }
                />
              ))}
          </div>
        </>
      ) : null}

      <h2 className="mt-6 text-sm font-semibold">{t("voiceParticipants")}</h2>
      <div className="mt-3 grid flex-1 auto-rows-min grid-cols-2 gap-3 overflow-y-auto md:grid-cols-3 xl:grid-cols-4">
        {participants.length === 0 ? (
          <p className="text-sm text-[var(--muted)]">
            {t("noVoiceParticipants")}
          </p>
        ) : (
          participants.map((participant) => (
            <div
              key={participant.user_id}
              className="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] p-4"
            >
              <div className="grid h-14 w-14 place-items-center rounded-full bg-[var(--surface-3)] text-lg font-semibold">
                {(participant.display_name ?? "?").slice(0, 1).toUpperCase()}
              </div>
              <p className="mt-3 truncate text-sm font-medium">
                {participant.display_name ?? participant.user_id}
              </p>
              <p className="mt-1 text-xs text-[var(--muted)]">
                {participant.server_mute || participant.self_mute
                  ? t("muted")
                  : t("micOn")}
                {participant.server_deaf || participant.self_deaf
                  ? ` · ${t("deafened")}`
                  : ""}
              </p>
              {participant.tracks?.length ? (
                <p className="mt-1 text-[10px] text-[var(--muted)]">
                  {t("tracks", { count: participant.tracks.length })}
                </p>
              ) : null}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function VideoTile({
  label,
  track,
  local,
  screen,
  pinned,
  onFocus,
  onPin,
}: {
  label: string;
  track: LiveKitTrack;
  local: boolean;
  screen: boolean;
  pinned: boolean;
  onFocus: () => void;
  onPin: () => void;
}) {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const element = videoRef.current;
    if (!element) {
      return;
    }
    track.attach(element);
    return () => {
      track.detach(element);
    };
  }, [track]);

  return (
    <div
      className={`group relative cursor-pointer overflow-hidden rounded-xl border bg-black ${
        pinned
          ? "border-[var(--accent)] md:col-span-2 xl:col-span-3"
          : "border-[var(--border)]"
      } ${screen ? "md:col-span-2" : ""}`}
      onClick={onFocus}
    >
      <video
        ref={videoRef}
        autoPlay
        playsInline
        muted={local}
        className="aspect-video h-full w-full object-contain"
      />
      <span className="absolute bottom-2 left-2 rounded bg-black/60 px-2 py-0.5 text-xs text-white">
        {label}
      </span>
      <div className="absolute right-2 top-2 hidden gap-1 group-hover:flex">
        <button
          className="rounded bg-black/60 px-2 py-1 text-xs text-white"
          onClick={(event) => {
            event.stopPropagation();
            onPin();
          }}
        >
          {pinned ? t("unpin") : t("pin")}
        </button>
        <button
          className="rounded bg-black/60 px-2 py-1 text-xs text-white"
          onClick={(event) => {
            event.stopPropagation();
            void videoRef.current?.requestFullscreen();
          }}
        >
          {t("fullscreen")}
        </button>
      </div>
    </div>
  );
}
