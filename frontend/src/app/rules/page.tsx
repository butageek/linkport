"use client";

import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, Pencil, Plus, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import type { Config, Rule } from "@/lib/types";
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
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

const BLOCK = "block";

function emptyRule(): Rule {
  return {
    name: "",
    enabled: true,
    host_glob: "",
    url_regex: "",
    scheme: "",
    target: "",
    incognito: false,
  };
}

function matcherSummary(rule: Rule): string[] {
  const parts: string[] = [];
  if (rule.host_glob) parts.push(`host: ${rule.host_glob}`);
  if (rule.url_regex) parts.push(`regex: ${rule.url_regex}`);
  if (rule.scheme) parts.push(`scheme: ${rule.scheme}`);
  return parts;
}

export default function RulesPage() {
  const [config, setConfig] = useState<Config | null>(null);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  // null = form closed; -1 = creating new; >= 0 = editing rules[index]
  const [editing, setEditing] = useState<{ index: number; draft: Rule } | null>(null);

  useEffect(() => {
    api
      .getConfig()
      .then((c) => setConfig(c))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));

    // Prefill from Dashboard "create rule from host".
    const host = new URLSearchParams(window.location.search).get("host");
    if (host) {
      setEditing({
        index: -1,
        draft: { ...emptyRule(), name: host, host_glob: `*.${host}` },
      });
    }
  }, []);

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

  function moveRule(i: number, dir: -1 | 1) {
    mutate((c) => {
      const j = i + dir;
      if (j < 0 || j >= c.rules.length) return;
      [c.rules[i], c.rules[j]] = [c.rules[j], c.rules[i]];
    });
  }

  function submitDraft(e: React.FormEvent) {
    e.preventDefault();
    if (!editing) return;
    const clean: Rule = {
      ...editing.draft,
      host_glob: editing.draft.host_glob?.trim() || null,
      url_regex: editing.draft.url_regex?.trim() || null,
      scheme: editing.draft.scheme?.trim() || null,
      name: editing.draft.name.trim(),
    };
    mutate((c) => {
      if (editing.index >= 0) c.rules[editing.index] = clean;
      else c.rules.push(clean);
    });
    setEditing(null);
  }

  const browserOptions = config
    ? Object.entries(config.browsers)
    : [];

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Rules</h1>
        <div className="flex items-center gap-3">
          {dirty && <Badge variant="outline">unsaved changes</Badge>}
          <Button
            variant="outline"
            onClick={() => setEditing({ index: -1, draft: emptyRule() })}
          >
            <Plus /> Add rule
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

      {editing && (
        <Card>
          <CardHeader>
            <CardTitle>{editing.index >= 0 ? "Edit rule" : "New rule"}</CardTitle>
            <CardDescription>
              All matchers are combined with AND; a rule with no matcher catches every
              URL. Rules apply top to bottom — first match wins.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submitDraft} className="grid gap-4 md:grid-cols-2">
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-name">Name</Label>
                <Input
                  id="rule-name"
                  required
                  value={editing.draft.name}
                  onChange={(e) =>
                    setEditing({ ...editing, draft: { ...editing.draft, name: e.target.value } })
                  }
                  placeholder="Work links"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-target">Open with</Label>
                <select
                  id="rule-target"
                  required
                  value={editing.draft.target}
                  onChange={(e) =>
                    setEditing({ ...editing, draft: { ...editing.draft, target: e.target.value } })
                  }
                  className="border-input flex h-9 rounded-md border bg-transparent px-3 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
                >
                  <option value="" disabled>
                    choose browser…
                  </option>
                  {browserOptions.map(([id, b]) => (
                    <option key={id} value={id}>
                      {b.display_name}
                    </option>
                  ))}
                  <option value={BLOCK}>
                    Block (open nothing)
                  </option>
                </select>
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-host">Host glob</Label>
                <Input
                  id="rule-host"
                  value={editing.draft.host_glob ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, draft: { ...editing.draft, host_glob: e.target.value } })
                  }
                  placeholder="*.mycompany.com"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-regex">URL regex</Label>
                <Input
                  id="rule-regex"
                  value={editing.draft.url_regex ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, draft: { ...editing.draft, url_regex: e.target.value } })
                  }
                  placeholder="docs\\.google\\.com"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-scheme">Scheme (optional)</Label>
                <Input
                  id="rule-scheme"
                  value={editing.draft.scheme ?? ""}
                  onChange={(e) =>
                    setEditing({ ...editing, draft: { ...editing.draft, scheme: e.target.value } })
                  }
                  placeholder="https, zoommtg, …"
                />
              </div>
              <div className="flex items-center gap-2 pt-6">
                <Switch
                  id="rule-incognito"
                  checked={editing.draft.incognito}
                  onCheckedChange={(v) =>
                    setEditing({ ...editing, draft: { ...editing.draft, incognito: v } })
                  }
                />
                <Label htmlFor="rule-incognito">Private / incognito window</Label>
              </div>
              <div className="flex gap-2 md:col-span-2">
                <Button type="submit">
                  {editing.index >= 0 ? "Update rule" : "Add rule"}
                </Button>
                <Button type="button" variant="ghost" onClick={() => setEditing(null)}>
                  Cancel
                </Button>
              </div>
            </form>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Rule list</CardTitle>
          <CardDescription>First match wins; drag with the arrows to reorder</CardDescription>
        </CardHeader>
        <CardContent>
          {!config || config.rules.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No rules yet. Add one, or create one from a host on the Dashboard.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-8" />
                  <TableHead className="w-16">On</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Matchers</TableHead>
                  <TableHead>Opens</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {config.rules.map((r, i) => (
                  <TableRow key={i} className={r.enabled ? "" : "opacity-50"}>
                    <TableCell>
                      <div className="flex">
                        <Button variant="ghost" size="icon" onClick={() => moveRule(i, -1)} disabled={i === 0} aria-label="Move up">
                          <ArrowUp />
                        </Button>
                        <Button variant="ghost" size="icon" onClick={() => moveRule(i, 1)} disabled={i === config.rules.length - 1} aria-label="Move down">
                          <ArrowDown />
                        </Button>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Switch
                        checked={r.enabled}
                        onCheckedChange={(v) =>
                          mutate((c) => {
                            c.rules[i].enabled = v;
                          })
                        }
                        aria-label={`Toggle ${r.name}`}
                      />
                    </TableCell>
                    <TableCell className="font-medium">{r.name}</TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {matcherSummary(r).map((m) => (
                          <Badge key={m} variant="outline">
                            {m}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell>
                      {r.target === BLOCK ? (
                        <Badge variant="destructive">blocked</Badge>
                      ) : (
                        <Badge variant="secondary">
                          {config.browsers[r.target]?.display_name ?? r.target}
                          {r.incognito ? " (private)" : ""}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end">
                        <Button variant="ghost" size="icon" onClick={() => setEditing({ index: i, draft: { ...r} })} aria-label="Edit">
                          <Pencil />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() =>
                            mutate((c) => {
                              c.rules.splice(i, 1);
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
    </div>
  );
}
