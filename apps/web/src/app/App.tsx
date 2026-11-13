import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { bootstrapSession } from "../features/auth/session";
import { useSession } from "../stores/session";
import { AppShell } from "../routes/AppShell";
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
  const token = useSession((state) => state.accessToken);
  const bootstrapped = useSession((state) => state.bootstrapped);
  if (!bootstrapped) {
    return <div className="p-8 text-sm text-[var(--muted)]">Loading…</div>;
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
  useEffect(() => {
    if (!bootstrapped) {
      void bootstrapSession();
    }
  }, [bootstrapped]);
  return children;
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Bootstrap>
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
            <Route path="*" element={<Navigate to="/app" replace />} />
          </Routes>
        </Bootstrap>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
