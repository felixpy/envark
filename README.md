# Envark

A desktop home for developer environments, installed tools, and disk space.

Envark uses a React interface based on the supplied Figma Make design and a standalone Rust core inside Tauri 2. The application targets Windows, macOS, and Ubuntu. This repository is under active development.

## Development

Use an existing Rust toolchain (1.99 or newer), Node.js 24, and pnpm 11.19.0. Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
pnpm install --frozen-lockfile
pnpm desktop
```

`pnpm dev` opens the interface in a browser. Browser mode clearly identifies itself and does not read or modify the host filesystem. Native operations require the desktop application.

For populated-screen development checks, open `/preview.html` on the development server. It uses clearly labeled synthetic fixtures, rejects host operations, and is excluded from production builds.

```sh
pnpm check
pnpm test
pnpm build
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm desktop:build
```

Run `pnpm locales` after adding interface strings. Traditional Chinese is generated at development time, keeping conversion dictionaries out of the application bundle. Source identifiers, comments, and contributor documentation use English.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). After installing dependencies, run `pnpm prepare` to enable the local `commit-msg` check; CI validates new commits and pull request titles. See [CONTRIBUTING.md](CONTRIBUTING.md) for examples and commit scope guidance.

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

- Project scanning prunes dependency and build trees before discovering nested projects. Directory measurement uses at most four workers. Child process concurrency is bounded independently.
- Scans support cancellation and report inaccessible paths. Unknown activity is never classified as inactivity.
- Native file notifications allow reuse of unchanged roots for up to 60 seconds. Cleanup always remeasures its targets.
- Cleanup plans are stored in the native process, expire after ten minutes, and can be executed only once. The renderer sends identifiers, never arbitrary executable commands.
- Project cleanup requires current project markers, an approved scan root, an allowlisted generated directory, and unchanged contents. Symbolic links and Windows junctions cannot be cleanup roots.
- Project artifacts and browser downloads go to the system Trash by default. Moving to Trash is not reported as reclaimed disk space.
- Shared caches use their owning tool's cleanup commands. Their displayed size is an estimate of the maximum available content, not a guarantee of reclaimed space.
- Settings, inventory, and activity are stored locally in the operating system's application data directory. Configuration edits create a backup and reject concurrent external changes.
- Public package registry checks are opt-in. Model usage and browser usage remain unknown when no reliable source exists.

Space figures are logical sizes. Hard links, shared model layers, sparse files, and filesystem compression can change physical disk usage.

## Builds

GitHub Actions validates the code and produces Windows NSIS installers, macOS DMGs for Apple Silicon and Intel, and Ubuntu DEB/AppImage packages. Every successful branch build uploads artifacts. A `v*` tag publishes the corresponding release after all platform jobs pass.

Signing certificates are not configured. Generated packages are unsigned; macOS notarization and Windows signing are release infrastructure work, not implied by a successful build.

## Reference

The interface follows the user-provided [Figma Make prototype](https://www.figma.com/make/KWKVJuPEKMrlT1dOhfEoOE/DevShelf-Desktop-App-Prototype), renamed to Envark. Prototype-only sample data and simulated successful operations are not included in the native backend.
