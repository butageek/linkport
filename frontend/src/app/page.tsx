"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { Plus, RefreshCw } from "lucide-react";

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
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    Promise.all([api.status(), api.events(30)])
      .then(([s, e]) => {
        setStatus(s);
        setEvents(e);
        setError(null);
      })
      .catch((err) => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  useEffect(load, [load]);

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Dashboard</h1>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <div className="grid gap-6 lg:grid-cols-2">
        <StatusCard status={status} />
        <TestCard />
      </div>
      <RecentEvents events={events} onRefresh={load} />
    </div>
  );
}

function StatusCard({ status }: { status: Status | null }) {
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
              {status.registered ? (
                <Badge>registered</Badge>
              ) : (
                <Badge variant="outline">not registered</Badge>
              )}
            </Row>
            <Row label="Browsers / rules">
              {status.browsers_count} / {status.rules_count}
            </Row>
            <Row label="Default browser">
              {status.default_browser ?? <span className="text-muted-foreground">none</span>}
            </Row>
            <Row label="Config">
              <code className="text-xs">{status.config_path}</code>
            </Row>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right">{children}</span>
    </div>
  );
}

function TestCard() {
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
              <span className="text-muted-foreground">Outcome:</span>
              <OutcomeBadge outcome={decision.outcome} />
            </div>
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

function RecentEvents({ events, onRefresh }: { events: EventItem[]; onRefresh: () => void }) {
  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <div className="flex flex-col gap-1.5">
          <CardTitle>Recent links</CardTitle>
          <CardDescription>Newest first — create a rule straight from a host</CardDescription>
        </div>
        <Button variant="ghost" size="icon" onClick={onRefresh} aria-label="Refresh">
          <RefreshCw />
        </Button>
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
                <TableHead>Outcome</TableHead>
                <TableHead className="text-right">Rule</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {events.map((e, i) => (
                <TableRow key={i}>
                  <TableCell className="text-muted-foreground">
                    {new Date(e.timestamp).toLocaleString()}
                  </TableCell>
                  <TableCell className="max-w-96 truncate">{e.url}</TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1">
                      <OutcomeBadge outcome={e.outcome} />
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
                        href={`/rules?host=${encodeURIComponent(e.host)}`}
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
