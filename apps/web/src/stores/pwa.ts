import { create } from "zustand";

export interface InstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

interface PwaState {
  prompt: InstallPromptEvent | null;
  installed: boolean;
  setPrompt: (prompt: InstallPromptEvent | null) => void;
  setInstalled: (installed: boolean) => void;
}

export const usePwa = create<PwaState>((set) => ({
  prompt: null,
  installed: false,
  setPrompt: (prompt) => set({ prompt }),
  setInstalled: (installed) =>
    set((state) => ({ installed, prompt: installed ? null : state.prompt })),
}));
