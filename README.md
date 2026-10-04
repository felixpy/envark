# Envark

Manage development environments, global tools, and disk usage from one desktop application.

Envark targets Windows, macOS, and Ubuntu and is under active development.

## Features

- Inspect runtimes, global tools, and caches across Node.js, Python, Java, Rust, and Go.
- Run supported installation, update, and removal operations through existing tool managers.
- Scan projects for generated dependencies and build artifacts, then review cleanup targets.
- Inspect Ollama models and browser downloads, including their reported disk usage.
- Edit tool configuration with backups and track operation results locally.

## Development

Use an existing Rust toolchain (1.99 or newer), Node.js 24, and pnpm 11.19.0. Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
pnpm install --frozen-lockfile
pnpm prepare
pnpm desktop
```

Use `pnpm dev` for frontend development in a browser. Environment and filesystem operations require the desktop application.

```sh
pnpm check
pnpm test
pnpm build
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm desktop:build
```

Run `pnpm locales` after adding interface strings to update Traditional Chinese translations.

See [CONTRIBUTING.md](CONTRIBUTING.md) for commit conventions and local checks.

## Structure

| Directory            | Responsibility                                                                  |
| -------------------- | ------------------------------------------------------------------------------- |
| `src/views`          | Overview, provider capabilities, projects, caches, activity, and settings       |
| `src/components/ui`  | Local shadcn/ui components                                                      |
| `src/bridge.ts`      | Typed native boundary and explicit browser preview behavior                     |
| `src-tauri`          | Desktop integration, permissions, and native command registration               |
| `crates/envark-core` | Inventory, provider adapters, scanning, operation plans, persistence, and tests |

Provider adapters discover installation owners and supported capabilities. Unknown ownership remains read-only. A new provider should supply inventory and operation planning without changing the page structure.

## Operations and storage

- Review cleanup targets and recovery guidance before execution. Protect projects in the project view to keep them out of cleanup.
- Project artifacts and browser downloads go to the system Trash by default and can be restored from there.
- Shared caches use their owning tool's cleanup commands. Actual reclaimed space can be smaller than the displayed cache size.
- Settings, inventory, and activity are stored locally in the operating system's application data directory. Configuration edits create a backup and reject concurrent external changes.
- Public package registry checks are opt-in. Activity and resource usage are shown as unknown when no reliable source exists.

Space figures are logical sizes. Hard links, shared model layers, sparse files, and filesystem compression can change physical disk usage.

## Builds

GitHub Actions validates the code and produces Windows NSIS installers, macOS DMGs for Apple Silicon and Intel, and Ubuntu DEB/AppImage packages. Every successful branch build uploads artifacts. A `v*` tag publishes the corresponding release after all platform jobs pass.

Build artifacts are currently unsigned.
