"use client";

import { useEffect, useState } from "react";
import { Copy } from "lucide-react";

import { api } from "@/lib/api";
import type { Config, Status } from "@/lib/types";
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
import { Textarea } from "@/components/ui/textarea";

export default function SettingsPage() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [dirty, setDirty] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);

  function load() {
    Promise.all([api.getConfig(), api.status()])
      .then(([c, s]) => {
        setConfig(c);
        setStatus(s);
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }

  useEffect(load, []);

  async function save() {
    if (!config) return;
    try {
      const res = await api.saveConfig(config);
      setDirty(false);
      setMessage(
        res.warnings.length > 0
          ? `Saved with warnings: ${res.warnings.join("; ")}`
          : "Saved. Restart `linkport-cli serve` for a port change to take effect.",
      );
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function doRegister(action: "register" | "unregister") {
    try {
      const res =
        action === "register" ? await api.register() : await api.unregister();
      setMessage(res.detail);
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function checkUpdate() {
    setCheckingUpdate(true);
    setError(null);
    try {
      const res = await api.checkUpdate();
      setMessage(
        res.update.available
          ? `Linkport v${res.update.latest} is available — download it from the releases page.`
          : (res.update.error ??
            `You are up to date (v${res.update.current}).`),
      );
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
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Settings</h1>
        <Button onClick={save} disabled={!config || !dirty}>
          Save
        </Button>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}
      {message && <p className="text-sm text-muted-foreground">{message}</p>}

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
                      setConfig({
                        ...config,
                        default_browser: e.target.value || null,
                      });
                      setDirty(true);
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
                    value={config.portal.port}
                    onChange={(e) => {
                      setConfig({
                        ...config,
                        portal: { ...config.portal, port: Number(e.target.value) },
                      });
                      setDirty(true);
                    }}
                  />
                  <p className="text-xs text-muted-foreground">
                    Takes effect after restarting <code>linkport-cli serve</code>.
                  </p>
                </div>
                <div className="flex flex-col gap-2">
                  <Label htmlFor="redirect-hosts">Redirecting hosts (one per line)</Label>
                  <Textarea
                    id="redirect-hosts"
                    value={config.redirect_hosts.join("\n")}
                    onChange={(e) => {
                      setConfig({
                        ...config,
                        redirect_hosts: e.target.value
                          .split("\n")
                          .map((h) => h.trim())
                          .filter((h) => h.length > 0),
                      });
                      setDirty(true);
                    }}
                    placeholder={"track.smtpsendemail.com\nbit.ly"}
                  />
                  <p className="text-xs text-muted-foreground">
                    Links from these hosts that match no rule are followed to their
                    final destination (headers only), and your rules are evaluated
                    against that — so email trackers can route per real target.
                    Only these hosts are ever contacted.
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
              <Button variant="outline" onClick={() => doRegister("register")}>
                Register
              </Button>
              <Button variant="outline" onClick={() => doRegister("unregister")}>
                Unregister
              </Button>
            </div>
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
                  if (!config) return;
                  setConfig({ ...config, check_updates: v });
                  setDirty(true);
                }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span>Latest release</span>
              <div className="flex items-center gap-2">
                {!status?.update ? (
                  <Badge variant="outline">not checked yet</Badge>
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
                  <Badge variant="outline" title={status.update.error}>
                    check failed
                  </Badge>
                ) : (
                  <Badge variant="outline">up to date (v{status.update.current})</Badge>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={checkUpdate}
                  disabled={checkingUpdate}
                >
                  {checkingUpdate ? "…" : "Check now"}
                </Button>
              </div>
            </div>
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
