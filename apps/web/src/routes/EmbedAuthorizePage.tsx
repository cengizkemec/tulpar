import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router-dom";

import { useSession } from "../stores/session";

type Status = "idle" | "pending" | "done" | "error";

export function EmbedAuthorizePage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const token = useSession((state) => state.accessToken);
  const key = params.get("key") ?? "";
  const origin = params.get("origin") ?? "";
  const widget = params.get("widget") ?? "community";
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token || !key || !origin || status !== "idle") {
      return;
    }
    setStatus("pending");
    void (async () => {
      try {
        const response = await fetch("/api/v1/embed/authorize", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            authorization: `Bearer ${token}`,
          },
          body: JSON.stringify({ public_key: key, origin, widget }),
        });
        if (!response.ok) {
          throw new Error(t("embedAuthError"));
        }
        const data = (await response.json()) as { code: string };
        window.opener?.postMessage(
          { type: "tulpar:embed-code", code: data.code, widget },
          window.location.origin,
        );
        setStatus("done");
      } catch (cause) {
        setError(
          cause instanceof Error ? cause.message : t("embedAuthError"),
        );
        setStatus("error");
      }
    })();
  }, [key, origin, status, t, token, widget]);

  const next = `/embed/authorize?key=${encodeURIComponent(key)}&origin=${encodeURIComponent(origin)}&widget=${encodeURIComponent(widget)}`;

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <section className="w-full max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-6 text-center">
        <h1 className="text-xl font-semibold">{t("embedAuthTitle")}</h1>
        <p className="mt-2 text-sm text-[var(--muted)]">
          {t("embedAuthSubtitle")}
        </p>

        {!token ? (
          <Link
            className="mt-5 inline-block rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white"
            to={`/login?next=${encodeURIComponent(next)}`}
          >
            {t("login")}
          </Link>
        ) : status === "done" ? (
          <div className="mt-5">
            <p className="text-sm text-[var(--success)]">
              {t("embedAuthApproved")}
            </p>
            <button
              className="mt-3 rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
              onClick={() => window.close()}
            >
              {t("embedAuthClose")}
            </button>
          </div>
        ) : status === "error" ? (
          <p className="mt-5 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : (
          <p className="mt-5 text-sm text-[var(--muted)]">{t("loading")}</p>
        )}
      </section>
    </main>
  );
}
