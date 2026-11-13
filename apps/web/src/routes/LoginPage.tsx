import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router-dom";

import { signIn } from "../features/auth/session";
import { useInstances } from "../stores/instances";

export function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const serverName = useInstances((state) => state.currentName);
  const serverUrl = useInstances((state) => state.currentUrl);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      await signIn(email, password);
      navigate("/app", { replace: true });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    } finally {
      setPending(false);
    }
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <form
        onSubmit={submit}
        className="w-full max-w-sm rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-7 shadow-2xl"
      >
        <h1 className="text-2xl font-semibold">{t("loginTitle")}</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">{t("loginSubtitle")}</p>
        <p className="mt-2 text-xs text-[var(--muted)]">
          {serverName || serverUrl}{" "}
          <Link className="text-[var(--accent)] hover:underline" to="/connect">
            change server
          </Link>
        </p>
        <label className="mt-6 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("email")}</span>
          <input
            data-testid="login-email"
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            autoComplete="username"
            required
          />
        </label>
        <label className="mt-4 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("password")}</span>
          <input
            data-testid="login-password"
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            autoComplete="current-password"
            required
          />
        </label>
        {error ? (
          <p className="mt-4 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}
        <button
          data-testid="login-submit"
          className="mt-6 w-full rounded-lg bg-[var(--accent)] px-3 py-2 font-medium text-white hover:bg-[var(--accent-strong)] disabled:opacity-60"
          disabled={pending}
        >
          {pending ? t("loading") : t("login")}
        </button>
        <Link className="mt-4 block text-center text-sm text-[var(--muted)] hover:text-white" to="/register">
          {t("noAccount")}
        </Link>
      </form>
    </main>
  );
}
