import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

import type { SelfUser } from "../../lib/types";
import {
  desktopOpenExternal,
  desktopPollDeviceLogin,
  desktopStartDeviceLogin,
  type DeviceStart,
} from "../../lib/desktop";
import { useInstances } from "../../stores/instances";
import { useSession } from "../../stores/session";

type Status =
  | "idle"
  | "starting"
  | "waiting"
  | "approved"
  | "denied"
  | "expired";

export function DesktopLogin() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const currentUrl = useInstances((state) => state.currentUrl);
  const currentName = useInstances((state) => state.currentName);
  const removeInstance = useInstances((state) => state.remove);
  const setSession = useSession((state) => state.setSession);
  const [start, setStart] = useState<DeviceStart | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function begin() {
    setStatus("starting");
    setError(null);
    try {
      const device = await desktopStartDeviceLogin(
        currentUrl,
        `Tulpar Desktop (${navigator.platform || "desktop"})`,
      );
      setStart(device);
      setStatus("waiting");
      try {
        await desktopOpenExternal(device.verificationUri);
      } catch {
        // The approval page can still be opened manually below.
      }
    } catch (cause) {
      setStatus("idle");
      setError(cause instanceof Error ? cause.message : t("errorGeneric"));
    }
  }

  useEffect(() => {
    if (status !== "waiting" || !start) {
      return;
    }
    let cancelled = false;
    const device = start;
    const deadline = Date.now() + device.expiresIn * 1000;
    const interval = Math.max(device.interval, 2) * 1000;

    async function poll() {
      while (!cancelled && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, interval));
        if (cancelled) {
          return;
        }
        try {
          const result = await desktopPollDeviceLogin(
            currentUrl,
            device.deviceCode,
          );
          if (result.status === "approved" && result.accessToken) {
            setSession(result.accessToken, result.user as SelfUser);
            setStatus("approved");
            navigate("/app", { replace: true });
            return;
          }
          if (result.status === "denied" || result.status === "expired") {
            setStatus(result.status);
            setError(
              result.message ??
                (result.status === "denied"
                  ? t("deviceDenied")
                  : t("desktopExpired")),
            );
            return;
          }
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : t("errorGeneric"));
        }
      }
      if (!cancelled) {
        setStatus("expired");
        setError(t("desktopExpired"));
      }
    }

    void poll();
    return () => {
      cancelled = true;
    };
  }, [currentUrl, navigate, setSession, start, status, t]);

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <section className="w-full max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-7 shadow-2xl">
        <h1 className="text-2xl font-semibold">{t("desktopLoginTitle")}</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">
          {currentName || currentUrl}
        </p>
        <p className="mt-2 text-sm text-[var(--muted)]">
          {t("desktopLoginSubtitle")}
        </p>

        {error ? (
          <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        ) : null}

        {status === "waiting" && start ? (
          <div className="mt-5 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4">
            <p className="text-sm text-[var(--muted)]">
              {t("desktopDeviceCodeHint")}
            </p>
            <p
              data-testid="desktop-user-code"
              className="mt-2 font-mono text-2xl font-semibold tracking-widest"
            >
              {start.userCode}
            </p>
            <div className="mt-3 flex flex-wrap gap-2">
              <button
                className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
                onClick={async () => {
                  await navigator.clipboard.writeText(start.userCode);
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 2000);
                }}
              >
                {copied ? t("desktopCopied") : t("desktopCopyCode")}
              </button>
              <button
                className="rounded-lg bg-[var(--surface-3)] px-3 py-2 text-sm"
                onClick={() => void desktopOpenExternal(start.verificationUri)}
              >
                {t("desktopOpenApproval")}
              </button>
            </div>
            <p className="mt-3 text-xs text-[var(--muted)]">
              {t("desktopWaitingApproval")}
            </p>
          </div>
        ) : status === "approved" ? (
          <p className="mt-5 text-sm text-[var(--success)]">
            {t("desktopApprovedRedirect")}
          </p>
        ) : (
          <button
            data-testid="desktop-start-signin"
            className="mt-5 w-full rounded-lg bg-[var(--accent)] px-3 py-2 font-medium text-white disabled:opacity-50"
            disabled={status === "starting"}
            onClick={() => void begin()}
          >
            {status === "starting" ? t("loading") : t("desktopStartSignIn")}
          </button>
        )}

        <button
          className="mt-4 text-xs text-[var(--muted)] hover:text-white"
          onClick={() => {
            removeInstance(currentUrl);
            navigate("/", { replace: true });
          }}
        >
          {t("desktopChangeServer")}
        </button>
      </section>
    </main>
  );
}
