"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { Plus, RefreshCw, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import type { Decision, EventItem, Status } from "@/lib/types";
import { OutcomeBadge } from "@/components/outcome-badge";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";

export default function DashboardPage() {
  const [status, setStatus] = useState<Status | null>(null);
  const [events, setEvents] = useState<EventItem[]>([]);
  const [browserNames, setBrowserNames] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    Promise.all([api.status(), api.events(30), api.getConfig()])
      .then(([s, e, c]) => {
        setStatus(s);
        setEvents(e);
        setBrowserNames(
          Object.fromEntries(
            Object.entries(c.browsers).map(([id, b]) => [id, b.display_name]),
          ),
        );
        setError(null);
      })
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  useEffect(load, [load]);

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Dashboard</h1>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <div className="grid gap-6 xl:grid-cols-2">
        <StatusCard status={status} browserNames={browserNames} />
        <TestCard browserNames={browserNames} />
      </div>
      <RecentEvents events={events} browserNames={browserNames} onRefresh={load} />
    </div>
  );
}

function StatusCard({
  status,
  browserNames,
}: {
  status: Status | null;
  browserNames: Record<string, string>;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Status</CardTitle>
        <CardDescription>Daemon and registration state</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2 text-sm">
        {!status ? (
          <span className="text-muted-foreground">loading…</span>
        ) : (
          <>
            <Row label="Version">{status.version}</Row>
            <Row label="Registered as browser">
              {!status.registered ? (
                <Badge variant="outline">not registered</Badge>
              ) : status.handler_ok ? (
                <Badge>registered</Badge>
              ) : (
                <Badge variant="destructive" title="The registered handler points at a moved or deleted install. Run linkport.exe once (it self-repairs) or press Register in Settings.">
                  handler broken
                </Badge>
              )}
            </Row>
            <Row label="Browsers / rules">
              {status.browsers_count} / {status.rules_count}
            </Row>
            <Row label="Routing">
              {status.paused ? (
                <Badge variant="secondary">paused — all links to default browser</Badge>
              ) : (
                <Badge>active</Badge>
              )}
            </Row>
            <Row label="Start at login">
              {status.autostart ? (
                <Badge>enabled</Badge>
              ) : (
                <Badge variant="outline">disabled</Badge>
              )}
            </Row>
            <Row label="Default browser">
              {status.default_browser ? (
                (browserNames[status.default_browser] ?? status.default_browser)
              ) : (
                <span className="text-muted-foreground">none</span>
              )}
            </Row>
            <Row label="Config">
              <code className="text-xs break-all">{status.config_path}</code>
            </Row>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      {/* min-w-0 lets long values (paths, URLs) wrap instead of overflowing */}
      <span className="min-w-0 text-right">{children}</span>
    </div>
  );
}

function TestCard({ browserNames }: { browserNames: Record<string, string> }) {
  const [url, setUrl] = useState("");
  const [decision, setDecision] = useState<Decision | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function run(e: React.FormEvent) {
    e.preventDefault();
    if (!url.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setDecision(await api.test(url.trim()));
    } catch (err) {
      setDecision(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Try a URL</CardTitle>
        <CardDescription>Dry-run a URL through your rules without opening anything</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <form onSubmit={run} className="flex gap-2">
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://docs.google.com/…"
          />
          <Button type="submit" disabled={busy || !url.trim()}>
            {busy ? "…" : "Test"}
          </Button>
        </form>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {decision && (
          <div className="flex flex-col gap-2 text-sm">
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground">Opened in:</span>
              <OutcomeBadge outcome={decision.outcome} browserNames={browserNames} />
            </div>
            {decision.resolved_url && (
              <p className="text-xs text-muted-foreground break-all">
                Linkport followed the redirect to{" "}
                <span className="font-medium">{decision.resolved_url}</span> and routed
                by that destination.
              </p>
            )}
            <div className="rounded-md border">
              {decision.trace.map((t, i) => (
                <div
                  key={i}
                  className="flex items-center justify-between gap-2 border-b px-3 py-1.5 text-xs last:border-b-0"
                >
                  <span>{t.name}</span>
                  <span className={t.matched ? "font-medium" : "text-muted-foreground"}>
                    {t.reason}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function RecentEvents({
  events,
  browserNames,
  onRefresh,
}: {
  events: EventItem[];
  browserNames: Record<string, string>;
  onRefresh: () => void;
}) {
  async function clear() {
    try {
      await api.clearEvents();
    } finally {
      onRefresh();
    }
  }

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <div className="flex flex-col gap-1.5">
          <CardTitle>Recent links</CardTitle>
          <CardDescription>Newest first — create a rule straight from a host</CardDescription>
        </div>
        <div className="flex">
          <Button
            variant="ghost"
            size="icon"
            onClick={clear}
            disabled={events.length === 0}
            aria-label="Clear history"
          >
            <Trash2 />
          </Button>
          <Button variant="ghost" size="icon" onClick={onRefresh} aria-label="Refresh">
            <RefreshCw />
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {events.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No links yet. Once Linkport is your default browser, every opened link shows
            up here.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>When</TableHead>
                <TableHead>URL</TableHead>
                <TableHead>Opened in</TableHead>
                <TableHead className="text-right">Rule</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {events.map((e, i) => (
                <TableRow key={i}>
                  <TableCell className="text-muted-foreground">
                    {new Date(e.timestamp).toLocaleString()}
                  </TableCell>
                  <TableCell className="max-w-96 truncate" title={e.origin_url ?? e.url}>
                    {e.url}
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1">
                      <OutcomeBadge outcome={e.outcome} browserNames={browserNames} />
                      {e.error && (
                        <Badge variant="destructive" title={e.error}>
                          error
                        </Badge>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-right">
                    {e.host && (
                      <Link
                        href={`/rules?${new URLSearchParams({ host: e.host, url: e.url })}`}
                        className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
                      >
                        <Plus /> from {e.host}
                      </Link>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
