"use client";

import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, Pencil, Plus, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import type { Rule } from "@/lib/types";
import { useConfigEditor } from "@/lib/use-config-editor";
import { WarningsBanner } from "@/components/warnings-banner";
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
import { NativeSelect } from "@/components/ui/native-select";
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
    url_contains: "",
    url_regex: "",
    scheme: "",
    target: "",
    incognito: false,
  };
}

function matcherSummary(rule: Rule): string[] {
  const parts: string[] = [];
  if (rule.host_glob) parts.push(`host: ${rule.host_glob}`);
  if (rule.url_contains) parts.push(`contains: ${rule.url_contains}`);
  if (rule.url_regex) parts.push(`regex: ${rule.url_regex}`);
  if (rule.scheme) parts.push(`scheme: ${rule.scheme}`);
  return parts;
}

// Well-known query parameters that carry the real destination on
// SSO/redirect interstitials (e.g. Microsoft login).
const DESTINATION_PARAMS = [
  "redirect_uri",
  "goto",
  "next",
  "returnTo",
  "continue",
  "dest",
  "destination",
  "target",
];

// Pull `param=raw-value` out of the URL so the rule form can prefill it as
// the distinguishing matcher.
function destinationParam(url: string): string {
  for (const p of DESTINATION_PARAMS) {
    const m = url.match(new RegExp(`[?&]${p}=([^&]*)`));
    if (m) return `${p}=${m[1]}`;
  }
  return "";
}

export default function RulesPage() {
  const { config, setConfig, dirty, saving, error, setError, warnings, mutate, save } =
    useConfigEditor();
  // null = form closed; -1 = creating new; >= 0 = editing rules[index]
  const [editing, setEditing] = useState<{ index: number; draft: Rule } | null>(null);

  function updateDraft(patch: Partial<Rule>) {
    setEditing((prev) => (prev ? { ...prev, draft: { ...prev.draft, ...patch } } : prev));
  }

  useEffect(() => {
    api
      .getConfig()
      .then((c) => setConfig(c))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));

    // Prefill from Dashboard "create rule from host" — when the history
    // entry carried a resolved URL with an SSO/redirect destination param,
    // prefill it as the distinguishing matcher.
    const params = new URLSearchParams(window.location.search);
    const host = params.get("host");
    if (host) {
      const fullUrl = params.get("url") ?? "";
      setEditing({
        index: -1,
        draft: {
          ...emptyRule(),
          name: host,
          host_glob: host,
          url_contains: destinationParam(fullUrl),
        },
      });
    }
  }, []);

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
      url_contains: editing.draft.url_contains?.trim() || null,
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

  const browserOptions = Object.entries(config?.browsers ?? {});

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
      <WarningsBanner warnings={warnings} />

      {editing && (
        <Card>
          <CardHeader>
            <CardTitle>{editing.index >= 0 ? "Edit rule" : "New rule"}</CardTitle>
            <CardDescription>
              All matchers are combined with AND; a rule with no matcher catches every URL.
              Rules apply top to bottom — first match wins. Host matchers:
              <code className="bg-muted rounded px-1">example.com</code> matches that host
              and all of its subdomains (any depth); the
              <code className="bg-muted rounded px-1">*.</code> prefix is an equivalent
              alias.
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
                  onChange={(e) => updateDraft({ name: e.target.value })}
                  placeholder="Work links"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-target">Open with</Label>
                <NativeSelect
                  id="rule-target"
                  required
                  value={editing.draft.target}
                  onChange={(e) => updateDraft({ target: e.target.value })}
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
                </NativeSelect>
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-host">Host glob</Label>
                <Input
                  id="rule-host"
                  value={editing.draft.host_glob ?? ""}
                  onChange={(e) => updateDraft({ host_glob: e.target.value })}
                  placeholder="example.com (covers its subdomains)"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-contains">URL contains (plain text)</Label>
                <Input
                  id="rule-contains"
                  value={editing.draft.url_contains ?? ""}
                  onChange={(e) => updateDraft({ url_contains: e.target.value })}
                  placeholder="redirect_uri=https%3A%2F%2Fsecurity.microsoft.com"
                />
                <p className="text-xs text-muted-foreground">
                  Optional. Text that must appear anywhere in the link — great for
                  matching a destination inside login/redirect URLs. No escaping
                  needed; matched case-insensitively.
                </p>
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-regex">URL regex (advanced)</Label>
                <Input
                  id="rule-regex"
                  value={editing.draft.url_regex ?? ""}
                  onChange={(e) => updateDraft({ url_regex: e.target.value })}
                  placeholder="docs\\.google\\.com"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="rule-scheme">Scheme (optional)</Label>
                <Input
                  id="rule-scheme"
                  value={editing.draft.scheme ?? ""}
                  onChange={(e) => updateDraft({ scheme: e.target.value })}
                  placeholder="https, zoommtg, …"
                />
              </div>
              <div className="flex items-center gap-2 pt-6">
                <Switch
                  id="rule-incognito"
                  checked={editing.draft.incognito}
                  onCheckedChange={(v) => updateDraft({ incognito: v })}
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
                        <Button variant="ghost" size="icon" onClick={() => setEditing({ index: i, draft: { ...r } })} aria-label="Edit">
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
