import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

import { request } from "../../lib/api";
import type {
  BlockedUser,
  DirectConversation,
  Friend,
  Friendship,
  Schemas,
} from "../../lib/types";

type PublicUser = Schemas["PublicUserDto"];

export function FriendsPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const friends = useQuery({
    queryKey: ["friends"],
    queryFn: () => request<Friend[]>("/api/v1/friends"),
  });
  const requests = useQuery({
    queryKey: ["friend-requests"],
    queryFn: () => request<Friendship[]>("/api/v1/friends/requests"),
  });
  const blocks = useQuery({
    queryKey: ["blocks"],
    queryFn: () => request<BlockedUser[]>("/api/v1/blocks"),
  });
  const search = useQuery({
    queryKey: ["user-search", query],
    queryFn: () =>
      request<PublicUser[]>(
        `/api/v1/users?query=${encodeURIComponent(query)}&limit=10`,
      ),
    enabled: query.trim().length >= 2,
  });

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["friends"] }),
      queryClient.invalidateQueries({ queryKey: ["friend-requests"] }),
      queryClient.invalidateQueries({ queryKey: ["blocks"] }),
      queryClient.invalidateQueries({ queryKey: ["sync"] }),
    ]);
  };

  const addFriend = useMutation({
    mutationFn: (userId: string) =>
      request<void>("/api/v1/friends/requests", {
        method: "POST",
        body: { user_id: userId },
      }),
    onSuccess: invalidate,
  });
  const accept = useMutation({
    mutationFn: (id: string) =>
      request<void>(`/api/v1/friends/requests/${id}/accept`, {
        method: "POST",
      }),
    onSuccess: invalidate,
  });
  const decline = useMutation({
    mutationFn: (id: string) =>
      request<void>(`/api/v1/friends/requests/${id}/decline`, {
        method: "POST",
      }),
    onSuccess: invalidate,
  });
  const remove = useMutation({
    mutationFn: (userId: string) =>
      request<void>(`/api/v1/friends/${userId}`, { method: "DELETE" }),
    onSuccess: invalidate,
  });
  const block = useMutation({
    mutationFn: (userId: string) =>
      request<void>(`/api/v1/blocks/${userId}`, { method: "PUT" }),
    onSuccess: invalidate,
  });
  const unblock = useMutation({
    mutationFn: (userId: string) =>
      request<void>(`/api/v1/blocks/${userId}`, { method: "DELETE" }),
    onSuccess: invalidate,
  });
  const dm = useMutation({
    mutationFn: (userId: string) =>
      request<DirectConversation>("/api/v1/dms", {
        method: "POST",
        body: { recipient_id: userId },
      }),
    onSuccess: (conversation) => {
      void queryClient.invalidateQueries({ queryKey: ["sync"] });
      navigate(`/app/dm/${conversation.channel_id}`);
    },
  });

  const incoming = (requests.data ?? []).filter(
    (entry) => entry.direction === "incoming",
  );
  const outgoing = (requests.data ?? []).filter(
    (entry) => entry.direction === "outgoing",
  );

  return (
    <div className="flex-1 overflow-y-auto p-6">
      <h1 className="text-xl font-semibold">{t("friends")}</h1>

      <section className="mt-5 rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
        <h2 className="text-sm font-semibold">Find people</h2>
        <input
          className="mt-2 w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
          placeholder="Search username…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className="mt-3 space-y-2">
          {(search.data ?? []).map((user) => (
            <div
              key={user.id}
              className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
            >
              <span>
                <strong>{user.display_name}</strong>{" "}
                <span className="text-[var(--muted)]">@{user.username}</span>
              </span>
              <span className="flex gap-2">
                <button
                  className="rounded bg-[var(--accent)] px-2 py-1 text-xs font-medium text-white"
                  onClick={() => addFriend.mutate(user.id!)}
                >
                  Add friend
                </button>
                <button
                  className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                  onClick={() => dm.mutate(user.id!)}
                >
                  Message
                </button>
                <button
                  className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                  onClick={() => block.mutate(user.id!)}
                >
                  Block
                </button>
              </span>
            </div>
          ))}
          {query.length >= 2 && !search.isLoading && !search.data?.length ? (
            <p className="text-xs text-[var(--muted)]">No users found.</p>
          ) : null}
        </div>
      </section>

      {incoming.length ? (
        <section className="mt-5">
          <h2 className="text-sm font-semibold">Incoming requests</h2>
          <div className="mt-2 space-y-2">
            {incoming.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center justify-between rounded-xl border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-sm"
              >
                <span>
                  {entry.user.display_name}{" "}
                  <span className="text-[var(--muted)]">@{entry.user.username}</span>
                </span>
                <span className="flex gap-2">
                  <button
                    className="rounded bg-[var(--success)] px-2 py-1 text-xs font-semibold text-slate-950"
                    onClick={() => accept.mutate(entry.id!)}
                  >
                    Accept
                  </button>
                  <button
                    className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                    onClick={() => decline.mutate(entry.id!)}
                  >
                    Decline
                  </button>
                </span>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {outgoing.length ? (
        <section className="mt-5">
          <h2 className="text-sm font-semibold">Outgoing requests</h2>
          <div className="mt-2 space-y-2">
            {outgoing.map((entry) => (
              <div
                key={entry.id}
                className="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-sm text-[var(--muted)]"
              >
                Waiting for {entry.user.display_name} to respond.
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <section className="mt-5">
        <h2 className="text-sm font-semibold">{t("friends")}</h2>
        <div className="mt-2 grid gap-2 md:grid-cols-2">
          {(friends.data ?? []).map((friend) => (
            <div
              key={friend.id}
              className="flex items-center justify-between rounded-xl border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-sm"
            >
              <span>
                <strong>{friend.display_name}</strong>{" "}
                <span className="text-[var(--muted)]">@{friend.username}</span>
              </span>
              <span className="flex gap-2">
                <button
                  className="rounded bg-[var(--accent)] px-2 py-1 text-xs font-medium text-white"
                  onClick={() => dm.mutate(friend.id!)}
                >
                  Message
                </button>
                <button
                  className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                  onClick={() => remove.mutate(friend.id!)}
                >
                  Remove
                </button>
              </span>
            </div>
          ))}
          {!friends.isLoading && !friends.data?.length ? (
            <p className="text-sm text-[var(--muted)]">No friends yet.</p>
          ) : null}
        </div>
      </section>

      {blocks.data?.length ? (
        <section className="mt-5">
          <h2 className="text-sm font-semibold">Blocked</h2>
          <div className="mt-2 space-y-2">
            {blocks.data.map((blocked) => (
              <div
                key={blocked.id}
                className="flex items-center justify-between rounded-xl border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-sm"
              >
                <span>@{blocked.username}</span>
                <button
                  className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                  onClick={() => unblock.mutate(blocked.id!)}
                >
                  Unblock
                </button>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
