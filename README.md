# Linkport

**Linkport** is a rule-based browser router for Windows with a web portal for
managing the rules. Register it as your default browser and every link you
click — from email, Slack, PDFs, anywhere — is routed to the right browser or
browser profile based on rules you define.

```
link clicked
  → linkport-open.exe "<url>"   (GUI subsystem: no console flash, reads config,
  │                              matches rules, spawns browser, exits)
  → event appended to history     (best-effort, includes errors)
linkport serve                    (daemon: web portal + API on 127.0.0.1)
```

Two binaries ship from one crate: `linkport.exe` (console CLI + portal
daemon) and `linkport-open.exe` (the OS-facing URL handler). Keep them in the
same directory; `linkport register` points the registry at the handler.

## How it works

- **Single binary.** `linkport open` is the OS-facing hot path — it never
  depends on the daemon, so links keep working even when the portal is down.
  `linkport serve` runs the management portal (axum) with the Next.js UI
  embedded in the binary.
- **Rules are ordered; first match wins.** Each rule can combine a host glob
  (`*.mycompany.com`), a full-URL regex (`docs\.google\.com`), and a scheme
  matcher — all specified matchers must match (AND). Host globs are
  cookie-style: `*.example.com` matches `example.com` **and** its subdomains.
  Targets are browser ids (with optional incognito mode) or `block` to
  swallow the URL.
- **Web portal.** Dashboard with a live "try a URL" dry-run trace and recent
  link history, a rules editor with drag-reorder, a browsers page with
  registry-based auto-detection, and one-click Windows registration.
- **Localhost-only + token auth.** The portal binds 127.0.0.1, validates the
  Host header (DNS-rebinding protection) and requires a bearer token created
  on first run.

## Workspace layout

```
crates/
├── linkport-core/   # config model, rule engine, launcher, event log (pure, tested)
├── linkport-win/    # default-browser registration + browser discovery (Windows;
│                    #  stubs elsewhere so the workspace builds on any host)
└── linkport/        # the binary: CLI (open/serve/register/test/…) + axum portal
frontend/            # Next.js 15 + TypeScript + shadcn/ui (static export, rust-embed)
scripts/build.sh     # full release build for Windows
```

## Development (from WSL or Linux)

Requirements: rustup (stable), Node 18.18+ / npm, and the frontend deps.

```bash
# terminal 1 — portal daemon (creates default config + token on first run)
cargo run -p linkport -- serve

# terminal 2 — frontend dev server with hot reload
cd frontend
npm install
npm run dev          # http://localhost:3000, proxies API calls to :14200
```

Grab the portal URL (with token) via:

```bash
cargo run -p linkport -- portal-url
```

Run the test suite and checks:

```bash
cargo test --workspace
cargo check --target x86_64-pc-windows-gnu   # validates the Windows-only code paths
```

## Building the Windows binary

From WSL (cross-compilation, no Visual Studio needed):

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64          # only needed for linking
./scripts/build.sh                  # builds frontend, then linkport.exe
```

Or build natively on Windows with the MSVC toolchain (the repo lives at
`\\wsl.localhost\<distro>\home\<user>\repo\linkport` — cloning it to the
Windows filesystem is recommended for native builds).

## Installing as the default browser (Windows)

1. Copy **both** `linkport.exe` and `linkport-open.exe` somewhere permanent
   (e.g. `C:\Tools\Linkport`, side by side).
2. Run `linkport register`.
3. Open **Settings → Apps → Default apps → Linkport** and set it as the
   default for **HTTP** and **HTTPS**. (Windows 10/11 deliberately prevents
   apps from setting this programmatically.)
4. Start the portal with `linkport serve` (or `linkport serve --open` to
   also open it in your browser) and configure browsers and rules at the
   printed tokenized URL. The token is remembered per browser, so plain
   `http://127.0.0.1:14200` works afterwards.

To undo: **Settings → Default apps** pick your old browser, then
`linkport unregister`.

## Configuration

Stored at `%APPDATA%\linkport\config.toml` (created on first run). The portal
edits this file atomically; you can also hand-edit it:

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

[browsers.firefox]
display_name = "Firefox"
exe = 'C:/Program Files/Mozilla Firefox/firefox.exe'
args = ["-new-window", "{url}"]

[[rules]]
name = "company links"
host_glob = "*.mycompany.com"
target = "work-chrome"

[[rules]]
name = "google docs to work"
url_regex = "docs\\.google\\.com"
target = "work-chrome"

[[rules]]
name = "no trackers"
host_glob = "*.tracker.io"
target = "block"
```

## Roadmap

- [ ] Native picker dialog for ambiguous links (websteer-style `ambiguous` flag)
- [ ] "Browser already running" heuristic (BrowserPicker-style)
- [ ] URL shortener expansion in the trace view
- [ ] Linux support (`linkport-linux`: `.desktop` + `xdg-settings`) — the core
      is already platform-neutral

## License

MIT — see [LICENSE](LICENSE).
