"use client";

import { useEffect, useState } from "react";
import { Copy } from "lucide-react";

import { api } from "@/lib/api";
import type { Status, UpdateInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import { SavedIndicator } from "@/components/saved-indicator";
import { WarningsBanner } from "@/components/warnings-banner";
import { useConfigEditor } from "@/lib/use-config-editor";
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
import { Label } from "@/components/ui/label";
import { NativeSelect } from "@/components/ui/native-select";
import { Switch } from "@/components/ui/switch";

/** Inline feedback for a manual update check, rendered next to the button. */
type UpdateResult = { text: string; tone: "ok" | "error" | "info" };

function describeUpdate(u: UpdateInfo): UpdateResult {
  if (u.available) {
    return {
      text: `Linkport v${u.latest} is available — download it from the releases page.`,
      tone: "info",
    };
  }
  if (u.error) {
    return { text: u.error, tone: "error" };
  }
  return { text: `You are up to date (v${u.current}).`, tone: "ok" };
}

export default function SettingsPage() {
  const { config, setConfig, error, setError, warnings, mutate, justSaved } =
    useConfigEditor();
  const [status, setStatus] = useState<Status | null>(null);
  const [regDetail, setRegDetail] = useState<string | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  // Port edits stay local until blur: saving per keystroke would persist
  // half-typed values. Committed when valid, reverted when not.
  const [portDraft, setPortDraft] = useState<string | null>(null);

  function load() {
    Promise.all([api.getConfig(), api.status()])
      .then(([c, s]) => {
        setConfig(c);
        setStatus(s);
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }

  useEffect(load, []);

  function commitPort() {
    if (!config || portDraft === null) return;
    const port = Number(portDraft);
    setPortDraft(null); // back to showing config.portal.port
    if (!Number.isInteger(port) || port < 1024 || port > 65535) return;
    if (port !== config.portal.port) {
      mutate((c) => {
        c.portal.port = port;
      });
    }
  }

  async function doRegister(action: "register" | "unregister") {
    try {
      const res =
        action === "register" ? await api.register() : await api.unregister();
      setRegDetail(res.detail);
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  // Update-check feedback renders inline in the Updates card next to the
  // button that triggered it — a page-top message is too easy to miss.
  const [updateResult, setUpdateResult] = useState<UpdateResult | null>(null);

  async function checkUpdate() {
    setCheckingUpdate(true);
    setError(null);
    setUpdateResult(null);
    try {
      const res = await api.checkUpdate();
      setUpdateResult(describeUpdate(res.update));
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setCheckingUpdate(false);
    }
  }

  // The select shows a valid choice even when default_browser dangles
  // (the backend re-resolves it on save).
  const browserEntries = config ? Object.entries(config.browsers) : [];
  const currentDefault =
    config?.default_browser && config.browsers[config.default_browser]
      ? config.default_browser
      : (browserEntries[0]?.[0] ?? "");

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center gap-3">
        <h1 className="text-2xl font-semibold">Settings</h1>
        <SavedIndicator show={justSaved} />
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}
      <WarningsBanner warnings={warnings} />

      <div className="grid gap-6 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Routing defaults</CardTitle>
            <CardDescription>Used when no rule matches</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            {config && (
              <>
                <div className="flex flex-col gap-2">
                  <Label htmlFor="default-browser">Default browser</Label>
                  <NativeSelect
                    id="default-browser"
                    value={currentDefault}
                    onChange={(e) => {
                      const v = e.target.value || null;
                      mutate((c) => {
                        c.default_browser = v;
                      });
                    }}
                  >
                    {browserEntries.length === 0 ? (
                      <option value="">(no browsers configured yet)</option>
                    ) : (
                      browserEntries.map(([id, b]) => (
                        <option key={id} value={id}>
                          {b.display_name}
                        </option>
                      ))
                    )}
                  </NativeSelect>
                  <p className="text-xs text-muted-foreground">
                    Used when no rule matches. Pre-set to your system default
                    browser on first save; Linkport keeps a valid choice if you
                    delete a browser.
                  </p>
                </div>
                <div className="flex flex-col gap-2">
                  <Label htmlFor="portal-port">Portal port</Label>
                  <Input
                    id="portal-port"
                    type="number"
                    min={1024}
                    max={65535}
                    value={portDraft ?? String(config.portal.port)}
                    onChange={(e) => setPortDraft(e.target.value)}
                    onBlur={commitPort}
                    onKeyDown={(e) => e.key === "Enter" && commitPort()}
                  />
                  <p className="text-xs text-muted-foreground">
                    Saved when you leave the field; takes effect after restarting
                    Linkport — right-click the tray icon and pick{" "}
                    <strong>Restart</strong>.
                  </p>
                </div>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Windows integration</CardTitle>
            <CardDescription>Default-browser registration</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 text-sm">
            <div className="flex items-center justify-between">
              <span>Registered as browser candidate</span>
              {!status?.registered ? (
                <Badge variant="outline">not registered</Badge>
              ) : status.handler_ok ? (
                <Badge>registered</Badge>
              ) : (
                <Badge variant="destructive">handler broken</Badge>
              )}
            </div>
            {status?.registered && !status.handler_ok && (
              <p className="text-xs text-destructive">
                The registered link handler points at a moved or deleted folder —
                clicked links fail with “Application not found”. Run linkport.exe
                from its new folder once (it self-repairs), or press Register.
              </p>
            )}
            <div className="flex gap-2">
              {/* Register doubles as the repair for a broken handler, so it
                  only greys out when registration is fully healthy. */}
              <Button
                variant="outline"
                onClick={() => doRegister("register")}
                disabled={!!status?.registered && !!status.handler_ok}
              >
                Register
              </Button>
              <Button
                variant="outline"
                onClick={() => doRegister("unregister")}
                disabled={!status?.registered}
              >
                Unregister
              </Button>
            </div>
            {regDetail && <p className="text-xs text-muted-foreground">{regDetail}</p>}
            <p className="text-xs text-muted-foreground">
              After registering, open Windows Settings → Apps → Default apps → Linkport
              and set it as the default for HTTP/HTTPS. Windows does not allow apps to
              set this programmatically.
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Updates</CardTitle>
            <CardDescription>GitHub release checks</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 text-sm">
            <div className="flex items-center justify-between gap-4">
              <Label htmlFor="check-updates">Check for updates on start</Label>
              <Switch
                id="check-updates"
                checked={config?.check_updates ?? true}
                disabled={!config}
                onCheckedChange={(v) => {
                  mutate((c) => {
                    c.check_updates = v;
                  });
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span>Latest release</span>
              <div className="flex items-center gap-2">
                {/* h-8 matches the Check-now button height so the pair reads as one control row */}
                {!status?.update ? (
                  <Badge variant="outline" className="h-8">
                    not checked yet
                  </Badge>
                ) : status.update.available ? (
                  <a
                    href={status.update.url}
                    target="_blank"
                    rel="noreferrer"
                    className={buttonVariants({ variant: "outline", size: "sm" })}
                  >
                    v{status.update.latest} available
                  </a>
                ) : status.update.error ? (
                  <Badge variant="outline" className="h-8" title={status.update.error}>
                    check failed
                  </Badge>
                ) : (
                  <Badge variant="outline" className="h-8">
                    up to date (v{status.update.current})
                  </Badge>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={checkUpdate}
                  disabled={checkingUpdate}
                >
                  {checkingUpdate ? "Checking…" : "Check now"}
                </Button>
              </div>
            </div>
            {updateResult && (
              <p
                className={cn(
                  "text-sm font-medium",
                  updateResult.tone === "ok" && "text-emerald-600 dark:text-emerald-400",
                  updateResult.tone === "error" && "text-destructive",
                  updateResult.tone === "info" && "text-foreground",
                )}
                role="status"
              >
                {updateResult.text}
              </p>
            )}
            <p className="text-xs text-muted-foreground">
              The startup check and the tray menu’s “Check for updates” contact only
              api.github.com to compare the newest published tag against the running
              version — nothing is downloaded or installed automatically. Use the
              tray menu’s download action or the button above to get the zip.
            </p>
          </CardContent>
        </Card>

        <Card className="xl:col-span-2">
          <CardHeader>
            <CardTitle>Portal access</CardTitle>
            <CardDescription>
              The portal only listens on 127.0.0.1 and requires this token
            </CardDescription>
          </CardHeader>
          <CardContent className="flex items-center gap-2 text-sm">
            <code className="bg-muted min-w-0 flex-1 truncate rounded px-2 py-1.5">
              {status?.portal_url ?? "…"}
            </code>
            <Button
              variant="outline"
              size="icon"
              aria-label="Copy portal URL"
              onClick={() =>
                status && navigator.clipboard.writeText(status.portal_url)
              }
            >
              <Copy />
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
