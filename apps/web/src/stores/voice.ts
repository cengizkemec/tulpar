import { create } from "zustand";

import type { VoiceParticipant } from "../lib/types";

interface VoiceState {
  participants: Record<string, VoiceParticipant[]>;
  setParticipants: (channelId: string, participants: VoiceParticipant[]) => void;
  upsert: (participant: VoiceParticipant) => void;
  remove: (channelId: string, userId: string) => void;
}

export const useVoiceStore = create<VoiceState>((set) => ({
  participants: {},
  setParticipants: (channelId, participants) =>
    set((state) => ({
      participants: { ...state.participants, [channelId]: participants },
    })),
  upsert: (participant) =>
    set((state) => {
      const current = state.participants[participant.channel_id ?? ""] ?? [];
      const next = current.filter((item) => item.user_id !== participant.user_id);
      next.push(participant);
      return {
        participants: {
          ...state.participants,
          [participant.channel_id ?? ""]: next,
        },
      };
    }),
  remove: (channelId, userId) =>
    set((state) => ({
      participants: {
        ...state.participants,
        [channelId]: (state.participants[channelId] ?? []).filter(
          (participant) => participant.user_id !== userId,
        ),
      },
    })),
}));
