# Envark rewrite

This document records the product direction for the MyGo rewrite and the
ideas borrowed from public descriptions of Mole and DiskTree. It is not an
implementation port of either project.

## Baseline

Envark 0.4.4 is a Tauri, Rust, and React desktop app. The Rust core
discovers language runtimes, measures project artifacts and global caches,
plans cleanup, and stores settings, inventory, and activity under the
Tauri data directory `dev.envark.desktop`. The interface exposes Overview,
Environments, Projects, Caches, Activity, and Settings, plus manager
installation, runtime defaults, package update checks, Ollama lifecycle,
worktree removal, and application auto-update.

That surface is broader than the questions a developer actually brings to
the tool: what is installed and who owns it, what consumes disk, what can
be reclaimed and at what risk, and how to perform one supported action
safely. The rewrite keeps those questions and drops operations that cannot
be offered completely and honestly in this version. Dropped behavior is
listed below. It is not silently replaced with a success-shaped control.

## Framework

MyGo (`github.com/egoist/mygo`, MIT, module `go 1.27.3` on the upstream
`go.mod` at the time of this rewrite) builds a desktop app in Go. A window
is either native UI drawn by MyGo, or a page in the system webview.

Envark uses native UI only (`package ui`). The native-UI documentation
describes it as the fit for tools, settings, inspectors, and utilities:
the view is a Go function, there is no HTML or JavaScript, and an app of
native UI does not ship a webview. That matches a table-and-tree desktop
tool and removes the second toolchain (Node, pnpm, Vite, Tauri).

The web-frontend route was rejected because it would keep a browser stack
inside a product whose problem is already too many tools. Tauri stays
removed on this branch so the repository does not contain two apps.

Documented runtime, from MyGo's requirements page:

| Platform | Native UI |
| --- | --- |
| macOS 12 or later | Metal, no extra webview |
| Windows 10 and 11, x64 and arm64 | Direct3D 11 |
| Linux x64 and arm64 | GTK 3, not WebKitGTK |

MyGo reports that it is pure Go without cgo, so other platforms can be
cross-compiled. Cross-compilation is not a runtime test. The verification
section of the README records which systems actually ran.

## Product

Three workflows, each finished before another manager button is added:

1. **Space.** The user chooses directories. Nothing is walked until they
   ask. The result is a sortable outline of real directories and large
   files, with logical and allocated sizes, a search box, a reclaimable
   filter, and a detail pane for the selected row.
2. **Environments.** Read-only probes of Node.js, Python, Java, Rust, and
   Go, plus known cache locations. Each row names the manager or says why
   ownership is unknown. Removal and cleanup commands exist only for an
   allowlisted argv and are absent, with a reason, otherwise.
3. **Review.** Marks and commands gather here. The page lists exact paths
   or exact argument vectors, risk, recoverability, and the expected
   effect. Trash is the file default. Irreversible commands and permanent
   deletion need their own confirmation. Nothing dangerous is pre-selected.
   History records every item result, including partial failure and cancel.

Settings hold scan roots, directory-name excludes, protected projects,
trash, and whether permanent deletion is even offered. History supports
restore when Envark still knows the trashed path.

## Measurement

- Logical size is the sum of file lengths.
- Allocated size is `st_blocks × 512` when the platform reports it.
  Windows reports logical size again and the UI says allocated figures are
  not distinct there.
- A hard-linked file's bytes count once per scan. Extra names are counted
  as names, not as more bytes.
- Symlinks are not followed and are not cleanup targets.
- A directory on another device is not entered. Its size is not guessed.
- Excluded directory names (default `.git`, `.svn`, `.hg`) are omitted, not
  measured, and not described as zero.
- Unreadable entries increment a skipped count. The scan is marked
  incomplete. Other entries are still reported.
- Cancelling a scan discards that scan. A previous completed tree can stay
  on screen and is labeled as previous.
- Nested marks keep the ancestor only. The logical total adds that
  ancestor once.
- Manager commands do not contribute a "bytes freed" number. The result is
  the command's exit status. File results report logical bytes of items
  that were actually moved or deleted, not a claim about free disk.

## Safety

- Scan, discovery, and measurement only read.
- Cleanup targets are recognized project artifacts or known cache
  directories. Arbitrary files are visible and are not delete buttons.
- The scan root, the home directory, system locations, protected projects,
  and anything that would contain a protected project are refused.
- A path is refused when a symlink exists at the scan root or anywhere
  below it on the way to the target. Ancestors above the root may be links
  (for example macOS `/tmp`) because both the root and the target share
  them. `EvalSymlinks` must still land inside the resolved root.
- The target's device and inode are recorded in the review. Immediately
  before the move or delete, Envark stats again and, for a directory,
  measures again. A mismatch skips that item. Other items continue.
- Trash uses the XDG trash on Linux, `~/.Trash` on macOS, and the Windows
  shell recycle operation. A cross-device rename is a failure, not a
  fallback to permanent delete.
- Permanent deletion is off until Settings enables it, and still requires
  a second confirmation that lists paths.
- Manager actions are constructors. They return a fixed argv or an error.
  Execution uses `exec.Command` argument arrays from an empty working
  directory. Version probes use the same rule. There is no shell.
- Active or default toolchains are not uninstall targets.
- A missing binary is an unavailable row, not a disabled button that
  reports success.

## What this version drops

These existed in 0.4.4 and are not in this rewrite:

- Installing or removing fnm, nvm-sh, Bun, uv, pnpm, Yarn, rustup,
  SDKMAN!, mise, or Ollama, and rewriting shell profiles.
- Setting the default runtime, Corepack and package updates, and online
  "latest version" checks.
- Envark's own updater, signed release metadata, and installer publishing.
- Ollama service start/stop and model deletion.
- Gradle's retention cleanup, Cargo's download locks, and Maven deletion.
  Those directories can be measured. Maven and the Go module cache cannot
  be removed here because they may be the only copy of an artifact.
- Git worktree removal and discarding local Git changes.
- Browser profile cleanup. Playwright and Puppeteer cache directories can
  be trashed only when the path is the known cache directory.
- The Traditional Chinese catalog and the configurable shortcut map.
  Previous `settings.json` is left in place and is not overwritten.

Removal that remains is narrow: non-default `rustup toolchain uninstall`,
non-default `fnm uninstall`, and non-default `uv python uninstall`, plus
`npm cache clean --force`, `python -m pip cache purge`, `go clean -cache`,
and `pnpm store prune`. Each is unavailable when its binary is absent.

## Migration

On Linux the Tauri data directory is
`$XDG_DATA_HOME/dev.envark.desktop` or
`~/.local/share/dev.envark.desktop`. macOS uses
`~/Library/Application Support/dev.envark.desktop`. Windows uses
`%APPDATA%\dev.envark.desktop`. The identifier is unchanged.

The first launch reads `settings.json` (schema 1, or a legacy unwrapped
document) and writes `preferences.json` (schema 2). Roots, excludes,
protected projects, theme, trash, and scan-on-launch are copied. The old
file is not modified. Corrupt new preferences are renamed aside and
replaced with defaults for the session. `inventory.json` and
`activity.json` are left untouched and are not shown; new operations use
`history.json`.

`ENVARK_DATA_DIR`, `ENVARK_HOME`, `ENVARK_PATH`, and `ENVARK_TRASH_DIR`
redirect those locations for tests and demos. A normal launch does not
need them and does not scan until a root exists and the user asks, unless
a migrated preference still says scan on launch.

## Provenance

Public pages read, and not their implementation trees:

- Mole README (`github.com/tw93/mole`): grouped cleanup, dry-run before
  deletion, protected locations, history, and developer-tool sections.
- DiskTree README (`github.com/tobi/disktree`): an explicit scan, sizes
  that explain allocated versus apparent bytes, hard links counted once,
  symlinks not followed, other mounts not entered, marks that absorb
  descendants without double counting, review before trash or permanent
  delete, and free-space honesty.

Envark does not use their names, layout, icons, copy, or source. Space is
an outline with a proportion bar for the open directory, not a disk
mosaic. Cleanup does not offer a general file shredder, a remote delete
script, app uninstall leftovers, or system optimization.

The safety rules that came from Envark itself stay: plan the target,
revalidate, reject link escapes, prefer trash for project artifacts, and
let a package manager clean its own cache.

## Verification

Commands run on Linux x86_64 (Go 1.27.4, GTK 3, MyGo v0.9.0). The display
was X.Org. MyGo logged that native UI draws without a GPU. The window
still mapped and the workflows below were clicked on a temporary fixture
under `/tmp`, with `ENVARK_DATA_DIR`, `ENVARK_HOME`, `ENVARK_PATH`, and
`ENVARK_TRASH_DIR` pointed at that fixture. Nothing under a real home
directory was scanned or deleted.

- `gofmt -l .` printed nothing.
- `go test ./...` passed, including scan accuracy, unreadable entries,
  other-volume refusal, symlink rejection, nested marks, stale-size
  revalidation, cancellation, manager argv rejection, and the Space to
  Trash path through `ui.NewTester`.
- `go vet ./...` passed.
- `go build -o envark .` produced a Linux binary that opened a window.
- `go tool mygo build` wrote `dist/linux-amd64/` (executable, `.deb`,
  `.rpm`, and `.tar.gz`) for Envark 0.5.0. Those packages were not
  installed.
- `GOOS=windows` and `GOOS=darwin`, each for `amd64` and `arm64`, compiled
  with `CGO_ENABLED=0`. Those binaries were not run.
- In the window: a fixture scan showed `node_modules` and a Cargo `target`
  with logical and allocated sizes; both were marked, confirmed, and moved
  to the fixture trash while `package.json`, `Cargo.toml`, and `README.md`
  stayed; History restored both; Environments probed fixture binaries and
  left missing tools as unavailable; Measure reported the fixture cache
  sizes; adding the npm cache command opened Review with
  `npm "cache" "clean" "--force"` and a disabled Run button until the
  acknowledgement checkbox. A 720px-wide window switched to the top
  navigation and kept the filter on its own row.

macOS and Windows were not launched. A cross-compile is not that test.
