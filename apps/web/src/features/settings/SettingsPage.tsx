import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { request } from "../../lib/api";
import type { NotificationPreferences, UserSettings } from "../../lib/types";
import { useSession } from "../../stores/session";

export function SettingsPage() {
  const { t } = useTranslation();
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
        <p className="mt-2 text-sm text-[var(--success)]">Saved.</p>
      ) : null}

      <section className="mt-5 max-w-2xl rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">Profile</h2>
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
          <span className="mb-1 block text-[var(--muted)]">Bio</span>
          <textarea
            className="min-h-20 w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
            value={bio}
            onChange={(event) => setBio(event.target.value)}
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">Status</span>
          <select
            className="rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <option value="online">Online</option>
            <option value="idle">Idle</option>
            <option value="dnd">Do not disturb</option>
            <option value="invisible">Invisible</option>
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
        <h2 className="text-sm font-semibold">Privacy</h2>
        {settings ? (
          <div className="mt-3 grid gap-3 md:grid-cols-2">
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">
                Friend requests
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
                <option value="everyone">Everyone</option>
                <option value="mutual_communities">Mutual communities</option>
                <option value="nobody">Nobody</option>
              </select>
            </label>
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">Direct messages</span>
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
                <option value="everyone">Everyone</option>
                <option value="friends">Friends</option>
                <option value="mutual_communities">Mutual communities</option>
                <option value="nobody">Nobody</option>
              </select>
            </label>
            <label className="text-sm">
              <span className="mb-1 block text-[var(--muted)]">Presence</span>
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
                <option value="everyone">Everyone</option>
                <option value="friends">Friends</option>
                <option value="nobody">Nobody</option>
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
              Read receipts
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
        <h2 className="text-sm font-semibold">Notifications</h2>
        {notifications ? (
          <div className="mt-3 space-y-2 text-sm">
            {[
              ["desktop_enabled", "Desktop notifications"],
              ["sound_enabled", "Notification sound"],
              ["mentions_only", "Mentions only"],
              ["suppress_everyone", "Suppress @everyone"],
              ["email_enabled", "Email notifications"],
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
