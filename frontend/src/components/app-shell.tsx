"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { Anchor, Globe, LayoutDashboard, Settings, Shuffle } from "lucide-react";

import { api, getToken, setToken } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

const NAV = [
  { href: "/", label: "Dashboard", icon: LayoutDashboard },
  { href: "/rules", label: "Rules", icon: Shuffle },
  { href: "/browsers", label: "Browsers", icon: Globe },
  { href: "/settings", label: "Settings", icon: Settings },
];

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const [status, setStatus] = useState<"loading" | "locked" | "ready">("loading");

  useEffect(() => {
    // Accept ?token= from the `linkport portal-url` launch link, then strip it.
    const params = new URLSearchParams(window.location.search);
    const t = params.get("token");
    if (t) {
      setToken(t);
      params.delete("token");
      const qs = params.toString();
      window.history.replaceState(
        {},
        "",
        window.location.pathname + (qs ? `?${qs}` : ""),
      );
    }

    if (!getToken()) {
      setStatus("locked");
      return;
    }
    api
      .status()
      .then(() => setStatus("ready"))
      .catch(() => setStatus("locked"));
  }, []);

  if (status === "loading") {
    return (
      <div className="flex min-h-screen items-center justify-center text-muted-foreground">
        Loading Linkport…
      </div>
    );
  }

  if (status === "locked") {
    return <TokenGate onUnlock={() => setStatus("ready")} />;
  }

  return (
    <div className="min-h-screen">
      <aside className="fixed inset-y-0 left-0 z-10 flex w-56 flex-col border-r bg-card">
        <div className="flex items-center gap-2 px-4 py-5 font-semibold">
          <Anchor className="size-5" />
          Linkport
        </div>
        <nav className="flex flex-col gap-1 px-2">
          {NAV.map(({ href, label, icon: Icon }) => (
            <Link
              key={href}
              href={href}
              className={cn(
                "flex items-center gap-2 rounded-md px-3 py-2 text-sm transition-colors",
                pathname === href
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
              )}
            >
              <Icon className="size-4" />
              {label}
            </Link>
          ))}
        </nav>
        <div className="mt-auto px-4 pb-4 text-xs text-muted-foreground">
          rule-based browser router
        </div>
      </aside>
      <main className="ml-56 p-8">{children}</main>
    </div>
  );
}

function TokenGate({ onUnlock }: { onUnlock: () => void }) {
  const [token, setTokenState] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setChecking(true);
    setError(null);
    setToken(token.trim());
    try {
      await api.ping();
      await api.status();
      onUnlock();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setChecking(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center">
      <form
        onSubmit={submit}
        className="w-full max-w-sm rounded-xl border bg-card p-6 shadow-sm flex flex-col gap-4"
      >
        <div className="flex items-center gap-2 font-semibold">
          <Anchor className="size-5" />
          Linkport portal
        </div>
        <p className="text-sm text-muted-foreground">
          Paste the token from <code className="rounded bg-muted px-1">linkport portal-url</code>{" "}
          to unlock the portal.
        </p>
        <Input
          value={token}
          onChange={(e) => setTokenState(e.target.value)}
          placeholder="portal token"
          autoFocus
        />
        {error && (
          <p className="text-sm text-destructive">
            Cannot reach the portal — is <code>linkport serve</code> running, and is the
            token correct?
          </p>
        )}
        <Button type="submit" disabled={!token.trim() || checking}>
          {checking ? "Checking…" : "Unlock"}
        </Button>
      </form>
    </div>
  );
}
