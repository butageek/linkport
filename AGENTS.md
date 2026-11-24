# AGENTS.md

Working notes for AI coding agents (and humans) contributing to Linkport.
Read this before building or editing anything.

## What this is

Linkport is a rule-based browser router for Windows: a Rust backend with an
embedded Next.js portal (rules/browsers/history management) and a system tray
daemon. One crate produces two binaries. See README.md for the user-facing
overview.

## Development environment

Development targets **Linux or WSL** (the Windows binaries are
cross-compiled); building on Windows directly works too.

- Toolchain: rustup stable with the `x86_64-pc-windows-gnu` target, Node
  18.18+ (v24 used in CI), and `mingw-w64` (`sudo apt install mingw-w64`)
  for cross-linking the Windows exes.
- Agents: make sure `cargo` and `npm` are on PATH — non-interactive shells
  often miss them (e.g. under fnm/nvm setups).
- `frontend/out` must exist for the Rust build (rust-embed); `build.rs`
  creates a placeholder automatically if missing.

## Commands

```bash
cargo test --workspace                      # unit tests (core + api)
cargo check --target x86_64-pc-windows-gnu --workspace --all-targets
(cd frontend && npm ci && npm run build)    # static export → frontend/out
cargo build --release --target x86_64-pc-windows-gnu -p linkport
./scripts/build.sh                          # frontend + release, all-in-one
./scripts/package.sh                        # build + dist/linkport-<ver>-win64.zip
cargo fmt --all                             # repo is kept fmt-clean; run before committing
# on Windows, with a local build (the README one-liner without the download):
powershell -ExecutionPolicy Bypass -File tools\install.ps1 -BinDir .\target\x86_64-pc-windows-gnu\release
```

## Release discipline (for AI agents and humans)

- Everyday pushes are for **preservation only** and deliberately trigger
  no workflows (see ci.yml). NEVER push a version tag unless the user
  asks for a release ("release", "cut a version", "ship 0.3.0").
- On a release request: confirm or propose the version (patch = fixes /
  polish, minor = new features), bump `version` in the workspace
  `Cargo.toml` **and** `frontend/package.json`, commit,
  `git pull --rebase origin main` first (the maintainer sometimes edits
  on the web), tag `vX.Y.Z`, push branch + tag, then watch release.yml
  and verify the published release + zip; polish notes with
  `gh release edit vX.Y.Z --notes-file …`.
- "Run CI" means: `gh workflow run CI` (no release).

CI (`.github/workflows/ci.yml`) runs the format check, tests, the Windows
cross-check and the portal build **on demand** (`gh workflow run CI` —
address it by workflow name or ID; the `CI.yml` filename form 404s on
GitHub's dispatch API) and on every PR — everyday pushes deliberately
trigger nothing (they are for preservation). Pushing a version tag
(`v*`) triggers `.github/workflows/release.yml`: tests + `package.sh` +
a published GitHub Release (idempotent: re-pushing a tag re-uploads the
zip with `--clobber` instead of failing). The installer and the in-app
update both fetch `linkport-*-win64.zip` from `releases/latest` — keep
the zip name pattern `package.sh` produces.

Maintainers keep machine-specific notes (WSL paths, deploy routine) in a
gitignored `AGENTS.local.md` next to this file.

## Windows integration state (all HKCU, no admin)
`<install>` below means the directory holding the two exes —
`%LOCALAPPDATA%\Programs\Linkport` for the one-liner install (per-user,
no admin, no UAC on upgrade; the location VS Code's user installer and
most modern per-user Win32 apps use). A manual copy elsewhere
(`C:\Tools\Linkport`) is migrated by quitting its daemon, deleting it and
running `tools/install.ps1`, which repoints the registration in place.
The install path is registered with Windows — moving it requires
re-registering, which the daemon's self-heal (below) also does in place.

| Purpose | Key / mechanism | Value shape |
|---|---|---|
| URL handler (ProgId) | `Software\Classes\Linkport.URL\shell\open\command` | `"<install>\linkport.exe" "%1"` |
| Entry icons | `…\Linkport.URL\DefaultIcon` + `…\StartMenuInternet\Linkport\DefaultIcon` | `<install>\linkport.exe,0` (exe embeds `resources/icon.ico` via winresource/windres) |
| Browser candidate | `Software\Clients\StartMenuInternet\Linkport\…` (+ `RegisteredApplications`) | capabilities + URLAssociations http/https |
| Default-browser choice | `…\UrlAssociations\{http,https}\UserChoice` | `Linkport.URL` (user sets via Settings; apps cannot) |
| Login auto-start | `Software\Microsoft\Windows\CurrentVersion\Run\Linkport` | `"<install>\linkport.exe" serve` |
| Start-menu restart entry | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Linkport.lnk` | target `linkport.exe serve`; daemon recreates it when missing. After a tray Quit, this — or double-clicking `linkport.exe` (no args = serve) — is how the daemon comes back |

After re-registering, verify `UserChoice` didn't get reset by Windows
(re-registering can invalidate its hash — if `http\UserChoice` is gone,
the user must re-set the default browser by hand in Settings).

**Moved/updated install self-heal**: the release zip extracts to a
version-named folder, so an upgrade-by-extract leaves the registered
handler (and the Run key) pointing at the deleted old copy — links then
fail with "Application not found" and NO events are logged (the handler
exe never runs; both symptoms share this one cause). Every daemon start
(`start_daemon`, before the port probe) calls
`portal::heal_windows_registration()`: it rewrites the handler command
values in place (no key create/delete, so `UserChoice` survives — the
Chrome-update model) and repairs a stale Run value. `api::status`
exposes `handler_ok` so the portal can show the real state.

## In-app updates & installer

Pattern borrowed from pound (same maintainer): **the official installer is
the upgrader**, and the app's "update" action just runs it.

- `tools/install.ps1` (README one-liner, download-then-run-as-file) installs
  to `%LOCALAPPDATA%\Programs\Linkport`: stops every running daemon,
  copies both exes (+ any DLLs, QUICK-START, LICENSE), Start Menu shortcut,
  user PATH (`-NoPath` to skip), registers **only when not registered
  yet** (the handler path is repaired in place otherwise — re-registering
  can reset the hashed `UserChoice`), repoints the Run auto-start value if
  one exists, then starts the daemon (`-NoLaunch` to skip). Settings never
  live in the install dir (`%APPDATA%\linkport` is untouched), which is
  all the migration a manual copy elsewhere needs: quit it, delete it, run
  the installer. `-BinDir` / `-DownloadUrl` / `-InstallDir` / `-Repo` for
  local builds and tests. `tools/uninstall.ps1` reverses it and keeps
  `%APPDATA%\linkport` unless `-PurgeData`.
- Startup check (config-gated) → tray item **Install update v…** /
  Settings **Install now** → `POST /api/update/install` →
  `update::install_in_background`: spawns PowerShell hidden with stdout+
  stderr appended to `%APPDATA%\linkport\update.log`; the script
  downloads the installer from raw `main` and runs it. The installer runs
  `linkport-cli stop` → `POST /api/quit` (bearer = the token file) → the
  daemon's `AppState.quit` watch channel → the same graceful exit as the
  tray's Quit (clean tray-icon removal). The installer then replaces the
  exes and relaunches `linkport.exe serve` (first run reopens the portal
  via the existing token logic). Installing is **never automatic** — it is
  always the user's click.
- If the installer exits while the daemon is still alive, it never reached
  the replace step (download failed, …): the waiting thread posts
  `CMD_UPDATE_FAILED`, the tray item goes back to "Install update v…"
  with "see update.log", and `INSTALLING` is cleared so the Settings page
  (which polls `/api/ping` for a new version) reports the failure instead
  of waiting out its 3-minute deadline.
- `is_newer` is strict (exactly three numeric parts, prerelease suffixes →
  not newer): a bad tag must never be *installed*.
- GOTCHA (PS 5.1): `irm … | iex` cannot run a script with a `param()` block
  — hence the download-then-`-File` one-liner, and the daemon's
  `installer_script()` doing the same.
- GOTCHA (PowerShell): the call operator does NOT wait for GUI-subsystem
  binaries and leaves `$LASTEXITCODE` unset; and with
  `$ErrorActionPreference = 'Stop'` any stderr line from a native command
  under `&` becomes a terminating error before the exit code can be read.
  Run `linkport-cli` from scripts with `Start-Process -Wait -PassThru`
  plus `-RedirectStandardOutput/Error` to temp files (see
  `Invoke-LinkportCli`), and launch the daemon with `Start-Process`.
- GOTCHA (PS 5.1): profiles with dots in the username get 8.3 short-form
  TEMP/LOCALAPPDATA (`C:\Users\HENDRY~1.CHO\…`), which trips `Remove-Item`
  with a *terminating* error that `-ErrorAction SilentlyContinue` cannot
  suppress. Canonicalize via `Resolve-Path`/`GetFolderPath`, use
  `-LiteralPath`, and guard cleanup with try/catch (the scripts do).
- GOTCHA: `Write-Host` from powershell.exe lands on its stdout when
  redirected — that is why the daemon can transcript the installer into
  `update.log` (and pound can stream it into a toast). Do not pipe it:
  the daemon is gone before the installer finishes, and a broken pipe
  under `ErrorActionPreference = Stop` could abort the install.
- GOTCHA: `Get-Process -Name linkport` matches `linkport.exe` only (the
  daemon, the URL handler, `restart`) — never `linkport-cli`, so the cli
  running `stop` never kills itself. No path filter on purpose: every
  `linkport.exe` shares `%APPDATA%\linkport` and therefore the portal
  port, so any of them must go before the new daemon can bind.
- WSL: none of this can run on the Linux dev box; the PowerShell files
  have no syntax check here. Read changes to them twice.

## Runtime state

`%APPDATA%\linkport` (Windows) / `~/.config/linkport` (Linux):

- `config.toml` — browsers + ordered rules (portal PUTs this atomically)
- `token` — portal bearer token (created on first run; first-run portal
  auto-open triggers when it did not exist before the boot)
- `events.jsonl` — append-only history of every routed URL + errors
  (**primary debugging tool**: check the newest line after a click)
- `paused.flag` — presence = routing paused (hot path sends everything to
  `default_browser`)
- `update.log` — output of the last in-place update the daemon started
  (the installer's transcript); the tray/portal point here when an update
  did not complete

## Architecture map

- `crates/linkport-core` — pure, platform-free, most unit tests live here.
  `config.rs` (serde model, atomic save, validation), `engine.rs`
  (ordered rules → `Decision` with per-rule `RuleTrace`), `launcher.rs`
  (`{url}` arg templates + Windows command-template parsing),
  `event.rs` (JSONL log).
- `crates/linkport-win` — Windows integration behind `#[cfg(windows)]`
  with non-Windows stubs (the workspace must `cargo check` on Linux):
  `registry/` (default-browser registration), `discover/` (StartMenuInternet
  scan + system-default detection), `autostart/` (Run key), `shortcut/`
  (Start-menu `.lnk`).
- `crates/linkport` — `lib.rs` (`open.rs` hot path + redirect-aware
  `decide()`, `redirect.rs` headers-only redirect resolution (ureq),
  `update.rs` GitHub `releases/latest` update checks (startup, tray,
  `POST /api/update/check`; strict semver compare + tray labels are
  unit-tested) plus the in-place upgrade (`install_in_background` spawns
  the official installer, see below), `portal.rs` axum daemon + static SPA
  serving + auth (+ `request_quit`, the client side of `/api/quit`),
  `api.rs` JSON API, `tray.rs`) and bins `linkport.rs` (the windowless
  GUI-subsystem main binary: URL handler + `serve`/no-args daemon) /
  `linkport_cli.rs` (console CLI: test/register/stop/browsers/dev serve).
- `tools/` — `install.ps1` / `uninstall.ps1`: the README one-liner.
  `install.ps1` is also the in-place upgrader the daemon spawns, so a
  change to it ships to every existing install immediately (raw
  `main` URL) — treat it like a release.
- `frontend/` — Next.js 15 **static export** (`output: 'export'`), React 19,
  shadcn/ui, Tailwind v4. All pages are client components. API client in
  `src/lib/api.ts`, shared types in `src/lib/types.ts` (mirror the Rust
  serde structs; JSON is snake_case on both sides).

### Where to add things

- New rule matcher → `linkport-core/src/engine.rs` (`matches_rule`) + test;
  surface in `frontend/src/app/rules/page.tsx` form + `types.ts`.
- New API endpoint → `crates/linkport/src/api.rs` + route in
  `portal.rs` + `frontend/src/lib/api.ts` (+ types) + page.
- New tray menu item → `tray.rs` (item + id). If it must mutate a
  `CheckMenuItem`, do the mutation on the tray thread via a new
  `WM_APP` command constant (see below).
- New config field → `linkport-core/src/config.rs` (serde defaults,
  roundtrip test) + portal UI.
- Update checks: `crates/linkport/src/update.rs` owns the model, the
  GitHub lookup, the tray label strings and the installer spawn (keep the
  strings host-testable; tray.rs only wires menu items). The result lives
  in `AppState.update` (Arc<Mutex<Option<UpdateCheck>>>); every writer —
  startup check, tray click, `/api/update/check` — stores it there and
  calls `tray::notify_update_checked()` so the tray thread re-texts the
  menu items (items are not `Send`; see the WM_APP gotcha). Installing
  (`/api/update/install` or the tray item) goes through
  `update::install_in_background`, which posts `notify_update_installing`
  / `notify_update_install_failed` the same way and toggles the
  `INSTALLING` flag that `/api/status` exposes as `installing`.
- New Windows registry integration → new module in `linkport-win` with
  imp/stub split.

## Gotchas & API quirks (learned the hard way)

- **muda/tray-icon menu items are not `Send`** (`Rc` inside). All item
  mutation (e.g. `set_checked`) must happen on the tray thread. The
  menu-event thread forwards toggles as `WM_APP` messages
  (`PostThreadMessageW`) that the tray thread handles inside its
  `GetMessageW` pump.
- **windows-sys 0.59**: module paths are nested
  (`Win32::System::Threading`, `Win32::UI::WindowsAndMessaging`);
  `HWND` and `HANDLE` are `*mut c_void` (use `std::ptr::null_mut()`,
  not `0`); `GetMessageW(lpmsg, hwnd, u32, u32)`.
- **winreg 0.52**: no `winreg::errors` module — operations return
  `std::io::Result`. `create_subkey` returns `(key, disposition)`;
  write the default value with `set_value("", &…)`; `delete_subkey_all`
  removes trees.
- **rust-embed 8.x**: `#[folder]` is resolved relative to
  `CARGO_MANIFEST_DIR` (NOT the source file), and `$CARGO_MANIFEST_DIR`
  interpolation is NOT supported. Our path: `"../../frontend/out"`.
- **Console flash**: never point registry commands at the console
  `linkport-cli.exe`. The GUI-subsystem `linkport.exe` exists precisely so
  no console window flashes (URL handling, login auto-start).
- **Glob semantics**: host matchers are cookie-domain style — the pattern
  (with an optional `*.` prefix stripped) matches the host itself and any
  subdomain at any depth, dot-boundary respected (`example.com` matches
  `a.example.com`; `notexample.com` does not). Patterns containing other
  wildcards fall back to `globset` matching (see `matches_rule`).
- **Redirect resolution** fires when a rule with the `resolve` target
  matches the clicked URL (the Rules-page "Follow redirects" option), or
  — legacy config — when NO rule matched and the host is listed in
  `config.redirect_hosts`. It follows 3xx `Location` headers (max 5 hops,
  5s timeout) without reading response bodies and re-runs the engine on
  the final URL; the event log stores the final URL plus `origin_url`.
  Chains that end at another `resolve` rule fall back to the default
  browser (loop guard) — and so does a fetch that yields no redirect
  (offline, timeout, non-web scheme, a 200 on the wrapper host):
  `open::decide()` never returns `Outcome::Resolve`; the clicked link goes
  to the default browser. Chains ending at an SSO interstitial (e.g.
  `login.microsoftonline.com`) can be routed precisely with host +
  `url_contains` on the destination query param (`redirect_uri=…`); the
  Dashboard "from host" button prefills this from the history entry's URL.
- **Chrome-style registry commands** parse via
  `launcher::parse_command_template` (`%1` → `{url}`); browsers with no
  `{url}` in args get the URL appended.
- **WSL development** (if applicable): a portal bound on the Windows side
  is NOT reachable from WSL at `127.0.0.1` (WSL2 NAT). Test Windows-side
  behavior by running the Windows exe via interop and read `events.jsonl`
  for outcomes; test the portal itself with the Linux build.
- **Formatting**: the repo is `cargo fmt`-clean; re-run `cargo fmt --all`
  before committing. (fmt also reformats strings/line breaks — when
  applying text-based edits, re-read the file first instead of trusting
  remembered content.)
- Frontend: Next static export forbids extra exports from `page.tsx`
  (one default export per route; put shared components in
  `src/components/`). `useSearchParams` needs Suspense; we read
  `window.location.search` in effects instead.

## Verification checklist for changes

1. `cargo test --workspace` (host)
2. `cargo check --target x86_64-pc-windows-gnu --workspace --all-targets`
   (validates the cfg(windows) code paths)
3. `(cd frontend && npm run build)` (type-checks the portal)
4. Release build: `cargo build --release --target x86_64-pc-windows-gnu -p linkport`
5. Behavior on a Windows install: `linkport-cli.exe test "<url>"`, then
   check the newest `events.jsonl` line; user verifies tray/portal
   behavior on the desktop
6. After touching `tools/*.ps1` (no PowerShell on the Linux dev box):
   `tools\install.ps1 -BinDir <build dir>` on Windows — fresh install,
   upgrade over it, and `linkport-cli stop` (the daemon's tray icon must
   vanish cleanly, no ghost); the in-app path: tray → "Install update"
   after bumping the version locally, then check `update.log`

## Current limitations / next ideas

- Ambiguous-link picker dialog (websteer-style), browser-running heuristic,
  shortener expansion — see README roadmap.
- No code signing (SmartScreen warns on the unsigned exe; the installer
  and quick-start say "More info" → "Run anyway") — a winget manifest
  is a natural next step once there is a public release.
- Linux port: core is ready; needs `.desktop` + `xdg-settings` glue in a
  `linkport-linux` crate.
