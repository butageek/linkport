"use client";

import { useState } from "react";

import { api } from "@/lib/api";
import type { Config } from "@/lib/types";

// Shared editor state for pages that mutate the config in the browser and
// save it via the API (Rules, Browsers). Settings has its own save flow.
export function useConfigEditor() {
  const [config, setConfig] = useState<Config | null>(null);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);

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
      if (res.config) setConfig(res.config); // server may amend default_browser
      setWarnings(res.warnings);
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return { config, setConfig, dirty, saving, error, setError, warnings, mutate, save };
}
