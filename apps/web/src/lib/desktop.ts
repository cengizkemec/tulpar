import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface DesktopInstanceInfo {
  url: string;
  name: string;
  version: string;
  registrationEnabled: boolean;
}

export interface DeviceStart {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export interface DevicePoll {
  status: "pending" | "approved" | "denied" | "expired";
  accessToken?: string;
  expiresIn?: number;
  sessionId?: string;
  user?: unknown;
  message?: string;
}

export interface DesktopCapabilities {
  platform: string;
  keychain: boolean;
  globalShortcut: boolean;
  tray: boolean;
}

export function isDesktop(): boolean {
  return isTauri();
}

export function desktopProbeInstance(url: string): Promise<DesktopInstanceInfo> {
  return invoke("desktop_probe_instance", { url });
}

export function desktopStartDeviceLogin(
  instance: string,
  clientName: string,
): Promise<DeviceStart> {
  return invoke("desktop_device_start", { instance, clientName });
}

export function desktopPollDeviceLogin(
  instance: string,
  deviceCode: string,
): Promise<DevicePoll> {
  return invoke("desktop_device_poll", { instance, deviceCode });
}

export function desktopRefreshSession(
  instance: string,
): Promise<{ accessToken: string; expiresIn: number }> {
  return invoke("desktop_refresh", { instance });
}

export function desktopLogout(instance: string): Promise<void> {
  return invoke("desktop_logout", { instance });
}

export function desktopHasSession(instance: string): Promise<boolean> {
  return invoke("desktop_keychain_has", { instance });
}

export function desktopNotify(title: string, body: string): Promise<void> {
  return invoke("desktop_notify", { title, body });
}

export function desktopOpenExternal(url: string): Promise<void> {
  return invoke("desktop_open_external", { url });
}

export function desktopRegisterPtt(shortcut: string): Promise<boolean> {
  return invoke("desktop_register_ptt", { shortcut });
}

export function desktopCapabilities(): Promise<DesktopCapabilities> {
  return invoke("desktop_capabilities");
}

export function desktopSetUnread(count: number): Promise<void> {
  return invoke("desktop_set_unread", { count });
}

export function desktopTakeDeepLinks(): Promise<string[]> {
  return invoke("desktop_take_deep_links");
}

export function desktopShowWindow(): Promise<void> {
  return invoke("desktop_show_window");
}

export function desktopAutostartGet(): Promise<boolean> {
  return invoke("desktop_autostart_get");
}

export function desktopAutostartSet(enabled: boolean): Promise<void> {
  return invoke("desktop_autostart_set", { enabled });
}

export async function onPtt(
  handler: (pressed: boolean) => void,
): Promise<() => void> {
  return listen<string>("ptt", (event) => handler(event.payload === "down"));
}

export async function onDeepLink(
  handler: (url: string) => void,
): Promise<() => void> {
  return listen<string>("deep-link", (event) => handler(event.payload));
}
