import { create } from "zustand";

import type { SelfUser } from "../lib/types";

interface SessionState {
  accessToken: string | null;
  user: SelfUser | null;
  bootstrapped: boolean;
  setSession: (accessToken: string, user: SelfUser) => void;
  setAccessToken: (accessToken: string) => void;
  setUser: (user: SelfUser) => void;
  setBootstrapped: () => void;
  clear: () => void;
}

export const useSession = create<SessionState>((set) => ({
  accessToken: null,
  user: null,
  bootstrapped: false,
  setSession: (accessToken, user) => {
    window.localStorage.setItem("tulpar.hasSession", "1");
    set({ accessToken, user });
  },
  setAccessToken: (accessToken) => set({ accessToken }),
  setUser: (user) => set({ user }),
  setBootstrapped: () => set({ bootstrapped: true }),
  clear: () => {
    window.localStorage.removeItem("tulpar.hasSession");
    set({ accessToken: null, user: null, bootstrapped: true });
  },
}));
