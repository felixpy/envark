# Contributing to Envark

Thank you for helping make development environments easier to understand and maintain. Bug reports, tool integrations, translations, and focused fixes are welcome. Keep discussions respectful and constructive.

## Before you start

Search [existing issues](https://github.com/felixpy/envark/issues) before opening a bug report or feature request. Include a minimal reproduction for bugs and explain the user problem for proposals. Discuss substantial features or new providers before implementing them so maintainers can confirm scope.

GitHub Issues and pull requests are the public contribution channels. Maintainers may track work in Linear, but contributors do not need a Linear account. When an existing Linear issue is relevant, include its identifier or link; otherwise use the GitHub issue.

Report vulnerabilities privately through [SECURITY.md](SECURITY.md), and follow [SUPPORT.md](SUPPORT.md) for usage questions.

## Development setup

Use an existing Rust toolchain 1.99 or newer, Node.js 24.15.0 or later in the Node 24 series, and pnpm 11.19.0. Install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
pnpm install --frozen-lockfile
pnpm prepare
pnpm desktop
```

Use `pnpm dev` for frontend-only development. Native inspection and operations require the desktop app.

| Directory            | Responsibility                                                   |
| -------------------- | ---------------------------------------------------------------- |
| `src/views`          | Overview, environments, projects, caches, activity, and settings |
| `src/components/ui`  | Shared interface components                                      |
| `src/bridge.ts`      | Typed native boundary and browser preview behavior               |
| `src-tauri`          | Desktop integration, permissions, and native commands            |
| `crates/envark-core` | Inventory, providers, scanning, operation plans, and persistence |

Keep provider integrations independent from the interface. Declare supported capabilities and discover the installation owner before enabling mutations. Unknown ownership must remain read-only.

Keep traversal and process execution in the Rust core. Bound concurrent work, support cancellation, and report partial failures. Use subprocess argument arrays rather than interpolated shell commands.

Cleanup changes must plan targets before execution, revalidate canonical paths, reject symlink escapes, preserve source and configuration, and use the system trash for eligible project artifacts. Shared caches should use their owner's supported cleanup command.

Worktree removal reviews staged, unstaged, and untracked paths before execution. By default, a batch keeps worktrees with changes and removes only the other reviewed checkouts. Discarding changes requires an explicit choice in the review dialog and confirmation; Git then removes those checkouts with a single `--force`. This permanently discards local changes without Trash or an automatic stash, while preserving branches and commits. Always revalidate checkout contents, Git status, index, HEAD, registration, scope, protection, and locks before removal. Force must never bypass those checks or authorize changes made after review. Internal links are removed without following their targets. Nested repositories and submodules must appear in the review and require explicit discard confirmation; nested repository history is also removed.

Package version checks use a dedicated provider-scoped command. They must not discover projects, measure artifacts or caches, or rediscover unrelated environments. Keep cancellation and progress tied to the check's job ID. Package managers use their verified installation owner: npm packages retain their owning runtime prefix, Homebrew packages require a matching Cellar and install receipt, Bun standalone installs use `bun upgrade`, and uv standalone installs require a matching installer receipt before `uv self update`. Revalidate ownership and the installed version before execution, then read the installed version again to verify the update actually happened. Do not report a successful update from a zero exit status alone.

Corepack-owned pnpm/Yarn updates change the global default only, with project spec lookup and auto-pinning disabled. Standalone pnpm updates run in an isolated empty workspace. Unix fnm/nvm installer updates download the reviewed official release script before execution, preserve the existing installation root, and disable shell-profile changes. Manager updates are separate from runtime installation/removal. Do not treat nvm-windows as nvm-sh.

First-time manager installation is available for fnm, nvm-sh, Bun, uv, pnpm, and Yarn. Resolve the published version before review, show the official source and destination, reject occupied targets, download installers completely before execution, and verify the installed version afterward. On Windows, use winget for fnm and official PowerShell installers for Bun/uv/pnpm; nvm-sh remains Unix-only. Yarn uses Corepack when available, otherwise explicitly offers npm's Yarn Classic. First-time installers may append shell initialization or user PATH entries; manager updates must not rewrite profiles.

Manager removal targets the verified installation owner (npm, Homebrew, Corepack, or winget), or only known standalone executable/initialization files. Retain Node/Python versions, projects, global package data, caches, and shell configuration. Revalidate reviewed files and versions before removal; a native command exiting successfully is insufficient if the installation remains. Preserve retained nvm Git metadata without treating a subsequent script reinstall as a Git-managed checkout. Installation and removal refresh only the affected provider and system disk usage, never project directories.

Global cache cleanup runs inside Envark and refreshes only affected cache measurements and system disk usage:

| Cache                        | Strategy                                                   | Boundary                                                                                                                                          |
| ---------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| pip                          | `pip cache purge` (or `python -m pip`)                     | Resolve the configured cache directory; pin and re-probe it before cleanup.                                                                       |
| Gradle caches/distributions  | Gradle 8+ native retention cleanup                         | Use an installed distribution and Java in a temporary empty project, offline; review both directories together and retain current/recent entries. |
| Cargo registry/Git downloads | Trash by default, or the reviewed permanent-delete setting | Acquire Cargo's mutate and download locks, revalidate contents, preserve installed tools/configuration, reject modified Git checkouts.            |
| Maven local repository       | Preserve                                                   | Local publications may be irreplaceable; do not label this as a safely disposable download cache.                                                 |

Native cleanup is not a promise to remove the entire displayed size. A Gradle run may retain all entries or create small metadata files; report the measured net change. Never substitute project-level `gradle clean` or `cargo clean` for global cache cleanup.

Never commit credentials, personal paths, local inventory snapshots, or downloaded toolchains. Use synthetic fixtures in tests and screenshots.

## Pull requests

Create a focused branch from `main`. Keep one logical change per PR and use an English Conventional Commit title. Explain the problem, resulting behavior, and how you verified it; include screenshots for interface changes.

Add meaningful regression coverage for behavioral fixes and state any platforms you could not verify. For installation or cleanup changes, explain ownership, path validation, and recovery behavior. Run the checks relevant to your changes and resolve CI failures before requesting a review.

Use `Fixes #123` when a PR resolves a GitHub issue. A reference such as `Fixes FEL-123` is appropriate only when it resolves that Linear issue and the integration is configured.

## Validation

```sh
pnpm format:check
pnpm check
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm desktop:build
```

Run `pnpm locales` after adding interface strings to update Traditional Chinese translations. GitHub Actions builds Windows x64, macOS Apple Silicon, macOS Intel, and Ubuntu x64; local checks cover your current platform.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) with an English imperative summary:

```text
type(scope): describe the change
```

Use a scope when it identifies a meaningful area, such as `scanner`, `providers`, `ui`, or `ci`. The shared `@commitlint/config-conventional` preset enforces the message format, allowed types, and a 100-character header limit. Start the summary with a lowercase verb and omit the final period.

```text
feat(providers): discover SDKMAN installations
fix(cleanup): reject artifacts changed after review
perf(scanner): reuse unchanged project roots
test(process): verify descendant cancellation
ci(build): package desktop installers
```

Each commit should express one logical change. Include the tests for that change, keep dependency manifests and lockfiles together, and keep commits buildable. Separate unrelated features, fixes, refactors, and formatting. Message linting cannot verify this; review the staged diff before committing.

Use a body to explain motivation, constraints, and non-obvious tradeoffs when they help a reviewer. Document breaking behavior with `!` or a `BREAKING CHANGE:` footer. Do not add a breaking-change marker for ordinary internal refactoring.

## Commit hooks

The `prepare` script installs Husky's repository-local hooks. Run it explicitly when first setting up an existing checkout or when installation skips lifecycle scripts. It is safe to run again.

The `commit-msg` hook runs the locally installed commitlint on the actual message file, including commits made through a Git client. Git must be able to find Node.js. Husky adds `node_modules/.bin` to the hook's path, so the hook does not require a global commitlint installation.

To inspect the last commit:

```sh
pnpm commitlint --last --verbose
```

Do not bypass a failed message check. Correct the message and commit again.

## CI and published history

CI checks every commit introduced by a push to `main`, every commit in a pull request, and the PR title used for squash merging. A manual workflow run checks the selected tip. Dependency update messages use the same convention through Dependabot's `build` prefix and dependency scope.

Dependabot generates version tables that exceed the body line limit. On its PRs, this one rule reports warnings; all other rules and the separate PR title check remain strict. Squash commit messages on `main` follow the normal rules.

Published history follows this policy. Routine checks validate incoming commits; a branch rewrite validates the replacement history in full. Rewriting public history requires explicit authorization, a recoverable backup, and coordination with existing pull requests. Use an explicit `--force-with-lease` expectation to avoid overwriting concurrent changes.

## Releases

Merge reviewed feature and fix PRs into `main` using Conventional Commits. The **Release** workflow maintains a release-please PR containing the next version and changelog. Review its changes and merge it when ready to ship. Features increase the minor version, fixes increase the patch version, and breaking changes increase the minor version while the application is below 1.0.

The release PR synchronizes `package.json`, the Cargo workspace version, both workspace entries in `Cargo.lock`, the Tauri configuration, and the release manifest. `pnpm check` rejects version drift. The Cargo lockfile JSONPath deliberately uses `name.value`: the pinned release-please TOML updater wraps scalar values. Validate these updates when upgrading the action.

The release manifest records the last released version. Do not set `release-as` permanently, which would override future version increments. Tagged builds require a populated manifest matching the application version.

After the release PR is merged, release-please creates a tag and draft release. The same workflow builds Windows x64, macOS Apple Silicon, macOS Intel, and Linux x64 from the tagged commit, verifies all five installers, uploads `SHA256SUMS`, and publishes the draft only after every build succeeds. Ordinary pushes cannot cancel a release in progress. Release automation is the sole owner of tags and releases; do not create a separate tag to start a build.

Releases also require the repository's `TAURI_SIGNING_PRIVATE_KEY` Actions secret. Its matching public key is embedded in the Tauri configuration. Keep a secure backup of the private key outside the repository; losing it prevents existing installations from trusting future updates. Never replace this key as part of routine builds. Updater signing is separate from operating-system code signing and notarization.

macOS bundles use an ad-hoc signing identity (`-`) to seal the complete application, including its resources. CI checks the application, the copy inside the DMG, and release updater archives with `codesign --verify --deep --strict` before uploading artifacts. This prevents distributing an app with a missing or invalid resource seal (FEL-23). Ad-hoc signing is free and does not provide Developer ID trust or Apple notarization; Gatekeeper can still require explicit user approval. Tauri must sign before producing the DMG and updater archive; do not modify the bundled app afterward.

The release pipeline verifies updater signatures for Windows installers, Linux AppImages, and separate macOS updater archives before publishing `latest.json`. Signature sidecars (`.sig`) are retained in internal build artifacts and embedded in `latest.json`, but are not uploaded as public Release assets or listed in `SHA256SUMS`. When resuming a draft created by older release tooling, the publisher removes only the known obsolete signature attachments after verifying the public uploads. Already published releases remain unchanged. Pull-request builds disable updater artifacts and require no signing secret. The first version containing the updater must be installed manually; earlier versions cannot gain an updater without being upgraded.

Enable **Settings → Actions → General → Allow GitHub Actions to create and approve pull requests**. The default `GITHUB_TOKEN` needs no additional secret. GitHub may require a maintainer to approve CI runs on its generated PR; use **Approve workflows to run** in the PR. An optional `RELEASE_TOKEN` with repository contents and pull-request access can trigger PR CI without this approval. The workflow never approves or merges its own PR.

If a platform build or upload fails, the release remains a draft. Re-run failed jobs, or run **Release** on `main` with the existing draft tag in the `tag` input. Recovery resolves the original tag's commit, revalidates every installer, and leaves already published releases unchanged. Release scripts run from the workflow revision so a tooling fix can resume an existing draft without changing its tag or application source. Signing and notarization certificates must be configured separately before distributing signed installers.
