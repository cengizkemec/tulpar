import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router-dom";

import { register } from "../lib/api";
import { signIn } from "../features/auth/session";

export function RegisterPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setNotice(null);
    try {
      const result = await register({
        username,
        display_name: displayName || username,
        email,
        password,
      });
      if (result.verification_required) {
        setNotice("Check your email to verify the account, then sign in.");
        return;
      }
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
        <h1 className="text-2xl font-semibold">{t("registerTitle")}</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">{t("registerSubtitle")}</p>
        <label className="mt-5 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("username")}</span>
          <input
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            autoComplete="username"
            required
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("displayName")}</span>
          <input
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("email")}</span>
          <input
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            type="email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            autoComplete="email"
            required
          />
        </label>
        <label className="mt-3 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">{t("password")}</span>
          <input
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            autoComplete="new-password"
            minLength={12}
            required
          />
        </label>
        {error ? (
          <p className="mt-4 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}
        {notice ? (
          <p className="mt-4 rounded-lg bg-emerald-500/10 px-3 py-2 text-sm text-emerald-200">
            {notice}
          </p>
        ) : null}
        <button
          className="mt-6 w-full rounded-lg bg-[var(--accent)] px-3 py-2 font-medium text-white hover:bg-[var(--accent-strong)] disabled:opacity-60"
          disabled={pending}
        >
          {pending ? t("loading") : t("register")}
        </button>
        <Link className="mt-4 block text-center text-sm text-[var(--muted)] hover:text-white" to="/login">
          {t("haveAccount")}
        </Link>
      </form>
    </main>
  );
}
