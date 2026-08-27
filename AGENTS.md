# AGENTS.md

Working notes for AI coding agents (and humans) continuing development on
Linkport. Read this before building or editing anything.

## What this is

Linkport is a rule-based browser router for Windows: a Rust backend with an
embedded Next.js portal (rules/browsers/history management) and a system tray
daemon. One crate produces two binaries. See README.md for the user-facing
overview.

## Environment (this machine)

- Development happens **inside WSL** (Ubuntu 24.04) at `~/repo/linkport`.
- The interactive user shell is **zsh** with an fnm hook in `~/.zshrc`.
  Non-interactive shells (most agents) will NOT have cargo/node on PATH.
  Always export explicitly:

  ```bash
  export PATH="$HOME/.cargo/bin:$HOME/.local/share/fnm/aliases/default/bin:$PATH"
  ```

- Toolchain: rustup stable, Node v24 (fnm), `mingw-w64` for cross-linking
  the Windows exe. Frontend deps installed (`frontend/node_modules`).
- Windows deployment target: `C:\Tools\Linkport` = `/mnt/c/Tools/Linkport`
  from WSL. The installed exes are real NTFS copies — the install never
  depends on WSL at runtime.
- Windows processes/registry can be managed from WSL via interop:
  `/mnt/c/Windows/System32/taskkill.exe`, `reg.exe`, and
  `powershell.exe` all work.

## Commands

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/share/fnm/aliases/default/bin:$PATH"

cargo test --workspace                      # unit tests (all in linkport-core)
cargo check --target x86_64-pc-windows-gnu --workspace --all-targets
(cd frontend && npm run build)              # static export → frontend/out
cargo build --release --target x86_64-pc-windows-gnu -p linkport
./scripts/build.sh                          # frontend + release, all-in-one
cargo fmt --all                             # repo is kept fmt-clean; run before committing
```

`frontend/out` must exist for the Rust build (rust-embed); `build.rs`
creates a placeholder automatically if missing.

## Deploy routine (WSL → Windows)

The running daemon **locks its exe** — overwriting a running executable
fails with a misleading "Permission denied". Always kill first:

```bash
/mnt/c/Windows/System32/taskkill.exe /IM linkport-open.exe /F
sleep 1
cp target/x86_64-pc-windows-gnu/release/linkport.exe \
   target/x86_64-pc-windows-gnu/release/linkport-open.exe /mnt/c/Tools/Linkport/
/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -Command \
  "Start-Process -WindowStyle Hidden 'C:\Tools\Linkport\linkport-open.exe' -ArgumentList 'serve'"
/mnt/c/Windows/System32/tasklist.exe | grep -i linkport   # verify
```

## Windows integration state (all HKCU, no admin)

| Purpose | Key / mechanism | Value shape |
|---|---|---|
| URL handler (ProgId) | `Software\Classes\Linkport.URL\shell\open\command` | `"C:\Tools\Linkport\linkport-open.exe" "%1"` |
| Browser candidate | `Software\Clients\StartMenuInternet\Linkport\…` (+ `RegisteredApplications`) | capabilities + URLAssociations http/https |
| Default-browser choice | `…\UrlAssociations\{http,https}\UserChoice` | `Linkport.URL` (user sets via Settings; apps cannot) |
| Login auto-start | `Software\Microsoft\Windows\CurrentVersion\Run\Linkport` | `"C:\Tools\Linkport\linkport-open.exe" serve` |

After re-registering, verify `UserChoice` didn't get reset by Windows.

## Runtime state

`%APPDATA%\linkport` (Windows) / `~/.config/linkport` (Linux), i.e.
`/mnt/c/Users/hendry.chou/AppData/Roaming/linkport` from WSL:

- `config.toml` — browsers + ordered rules (portal PUTs this atomically)
- `token` — portal bearer token (created on first run; first-run portal
  auto-open triggers when it did not exist before the boot)
- `events.jsonl` — append-only history of every routed URL + errors
  (**primary debugging tool**: `tail -1 …/events.jsonl` after a click)
- `paused.flag` — presence = routing paused (hot path sends everything to
  `default_browser`)

## Architecture map

- `crates/linkport-core` — pure, platform-free, all unit tests live here.
  `config.rs` (serde model, atomic save, validation), `engine.rs`
  (ordered rules → `Decision` with per-rule `RuleTrace`), `launcher.rs`
  (`{url}` arg templates + Windows command-template parsing),
  `event.rs` (JSONL log).
- `crates/linkport-win` — registry code behind `#[cfg(windows)]` with
  non-Windows stubs (workspace must `cargo check` on Linux):
  `registry.rs` (default-browser registration), `discover.rs`
  (StartMenuInternet scan, deduped), `autostart.rs` (Run key).
- `crates/linkport` — `lib.rs` (`open.rs` hot path, `portal.rs` axum daemon
  + static SPA serving + auth, `api.rs` JSON API, `tray.rs`) and bins
  `linkport.rs` (console CLI) / `linkport_open.rs` (GUI subsystem:
  `<url>` handler + `serve` mode).
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
- **Console flash**: never point registry commands at console
  `linkport.exe`. The GUI-subsystem `linkport-open.exe` exists precisely so
  no console window flashes (URL handling, login auto-start).
- **Glob semantics**: `globset`'s `*` does not cross `.` boundaries the way
  users expect, so the engine adds cookie-style handling:
  `*.example.com` also matches `example.com` (see `matches_rule`).
- **Chrome-style registry commands** parse via
  `launcher::parse_command_template` (`%1` → `{url}`); browsers with no
  `{url}` in args get the URL appended.
- **WSL networking**: a portal bound on the Windows side is NOT reachable
  from WSL at `127.0.0.1` (WSL2 NAT). Test Windows-side behavior by
  running the Windows exe via interop (works fine) and read
  `events.jsonl` for outcomes; test the portal itself with the Linux
  build.
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
4. Release build + deploy routine above
5. Behavior: `./target/x86_64-pc-windows-gnu/release/linkport.exe test "<url>"`
   via interop, then `tail /mnt/c/Users/hendry.chou/AppData/Roaming/linkport/events.jsonl`
6. User verifies tray/portal behavior on the desktop

## Current limitations / next ideas

- Ambiguous-link picker dialog (websteer-style), browser-running heuristic,
  shortener expansion — see README roadmap.
- Detected browser names are raw registry key names (`Firefox-308046B0AF4A39CB`).
- No installer, no code signing (SmartScreen warns on the unsigned exe).
- Linux port: core is ready; needs `.desktop` + `xdg-settings` glue in a
  `linkport-linux` crate.
