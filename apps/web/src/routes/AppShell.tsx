import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useLocation, useNavigate, useParams } from "react-router-dom";

import { signOut } from "../features/auth/session";
import { MessagePanel } from "../features/chat/MessagePanel";
import { FriendsPage } from "../features/friends/FriendsPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { VoicePanel } from "../features/voice/VoicePanel";
import { request } from "../lib/api";
import { useGateway } from "../lib/gateway";
import type { Channel, SyncCommunity, SyncResponse } from "../lib/types";
import { useSession } from "../stores/session";

export function AppShell() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const params = useParams();
  const view = location.pathname.startsWith("/app/friends")
    ? "friends"
    : location.pathname.startsWith("/app/settings")
      ? "settings"
      : "channel";
  const user = useSession((state) => state.user);
  useGateway(queryClient);

  const sync = useQuery({
    queryKey: ["sync"],
    queryFn: () => request<SyncResponse>("/api/v1/sync"),
  });

  const communities = sync.data?.communities ?? [];
  const selectedCommunity = useMemo(() => {
    if (!params.communityId) {
      return communities[0];
    }
    return (
      communities.find(
        (entry) => entry.community?.id === params.communityId,
      ) ?? communities[0]
    );
  }, [communities, params.communityId]);

  const allChannels = selectedCommunity?.channels ?? [];
  const selectedChannel = useMemo(() => {
    if (params.channelId) {
      const direct = sync.data?.dms?.find(
        (dm) => dm.channel_id === params.channelId,
      );
      if (direct) {
        return {
          id: direct.channel_id,
          kind: "direct",
          name: t("directMessages"),
          topic: undefined,
        } satisfies Partial<Channel>;
      }
      return allChannels.find((channel) => channel.id === params.channelId);
    }
    return undefined;
  }, [allChannels, params.channelId, sync.data?.dms, t]);

  useEffect(() => {
    if (view !== "channel" || params.channelId || !selectedCommunity) {
      return;
    }
    const first = firstTextChannel(selectedCommunity);
    if (first) {
      navigate(`/app/c/${selectedCommunity.community?.id}/${first}`, {
        replace: true,
      });
    }
  }, [navigate, params.channelId, selectedCommunity, view]);

  const unread = sync.data?.unread_counts ?? {};

  return (
    <div className="flex h-screen overflow-hidden bg-[var(--surface)] text-slate-100">
      <aside className="flex w-64 shrink-0 flex-col border-r border-[var(--border)] bg-[var(--surface-2)]">
        <div className="flex items-center gap-2 border-b border-[var(--border)] px-4 py-4">
          <span className="grid h-8 w-8 place-items-center rounded-xl bg-[var(--accent)] font-bold">
            T
          </span>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold">{t("appName")}</p>
            <p className="truncate text-xs text-[var(--muted)]">
              {sync.data?.instance?.name ?? ""}
            </p>
          </div>
        </div>

        <div className="border-b border-[var(--border)] px-2 py-3">
          <p className="px-2 pb-2 text-[11px] font-semibold uppercase tracking-wider text-[var(--muted)]">
            {t("communities")}
          </p>
          <div className="flex flex-wrap gap-1">
            {communities.map((entry) => {
              const community = entry.community;
              if (!community?.id) {
                return null;
              }
              const active = community.id === selectedCommunity?.community?.id;
              const first = firstTextChannel(entry);
              return (
                <button
                  key={community.id}
                  title={community.name}
                  className={`grid h-9 w-9 place-items-center rounded-xl text-sm font-semibold ${
                    active
                      ? "bg-[var(--accent)] text-white"
                      : "bg-[var(--surface-3)] text-slate-300 hover:bg-[var(--border)]"
                  }`}
                  onClick={() =>
                    first
                      ? navigate(`/app/c/${community.id}/${first}`)
                      : navigate(`/app/c/${community.id}`)
                  }
                >
                  {(community.name ?? "?").slice(0, 1).toUpperCase()}
                </button>
              );
            })}
          </div>
        </div>

        <div className="flex gap-1 border-b border-[var(--border)] px-2 py-2">
          <button
            className={`flex-1 rounded-lg px-2 py-1.5 text-xs font-medium ${
              view === "friends"
                ? "bg-[var(--surface-3)] text-white"
                : "text-[var(--muted)] hover:bg-[var(--surface-3)]"
            }`}
            onClick={() => navigate("/app/friends")}
          >
            {t("friends")}
          </button>
          <button
            className={`flex-1 rounded-lg px-2 py-1.5 text-xs font-medium ${
              view === "settings"
                ? "bg-[var(--surface-3)] text-white"
                : "text-[var(--muted)] hover:bg-[var(--surface-3)]"
            }`}
            onClick={() => navigate("/app/settings")}
          >
            {t("settings")}
          </button>
        </div>

        <nav className="flex-1 overflow-y-auto px-2 py-3">
          <p className="px-2 pb-2 text-[11px] font-semibold uppercase tracking-wider text-[var(--muted)]">
            {t("channels")}
          </p>
          {selectedCommunity ? (
            <ChannelGroups
              community={selectedCommunity}
              selectedChannelId={selectedChannel?.id}
              unread={unread}
              onSelect={(channelId) =>
                navigate(
                  `/app/c/${selectedCommunity.community?.id}/${channelId}`,
                )
              }
            />
          ) : (
            <p className="px-2 text-sm text-[var(--muted)]">{t("loading")}</p>
          )}

          <p className="mt-5 px-2 pb-2 text-[11px] font-semibold uppercase tracking-wider text-[var(--muted)]">
            {t("directMessages")}
          </p>
          <div className="space-y-0.5">
            {(sync.data?.dms ?? []).map((dm) => (
              <button
                key={dm.channel_id}
                className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-left text-sm ${
                  dm.channel_id === params.channelId
                    ? "bg-[var(--surface-3)] text-white"
                    : "text-slate-300 hover:bg-[var(--surface-3)]"
                }`}
                onClick={() => navigate(`/app/dm/${dm.channel_id}`)}
              >
                <span className="truncate">DM · {dm.other_user_id?.slice(0, 8)}</span>
                {unread[dm.channel_id ?? ""] ? (
                  <span className="rounded-full bg-[var(--accent)] px-1.5 text-[10px] font-semibold">
                    {unread[dm.channel_id ?? ""]}
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </nav>

        <div className="flex items-center gap-2 border-t border-[var(--border)] px-3 py-3">
          <span className="grid h-8 w-8 place-items-center rounded-full bg-[var(--surface-3)] text-xs font-semibold">
            {(user?.display_name ?? "?").slice(0, 1).toUpperCase()}
          </span>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">
              {user?.display_name ?? user?.username}
            </p>
            <p className="truncate text-xs text-[var(--muted)]">
              {user?.username}
            </p>
          </div>
          <button
            className="rounded-lg px-2 py-1 text-xs text-[var(--muted)] hover:bg-[var(--surface-3)] hover:text-white"
            onClick={() => void signOut()}
          >
            {t("logout")}
          </button>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        {view === "friends" ? (
          <FriendsPage />
        ) : view === "settings" ? (
          <SettingsPage />
        ) : selectedChannel ? (
          <>
            <header className="flex items-center justify-between border-b border-[var(--border)] px-5 py-3">
              <div>
                <h1 className="text-base font-semibold">
                  {selectedChannel.kind === "direct"
                    ? t("directMessages")
                    : `# ${selectedChannel.name ?? ""}`}
                </h1>
                <p className="text-xs text-[var(--muted)]">
                  {selectedChannel.topic ?? ""}
                </p>
              </div>
              <span className="rounded-full bg-[var(--surface-3)] px-2.5 py-1 text-xs text-[var(--muted)]">
                {selectedChannel.kind === "direct"
                  ? t("directMessages")
                  : t(
                      selectedChannel.kind === "voice"
                        ? "typeVoice"
                        : "typeText",
                    )}
              </span>
            </header>
            {selectedChannel.kind === "voice" ? (
              <VoicePanel channel={selectedChannel as Channel} />
            ) : (
              <MessagePanel channelId={selectedChannel.id ?? ""} />
            )}
          </>
        ) : (
          <div className="grid flex-1 place-items-center text-sm text-[var(--muted)]">
            {sync.isLoading ? t("loading") : t("noChannel")}
          </div>
        )}
      </main>
    </div>
  );
}

function firstTextChannel(community: SyncCommunity): string | undefined {
  return community.channels?.find(
    (channel) => channel.kind === "text" || channel.kind === "announcement",
  )?.id;
}

function ChannelGroups({
  community,
  selectedChannelId,
  unread,
  onSelect,
}: {
  community: SyncCommunity;
  selectedChannelId?: string;
  unread: Record<string, number>;
  onSelect: (channelId: string) => void;
}) {
  const channels = community.channels ?? [];
  const categories = channels.filter((channel) => channel.kind === "category");
  const uncategorized = channels.filter(
    (channel) => !channel.parent_id && channel.kind !== "category",
  );

  const renderChannel = (channel: Channel) => (
    <button
      key={channel.id}
      className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-left text-sm ${
        channel.id === selectedChannelId
          ? "bg-[var(--surface-3)] text-white"
          : "text-slate-300 hover:bg-[var(--surface-3)]"
      }`}
      onClick={() => channel.id && onSelect(channel.id)}
    >
      <span className="truncate">
        {channel.kind === "voice" ? "🔊" : "#"} {channel.name}
      </span>
      {channel.id && unread[channel.id] ? (
        <span className="rounded-full bg-[var(--accent)] px-1.5 text-[10px] font-semibold">
          {unread[channel.id]}
        </span>
      ) : null}
    </button>
  );

  return (
    <div className="space-y-1">
      {uncategorized.map(renderChannel)}
      {categories.map((category) => (
        <div key={category.id} className="pt-2">
          <p className="px-2 pb-1 text-[11px] font-semibold uppercase tracking-wide text-[var(--muted)]">
            {category.name}
          </p>
          {(channels ?? [])
            .filter((channel) => channel.parent_id === category.id)
            .map(renderChannel)}
        </div>
      ))}
    </div>
  );
}
