import { useSession } from "../stores/session";
import type { LoginResponse, Schemas, SelfUser } from "./types";

const baseUrl = (import.meta.env.VITE_API_URL ?? "").replace(/\/$/, "");

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

interface RequestOptions extends Omit<RequestInit, "body"> {
  body?: unknown;
  auth?: boolean;
  retryAuth?: boolean;
}

let refreshPromise: Promise<boolean> | null = null;

async function parse<T>(response: Response): Promise<T> {
  const text = await response.text();
  if (!response.ok) {
    let code = "request_failed";
    let message = text || response.statusText;
    try {
      const problem = JSON.parse(text) as {
        code?: string;
        detail?: string;
        title?: string;
      };
      code = problem.code ?? code;
      message = problem.detail ?? problem.title ?? message;
    } catch {
      // Keep the plain response text.
    }
    throw new ApiError(response.status, code, message);
  }
  return (text ? JSON.parse(text) : undefined) as T;
}

async function refreshAccessToken(): Promise<boolean> {
  if (!refreshPromise) {
    refreshPromise = (async () => {
      try {
        const response = await fetch(`${baseUrl}/api/v1/auth/refresh`, {
          method: "POST",
          credentials: "include",
        });
        if (!response.ok) {
          return false;
        }
        const payload = (await response.json()) as { access_token: string };
        useSession.getState().setAccessToken(payload.access_token);
        return true;
      } catch {
        return false;
      } finally {
        refreshPromise = null;
      }
    })();
  }
  return refreshPromise;
}

async function rawRequest<T>(
  path: string,
  options: RequestOptions,
  retried: boolean,
): Promise<T> {
  const headers = new Headers(options.headers);
  if (options.body !== undefined) {
    headers.set("content-type", "application/json");
  }
  if (options.auth !== false) {
    const token = useSession.getState().accessToken;
    if (token) {
      headers.set("authorization", `Bearer ${token}`);
    }
  }
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers,
    credentials: "include",
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  if (
    response.status === 401 &&
    options.auth !== false &&
    options.retryAuth !== false &&
    !retried
  ) {
    const refreshed = await refreshAccessToken();
    if (refreshed) {
      return rawRequest<T>(path, options, true);
    }
    useSession.getState().clear();
  }
  return parse<T>(response);
}

export function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  return rawRequest<T>(path, options, false);
}

export async function login(email: string, password: string): Promise<LoginResponse> {
  return request<LoginResponse>("/api/v1/auth/login", {
    method: "POST",
    auth: false,
    body: { login: email, password },
  });
}

export async function register(input: {
  username: string;
  display_name: string;
  email: string;
  password: string;
}): Promise<Schemas["RegistrationResponse"]> {
  return request<Schemas["RegistrationResponse"]>("/api/v1/auth/register", {
    method: "POST",
    auth: false,
    body: { ...input, locale: "en" },
  });
}

export async function currentUser(): Promise<SelfUser> {
  return request<SelfUser>("/api/v1/users/me", { retryAuth: false });
}

export async function refreshSession(): Promise<boolean> {
  return refreshAccessToken();
}

export async function logout(): Promise<void> {
  try {
    await request("/api/v1/auth/logout", {
      method: "POST",
      body: {},
    });
  } finally {
    useSession.getState().clear();
  }
}
