import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
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

interface EmbedApp {
  id: string;
  name: string;
  public_key: string;
  allowed_origins: string[];
  widgets: string[];
  allow_authenticated: boolean;
  revoked_at?: string | null;
}

interface Webhook {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  created_at: string;
  last_delivery_at?: string | null;
  last_error?: string | null;
}

export function CommunitySettingsPage() {
  const { t } = useTranslation();
  const { communityId = "" } = useParams();
  const queryClient = useQueryClient();
  const currentUser = useSession((state) => state.user);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [maxUses, setMaxUses] = useState("0");
  const [expiresHours, setExpiresHours] = useState("24");
  const [embedName, setEmbedName] = useState("");
  const [embedOrigins, setEmbedOrigins] = useState("http://localhost:8090");
  const [embedWidgets, setEmbedWidgets] = useState<string[]>([
    "community",
    "join",
  ]);
  const [embedAuth, setEmbedAuth] = useState(false);
  const [embedPreview, setEmbedPreview] = useState(false);
  const [embedChannel, setEmbedChannel] = useState("");
  const [webhookUrl, setWebhookUrl] = useState("");
  const [webhookSecret, setWebhookSecret] = useState<string | null>(null);
  const [webhookError, setWebhookError] = useState<string | null>(null);
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
  const embedApps = useQuery({
    queryKey: ["community-embed-apps", communityId],
    queryFn: () =>
      request<EmbedApp[]>(`/api/v1/communities/${communityId}/embed-apps`),
    enabled: can("MANAGE_COMMUNITY"),
  });
  const webhooks = useQuery({
    queryKey: ["community-webhooks", communityId],
    queryFn: () =>
      request<Webhook[]>(`/api/v1/communities/${communityId}/webhooks`),
    enabled: can("MANAGE_COMMUNITY"),
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
      queryClient.invalidateQueries({
        queryKey: ["community-embed-apps", communityId],
      }),
      queryClient.invalidateQueries({
        queryKey: ["community-webhooks", communityId],
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
      setError(cause instanceof Error ? cause.message : t("saveFailed")),
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
      setError(cause instanceof Error ? cause.message : t("inviteFailed")),
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
      setError(cause instanceof Error ? cause.message : t("kickFailed")),
  });

  const ban = useMutation({
    mutationFn: (userId: string) =>
      request(`/api/v1/communities/${communityId}/bans/${userId}`, {
        method: "PUT",
        body: { reason: t("removedByModerator") },
      }),
    onSuccess: invalidate,
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : t("banFailed")),
  });

  const unban = useMutation({
    mutationFn: (userId: string) =>
      request(`/api/v1/communities/${communityId}/bans/${userId}`, {
        method: "DELETE",
      }),
    onSuccess: invalidate,
  });

  const createEmbed = useMutation({
    mutationFn: () =>
      request<EmbedApp>(`/api/v1/communities/${communityId}/embed-apps`, {
        method: "POST",
        body: {
          name: embedName,
          allowed_origins: embedOrigins
            .split(",")
            .map((value) => value.trim())
            .filter(Boolean),
          widgets: embedWidgets,
          allow_authenticated: embedAuth,
          allow_message_preview: embedPreview,
          public_channel_id:
            embedPreview && embedChannel ? embedChannel : undefined,
        },
      }),
    onSuccess: async () => {
      setEmbedName("");
      setError(null);
      await invalidate();
    },
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : t("embedCreateFailed")),
  });

  const revokeEmbed = useMutation({
    mutationFn: (id: string) =>
      request(`/api/v1/embed-apps/${id}`, { method: "DELETE" }),
    onSuccess: invalidate,
    onError: (cause) =>
      setError(cause instanceof Error ? cause.message : t("embedRevokeFailed")),
  });

  const createWebhook = useMutation({
    mutationFn: () =>
      request<{ webhook: Webhook; secret: string }>(
        `/api/v1/communities/${communityId}/webhooks`,
        {
          method: "POST",
          body: { url: webhookUrl, events: ["message.created"] },
        },
      ),
    onSuccess: async (data) => {
      setWebhookSecret(data.secret);
      setWebhookUrl("");
      setWebhookError(null);
      await invalidate();
    },
    onError: (cause) =>
      setWebhookError(
        cause instanceof Error ? cause.message : t("webhookCreateFailed"),
      ),
  });

  const deleteWebhook = useMutation({
    mutationFn: (id: string) =>
      request(`/api/v1/webhooks/${id}`, { method: "DELETE" }),
    onSuccess: invalidate,
    onError: (cause) =>
      setWebhookError(
        cause instanceof Error ? cause.message : t("webhookDeleteFailed"),
      ),
  });

  const toggleEmbedWidget = (widget: string) => {
    setEmbedWidgets((current) =>
      current.includes(widget)
        ? current.filter((value) => value !== widget)
        : [...current, widget],
    );
  };

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <div className="mx-auto max-w-5xl">
        <header className="flex items-center justify-between">
          <div>
            <h1 className="text-xl font-semibold">{t("communitySettings")}</h1>
            <p className="text-sm text-[var(--muted)]">
              {details.data?.community.name ?? t("loading")}
            </p>
          </div>
          <Link
            className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
            to={`/app/c/${communityId}`}
          >
            {t("backToCommunity")}
          </Link>
        </header>

        {error ? (
          <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}

        {can("MANAGE_COMMUNITY") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("general")}</h2>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">{t("name")}</span>
                <input
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("visibility")}
                </span>
                <select
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={visibility}
                  onChange={(event) => setVisibility(event.target.value)}
                >
                  <option value="private">{t("private")}</option>
                  <option value="public">{t("public")}</option>
                </select>
              </label>
              <label className="text-sm md:col-span-2">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("description")}
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
              {t("saveChanges")}
            </button>
          </section>
        ) : null}

        {can("VIEW_MEMBER_LIST") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">
              {t("members")} ({members.data?.length ?? details.data?.member_count ?? 0})
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
                      {t("joined")} {new Date(member.joined_at).toLocaleDateString()}
                      {member.communication_disabled_until
                        ? ` · ${t("timedOut")}`
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
                          {t("kick")}
                        </button>
                      ) : null}
                      {can("BAN_MEMBERS") ? (
                        <button
                          className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                          onClick={() => ban.mutate(member.user_id)}
                        >
                          {t("ban")}
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
          <h2 className="text-sm font-semibold">{t("roles")}</h2>
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
                  {t("position")} {role.position} · {role.permissions.length}{" "}
                  {t("permissions")}
                  {role.is_default ? ` · ${t("defaultRole")}` : ""}
                </span>
              </div>
            ))}
          </div>
        </section>

        {can("MANAGE_COMMUNITY") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("embeds")}</h2>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("embedName")}
                </span>
                <input
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={embedName}
                  onChange={(event) => setEmbedName(event.target.value)}
                />
              </label>
              <label className="text-sm">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("embedOrigins")}
                </span>
                <input
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={embedOrigins}
                  onChange={(event) => setEmbedOrigins(event.target.value)}
                />
              </label>
              <div className="text-sm md:col-span-2">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("embedWidgets")}
                </span>
                <div className="flex flex-wrap gap-3">
                  {["community", "join", "members", "voice_preview", "channel_preview"].map(
                    (widget) => (
                      <label
                        key={widget}
                        className="flex items-center gap-1.5"
                      >
                        <input
                          type="checkbox"
                          checked={embedWidgets.includes(widget)}
                          onChange={() => toggleEmbedWidget(widget)}
                        />
                        {t(`embedWidget_${widget}`)}
                      </label>
                    ),
                  )}
                </div>
              </div>
              <label className="flex items-center gap-2 text-sm md:col-span-2">
                <input
                  type="checkbox"
                  checked={embedAuth}
                  onChange={(event) => setEmbedAuth(event.target.checked)}
                />
                {t("embedAllowAuth")}
              </label>
              <label className="flex items-center gap-2 text-sm md:col-span-2">
                <input
                  type="checkbox"
                  checked={embedPreview}
                  onChange={(event) => setEmbedPreview(event.target.checked)}
                />
                {t("embedAllowPreview")}
              </label>
              {embedPreview ? (
                <label className="text-sm md:col-span-2">
                  <span className="mb-1 block text-[var(--muted)]">
                    {t("embedPreviewChannel")}
                  </span>
                  <select
                    className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                    value={embedChannel}
                    onChange={(event) => setEmbedChannel(event.target.value)}
                  >
                    <option value="">{t("embedSelectChannel")}</option>
                    {(details.data?.channels ?? [])
                      .filter(
                        (channel) =>
                          channel.kind === "text" ||
                          channel.kind === "announcement",
                      )
                      .map((channel) => (
                        <option key={channel.id} value={channel.id}>
                          {channel.name}
                        </option>
                      ))}
                  </select>
                </label>
              ) : null}
            </div>
            <button
              className="mt-3 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
              disabled={
                createEmbed.isPending ||
                !embedName.trim() ||
                !embedWidgets.length
              }
              onClick={() => createEmbed.mutate()}
            >
              {t("embedCreate")}
            </button>
            <div className="mt-4 space-y-2">
              {(embedApps.data ?? []).map((app) => (
                <div
                  key={app.id}
                  className="rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <p className="font-medium">{app.name}</p>
                      <code className="text-xs text-[var(--muted)]">
                        {app.public_key}
                      </code>
                    </div>
                    {app.revoked_at ? (
                      <span className="text-xs text-[var(--muted)]">
                        {t("embedRevoked")}
                      </span>
                    ) : (
                      <button
                        className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                        onClick={() => revokeEmbed.mutate(app.id)}
                      >
                        {t("embedRevoke")}
                      </button>
                    )}
                  </div>
                  <p className="mt-1 text-xs text-[var(--muted)]">
                    {app.allowed_origins.join(", ")} ·{" "}
                    {app.widgets.join(", ")}
                  </p>
                </div>
              ))}
              {!embedApps.isLoading && !embedApps.data?.length ? (
                <p className="text-sm text-[var(--muted)]">{t("embedNoApps")}</p>
              ) : null}
            </div>
          </section>
        ) : null}

        {can("MANAGE_COMMUNITY") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("webhooks")}</h2>
            <p className="mt-1 text-xs text-[var(--muted)]">
              {t("webhooksHint")}
            </p>
            <div className="mt-3 flex flex-wrap items-end gap-2">
              <label className="min-w-64 flex-1 text-sm">
                <span className="mb-1 block text-[var(--muted)]">
                  {t("webhookUrl")}
                </span>
                <input
                  className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2"
                  value={webhookUrl}
                  onChange={(event) => setWebhookUrl(event.target.value)}
                  placeholder="https://example.com/tulpar-webhook"
                />
              </label>
              <button
                className="rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
                disabled={createWebhook.isPending || !webhookUrl.trim()}
                onClick={() => createWebhook.mutate()}
              >
                {t("webhookCreate")}
              </button>
            </div>
            {webhookSecret ? (
              <div className="mt-3 rounded-lg bg-[var(--surface)] px-3 py-2">
                <p className="text-xs text-[var(--muted)]">
                  {t("webhookSecretHint")}
                </p>
                <div className="mt-1 flex items-center gap-2">
                  <code className="min-w-0 flex-1 truncate text-xs">
                    {webhookSecret}
                  </code>
                  <button
                    className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                    onClick={() =>
                      void navigator.clipboard.writeText(webhookSecret)
                    }
                  >
                    {t("copy")}
                  </button>
                </div>
              </div>
            ) : null}
            {webhookError ? (
              <p className="mt-2 text-sm text-red-300">{webhookError}</p>
            ) : null}
            <div className="mt-4 space-y-2">
              {(webhooks.data ?? []).map((hook) => (
                <div
                  key={hook.id}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium">{hook.url}</p>
                    <p className="text-xs text-[var(--muted)]">
                      {hook.events.join(", ")}
                      {hook.last_delivery_at
                        ? ` · ${t("webhookLastDelivery")} ${new Date(
                            hook.last_delivery_at,
                          ).toLocaleString()}`
                        : ""}
                    </p>
                    {hook.last_error ? (
                      <p className="text-xs text-red-300">{hook.last_error}</p>
                    ) : null}
                  </div>
                  <button
                    className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                    onClick={() => deleteWebhook.mutate(hook.id)}
                  >
                    {t("webhookDelete")}
                  </button>
                </div>
              ))}
              {!webhooks.isLoading && !webhooks.data?.length ? (
                <p className="text-sm text-[var(--muted)]">
                  {t("webhookNoHooks")}
                </p>
              ) : null}
            </div>
          </section>
        ) : null}

        {can("MANAGE_INVITES") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("invites")}</h2>
            {can("CREATE_INVITES") ? (
              <div className="mt-3 flex flex-wrap items-end gap-2">
                <label className="text-xs">
                  {t("maxUses")}
                  <input
                    className="mt-1 w-28 rounded border border-[var(--border)] bg-[var(--surface)] px-2 py-1"
                    type="number"
                    min="0"
                    value={maxUses}
                    onChange={(event) => setMaxUses(event.target.value)}
                  />
                </label>
                <label className="text-xs">
                  {t("expiresHours")}
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
                  {t("createInvite")}
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
                    {invite.max_uses ? `/${invite.max_uses}` : ""} {t("uses")}
                    <button
                      className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                      onClick={() => void navigator.clipboard.writeText(invite.code)}
                    >
                      {t("copy")}
                    </button>
                    <button
                      className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                      onClick={() => revokeInvite.mutate(invite.code)}
                    >
                      {t("revoke")}
                    </button>
                  </span>
                </div>
              ))}
            </div>
          </section>
        ) : null}

        {can("BAN_MEMBERS") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("bans")}</h2>
            <div className="mt-3 space-y-2">
              {(bans.data ?? []).map((entry) => (
                <div
                  key={entry.user_id}
                  className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <span>
                    @{entry.username}{" "}
                    <span className="text-xs text-[var(--muted)]">
                      {entry.reason ?? t("noReason")}
                    </span>
                  </span>
                  <button
                    className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                    onClick={() => unban.mutate(entry.user_id)}
                  >
                    {t("unban")}
                  </button>
                </div>
              ))}
              {!bans.isLoading && !bans.data?.length ? (
                <p className="text-sm text-[var(--muted)]">{t("noActiveBans")}</p>
              ) : null}
            </div>
          </section>
        ) : null}

        {can("VIEW_AUDIT_LOG") ? (
          <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
            <h2 className="text-sm font-semibold">{t("auditLog")}</h2>
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
