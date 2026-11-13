import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { request } from "../../lib/api";
import type { Notification } from "../../lib/types";

export function NotificationsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const notifications = useQuery({
    queryKey: ["notifications"],
    queryFn: () => request<Notification[]>("/api/v1/notifications?limit=50"),
  });

  const markAll = useMutation({
    mutationFn: () =>
      request("/api/v1/notifications/read", {
        method: "POST",
        body: { all: true },
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["notifications"] }),
        queryClient.invalidateQueries({ queryKey: ["sync"] }),
      ]);
    },
  });

  const markOne = useMutation({
    mutationFn: (id: string) =>
      request("/api/v1/notifications/read", {
        method: "POST",
        body: { ids: [id] },
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["notifications"] }),
        queryClient.invalidateQueries({ queryKey: ["sync"] }),
      ]);
    },
  });

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("notifications")}</h1>
        <button
          className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm text-slate-200 disabled:opacity-50"
          disabled={markAll.isPending || !notifications.data?.length}
          onClick={() => markAll.mutate()}
        >
          {t("markAllRead")}
        </button>
      </div>

      <div className="mt-4 max-w-3xl space-y-2">
        {notifications.isLoading ? (
          <p className="text-sm text-[var(--muted)]">{t("loading")}</p>
        ) : notifications.data?.length ? (
          notifications.data.map((notification) => (
            <article
              key={notification.id}
              className={`rounded-xl border px-4 py-3 ${
                notification.read_at
                  ? "border-[var(--border)] bg-[var(--surface-2)]"
                  : "border-[var(--accent)]/40 bg-[var(--accent)]/10"
              }`}
            >
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="text-sm font-medium">
                    {notification.kind === "mention"
                      ? t("mentioned")
                      : notification.kind}
                  </p>
                  <p className="mt-1 text-xs text-[var(--muted)]">
                    {notification.created_at
                      ? new Date(notification.created_at).toLocaleString()
                      : ""}
                  </p>
                </div>
                {!notification.read_at ? (
                  <button
                    className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                    onClick={() => markOne.mutate(notification.id ?? "")}
                  >
                    {t("markRead")}
                  </button>
                ) : (
                  <span className="text-xs text-[var(--muted)]">{t("read")}</span>
                )}
              </div>
            </article>
          ))
        ) : (
          <p className="text-sm text-[var(--muted)]">{t("noNotifications")}</p>
        )}
      </div>
    </div>
  );
}
