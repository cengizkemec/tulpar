import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface SavedInstance {
  url: string;
  name: string;
}

interface InstanceState {
  currentUrl: string;
  currentName: string;
  instances: SavedInstance[];
  setCurrent: (url: string, name: string) => void;
  remove: (url: string) => void;
}

export const useInstances = create<InstanceState>()(
  persist(
    (set) => ({
      currentUrl: "",
      currentName: "",
      instances: [],
      setCurrent: (url, name) =>
        set((state) => {
          const normalized = url.replace(/\/$/, "");
          const existing = state.instances.filter(
            (instance) => instance.url !== normalized,
          );
          return {
            currentUrl: normalized,
            currentName: name,
            instances: [{ url: normalized, name }, ...existing],
          };
        }),
      remove: (url) =>
        set((state) => ({
          instances: state.instances.filter((instance) => instance.url !== url),
          currentUrl: state.currentUrl === url ? "" : state.currentUrl,
          currentName: state.currentUrl === url ? "" : state.currentName,
        })),
    }),
    { name: "tulpar.instances" },
  ),
);
