import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router-dom";

import { ApiError, request } from "../../lib/api";
import type { AdminOverview, AdminUser, AuditLog } from "../../lib/types";

export function AdminPage() {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [actionError, setActionError] = useState<string | null>(null);

  const overview = useQuery({
    queryKey: ["admin-overview"],
    queryFn: () => request<AdminOverview>("/api/v1/admin/overview"),
    retry: false,
  });
  const users = useQuery({
    queryKey: ["admin-users", query],
    queryFn: () =>
      request<AdminUser[]>(
        `/api/v1/admin/users?query=${encodeURIComponent(query)}&limit=50`,
      ),
    enabled: overview.isSuccess,
  });
  const audit = useQuery({
    queryKey: ["admin-audit"],
    queryFn: () =>
      request<AuditLog[]>("/api/v1/admin/audit-logs?limit=30"),
    enabled: overview.isSuccess,
  });

  const update = useMutation({
    mutationFn: ({
      userId,
      body,
    }: {
      userId: string;
      body: Record<string, boolean>;
    }) =>
      request<AdminUser>(`/api/v1/admin/users/${userId}`, {
        method: "PATCH",
        body,
      }),
    onSuccess: async () => {
      setActionError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["admin-users"] }),
        queryClient.invalidateQueries({ queryKey: ["admin-overview"] }),
        queryClient.invalidateQueries({ queryKey: ["admin-audit"] }),
      ]);
    },
    onError: (error) => {
      setActionError(
        error instanceof Error ? error.message : "Action failed.",
      );
    },
  });

  if (
    overview.error instanceof ApiError &&
    overview.error.status === 403
  ) {
    return (
      <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
        <div className="max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-6 text-center">
          <h1 className="text-lg font-semibold">Instance administration</h1>
          <p className="mt-2 text-sm text-[var(--muted)]">
            This account is not an instance administrator.
          </p>
          <Link
            className="mt-4 inline-block rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white"
            to="/app"
          >
            Back to Tulpar
          </Link>
        </div>
      </main>
    );
  }

  return (
    <main className="min-h-screen bg-[var(--surface)] p-6 text-slate-100">
      <div className="mx-auto max-w-6xl">
        <header className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold">Instance administration</h1>
            <p className="text-sm text-[var(--muted)]">
              Users, activity, and audit records.
            </p>
          </div>
          <Link
            className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
            to="/app"
          >
            Back to app
          </Link>
        </header>

        <section className="mt-5 grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
          {[
            ["Users", overview.data?.users],
            ["Communities", overview.data?.communities],
            ["Messages", overview.data?.messages],
            ["Sessions", overview.data?.active_sessions],
            ["Files", overview.data?.files],
            ["Voice rooms", overview.data?.voice_channels],
          ].map(([label, value]) => (
            <div
              key={String(label)}
              className="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] p-3"
            >
              <p className="text-xs text-[var(--muted)]">{label}</p>
              <p className="mt-1 text-2xl font-semibold">{value ?? "—"}</p>
            </div>
          ))}
        </section>

        <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="text-sm font-semibold">Users</h2>
            <input
              className="ml-auto w-64 rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
              placeholder="Search users…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          {actionError ? (
            <p className="mt-2 text-sm text-red-300">{actionError}</p>
          ) : null}
          <div className="mt-3 overflow-x-auto">
            <table className="w-full min-w-[720px] text-left text-sm">
              <thead className="text-xs uppercase tracking-wide text-[var(--muted)]">
                <tr>
                  <th className="px-2 py-2">User</th>
                  <th className="px-2 py-2">Email</th>
                  <th className="px-2 py-2">Status</th>
                  <th className="px-2 py-2">Admin</th>
                  <th className="px-2 py-2" />
                </tr>
              </thead>
              <tbody>
                {(users.data ?? []).map((user) => (
                  <tr key={user.id} className="border-t border-[var(--border)]">
                    <td className="px-2 py-2">
                      <div className="font-medium">{user.display_name}</div>
                      <div className="text-xs text-[var(--muted)]">
                        @{user.username}
                      </div>
                    </td>
                    <td className="px-2 py-2 text-[var(--muted)]">
                      {user.email}
                    </td>
                    <td className="px-2 py-2">
                      {user.disabled_at ? (
                        <span className="text-red-300">disabled</span>
                      ) : user.deleted_at ? (
                        <span className="text-[var(--muted)]">deleted</span>
                      ) : (
                        <span className="text-[var(--success)]">active</span>
                      )}
                    </td>
                    <td className="px-2 py-2">
                      {user.is_instance_admin ? "yes" : "no"}
                    </td>
                    <td className="px-2 py-2">
                      <div className="flex justify-end gap-2">
                        <button
                          className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                          onClick={() =>
                            update.mutate({
                              userId: user.id!,
                              body: { disabled: !user.disabled_at },
                            })
                          }
                        >
                          {user.disabled_at ? "Enable" : "Disable"}
                        </button>
                        <button
                          className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                          onClick={() =>
                            update.mutate({
                              userId: user.id!,
                              body: {
                                instance_admin: !user.is_instance_admin,
                              },
                            })
                          }
                        >
                          {user.is_instance_admin
                            ? "Revoke admin"
                            : "Make admin"}
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
          <h2 className="text-sm font-semibold">Audit log</h2>
          <div className="mt-3 space-y-2 text-sm">
            {(audit.data ?? []).map((entry) => (
              <div
                key={entry.id}
                className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2"
              >
                <span>
                  <strong>{entry.action}</strong>{" "}
                  <span className="text-[var(--muted)]">
                    {entry.actor_username ?? entry.actor_type}
                  </span>
                </span>
                <time className="text-xs text-[var(--muted)]">
                  {entry.created_at
                    ? new Date(entry.created_at).toLocaleString()
                    : ""}
                </time>
              </div>
            ))}
          </div>
        </section>
      </div>
    </main>
  );
}
