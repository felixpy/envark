# Envark contributor instructions

- Keep source code, comments, identifiers, commit messages, and technical documentation in English.
- Follow Conventional Commits and the guidance in CONTRIBUTING.md. Keep each commit focused on one logical change with its relevant tests; separate unrelated fixes, refactors, and formatting.
- Keep provider integrations independent from the interface. A new provider is a probe or a known location with an explicit unavailable reason, not a new page.
- Perform filesystem traversal and process execution off the UI thread.
- Bound parallel work, support cancellation, and report partial failures without inventing successful results.
- Use argument arrays for subprocesses. Never interpolate user input into shell commands.
- Resolve installation ownership before changing an environment or tool.
- Plan destructive operations before execution. Revalidate canonical paths, preserve source and configuration, and reject symlink escapes.
- Use the platform trash for eligible project artifacts by default. Prefer each provider's supported cleanup command for shared caches.
- Never commit local environment snapshots, credentials, personal paths, or downloaded development toolchains.
- Do not commit temporary review screenshots, recordings, or validation reports anywhere in the repository, including `.github/screenshots`. Keep local evidence in ignored `.tools/` or a temporary directory; share it through PR attachments or CI artifacts and reference those links in the PR. Commit images only when they are intentional application assets, documentation assets, or test fixtures.
- Run formatting, type checks, meaningful tests, and native build checks before publishing changes.
