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

Published history follows this policy. Routine checks validate incoming commits; a branch rewrite validates the replacement history in full. Rewriting public history requires explicit authorization, a recoverable backup, and coordination with existing pull requests. Use an explicit `--force-with-lease` expectation to avoid overwriting concurrent changes.
