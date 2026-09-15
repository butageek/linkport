# Linkport

[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078d6)
![Language](https://img.shields.io/badge/built%20with-Rust-dea584)

**Repo:** [github.com/butageek/linkport](https://github.com/butageek/linkport) · Issues and PRs welcome.

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
  → linkport.exe "<url>"   (GUI subsystem: no console flash; reads
  │                              config, matches rules, spawns browser, exits)
  → event appended to history    (best-effort, includes errors)
linkport.exe serve         (daemon: portal + tray, started at login)
  ├─ axum API + SPA on 127.0.0.1:14200 (token-gated)
  └─ system tray icon            (left click: portal; menu: toggles, quit)
```

Two binaries ship from one crate and must sit side by side:

- **`linkport.exe`** — the app itself, built with the Windows GUI subsystem
  so nothing ever flashes: invoked with a URL as the OS-facing handler
  registered with Windows, with `serve` (login auto-start entry), or with
  no arguments (double-click / Start menu — starts the daemon, or opens
  the portal if one is already running).
- **`linkport-cli.exe`** — the terminal tool: `test` (dry-run a URL with
  the full decision trace), `register`/`unregister`, `browsers`, `init`,
  `portal-url`, and `serve` for dev-mode running with console logs.

### Features

- **Rule engine** — ordered rules, first match wins. Each rule combines
  matchers with AND: host, plain-text URL contains, full-URL regex, scheme.
  Host matchers use cookie-domain semantics: `example.com` matches that
  host **and** all of its subdomains (`*.example.com` is an equivalent
  alias). Targets are browser ids (optionally incognito/private),
  `block`, or `resolve`.
- **Redirect resolution** — rules with the `resolve` target mark
  tracker/shortener domains: their links are followed to the final
  destination — headers only, never page content, only hosts you mark —
  and the rules are re-evaluated against it, so one email tracker can
  land in different browsers per real target.
- **Web portal** (Next.js 15 + TypeScript + shadcn/ui, embedded in the
  binary): dashboard with live dry-run trace + link history and one-click
  "create rule from this host", rules editor with reorder/toggles, browsers
  page with registry auto-detection (friendly names), settings with
  one-click registration.
- **System tray** — version, portal URL, **Start Linkport when I sign in**
  (native `HKCU\…\Run` key, no Task Scheduler), **Pause routing** (all links
  → default browser until unpaused), Quit (graceful shutdown; link routing
  keeps working — restart via the Start-menu "Linkport" entry, by
  double-clicking `linkport.exe`, or by signing in again).
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
# terminal 1 — portal daemon (prints to the console on Linux/macOS)
cargo run -p linkport -- serve

# terminal 2 — frontend with hot reload (proxies API to :14200)
cd frontend && npm install && npm run dev   # http://localhost:3000
```

```bash
cargo test --workspace                        # unit tests (pure core)
cargo check --target x86_64-pc-windows-gnu --workspace --all-targets
./scripts/build.sh                            # frontend + release linkport*.exe
./scripts/package.sh                          # shareable zip: exes + QUICK-START + LICENSE
```

CI runs on demand (`gh workflow run CI.yml` or the Actions tab) and on
every pull request; pushing a version tag (`v*`) runs the tests, builds
the zip and publishes a GitHub Release automatically.

## Installing as the default browser (Windows)

1. Copy **both** `linkport.exe` and `linkport-cli.exe` somewhere permanent,
   side by side (e.g. `C:\Tools\Linkport`).
2. Run `linkport-cli.exe register`.
3. **Settings → Apps → Default apps → Linkport** → set default for HTTP and
   HTTPS. (Windows deliberately prevents apps from doing this
   programmatically.)
4. Start the daemon once — double-click `linkport.exe` (or run
   `linkport-cli.exe serve` to see console logs) — and enable **Start
   Linkport when I sign in** in the tray menu. The daemon also creates a
   **Linkport** Start-menu shortcut for restarting after a tray Quit.

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
host_glob = "tracker.io"
target = "block"

[[rules]]
# tracker/shortener domains: follow the redirect, route by destination
name = "email tracker"
host_glob = "track.smtpsendemail.com"
target = "resolve"
```

## Debugging

- `linkport-cli.exe test "<url>"` — dry-run printing the full decision trace.
- `%APPDATA%\linkport\events.jsonl` — every opened link with its outcome and
  any launch errors (also visible on the Dashboard).
- Dashboard → "Try a URL" — same trace, in the portal.

## Roadmap

- [ ] "Browser already running" heuristic (route to the browser you're
      currently in)
- [ ] Shift-to-choose override: hold Shift while clicking a link to pick a
      browser once (optionally saving the choice as a rule) — opt-in only,
      never an unprompted dialog
- [ ] Installer + code signing (today: `./scripts/package.sh` builds a
      shareable zip with a quick-start guide; SmartScreen warns on the
      unsigned exes — recipients click "More info" → "Run anyway")
- [ ] winget manifest once the release has some real-world use
- [ ] Linux support (`.desktop` + `xdg-settings`) — the core is portable

## License

MIT — see [LICENSE](LICENSE).
