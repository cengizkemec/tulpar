import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";

interface EmbedToken {
  token: string;
  expires_in: number;
  widget: string;
  community_id: string;
  authenticated: boolean;
}

interface EmbedCommunity {
  id: string;
  name: string;
  description?: string | null;
  member_count: number;
  voice_channel_count: number;
  public_url: string;
  authenticated: boolean;
}

export function EmbedWidgetPage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const key = params.get("key") ?? "";
  const widget = params.get("widget") ?? "community";
  const origin = params.get("origin") ?? "";
  const [community, setCommunity] = useState<EmbedCommunity | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!key || !origin) {
      setError(t("embedInvalidRequest"));
      return;
    }
    let cancelled = false;
    async function load() {
      try {
        const tokenResponse = await fetch("/api/v1/embed/token", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ public_key: key, origin, widget }),
        });
        if (!tokenResponse.ok) {
          throw new Error(t("embedUnavailable"));
        }
        const token = (await tokenResponse.json()) as EmbedToken;
        const communityResponse = await fetch("/api/v1/embed/community", {
          headers: { authorization: `Bearer ${token.token}` },
        });
        if (!communityResponse.ok) {
          throw new Error(t("embedUnavailable"));
        }
        const data = (await communityResponse.json()) as EmbedCommunity;
        if (!cancelled) {
          setCommunity(data);
          setError(null);
        }
      } catch (cause) {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : t("embedUnavailable"));
        }
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [key, origin, t, widget]);

  useEffect(() => {
    if (!community || !origin) {
      return;
    }
    const send = (type: string) => {
      window.parent.postMessage(
        { type, widget, height: document.documentElement.scrollHeight },
        origin,
      );
    };
    send("tulpar:ready");
    const observer = new ResizeObserver(() => send("tulpar:resize"));
    observer.observe(document.body);
    return () => observer.disconnect();
  }, [community, origin, widget]);

  return (
    <main className="min-h-screen bg-[var(--surface)] p-4 text-slate-100">
      {error ? (
        <div className="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] p-4 text-sm text-red-300">
          {error}
        </div>
      ) : community ? (
        <article className="rounded-xl border border-[var(--border)] bg-[var(--surface-2)] p-4">
          <h1 className="text-lg font-semibold">{community.name}</h1>
          {community.description ? (
            <p className="mt-1 text-sm text-[var(--muted)]">
              {community.description}
            </p>
          ) : null}
          <div className="mt-3 flex flex-wrap gap-3 text-xs text-[var(--muted)]">
            <span>{t("embedMembers", { count: community.member_count })}</span>
            <span>
              {t("embedVoiceChannels", { count: community.voice_channel_count })}
            </span>
            {community.authenticated ? <span>{t("embedMemberView")}</span> : null}
          </div>
          <a
            className="mt-4 inline-block rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white"
            href={`${community.public_url}/app`}
            target="_blank"
            rel="noreferrer"
          >
            {t("embedOpenTulpar")}
          </a>
        </article>
      ) : (
        <p className="text-sm text-[var(--muted)]">{t("loading")}</p>
      )}
    </main>
  );
}
