# Contributing to Envark

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

## Local checks

Use Node.js 24.15.0 or later in the Node 24 series. The test environment requires this minimum version; CI and Node type declarations also target Node 24.

```sh
pnpm install --frozen-lockfile
pnpm prepare
```

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

The first release is `0.1.0`, set by `initial-version`. Until its release PR is merged, the release manifest stays empty because no version has been released. The first release PR records `0.1.0` in the manifest; subsequent versions follow Conventional Commits. Do not set `release-as` permanently, which would override future version increments. Tagged builds require a populated manifest matching the application version.

After the release PR is merged, release-please creates a tag and draft release. The same workflow builds Windows x64, macOS Apple Silicon, macOS Intel, and Linux x64 from the tagged commit, verifies all five installers, uploads `SHA256SUMS`, and publishes the draft only after every build succeeds. Ordinary pushes cannot cancel a release in progress. Release automation is the sole owner of tags and releases; do not create a separate tag to start a build.

Enable **Settings → Actions → General → Allow GitHub Actions to create and approve pull requests**. The default `GITHUB_TOKEN` needs no additional secret. GitHub may require a maintainer to approve CI runs on its generated PR; use **Approve workflows to run** in the PR. An optional `RELEASE_TOKEN` with repository contents and pull-request access can trigger PR CI without this approval. The workflow never approves or merges its own PR.

If a platform build or upload fails, the release remains a draft. Re-run failed jobs, or run **Release** on `main` with the existing draft tag in the `tag` input. Recovery resolves the original tag's commit, revalidates every installer, and leaves already published releases unchanged. Release scripts run from the workflow revision so a tooling fix can resume an existing draft without changing its tag or application source. Signing and notarization certificates must be configured separately before distributing signed installers.
