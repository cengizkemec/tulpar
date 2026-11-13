import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";

import { request } from "../../lib/api";
import type {
  AuditLog,
  Ban,
  CommunityDetails,
  CommunityPermissions,
  Invite,
  Member,
  Role,
} from "../../lib/types";
import { useSession } from "../../stores/session";

export function CommunitySettingsPage() {
  const { communityId = "" } = useParams();
  const queryClient = useQueryClient();
  const currentUser = useSession((state) => state.user);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [maxUses, setMaxUses] = useState("0");
  const [expiresHours, setExpiresHours] = useState("24");
  const [error, setError] = useState<string | null>(null);
  const [createdCode, setCreatedCode] = useState<string | null>(null);

  const details = useQuery({
    queryKey: ["community", communityId],
    queryFn: () =>
      request<CommunityDetails>(`/api/v1/communities/${communityId}`),
    enabled: Boolean(communityId),
  });
  const permissions = useQuery({
    queryKey: ["community-permissions", communityId],
    queryFn: () =>
      request<CommunityPermissions>(
        `/api/v1/communities/${communityId}/permissions/me`,
      ),
    enabled: Boolean(communityId),
  });
  useEffect(() => {
    if (details.data?.community) {
      setName(details.data.community.name);
      setDescription(details.data.community.description ?? "");
      setVisibility(details.data.community.visibility);
    }
  }, [details.data]);

  const allowed = new Set(permissions.data?.permissions ?? []);
  const isOwner = permissions.data?.is_owner ?? false;
  const can = (permission: string) => isOwner || allowed.has(permission);

  const members = useQuery({
    queryKey: ["community-members", communityId],
    queryFn: () =>
      request<Member[]>(`/api/v1/communities/${communityId}/members`),
    enabled: can("VIEW_MEMBER_LIST"),
  });
  const roles = useQuery({
    queryKey: ["community-roles", communityId],
    queryFn: () => request<Role[]>(`/api/v1/communities/${communityId}/roles`),
    enabled: Boolean(communityId),
  });
  const invites = useQuery({
    queryKey: ["community-invites", communityId],
    queryFn: () =>
      request<Invite[]>(`/api/v1/communities/${communityId}/invites`),
    enabled: can("MANAGE_INVITES"),
  });
  const bans = useQuery({
    queryKey: ["community-bans", communityId],
    queryFn: () => request<Ban[]>(`/api/v1/communities/${communityId}/bans`),
    enabled: can("BAN_MEMBERS"),
  });
  const audit = useQuery({
    queryKey: ["community-audit", communityId],
    queryFn: () =>
      request<AuditLog[]>(
        `/api/v1/communities/${communityId}/audit-logs?limit=50`,
      ),
    enabled: can("VIEW_AUDIT_LOG"),
  });

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["community", communityId] }),
      queryClient.invalidateQueries({
        queryKey: ["community-members", communityId],
      }),
      queryClient.invalidateQueries({
        queryKey: ["community-invites", communityId],
      }),
      queryClient.invalidateQueries({
        queryKey: ["community-bans", communityId],
      }),
      queryClient.invalidateQueries({
        queryKey: ["community-audit", communityId],
      }),
      queryClient.invalidateQueries({ queryKey: ["sync"] }),
    ]);
  };

  const saveGeneral = useMutation({
    mutationFn: () =>
      request(`/api/v1/communities/${communityId}`, {
        method: "PATCH",
        body: { name, description, visibility },
      }),
    onSuccess: async () => {
      setError(null);
      await invalidate();
    },
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : "Save failed."),
  });

  const createInvite = useMutation({
    mutationFn: () =>
      request<Invite>(`/api/v1/communities/${communityId}/invites`, {
        method: "POST",
        body: {
          max_uses: Number(maxUses) > 0 ? Number(maxUses) : undefined,
          expires_in_hours:
            Number(expiresHours) > 0 ? Number(expiresHours) : undefined,
        },
      }),
    onSuccess: async (invite) => {
      setCreatedCode(invite.code);
      setError(null);
      await invalidate();
    },
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : "Invite failed."),
  });

  const revokeInvite = useMutation({
    mutationFn: (code: string) =>
      request(`/api/v1/invites/${code}`, { method: "DELETE" }),
    onSuccess: invalidate,
  });

  const kick = useMutation({
    mutationFn: (userId: string) =>
      request(`/api/v1/communities/${communityId}/members/${userId}`, {
        method: "DELETE",
      }),
    onSuccess: invalidate,
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : "Kick failed."),
  });

  const ban = useMutation({
    mutationFn: (userId: string) =>
      request(`/api/v1/communities/${communityId}/bans/${userId}`, {
        method: "PUT",
        body: { reason: "Removed by moderator" },
      }),
    onSuccess: invalidate,
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : "Ban failed."),
  });

  const unban = useMutation({
    mutationFn: (userId: string) =>
      request(`/api/v1/communities/${communityId}/bans/${userId}`, {
        method: "DELETE",
      }),
    onSuccess: invalidate,
  });

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl">
        <header className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold">Community settings</h1>
            <p className="text-sm text-[var(--muted)]">
              {details.data?.community.name ?? "Loading…"}
            </p>
          </div>
          <Link
            className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
            to={`/app/c/${communityId}`}
          >
            Back to community
          </Link>
        </header>

        {error ? (
          <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}

        {can("MANAGE_COMMUNITY") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">General</h2>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">Name</span>
                <input
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">
                  Visibility
                </span>
                <select
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={visibility}
                  onChange={(event) => setVisibility(event.target.value)}
                >
                  <option value="private">Private</option>
                  <option value="public">Public</option>
                </select>
              </label>
              <label className="text-sm md:col-span-2">
                <span className="mb-1 block text-[var(--muted)]">
                  Description
                </span>
                <textarea
                  className="min-h-20 w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={description}
                  onChange={(event) => setDescription(event.target.value)}
                />
              </label>
            </div>
            <button
              className="mt-3 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
              disabled={saveGeneral.isPending}
              onClick={() => saveGeneral.mutate()}
            >
              Save changes
            </button>
          </section>
        ) : null}

        {can("VIEW_MEMBER_LIST") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">
              Members ({members.data?.length ?? details.data?.member_count ?? 0})
            </h2>
            <div className="mt-3 divide-y divide-[var(--border)]">
              {(members.data ?? []).map((member) => (
                <div
                  key={member.user_id}
                  className="flex items-center justify-between gap-3 py-2 text-sm"
                >
                  <div>
                    <div className="font-medium">
                      {member.nickname || member.display_name}{" "}
                      <span className="text-xs text-[var(--muted)]">
                        @{member.username}
                      </span>
                    </div>
                    <div className="text-xs text-[var(--muted)]">
                      Joined {new Date(member.joined_at).toLocaleDateString()}
                      {member.communication_disabled_until
                        ? " · timed out"
                        : ""}
                    </div>
                  </div>
                  {member.user_id !== currentUser?.id ? (
                    <div className="flex gap-2">
                      {can("KICK_MEMBERS") ? (
                        <button
                          className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                          onClick={() => kick.mutate(member.user_id)}
                        >
                          Kick
                        </button>
                      ) : null}
                      {can("BAN_MEMBERS") ? (
                        <button
                          className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                          onClick={() => ban.mutate(member.user_id)}
                        >
                          Ban
                        </button>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </section>
        ) : null}

        <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
          <h2 className="text-sm font-semibold">Roles</h2>
          <div className="mt-3 space-y-2">
            {(roles.data ?? []).map((role) => (
              <div
                key={role.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
              >
                <span className="font-medium" style={{ color: `#${(role.color ?? 0).toString(16).padStart(6, "0")}` }}>
                  {role.name}
                </span>
                <span className="text-xs text-[var(--muted)]">
                  position {role.position} · {role.permissions.length} permissions
                  {role.is_default ? " · default" : ""}
                </span>
              </div>
            ))}
          </div>
        </section>

        {can("MANAGE_INVITES") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">Invites</h2>
            {can("CREATE_INVITES") ? (
              <div className="mt-3 flex flex-wrap items-end gap-2">
                <label className="text-xs">
                  Max uses (0 = unlimited)
                  <input
                    className="mt-1 w-28 rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1"
                    type="number"
                    min="0"
                    value={maxUses}
                    onChange={(event) => setMaxUses(event.target.value)}
                  />
                </label>
                <label className="text-xs">
                  Expires (hours)
                  <input
                    className="mt-1 w-28 rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1"
                    type="number"
                    min="0"
                    value={expiresHours}
                    onChange={(event) => setExpiresHours(event.target.value)}
                  />
                </label>
                <button
                  className="rounded bg-[var(--accent)] px-3 py-2 text-xs font-medium text-white"
                  disabled={createInvite.isPending}
                  onClick={() => createInvite.mutate()}
                >
                  Create invite
                </button>
                {createdCode ? (
                  <code className="rounded bg-[var(--surface)] px-2 py-1 text-xs">
                    {createdCode}
                  </code>
                ) : null}
              </div>
            ) : null}
            <div className="mt-3 space-y-2">
              {(invites.data ?? []).map((invite) => (
                <div
                  key={invite.code}
                  className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <code>{invite.code}</code>
                  <span className="flex items-center gap-3 text-xs text-[var(--muted)]">
                    {invite.uses}
                    {invite.max_uses ? `/${invite.max_uses}` : ""} uses
                    <button
                      className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                      onClick={() => void navigator.clipboard.writeText(invite.code)}
                    >
                      Copy
                    </button>
                    <button
                      className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                      onClick={() => revokeInvite.mutate(invite.code)}
                    >
                      Revoke
                    </button>
                  </span>
                </div>
              ))}
            </div>
          </section>
        ) : null}

        {can("BAN_MEMBERS") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">Bans</h2>
            <div className="mt-3 space-y-2">
              {(bans.data ?? []).map((entry) => (
                <div
                  key={entry.user_id}
                  className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <span>
                    @{entry.username}{" "}
                    <span className="text-xs text-[var(--muted)]">
                      {entry.reason ?? "no reason"}
                    </span>
                  </span>
                  <button
                    className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                    onClick={() => unban.mutate(entry.user_id)}
                  >
                    Unban
                  </button>
                </div>
              ))}
              {!bans.isLoading && !bans.data?.length ? (
                <p className="text-sm text-[var(--muted)]">No active bans.</p>
              ) : null}
            </div>
          </section>
        ) : null}

        {can("VIEW_AUDIT_LOG") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">Audit log</h2>
            <div className="mt-3 space-y-2">
              {(audit.data ?? []).map((entry) => (
                <div
                  key={entry.id}
                  className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <span>
                    <strong>{entry.action}</strong>{" "}
                    <span className="text-xs text-[var(--muted)]">
                      {entry.actor_username ?? entry.actor_type}
                    </span>
                  </span>
                  <time className="text-xs text-[var(--muted)]">
                    {new Date(entry.created_at).toLocaleString()}
                  </time>
                </div>
              ))}
            </div>
          </section>
        ) : null}
      </div>
    </div>
  );
}
