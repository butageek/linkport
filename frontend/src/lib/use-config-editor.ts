"use client";

import { useRef, useState } from "react";

import { api } from "@/lib/api";
import type { Config } from "@/lib/types";

// Shared editor state for pages that mutate the config (Rules, Browsers).
// Every mutation autosaves: `mutate` applies the change and PUTs the whole
// config in one step, so the disk never waits for a separate Save click.
// Settings keeps its own explicit save flow.
export function useConfigEditor() {
  const [config, setConfig] = useState<Config | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  // Flips true for a couple of seconds after a successful autosave so pages
  // can show an obvious "Saved" confirmation.
  const [justSaved, setJustSaved] = useState(false);

  // Latest config incl. not-yet-persisted mutations (setConfig alone lags
  // one render, which loses rapid consecutive mutations).
  const configRef = useRef<Config | null>(null);
  // Only the newest PUT's response may touch state; an older in-flight
  // response must not clobber a newer mutation.
  const seq = useRef(0);

  function updateConfig(c: Config | null) {
    configRef.current = c;
    setConfig(c);
  }

  function flashSaved() {
    setJustSaved(true);
    window.setTimeout(() => setJustSaved(false), 2000);
  }

  async function mutate(fn: (c: Config) => void) {
    const base = configRef.current;
    if (!base) return;
    const next = structuredClone(base);
    fn(next);
    const mySeq = ++seq.current;
    updateConfig(next);
    setWarnings([]);
    setBusy(true);
    setError(null);
    try {
      const res = await api.saveConfig(next);
      if (mySeq !== seq.current) return; // superseded by a newer mutation
      updateConfig(res.config ?? next); // server may amend default_browser
      setWarnings(res.warnings);
      flashSaved();
    } catch (e) {
      if (mySeq !== seq.current) return;
      setError(e instanceof Error ? e.message : String(e));
      try {
        updateConfig(await api.getConfig()); // resync: disk is the truth
      } catch {
        /* portal unreachable; the error above already says so */
      }
    } finally {
      if (mySeq === seq.current) setBusy(false);
    }
  }

  return {
    config,
    setConfig: updateConfig,
    busy,
    error,
    setError,
    warnings,
    mutate,
    justSaved,
  };
}
