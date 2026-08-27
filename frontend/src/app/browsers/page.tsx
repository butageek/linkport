"use client";

import { useEffect, useState } from "react";
import { Pencil, Plus, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import type { BrowsersResponse, Browser, Config } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

interface BrowserDraft {
  id: string;
  browser: Browser;
  originalId?: string; // set when editing an existing entry
}

function emptyDraft(): BrowserDraft {
  return {
    id: "",
    browser: { display_name: "", exe: "", args: [], incognito_args: null },
  };
}

function slug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

export default function BrowsersPage() {
  const [config, setConfig] = useState<Config | null>(null);
  const [discovered, setDiscovered] = useState<BrowsersResponse["discovered"]>([]);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [draft, setDraft] = useState<BrowserDraft | null>(null);

  function load() {
    Promise.all([api.getConfig(), api.browsers()])
      .then(([c, b]) => {
        setConfig(c);
        setDiscovered(b.discovered);
        setError(null);
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }

  useEffect(load, []);

  function mutate(fn: (c: Config) => void) {
    setConfig((c) => {
      if (!c) return c;
      const next = structuredClone(c);
      fn(next);
      return next;
    });
    setDirty(true);
    setWarnings([]);
  }

  async function save() {
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      const res = await api.saveConfig(config);
      setWarnings(res.warnings);
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  function submitDraft(e: React.FormEvent) {
    e.preventDefault();
    if (!draft || !config) return;
    const id = draft.id.trim() || slug(draft.browser.display_name);
    if (!id) return;
    mutate((c) => {
      if (draft.originalId && draft.originalId !== id) {
        delete c.browsers[draft.originalId];
        if (c.default_browser === draft.originalId) c.default_browser = id;
        for (const r of c.rules) {
          if (r.target === draft.originalId) r.target = id;
        }
      }
      c.browsers[id] = {
        ...draft.browser,
        args: draft.browser.args.filter((a) => a.length > 0),
        incognito_args: draft.browser.incognito_args?.filter((a) => a.length > 0) ?? null,
      };
    });
    setDraft(null);
  }

  function addDiscovered(name: string, exe: string, args: string[]) {
    const id = slug(name);
    mutate((c) => {
      c.browsers[id] = { display_name: name, exe, args };
    });
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Browsers</h1>
        <div className="flex items-center gap-3">
          {dirty && <Badge variant="outline">unsaved changes</Badge>}
          <Button variant="outline" onClick={() => setDraft(emptyDraft())}>
            <Plus /> Add browser
          </Button>
          <Button onClick={save} disabled={!config || saving || !dirty}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}
      {warnings.length > 0 && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
          <p className="mb-1 font-medium">Saved with warnings:</p>
          <ul className="list-disc pl-4">
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </div>
      )}

      {draft && (
        <Card>
          <CardHeader>
            <CardTitle>{draft.originalId ? "Edit browser" : "New browser"}</CardTitle>
            <CardDescription>
              Args use <code className="bg-muted rounded px-1">{"{url}"}</code> as the
              placeholder; if absent, the URL is appended.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submitDraft} className="grid gap-4 md:grid-cols-2">
              <div className="flex flex-col gap-2">
                <Label htmlFor="b-id">Id (used by rules)</Label>
                <Input
                  id="b-id"
                  value={draft.id}
                  onChange={(e) => setDraft({ ...draft, id: e.target.value })}
                  placeholder="work-chrome (auto from name if empty)"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="b-name">Display name</Label>
                <Input
                  id="b-name"
                  required
                  value={draft.browser.display_name}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      browser: { ...draft.browser, display_name: e.target.value },
                    })
                  }
                  placeholder="Chrome — Work"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="b-exe">Executable</Label>
                <Input
                  id="b-exe"
                  required
                  value={draft.browser.exe}
                  onChange={(e) =>
                    setDraft({ ...draft, browser: { ...draft.browser, exe: e.target.value } })
                  }
                  placeholder="C:/Program Files/Google/Chrome/Application/chrome.exe"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="b-args">Args (one per line)</Label>
                <textarea
                  id="b-args"
                  value={draft.browser.args.join("\n")}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      browser: {
                        ...draft.browser,
                        args: e.target.value.split("\n"),
                      },
                    })
                  }
                  placeholder={"--profile-directory=Work\n{url}"}
                  className="border-input placeholder:text-muted-foreground min-h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
                />
              </div>
              <div className="flex flex-col gap-2 md:col-span-2">
                <Label htmlFor="b-incog">Incognito args (optional, one per line)</Label>
                <textarea
                  id="b-incog"
                  value={draft.browser.incognito_args?.join("\n") ?? ""}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      browser: {
                        ...draft.browser,
                        incognito_args: e.target.value ? e.target.value.split("\n") : null,
                      },
                    })
                  }
                  placeholder={"--incognito\n{url}"}
                  className="border-input placeholder:text-muted-foreground min-h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
                />
              </div>
              <div className="flex gap-2 md:col-span-2">
                <Button type="submit">{draft.originalId ? "Update" : "Add"}</Button>
                <Button type="button" variant="ghost" onClick={() => setDraft(null)}>
                  Cancel
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Configured browsers</CardTitle>
        </CardHeader>
        <CardContent>
          {!config || Object.keys(config.browsers).length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No browsers configured yet. Add one below or pick from the detected list.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Id</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Executable</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {Object.entries(config.browsers).map(([id, b]) => (
                  <TableRow key={id}>
                    <TableCell>
                      <code className="text-xs">{id}</code>
                    </TableCell>
                    <TableCell className="font-medium">{b.display_name}</TableCell>
                    <TableCell className="max-w-72 truncate text-muted-foreground">
                      {b.exe}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end">
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() =>
                            setDraft({
                              id,
                              browser: {
                                ...b,
                                incognito_args: b.incognito_args ?? null,
                              },
                              originalId: id,
                            })
                          }
                          aria-label="Edit"
                        >
                          <Pencil />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() =>
                            mutate((c) => {
                              delete c.browsers[id];
                            })
                          }
                          aria-label="Delete"
                        >
                          <Trash2 />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Detected on this system</CardTitle>
          <CardDescription>
            From the Windows registry — one click prefills a browser entry
          </CardDescription>
        </CardHeader>
        <CardContent>
          {discovered.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Nothing detected (or not running on Windows). Add browsers manually above.
            </p>
          ) : (
            <div className="flex flex-col gap-2">
              {discovered.map((d) => (
                <div
                  key={d.name}
                  className="flex items-center justify-between gap-4 rounded-md border px-3 py-2"
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{d.name}</p>
                    <p className="truncate text-xs text-muted-foreground">{d.command}</p>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      addDiscovered(d.name, d.suggested.exe, d.suggested.args)
                    }
                  >
                    <Plus /> Add
                  </Button>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
