import type { SyncInstance } from "./types";

export function normalizeServerAddress(input: string): string {
  const value = input.trim();
  if (!value) {
    throw new Error("Enter a server address.");
  }
  const withScheme = /^https?:\/\//i.test(value)
    ? value
    : `http://${value}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    throw new Error("That is not a valid server address.");
  }
  if (!["http:", "https:"].includes(url.protocol)) {
    throw new Error("Server addresses must use http or https.");
  }
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

export async function probeInstance(baseUrl: string): Promise<SyncInstance> {
  const response = await fetch(`${baseUrl}/api/v1/sync/instance`, {
    headers: { accept: "application/json" },
  });
  if (!response.ok) {
    throw new Error(`Server responded with ${response.status}.`);
  }
  const instance = (await response.json()) as SyncInstance;
  if (!instance?.name || !instance?.version) {
    throw new Error("That address does not look like a Tulpar instance.");
  }
  return instance;
}
