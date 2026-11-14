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
   ask. The result is a squarified map of logical size, linked to a
   sortable outline of real directories and large files, with logical and
   allocated sizes, a search box, a reclaimable filter, and a detail pane.
2. **Environments.** Probes of Node.js, Python, Java, Rust, and Go, plus
   known cache locations. Each row names the manager, the scope, and why
   an action is absent. fnm and uv can install a version and can change
   what a new shell or a project file selects. That does not change the
   terminal that is already open. Other managers stay unavailable.
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

## Capability matrix

| 0.4.4 capability | This version | Why |
| --- | --- | --- |
| See installed Node, Python, Java, Rust, Go and who owns them | Kept | The inspection half of environment management |
| `fnm install`, `fnm default`, `uv python install`, `uv python pin` | Kept, after an explicit preview | These are the supported end-to-end manager paths. `fnm default` affects new shells. `uv python pin` writes `.python-version`. The open terminal is unchanged, and the review says so |
| `fnm list-remote`, `uv python list` | Kept, on Check available versions | Not run during a passive refresh. The Node list is capped at 12 newest names and says the full list is longer |
| Uninstall a non-default fnm, uv, or rustup toolchain | Kept | Active and default versions are refused |
| npm, pnpm, pip, and `go clean -cache` | Kept | The manager's own cache command, with no byte claim |
| Install or remove the managers, rewrite shell profiles | Dropped | A partial installer that edits rc files is worse than leaving the manager's own install instructions |
| nvm, mise, asdf, volta, sdkman, Homebrew, Go, Java changes | Dropped, shown as unavailable | nvm is a shell function. The others are not an allowlisted argv here |
| Corepack, package updates, background "latest" checks | Dropped | Not required to install or select a runtime, and easy to present as success without doing the work |
| Envark updater, signing, installer publishing | Dropped | Release machinery, not a disk or runtime workflow |
| Ollama start/stop and model deletion | Dropped | A service and model store, measured only |
| Gradle retention, Cargo download locks, Maven deletion, Go module cache deletion | Dropped | Those trees can be the only copy of an artifact. Measurement stays |
| Git worktree removal | Dropped | It discards local checkouts |
| Browser profile cleanup | Dropped | Playwright and Puppeteer cache directories can still be trashed when the path is the known cache directory |
| Traditional Chinese catalog, editable shortcut map | Simplified Chinese, system, or English | `zh-TW` and `zh-Hant` preferences use the Simplified catalog and say so. Missing strings stay English |

## Space map

The scanned directory is a squarified treemap and it is the main view.
Cell area is logical size. A directory shifts from stone toward clay as
more of its bytes are reclaimable; files stay sage. That color is the
data, not a second theme. Empty entries stay in the list so a zero-byte
directory is not drawn as if it had size. At most 12 cells are drawn; the
rest is one remainder cell labeled with its count and its real byte total.
A bar above the map shows the breadcrumb, the selected path, its logical
size, and whether any of that size is reclaimable, plus Up, Open, and the
mark action when the selection is a cleanup target. Click selects the same
path as the outline. Double-click or Open drills into a directory.
Breadcrumbs and Up move back. A short zoom plays on that change and
finishes; Reduce motion holds the final frame. On a wide window the
outline scrolls beside the map and shows the name and logical size. On a
narrow window the page scrolls, so the list is not clipped under the map,
and that outline keeps the name, logical size, and risk. Kind and allocated
size stay in the detail panel. Small cells stay unlabeled; the list and
the inspector carry their names.

The cell shader colors the pixel of the cell that contains it, plus a
selection edge, an incomplete hatch, and a scan band only while a scan is
running on a GPU renderer. It does not cover the window with a gradient.
Where `Painter.Accelerated` is false and the map is larger than about
24,000 DIPs squared, the same rectangles are filled on the CPU. Renderer
diagnostics and a 220×88 shader sample stay on Settings. The sample always
runs the shader, including MyGo's CPU interpreter, so the shader path is
exercised without pretending the main map was drawn by a GPU.

Environments uses the same graphite surface. Each runtime is a card with
its version, scope, and manager. The long reason wraps in the panel under
the selected card. fnm and uv actions stay on the card. The page says once
that those commands do not change the open terminal.

MyGo draws a frame only when something changes. The scan band calls
`AnimationFrameRate(30)` only while a scan is running, the GPU renderer is
active, and Reduce motion is off. Drill motion is one `Keyframes` pass of
180ms. An idle window does not ask for another frame.

## What this version drops

The matrix above is the list. In short: manager installation and shell
profiles, unsupported version managers, package and app updates, Ollama
lifecycle, deletion of caches that may be unique, Git worktrees, browser
profiles, and a separate Traditional Chinese catalog. Previous
`settings.json` is left in place and is not overwritten.

Removal that remains is narrow: non-default `rustup toolchain uninstall`,
non-default `fnm uninstall`, and non-default `uv python uninstall`, plus
`npm cache clean --force`, `python -m pip cache purge`, `go clean -cache`,
and `pnpm store prune`. Install and default changes that remain are
`fnm install`, `fnm default`, `uv python install`, and `uv python pin`.
Each is unavailable when its binary is absent. A plan whose catalog
generation is no longer current fails as stale and does not run.

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

Envark does not use their names, layout, icons, copy, or source. The space
map is a squarified treemap of one directory at a time, drawn with
Envark's own ink and MyGo's shader or CPU rectangles. It is not a copy of
another disk map. Cleanup does not offer a general file shredder, a
remote delete script, app uninstall leftovers, or system optimization.

The safety rules that came from Envark itself stay: plan the target,
revalidate, reject link escapes, prefer trash for project artifacts, and
let a package manager clean its own cache.

## Verification

Commands run on Linux x86_64 (Go 1.27.4, GTK 3, MyGo v0.9.0). The display
was X.Org. There is no `/dev/dri`. With the default renderer, MyGo logged
four startup frames, each `drawn in memory`, and then no further frames
while the window sat idle (`MYGO_FRAME_STATS=all`). A click or scroll
produced more frames and then stopped. `MYGO_GPU=1` produced one later
frame logged as `drawn on the GPU` (950 ms, software OpenGL; there is
still no DRM device). `Painter.Accelerated` stayed false, and the map
kept the CPU rectangles. The shader itself was executed by MyGo's CPU
interpreter: an 80×40 cell painted `#ead9b0` in 2.8 ms inside
`ui.NewTester`, and Settings shows the same three-cell sample.

Nothing under a real home directory was scanned or deleted. The live
window used `/tmp/envark-live` for data, home, PATH, trash, and the
scanned tree.

- `gofmt -l .` printed nothing.
- `go test ./...` passed (35 tests), including argv, cancellation, stale
  plans, missing managers, the size map, selected-row contrast, and
  Simplified Chinese at 720 px.
- `go vet ./...` passed.
- `go build -o envark .` produced a Linux binary. The window mapped.
  Double-clicking the `api` cell drilled to `target` (4 MiB, reclaimable)
  beside `photo.bin` (3 MiB, a file). The permission failure on `locked`
  stayed on screen.
- macOS and Windows app processes were not run. A cross-compile is not
  that test. The earlier CI compile jobs are not a runtime result.
