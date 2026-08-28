// Types mirroring the Rust core structs (serde snake_case JSON).

export interface Browser {
  display_name: string;
  exe: string;
  args: string[];
  incognito_args?: string[] | null;
}

export interface Rule {
  name: string;
  enabled: boolean;
  host_glob?: string | null;
  url_regex?: string | null;
  scheme?: string | null;
  target: string;
  incognito: boolean;
}

export interface PortalConfig {
  port: number;
}

export interface Config {
  version: number;
  portal: PortalConfig;
  default_browser?: string | null;
  browsers: Record<string, Browser>;
  rules: Rule[];
}

export type Outcome =
  | { type: "rule_matched"; rule: string; target: string; incognito: boolean }
  | { type: "default"; target: string }
  | { type: "blocked"; rule: string }
  | { type: "no_match" };

export interface RuleTrace {
  name: string;
  matched: boolean;
  reason: string;
}

export interface Decision {
  url: string;
  host?: string | null;
  scheme?: string | null;
  outcome: Outcome;
  trace: RuleTrace[];
}

export interface EventItem {
  timestamp: string;
  url: string;
  host?: string | null;
  outcome: Outcome;
  error?: string | null;
}

export interface Status {
  version: string;
  registered: boolean;
  paused: boolean;
  autostart: boolean;
  config_path: string;
  portal_url: string;
  default_browser?: string | null;
  rules_count: number;
  browsers_count: number;
}

export interface DiscoveredBrowser {
  /** Raw registry key name (e.g. Firefox-308046B0AF4A39CB) — for debug. */
  name: string;
  /** Friendly name from the registry (e.g. Mozilla Firefox). */
  display_name: string;
  command: string;
  suggested: { exe: string; args: string[] };
}

export interface BrowsersResponse {
  configured: { id: string; browser: Browser }[];
  discovered: DiscoveredBrowser[];
}
