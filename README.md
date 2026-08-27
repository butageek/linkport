# Linkport

**Linkport** is a rule-based browser router for Windows with a web portal for
managing the rules. Register it as your default browser and every link you
click — from email, Slack, PDFs, anywhere — is routed to the right browser or
browser profile based on rules you define, with zero configuration windows
getting in your way.

> Working MVP: Windows 10/11. Development happens in WSL; see
> [AGENTS.md](AGENTS.md) for the full environment, build, deploy and
> debugging playbook (written for humans *and* AI coding agents).

## How it works

```
link clicked (email, Slack, PDF, terminal…)
  → linkport-open.exe "<url>"   (GUI subsystem: no console flash; reads
  │                              config, matches rules, spawns browser, exits)
  → event appended to history    (best-effort, includes errors)
linkport-open.exe serve         (daemon: portal + tray, started at login)
  ├─ axum API + SPA on 127.0.0.1:14200 (token-gated)
  └─ system tray icon            (left click: portal; menu: toggles, quit)
```

Two binaries ship from one crate and must sit side by side:

- **`linkport.exe`** — console CLI: `serve`, `register`, `unregister`, `open`
  (console hot path for debugging), `test`, `browsers`, `init`, `portal-url`.
- **`linkport-open.exe`** — windowless entry points: `<url>` as the OS-facing
  handler registered with Windows, and `serve` used by the login auto-start
  entry. Built with the Windows GUI subsystem so nothing ever flashes.

### Features

- **Rule engine** — ordered rules, first match wins. Each rule combines
  matchers with AND: host glob, full-URL regex, scheme. Host globs are
  cookie-style: `*.example.com` matches `example.com` **and** subdomains.
  Targets are browser ids (optionally incognito/private) or `block`.
- **Web portal** (Next.js 15 + TypeScript + shadcn/ui, embedded in the
  binary): dashboard with live dry-run trace + link history and one-click
  "create rule from this host", rules editor with reorder/toggles, browsers
  page with registry auto-detection, settings with one-click registration.
- **System tray** — version, portal URL, **Start Linkport when I sign in**
  (native `HKCU\…\Run` key, no Task Scheduler), **Pause routing** (all links
  → default browser until unpaused), Quit (graceful shutdown).
- **First-run friendly** — on the very first daemon start ever, the portal
  opens by itself in a real browser; afterwards the tray icon is the way in.
  The auth token is transparent (stored per-browser via localStorage).
- **Localhost-only + token auth** — the portal binds 127.0.0.1, validates the
  Host header (DNS-rebinding protection) and requires a bearer token.

## Repository layout

```
crates/
├── linkport-core/   # pure & unit-tested: config model, rule engine,
│                    # browser launcher ({url} arg templates), JSONL event log
├── linkport-win/    # Windows registry integration (stubbed elsewhere):
│                    # default-browser registration, browser discovery,
│                    # login auto-start (Run key)
└── linkport/        # lib (open/portal/api/tray) + the two binaries above
frontend/            # Next.js 15 static export + shadcn/ui (Tailwind v4),
                    # embedded via rust-embed and served by axum
resources/           # tray icon assets (see scripts/gen_icon.py)
scripts/             # build.sh (full Windows release), gen_icon.py
```

State lives in `%APPDATA%\linkport` (`~/.config/linkport` on Linux):
`config.toml` (rules & browsers), `token` (portal auth), `events.jsonl`
(history), `paused.flag` (pause toggle).

## Development

Requirements: rustup stable, Node 18.18+/npm, and for the Windows binary
`mingw-w64` (`sudo apt install mingw-w64`).

```bash
# terminal 1 — portal daemon
cargo run -p linkport -- serve

# terminal 2 — frontend with hot reload (proxies API to :14200)
cd frontend && npm install && npm run dev   # http://localhost:3000
```

```bash
cargo test --workspace                        # unit tests (pure core)
cargo check --target x86_64-pc-windows-gnu --workspace --all-targets
./scripts/build.sh                            # frontend + release linkport*.exe
```

## Installing as the default browser (Windows)

1. Copy **both** `linkport.exe` and `linkport-open.exe` somewhere permanent,
   side by side (e.g. `C:\Tools\Linkport`).
2. Run `linkport register`.
3. **Settings → Apps → Default apps → Linkport** → set default for HTTP and
   HTTPS. (Windows deliberately prevents apps from doing this
   programmatically.)
4. Start `linkport serve` once (or `linkport-open.exe serve` windowless) and
   enable **Start Linkport when I sign in** in the tray menu.

From then on: reboot → tray appears, links route by your rules, and the
portal is one click (or one reboot on first run) away.

## Configuration

`%APPDATA%\linkport\config.toml` — edited atomically by the portal or by
hand:

```toml
version = 1
default_browser = "firefox"

[portal]
port = 14200

[browsers.work-chrome]
display_name = "Chrome — Work"
exe = 'C:/Program Files/Google/Chrome/Application/chrome.exe'
args = ["--profile-directory=Work", "{url}"]
incognito_args = ["--incognito", "{url}"]

[[rules]]
name = "google docs to work"
url_regex = "docs\\.google\\.com"
target = "work-chrome"

[[rules]]
name = "no trackers"
host_glob = "*.tracker.io"
target = "block"
```

## Debugging

- `linkport test "<url>"` — dry-run printing the full decision trace.
- `%APPDATA%\linkport\events.jsonl` — every opened link with its outcome and
  any launch errors (also visible on the Dashboard).
- Dashboard → "Try a URL" — same trace, in the portal.

## Roadmap

- [ ] Native picker dialog for ambiguous links (websteer-style)
- [ ] "Browser already running" heuristic (BrowserPicker-style)
- [ ] URL shortener expansion in the trace view
- [ ] Friendly names for detected browsers (`Firefox-308046B0AF4A39CB` → `Firefox`)
- [ ] Installer + code signing
- [ ] Linux support (`.desktop` + `xdg-settings`) — the core is portable

## License

MIT — see [LICENSE](LICENSE).
