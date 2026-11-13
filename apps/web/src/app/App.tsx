import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { BrowserRouter, Navigate, Route, Routes, useNavigate } from "react-router-dom";

import { AdminPage } from "../features/admin/AdminPage";
import { bootstrapSession } from "../features/auth/session";
import { CommunitySettingsPage } from "../features/communities/CommunitySettingsPage";
import { DesktopConnectPage } from "../features/desktop/DesktopConnectPage";
import { isDesktop, onDeepLink, onPtt, desktopRegisterPtt, desktopTakeDeepLinks } from "../lib/desktop";
import { useInstances } from "../stores/instances";
import { type InstallPromptEvent, usePwa } from "../stores/pwa";
import { useSession } from "../stores/session";
import { AppShell } from "../routes/AppShell";
import { DevicePage } from "../routes/DevicePage";
import { LoginPage } from "../routes/LoginPage";
import { RegisterPage } from "../routes/RegisterPage";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

function Protected({ children }: { children: React.ReactNode }) {
  const { t } = useTranslation();
  const token = useSession((state) => state.accessToken);
  const bootstrapped = useSession((state) => state.bootstrapped);
  if (!bootstrapped) {
    return <div className="p-8 text-sm text-[var(--muted)]">{t("loading")}</div>;
  }
  if (!token) {
    return <Navigate to="/login" replace />;
  }
  return children;
}

function PublicOnly({ children }: { children: React.ReactNode }) {
  const token = useSession((state) => state.accessToken);
  const bootstrapped = useSession((state) => state.bootstrapped);
  if (bootstrapped && token) {
    return <Navigate to="/app" replace />;
  }
  return children;
}

function Bootstrap({ children }: { children: React.ReactNode }) {
  const bootstrapped = useSession((state) => state.bootstrapped);
  const currentUrl = useInstances((state) => state.currentUrl);
  const setCurrent = useInstances((state) => state.setCurrent);
  useEffect(() => {
    if (!currentUrl) {
      if (!isDesktop()) {
        setCurrent(window.location.origin, window.location.host);
      }
      return;
    }
    if (!bootstrapped) {
      void bootstrapSession();
    }
  }, [bootstrapped, currentUrl, setCurrent]);
  return children;
}

function RequireInstance({ children }: { children: React.ReactNode }) {
  const currentUrl = useInstances((state) => state.currentUrl);
  if (isDesktop() && !currentUrl) {
    return <DesktopConnectPage />;
  }
  return children;
}

function DesktopEvents() {
  const navigate = useNavigate();
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    const shortcut = window.localStorage.getItem("tulpar.ptt");
    if (shortcut) {
      void desktopRegisterPtt(shortcut).catch(() => undefined);
    }
    let unlisten: (() => void) | undefined;
    const handleDeepLink = (payload: string) => {
      try {
        const url = new URL(payload);
        if (url.protocol !== "tulpar:" || url.hostname !== "add-instance") {
          return;
        }
        const target = url.searchParams.get("url");
        if (target && target.startsWith("https://")) {
          navigate(`/connect?url=${encodeURIComponent(target)}`);
        }
      } catch {
        // Ignore malformed deep links.
      }
    };
    void onDeepLink(handleDeepLink).then((fn) => {
      unlisten = fn;
    });
    void onPtt((pressed) => {
      document.documentElement.dataset.ptt = pressed ? "down" : "up";
    });
    void desktopTakeDeepLinks().then((links) => {
      for (const link of links) {
        handleDeepLink(link);
      }
    });
    return () => unlisten?.();
  }, [navigate]);
  return null;
}

export function App() {
  const setPrompt = usePwa((state) => state.setPrompt);
  const setInstalled = usePwa((state) => state.setInstalled);
  useEffect(() => {
    const onPrompt = (event: Event) => {
      event.preventDefault();
      setPrompt(event as InstallPromptEvent);
    };
    const onInstalled = () => setInstalled(true);
    window.addEventListener("beforeinstallprompt", onPrompt);
    window.addEventListener("appinstalled", onInstalled);
    if (window.matchMedia("(display-mode: standalone)").matches) {
      setInstalled(true);
    }
    return () => {
      window.removeEventListener("beforeinstallprompt", onPrompt);
      window.removeEventListener("appinstalled", onInstalled);
    };
  }, [setInstalled, setPrompt]);

  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Bootstrap>
          <DesktopEvents />
          <RequireInstance>
            <Routes>
            <Route
              path="/login"
              element={
                <PublicOnly>
                  <LoginPage />
                </PublicOnly>
              }
            />
            <Route
              path="/register"
              element={
                <PublicOnly>
                  <RegisterPage />
                </PublicOnly>
              }
            />
            <Route
              path="/device"
              element={<DevicePage />}
            />
            <Route
              path="/connect"
              element={
                isDesktop() ? (
                  <DesktopConnectPage />
                ) : (
                  <Navigate to="/app" replace />
                )
              }
            />
            <Route
              path="/app"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/app/c/:communityId/:channelId"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/app/dm/:channelId"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/app/c/:communityId/settings"
              element={
                <Protected>
                  <CommunitySettingsPage />
                </Protected>
              }
            />
            <Route
              path="/app/friends"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/app/settings"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/app/notifications"
              element={
                <Protected>
                  <AppShell />
                </Protected>
              }
            />
            <Route
              path="/admin"
              element={
                <Protected>
                  <AdminPage />
                </Protected>
              }
            />
            <Route path="*" element={<Navigate to="/app" replace />} />
            </Routes>
          </RequireInstance>
        </Bootstrap>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
