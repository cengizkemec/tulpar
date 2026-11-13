import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { request } from "../../lib/api";
import type { Message } from "../../lib/types";
import { useSession } from "../../stores/session";

export function MessagePanel({ channelId }: { channelId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const user = useSession((state) => state.user);
  const [content, setContent] = useState("");

  const messages = useQuery({
    queryKey: ["messages", channelId],
    queryFn: () =>
      request<Message[]>(
        `/api/v1/channels/${channelId}/messages?limit=50`,
      ),
    enabled: Boolean(channelId),
  });

  const send = useMutation({
    mutationFn: (text: string) =>
      request<Message>(`/api/v1/channels/${channelId}/messages`, {
        method: "POST",
        body: { content: text, client_nonce: crypto.randomUUID() },
      }),
    onSuccess: async () => {
      setContent("");
      await queryClient.invalidateQueries({
        queryKey: ["messages", channelId],
      });
    },
  });

  const remove = useMutation({
    mutationFn: (messageId: string) =>
      request<void>(`/api/v1/messages/${messageId}`, { method: "DELETE" }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: ["messages", channelId],
      });
    },
  });

  const ordered = [...(messages.data ?? [])].reverse();

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex-1 space-y-3 overflow-y-auto px-5 py-4">
        {messages.isLoading ? (
          <p className="text-sm text-[var(--muted)]">{t("loading")}</p>
        ) : ordered.length === 0 ? (
          <p className="text-sm text-[var(--muted)]">{t("emptyMessages")}</p>
        ) : (
          ordered.map((message) => {
            const mine = message.author_id === user?.id;
            return (
              <article
                key={message.id}
                className="group rounded-xl border border-transparent px-3 py-2 hover:border-[var(--border)] hover:bg-[var(--surface-2)]"
              >
                <div className="flex items-baseline gap-2">
                  <span className="text-sm font-semibold">
                    {message.author_id?.slice(0, 8) ?? "system"}
                  </span>
                  <time className="text-[11px] text-[var(--muted)]">
                    {message.created_at
                      ? new Date(message.created_at).toLocaleTimeString()
                      : ""}
                  </time>
                  {message.edited_at ? (
                    <span className="text-[10px] text-[var(--muted)]">edited</span>
                  ) : null}
                  {mine ? (
                    <button
                      className="ml-auto hidden rounded px-1.5 py-0.5 text-[10px] text-[var(--muted)] hover:bg-red-500/15 hover:text-red-300 group-hover:block"
                      onClick={() => remove.mutate(message.id ?? "")}
                    >
                      {t("delete")}
                    </button>
                  ) : null}
                </div>
                <p className="mt-0.5 whitespace-pre-wrap break-words text-sm text-slate-200">
                  {message.content ?? (message.deleted_at ? "message deleted" : "")}
                </p>
                {message.attachments?.length ? (
                  <p className="mt-1 text-xs text-[var(--muted)]">
                    📎 {message.attachments.length} attachment(s)
                  </p>
                ) : null}
              </article>
            );
          })
        )}
      </div>

      <form
        className="border-t border-[var(--border)] p-3"
        onSubmit={(event) => {
          event.preventDefault();
          const text = content.trim();
          if (text && !send.isPending) {
            send.mutate(text);
          }
        }}
      >
        <div className="flex items-end gap-2">
          <textarea
            data-testid="message-input"
            className="max-h-40 min-h-10 flex-1 resize-y rounded-xl border border-[var(--border)] bg-[var(--surface-2)] px-3 py-2 text-sm outline-none focus:border-[var(--accent)]"
            placeholder={t("messagePlaceholder")}
            value={content}
            onChange={(event) => setContent(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
              }
            }}
          />
          <button
            data-testid="message-send"
            className="rounded-xl bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white hover:bg-[var(--accent-strong)] disabled:opacity-50"
            disabled={!content.trim() || send.isPending}
          >
            {send.isPending ? t("loading") : t("send")}
          </button>
        </div>
        {send.error ? (
          <p className="mt-2 text-xs text-red-300">
            {send.error instanceof Error ? send.error.message : t("errorGeneric")}
          </p>
        ) : null}
      </form>
    </div>
  );
}
