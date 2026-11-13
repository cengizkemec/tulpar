import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Room } from "livekit-client";

import { request } from "../../lib/api";
import type { Channel, VoiceParticipant, VoiceToken } from "../../lib/types";
import { useVoiceStore } from "../../stores/voice";

const EMPTY_PARTICIPANTS: VoiceParticipant[] = [];

export function VoicePanel({ channel }: { channel: Channel }) {
  const { t } = useTranslation();
  const channelId = channel.id ?? "";
  const roomRef = useRef<Room | null>(null);
  const [connected, setConnected] = useState(false);
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mic, setMic] = useState(false);
  const [camera, setCamera] = useState(false);
  const [screen, setScreen] = useState(false);
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

  useEffect(() => {
    return () => {
      roomRef.current?.disconnect();
      roomRef.current = null;
    };
  }, [channelId]);

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
      room.on(livekit.RoomEvent.Disconnected, () => {
        setConnected(false);
        setMic(false);
        setCamera(false);
        setScreen(false);
      });
      await room.connect(voice.url, voice.token);
      await room.startAudio().catch(() => undefined);
      await room.localParticipant.setMicrophoneEnabled(true);
      setMic(true);
      setConnected(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
      roomRef.current?.disconnect();
      roomRef.current = null;
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
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    }
  }

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
          {connected ? t("connected") : ""}
        </span>
      </div>

      {error ? (
        <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
          {error}
        </p>
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
                {participant.server_mute || participant.self_mute ? "muted" : "mic on"}
                {participant.server_deaf || participant.self_deaf ? " · deafened" : ""}
              </p>
              {participant.tracks?.length ? (
                <p className="mt-1 text-[10px] text-[var(--muted)]">
                  {participant.tracks.length} track(s)
                </p>
              ) : null}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
