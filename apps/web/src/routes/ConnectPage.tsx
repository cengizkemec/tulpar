import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";

import { normalizeServerAddress, probeInstance } from "../lib/instance";
import { useInstances } from "../stores/instances";
import { useSession } from "../stores/session";

export function ConnectPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const currentUrl = useInstances((state) => state.currentUrl);
  const currentName = useInstances((state) => state.currentName);
  const instances = useInstances((state) => state.instances);
  const setCurrent = useInstances((state) => state.setCurrent);
  const remove = useInstances((state) => state.remove);
  const [address, setAddress] = useState(currentUrl);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function connect(rawAddress: string) {
    setPending(true);
    setError(null);
    try {
      const normalized = normalizeServerAddress(rawAddress);
      const instance = await probeInstance(normalized);
      setCurrent(normalized, instance.name);
      useSession.getState().clear();
      queryClient.clear();
      navigate("/login", { replace: true });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Connection failed.");
    } finally {
      setPending(false);
    }
  }

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-6">
      <div className="w-full max-w-lg rounded-2xl border border-[var(--border)] bg-[var(--surface-2)] p-7 shadow-2xl">
        <h1 className="text-2xl font-semibold">Connect to a Tulpar server</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">
          Enter a domain or IP address, like TeamSpeak or Discord.
        </p>
        {currentName ? (
          <p className="mt-3 rounded-lg bg-[var(--accent)]/10 px-3 py-2 text-sm">
            Current server: <strong>{currentName}</strong> ({currentUrl})
          </p>
        ) : null}

        <form
          className="mt-5"
          onSubmit={(event) => {
            event.preventDefault();
            void connect(address);
          }}
        >
          <label className="block text-sm">
            <span className="mb-1 block text-[var(--muted)]">
              Server address
            </span>
            <input
              data-testid="connect-address"
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--surface)] px-3 py-2 outline-none focus:border-[var(--accent)]"
              placeholder="server.tulpar.com or 192.168.1.10:8080"
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              autoFocus
            />
          </label>
          {error ? (
            <p className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-sm text-red-300">
              {error}
            </p>
          ) : null}
          <button
            data-testid="connect-submit"
            className="mt-4 w-full rounded-lg bg-[var(--accent)] px-3 py-2 font-medium text-white hover:bg-[var(--accent-strong)] disabled:opacity-60"
            disabled={pending}
          >
            {pending ? "Connecting…" : "Connect"}
          </button>
        </form>

        {instances.length ? (
          <div className="mt-6">
            <h2 className="text-sm font-semibold">Saved servers</h2>
            <div className="mt-2 space-y-2">
              {instances.map((instance) => (
                <div
                  key={instance.url}
                  className="flex items-center justify-between rounded-lg bg-[var(--surface)] px-3 py-2 text-sm"
                >
                  <button
                    className="min-w-0 flex-1 text-left"
                    onClick={() => {
                      setAddress(instance.url);
                      void connect(instance.url);
                    }}
                  >
                    <span className="font-medium">{instance.name}</span>
                    <span className="ml-2 truncate text-xs text-[var(--muted)]">
                      {instance.url}
                    </span>
                  </button>
                  <button
                    className="ml-2 rounded px-2 py-1 text-xs text-[var(--muted)] hover:bg-red-500/15 hover:text-red-300"
                    onClick={() => remove(instance.url)}
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <Link
          className="mt-5 block text-center text-sm text-[var(--muted)] hover:text-white"
          to={useSession.getState().accessToken ? "/app" : "/login"}
        >
          Back
        </Link>
      </div>
    </main>
  );
}
