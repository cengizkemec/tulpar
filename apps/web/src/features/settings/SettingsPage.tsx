import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

import { request } from "../../lib/api";
import {
  desktopAutostartGet,
  desktopAutostartSet,
  desktopCapabilities,
  desktopRegisterPtt,
  isDesktop,
  type DesktopCapabilities,
} from "../../lib/desktop";
import type { NotificationPreferences, UserSettings } from "../../lib/types";
import { usePwa } from "../../stores/pwa";
import { useSession } from "../../stores/session";

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const installPrompt = usePwa((state) => state.prompt);
  const installed = usePwa((state) => state.installed);
  const setInstalled = usePwa((state) => state.setInstalled);
  const setPrompt = usePwa((state) => state.setPrompt);
  const queryClient = useQueryClient();
  const user = useSession((state) => state.user);
  const setUser = useSession((state) => state.setUser);
  const [displayName, setDisplayName] = useState(user?.display_name ?? "");
  const [bio, setBio] = useState(user?.bio ?? "");
  const [status, setStatus] = useState(user?.preferred_status ?? "online");
  const [settings, setSettings] = useState<UserSettings | null>(null);
  const [notifications, setNotifications] =
    useState<NotificationPreferences | null>(null);
  const [saved, setSaved] = useState(false);
  const [browserPermission, setBrowserPermission] = useState(
    typeof Notification === "undefined" ? "unsupported" : Notification.permission,
  );
  const [pttShortcut, setPttShortcut] = useState(
    () => window.localStorage.getItem("tulpar.ptt") ?? "",
  );
  const [autostart, setAutostart] = useState(false);
  const [capabilities, setCapabilities] = useState<DesktopCapabilities | null>(
    null,
  );
  const [desktopMessage, setDesktopMessage] = useState<string | null>(null);
  const [desktopError, setDesktopError] = useState<string | null>(null);

  const settingsQuery = useQuery({
    queryKey: ["user-settings"],
    queryFn: () => request<UserSettings>("/api/v1/users/me/settings"),
  });
  const notificationQuery = useQuery({
    queryKey: ["notification-preferences"],
    queryFn: () =>
      request<NotificationPreferences>(
        "/api/v1/users/me/notification-preferences",
      ),
  });

  useEffect(() => {
    if (settingsQuery.data) setSettings(settingsQuery.data);
  }, [settingsQuery.data]);
  useEffect(() => {
    if (notificationQuery.data) setNotifications(notificationQuery.data);
  }, [notificationQuery.data]);
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    void desktopAutostartGet().then(setAutostart).catch(() => undefined);
    void desktopCapabilities().then(setCapabilities).catch(() => undefined);
  }, []);

  const saveProfile = useMutation({
    mutationFn: () =>
      request<typeof user>("/api/v1/users/me", {
        method: "PATCH",
        body: {
          display_name: displayName,
          bio,
          preferred_status: status,
        },
      }),
    onSuccess: async (updated) => {
      if (updated) setUser(updated);
      await queryClient.invalidateQueries({ queryKey: ["sync"] });
      setSaved(true);
    },
  });

  const saveSettings = useMutation({
    mutationFn: () =>
      request<UserSettings>("/api/v1/users/me/settings", {
        method: "PATCH",
        body: settings ?? {},
      }),
    onSuccess: (updated) => {
      setSettings(updated);
      setSaved(true);
    },
  });

  const saveNotifications = useMutation({
    mutationFn: () =>
      request<NotificationPreferences>(
        "/api/v1/users/me/notification-preferences",
        {
          method: "PUT",
          body: notifications ?? {},
        },
      ),
    onSuccess: (updated) => {
      setNotifications(updated);
      setSaved(true);
    },
  });

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <h1 className="text-xl font-semibold">{t("settings")}</h1>
      {saved ? (
        <p className="mt-2 text-sm text-[var(--success)]">{t("saved")}</p>
      ) : null}
      <Link
        className="mt-3 inline-block rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
        to="/admin"
      >
        {t("instanceAdministration")}
      </Link>

      <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">{t("language")}</h2>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <select
            data-testid="language-select"
            className="rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-sm"
            value={i18n.language.startsWith("tr") ? "tr" : "en"}
            onChange={(event) => {
              const language = event.target.value;
              void i18n.changeLanguage(language);
              window.localStorage.setItem("tulpar.language", language);
              document.documentElement.lang = language;
            }}
          >
            <option value="en">{t("english")}</option>
            <option value="tr">{t("turkish")}</option>
          </select>
          {installed ? (
            <span className="text-xs text-[var(--success)]">
              {t("appInstalled")}
            </span>
          ) : installPrompt ? (
            <button
              className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
              onClick={async () => {
                await installPrompt.prompt();
                const choice = await installPrompt.userChoice;
                if (choice.outcome === "accepted") {
                  setInstalled(true);
                } else {
                  setPrompt(null);
                }
              }}
            >
              {t("installApp")}
            </button>
          ) : null}
        </div>
      </section>

      {isDesktop() ? (
        <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
          <h2 className="text-sm font-semibold">{t("desktopSettings")}</h2>
          <label className="mt-3 block text-sm">
            <span className="mb-1 block text-[var(--muted)]">
              {t("desktopGlobalPtt")}
            </span>
            <div className="flex gap-2">
              <input
                className="flex-1 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 font-mono"
                value={pttShortcut}
                onChange={(event) => setPttShortcut(event.target.value)}
                placeholder="F8"
              />
              <button
                className="rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white"
                onClick={async () => {
                  try {
                    await desktopRegisterPtt(pttShortcut);
                    window.localStorage.setItem("tulpar.ptt", pttShortcut);
                    setDesktopMessage(t("desktopPttSaved"));
                    setDesktopError(null);
                  } catch (cause) {
                    setDesktopError(
                      cause instanceof Error ? cause.message : t("errorGeneric"),
                    );
                  }
                }}
              >
                {t("save")}
              </button>
            </div>
            <span className="mt-1 block text-xs text-[var(--muted)]">
              {t("desktopPttHint")}
            </span>
          </label>
          <label className="mt-3 flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={autostart}
              onChange={async (event) => {
                const next = event.target.checked;
                try {
                  await desktopAutostartSet(next);
                  setAutostart(next);
                  setDesktopError(null);
                } catch (cause) {
                  setDesktopError(
                    cause instanceof Error ? cause.message : t("errorGeneric"),
                  );
                }
              }}
            />
            {t("desktopAutostart")}
          </label>
          {capabilities ? (
            <p className="mt-3 text-xs text-[var(--muted)]">
              {t("desktopKeychain")}:{" "}
              {capabilities.keychain ? t("yes") : t("no")} · PTT:{" "}
              {capabilities.globalShortcut ? t("yes") : t("no")}
            </p>
          ) : null}
          {desktopMessage ? (
            <p className="mt-2 text-sm text-[var(--success)]">{desktopMessage}</p>
          ) : null}
          {desktopError ? (
            <p className="mt-2 text-sm text-red-300">{desktopError}</p>
          ) : null}
        </section>
      ) : null}

      <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">{t("profile")}</h2>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">
            {t("displayName")}
          </span>
          <input
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("bio")}</span>
          <textarea
            className="min-h-20 w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
            value={bio}
            onChange={(event) => setBio(event.target.value)}
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("status")}</span>
          <select
            className="rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <option value="online">{t("online")}</option>
            <option value="idle">{t("idle")}</option>
            <option value="dnd">{t("dnd")}</option>
            <option value="invisible">{t("invisible")}</option>
          </select>
        </label>
        <button
          className="mt-4 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          disabled={saveProfile.isPending}
          onClick={() => saveProfile.mutate()}
        >
          {t("save")}
        </button>
      </section>

      <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">{t("privacy")}</h2>
        {settings ? (
          <div className="mt-3 grid gap-3 md:grid-cols-2">
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">
                {t("friendRequests")}
              </span>
              <select
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                value={settings.allow_friend_requests_from}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    allow_friend_requests_from: event.target.value,
                  })
                }
              >
                <option value="everyone">{t("everyone")}</option>
                <option value="mutual_communities">{t("mutualCommunities")}</option>
                <option value="nobody">{t("nobody")}</option>
              </select>
            </label>
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">{t("directMessages")}</span>
              <select
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                value={settings.allow_dms_from}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    allow_dms_from: event.target.value,
                  })
                }
              >
                <option value="everyone">{t("everyone")}</option>
                <option value="friends">{t("friends")}</option>
                <option value="mutual_communities">{t("mutualCommunities")}</option>
                <option value="nobody">{t("nobody")}</option>
              </select>
            </label>
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">{t("presence")}</span>
              <select
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-2 py-2"
                value={settings.presence_visibility}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    presence_visibility: event.target.value,
                  })
                }
              >
                <option value="everyone">{t("everyone")}</option>
                <option value="friends">{t("friends")}</option>
                <option value="nobody">{t("nobody")}</option>
              </select>
            </label>
            <label className="flex items-center gap-2 self-end text-sm">
              <input
                type="checkbox"
                checked={settings.read_receipts}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    read_receipts: event.target.checked,
                  })
                }
              />
              {t("readReceipts")}
            </label>
          </div>
        ) : (
          <p className="mt-2 text-sm text-[var(--muted)]">{t("loading")}</p>
        )}
        <button
          className="mt-4 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          disabled={!settings || saveSettings.isPending}
          onClick={() => saveSettings.mutate()}
        >
          {t("save")}
        </button>
      </section>

      <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">{t("notifications")}</h2>
        <div className="mt-3 flex items-center gap-3 text-sm">
          <button
            className="rounded-lg bg-[var(--surface-3)] px-3 py-2"
            disabled={browserPermission === "unsupported"}
            onClick={async () => {
              const permission = await Notification.requestPermission();
              setBrowserPermission(permission);
            }}
          >
            {t("enableBrowserNotifications")}
          </button>
          <span className="text-xs text-[var(--muted)]">
            {browserPermission}
          </span>
        </div>
        {notifications ? (
          <div className="mt-3 space-y-2 text-sm">
            {[
              ["desktop_enabled", t("desktopNotifications")],
              ["sound_enabled", t("notificationSound")],
              ["mentions_only", t("mentionsOnly")],
              ["suppress_everyone", t("suppressEveryone")],
              ["email_enabled", t("emailNotifications")],
            ].map(([key, label]) => (
              <label key={key} className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={Boolean(
                    notifications[key as keyof NotificationPreferences],
                  )}
                  onChange={(event) =>
                    setNotifications({
                      ...notifications,
                      [key]: event.target.checked,
                    })
                  }
                />
                {label}
              </label>
            ))}
          </div>
        ) : (
          <p className="mt-2 text-sm text-[var(--muted)]">{t("loading")}</p>
        )}
        <button
          className="mt-4 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          disabled={!notifications || saveNotifications.isPending}
          onClick={() => saveNotifications.mutate()}
        >
          {t("save")}
        </button>
      </section>
    </div>
  );
}
