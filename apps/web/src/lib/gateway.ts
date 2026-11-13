import type { QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { useVoiceStore } from "../stores/voice";
import type { GatewayTicket, VoiceParticipant } from "./types";
import { request } from "./api";

interface DispatchFrame {
  op: number;
  t?: string;
  s?: number;
  d?: unknown;
}

export function useGateway(queryClient: QueryClient) {
  useEffect(() => {
    let socket: WebSocket | null = null;
    let heartbeat: ReturnType<typeof setInterval> | null = null;
    let reconnect: ReturnType<typeof setTimeout> | null = null;
    let stopped = false;

    const clear = () => {
      if (heartbeat) {
        clearInterval(heartbeat);
        heartbeat = null;
      }
      if (reconnect) {
        clearTimeout(reconnect);
        reconnect = null;
      }
      if (socket) {
        socket.onclose = null;
        socket.close();
        socket = null;
      }
    };

    const connect = async () => {
      if (stopped) {
        return;
      }
      try {
        const ticket = await request<GatewayTicket>("/api/v1/gateway/ticket", {
          method: "POST",
        });
        socket = new WebSocket(ticket.gateway_url);
        socket.onmessage = (event) => {
          let frame: DispatchFrame;
          try {
            frame = JSON.parse(String(event.data)) as DispatchFrame;
          } catch {
            return;
          }
          if (frame.op === 10) {
            socket?.send(
              JSON.stringify({
                op: 2,
                d: { ticket: ticket.ticket, protocol_version: 1 },
              }),
            );
          } else if (frame.op === 0) {
            handleDispatch(frame, queryClient);
          }
        };
        socket.onclose = () => {
          clear();
          if (!stopped) {
            reconnect = setTimeout(() => void connect(), 3000);
          }
        };
        heartbeat = setInterval(() => {
          if (socket?.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ op: 1, d: { seq: 0 } }));
          }
        }, 25_000);
      } catch {
        if (!stopped) {
          reconnect = setTimeout(() => void connect(), 5000);
        }
      }
    };

    void connect();
    return () => {
      stopped = true;
      clear();
    };
  }, [queryClient]);
}

function handleDispatch(frame: DispatchFrame, queryClient: QueryClient) {
  if (!frame.t) {
    return;
  }
  const payload = frame.d as Record<string, unknown> | undefined;
  const channelId = payload?.channel_id as string | undefined;
  if (frame.t.startsWith("MESSAGE_") || frame.t === "MESSAGE_PIN_UPDATE") {
    if (channelId) {
      void queryClient.invalidateQueries({
        queryKey: ["messages", channelId],
      });
    } else {
      void queryClient.invalidateQueries({ queryKey: ["messages"] });
    }
    return;
  }
  if (frame.t === "VOICE_STATE_UPDATE" && payload) {
    useVoiceStore.getState().upsert(payload as unknown as VoiceParticipant);
    return;
  }
  if (frame.t === "VOICE_SESSION_ENDED" && channelId && payload?.user_id) {
    useVoiceStore
      .getState()
      .remove(channelId, String(payload.user_id));
    return;
  }
  if (frame.t === "NOTIFICATION_CREATE") {
    void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    void queryClient.invalidateQueries({ queryKey: ["sync"] });
  }
}
