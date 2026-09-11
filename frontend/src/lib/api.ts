import type {
  BrowsersResponse,
  Config,
  Decision,
  EventItem,
  Status,
  UpdateInfo,
} from "@/lib/types";

export const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

const TOKEN_KEY = "linkport_token";

export function getToken(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem(TOKEN_KEY) ?? "";
}

export function setToken(token: string) {
  window.localStorage.setItem(TOKEN_KEY, token);
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${getToken()}`,
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    throw new Error(`request failed (${res.status}): ${await res.text()}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  ping: () => call<{ ok: boolean; version: string }>("/api/ping"),
  status: () => call<Status>("/api/status"),
  getConfig: () => call<Config>("/api/config"),
  saveConfig: (config: Config) =>
    call<{ ok: boolean; warnings: string[]; config?: Config }>("/api/config", {
      method: "PUT",
      body: JSON.stringify(config),
    }),
  browsers: () => call<BrowsersResponse>("/api/browsers"),
  test: (url: string) =>
    call<Decision>("/api/test", {
      method: "POST",
      body: JSON.stringify({ url }),
    }),
  events: (limit = 50) => call<EventItem[]>(`/api/events?limit=${limit}`),
  clearEvents: () => call<{ ok: boolean }>("/api/events", { method: "DELETE" }),
  register: () =>
    call<{ ok: boolean; detail: string }>("/api/register", { method: "POST" }),
  unregister: () =>
    call<{ ok: boolean; detail: string }>("/api/unregister", { method: "POST" }),
  checkUpdate: () =>
    call<{ ok: boolean; update: UpdateInfo }>("/api/update/check", {
      method: "POST",
    }),
};
