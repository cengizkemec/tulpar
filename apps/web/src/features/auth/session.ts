import {
  currentUser,
  login as apiLogin,
  logout as apiLogout,
  refreshSession,
} from "../../lib/api";
import { useSession } from "../../stores/session";

export async function bootstrapSession(): Promise<void> {
  const session = useSession.getState();
  try {
    if (!session.accessToken) {
      const hasSessionHint =
        window.localStorage.getItem("tulpar.hasSession") === "1";
      if (!hasSessionHint) {
        session.setBootstrapped();
        return;
      }
      const refreshed = await refreshSession();
      if (!refreshed) {
        session.clear();
        return;
      }
    }
    const user = await currentUser();
    useSession.getState().setSession(
      useSession.getState().accessToken ?? "",
      user,
    );
  } catch {
    session.clear();
  } finally {
    useSession.getState().setBootstrapped();
  }
}

export async function signIn(email: string, password: string): Promise<void> {
  const response = await apiLogin(email, password);
  useSession.getState().setSession(response.access_token, response.user);
}

export async function signOut(): Promise<void> {
  try {
    await apiLogout();
  } finally {
    useSession.getState().clear();
  }
}
