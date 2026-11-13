import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router-dom";

import { request } from "../lib/api";
import { useSession } from "../stores/session";

export function DevicePage() {
  const { t } = useTranslation();
  const [params] = useSearchParams();
  const token = useSession((state) => state.accessToken);
  const [code, setCode] = useState(params.get("user_code") ?? "");
  const [status, setStatus] = useState<"idle" | "approved" | "denied">("idle");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function submit(action: "approve" | "deny") {
    setPending(true);
    setError(null);
    try {
      await request(`/api/v1/auth/device/${action}`, {
        method: "POST",
        body: { user_code: code.trim() },
      });
      setStatus(action === "approve" ? "approved" : "denied");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    } finally {
      setPending(false);
    }
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <section className="w-full max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-7 shadow-2xl">
        <h1 className="text-2xl font-semibold">{t("deviceTitle")}</h1>
        <p className="mt-2 text-sm text-[var(--muted)]">{t("deviceSubtitle")}</p>

        {!token ? (
          <div className="mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4 text-sm">
            <p>{t("deviceSignInRequired")}</p>
            <Link
              className="mt-3 inline-block rounded-lg bg-[var(--accent)] px-3 py-2 font-medium text-white"
              to="/login?next=/device"
            >
              {t("login")}
            </Link>
          </div>
        ) : status !== "idle" ? (
          <div className="mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4 text-sm">
            <p className="font-medium">
              {status === "approved" ? t("deviceApproved") : t("deviceDenied")}
            </p>
            <p className="mt-1 text-[var(--muted)]">
              {status === "approved"
                ? t("deviceApprovedHint")
                : t("deviceDeniedHint")}
            </p>
          </div>
        ) : (
          <form
            className="mt-6"
            onSubmit={(event) => {
              event.preventDefault();
              void submit("approve");
            }}
          >
            <label className="block text-sm">
              <span className="mb-1 block text-[var(--muted)]">
                {t("deviceCode")}
              </span>
              <input
                data-testid="device-code"
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 font-mono text-lg tracking-widest outline-none focus:border-[var(--accent)]"
                value={code}
                onChange={(event) => setCode(event.target.value.toUpperCase())}
                autoComplete="one-time-code"
                required
              />
            </label>
            {error ? (
              <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
                {error}
              </p>
            ) : null}
            <div className="mt-4 flex gap-2">
              <button
                data-testid="device-approve"
                className="rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
                disabled={pending || !code.trim()}
              >
                {t("deviceApprove")}
              </button>
              <button
                className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
                type="button"
                disabled={pending || !code.trim()}
                onClick={() => void submit("deny")}
              >
                {t("deviceDeny")}
              </button>
            </div>
          </form>
        )}

        <Link
          className="mt-6 block text-center text-sm text-[var(--muted)] hover:text-white"
          to="/app"
        >
          {t("backToApp")}
        </Link>
      </section>
    </main>
  );
}
