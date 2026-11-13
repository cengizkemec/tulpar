import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useSearchParams } from "react-router-dom";

import {
  desktopLogout,
  desktopProbeInstance,
  type DesktopInstanceInfo,
} from "../../lib/desktop";
import { useInstances } from "../../stores/instances";

export function DesktopConnectPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const instances = useInstances((state) => state.instances);
  const setCurrent = useInstances((state) => state.setCurrent);
  const remove = useInstances((state) => state.remove);
  const [url, setUrl] = useState(params.get("url") ?? "");
  const [info, setInfo] = useState<DesktopInstanceInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function probe() {
    setPending(true);
    setError(null);
    setInfo(null);
    try {
      const result = await desktopProbeInstance(url);
      setInfo(result);
      setUrl(result.url);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    } finally {
      setPending(false);
    }
  }

  function connect(instance: { url: string; name: string }) {
    setCurrent(instance.url, instance.name);
    navigate("/login", { replace: true });
  }

  async function forget(instanceUrl: string) {
    if (!window.confirm(t("desktopForgetConfirm"))) {
      return;
    }
    try {
      await desktopLogout(instanceUrl);
    } catch {
      // Removing the local entry still makes the client usable.
    }
    remove(instanceUrl);
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <section className="w-full max-w-lg rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-7 shadow-2xl">
        <h1 className="text-2xl font-semibold">{t("desktopConnectTitle")}</h1>
        <p className="mt-2 text-sm text-[var(--muted)]">
          {t("desktopConnectSubtitle")}
        </p>

        <label className="mt-6 block text-sm">
          <span className="mb-1 block text-[var(--muted)]">
            {t("desktopServerAddress")}
          </span>
          <input
            data-testid="desktop-server-url"
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            placeholder="https://tulpar.example.com"
            autoComplete="url"
          />
        </label>
        {error ? (
          <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}
        {info ? (
          <div className="mt-4 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4 text-sm">
            <p className="font-semibold">{info.name}</p>
            <p className="mt-1 text-xs text-[var(--muted)]">
              {info.url} · v{info.version}
            </p>
            <button
              data-testid="desktop-connect"
              className="mt-3 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white"
              onClick={() => connect(info)}
            >
              {t("desktopConnectAction")}
            </button>
          </div>
        ) : (
          <button
            data-testid="desktop-check-server"
            className="mt-4 rounded-lg bg-[var(--accent)] px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
            disabled={pending || !url.trim()}
            onClick={() => void probe()}
          >
            {pending ? t("loading") : t("desktopCheckServer")}
          </button>
        )}

        {instances.length ? (
          <div className="mt-6">
            <h2 className="text-sm font-semibold">{t("desktopSavedServers")}</h2>
            <div className="mt-2 space-y-2">
              {instances.map((instance) => (
                <div
                  key={instance.url}
                  className="flex items-center justify-between gap-3 rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium">{instance.name}</p>
                    <p className="truncate text-xs text-[var(--muted)]">
                      {instance.url}
                    </p>
                  </div>
                  <div className="flex shrink-0 gap-2">
                    <button
                      className="rounded bg-[var(--surface-3)] px-2 py-1 text-xs"
                      onClick={() => connect(instance)}
                    >
                      {t("desktopOpen")}
                    </button>
                    <button
                      className="rounded bg-red-500/15 px-2 py-1 text-xs text-red-200"
                      onClick={() => void forget(instance.url)}
                    >
                      {t("remove")}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </section>
    </main>
  );
}
