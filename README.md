# Envark

Envark is a desktop tool for three questions: what is installed and who manages it, what a chosen folder uses on disk, and what can be reclaimed after you review the exact action.

The interface is English. The app does not scan your home directory until you add a folder. Cleanup is never pre-selected.

[Report a bug](https://github.com/felixpy/envark/issues/new?template=bug_report.yml) · [Suggest a feature](https://github.com/felixpy/envark/issues/new?template=feature_request.yml) · [Contribute](CONTRIBUTING.md)

## Workflows

**Space.** Add a folder and scan it. The outline shows directory names, kind, logical size, allocated size, and risk. Search filters by name. Reclaimable hides everything that is not a recognized project artifact. The detail pane shows the path, why it was classified, and how the largest children share the logical size.

**Environments.** Refresh reads Node.js, Python, Java, Rust, and Go from `PATH`. Check available versions asks `fnm` and `uv` and does not install anything. For those two managers, Envark can preview `fnm install`, `fnm default`, `uv python install`, and `uv python pin`. `fnm default` changes the version new shells start with. `uv python pin` writes `.python-version` in a directory you choose. Neither changes `PATH` in a terminal that is already open; the review shows the `fnm use` text to run there yourself. Removal stays limited to a non-default toolchain (`fnm uninstall`, `uv python uninstall`, `rustup toolchain uninstall`). nvm, mise, asdf, volta, sdkman, Homebrew, Go, and Java stay unavailable, with a reason. Known cache locations are measured only after you ask. A missing tool stays missing.

**Review.** Marks and commands collect here with the exact path or the exact argument vector, the risk, and whether Trash can restore the item. Moving files to Trash is the default and still asks for confirmation. Permanent deletion stays disabled until Settings turns it on, and then asks again. Manager commands stay disabled until you acknowledge that Trash cannot restore them, and they do not report a byte count. History lists each item, including failures, and can restore a trashed directory while the trash path still exists.

## Measurement

Logical size is the sum of file lengths. Allocated size is filesystem block usage when the platform reports it (`st_blocks × 512` on Unix). Windows repeats the logical size and says so. Hard links count once per scan. Symlinks are not followed and are not cleanup targets. Other volumes are not entered. Directory names in the exclude list (`.git`, `.svn`, and `.hg` by default) are omitted rather than shown as zero. Unreadable entries make the scan incomplete and are named. Cancelling discards that scan.

A cleanup result reports the logical bytes of items that were actually moved or deleted. That number is not a measurement of free disk. Nested marks count the ancestor once. Manager commands add no byte claim.

## What this version does not do

Envark 0.4.4 installed managers, changed the default runtime, updated packages, published an updater, managed Ollama models, removed Git worktrees, and cleaned browser profiles. This version does not. Those actions are omitted, not stubbed as success.

Still unavailable on purpose:

- Installing the managers themselves (fnm, nvm, Bun, uv, pnpm, Yarn, rustup, SDKMAN!, mise, Ollama) and editing shell profiles. Version install and default/pin are offered only for fnm and uv, and they do not rewrite the shell that is already open.
- Corepack, package updates, and online "latest version" checks.
- Envark's own installer, signed updates, and release publishing.
- Starting or stopping Ollama, or deleting models.
- Deleting the Go module cache or a Maven repository. They can be measured. They may be the only copy of an artifact.
- Gradle retention cleanup and Cargo download-lock cleanup.
- Removing Git worktrees or discarding local Git changes.
- Cleaning a browser profile. Playwright and Puppeteer cache directories can be moved to Trash only when the path is that known cache directory.
- The Traditional Chinese catalog and a configurable shortcut map.

Removal that remains is the allowlist above, plus `npm cache clean --force`, `python -m pip cache purge`, `go clean -cache` (not the module cache), and `pnpm store prune`. Each one errors when its binary is missing.

## Platforms

MyGo's native UI, which this app uses, documents these runtimes:

| System | Native UI |
| --- | --- |
| macOS 12 or later | Metal |
| Windows 10 and 11, x64 and arm64 | Direct3D 11 |
| Linux x64 and arm64 | GTK 3. WebKitGTK is not required |

The app is pure Go. `GOOS=windows` and `GOOS=darwin` compile the packages. That is not a runtime test. The only system exercised here is Linux x86_64 with GTK 3, on a machine whose native window draws without a GPU. See [DESIGN.md](DESIGN.md) for the provenance of the product decisions and the commands that were run.

## Your existing data

The bundle identifier is still `dev.envark.desktop`. The first launch reads the previous `settings.json` (schema 1, or an older unwrapped file) and writes `preferences.json` (schema 2) beside it. Roots, excludes, protected projects, theme, trash, and scan-on-launch are copied. `settings.json`, `inventory.json`, and `activity.json` are not modified and are not shown. New operations go to `history.json`. A language other than English is kept in the old file and is not applied.

| System | Data directory |
| --- | --- |
| Linux | `$XDG_DATA_HOME/dev.envark.desktop` or `~/.local/share/dev.envark.desktop` |
| macOS | `~/Library/Application Support/dev.envark.desktop` |
| Windows | `%APPDATA%\dev.envark.desktop` |

`ENVARK_DATA_DIR`, `ENVARK_HOME`, `ENVARK_PATH`, and `ENVARK_TRASH_DIR` override those locations. They are for tests and disposable demos.

## Development

Go 1.27.4. On Linux, GTK 3 (`libgtk-3-0`). No Node, Rust, or webview toolchain.

```sh
go test ./...
go vet ./...
gofmt -l .
go build -o envark .
./envark
```

`go tool mygo build` packages a native bundle for the host when you want an installable artifact. `go build` is enough to run the app. Do not point a scan or a cleanup at a real home directory, cache, project, runtime, or Docker data while developing. Use a temporary fixture.

See [CONTRIBUTING.md](CONTRIBUTING.md) for safety rules and commit conventions.
