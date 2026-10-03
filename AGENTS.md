# Envark contributor instructions

- Keep source code, comments, identifiers, commit messages, and technical documentation in English.
- Preserve the Figma Make design while replacing prototype data and simulated actions with real desktop services.
- Keep provider integrations independent from the interface. New providers should declare capabilities rather than require new page structures.
- Perform filesystem traversal and process execution in the Rust core, away from the UI thread.
- Bound parallel work, support cancellation, and report partial failures without inventing successful results.
- Use argument arrays for subprocesses. Never interpolate user input into shell commands.
- Resolve installation ownership before changing an environment or tool.
- Plan destructive operations before execution. Revalidate canonical paths, preserve source and configuration, and reject symlink escapes.
- Use the platform trash for eligible project artifacts by default. Prefer each provider's supported cleanup command for shared caches.
- Never commit local environment snapshots, credentials, personal paths, or downloaded development toolchains.
- Run formatting, type checks, meaningful tests, and native build checks before publishing changes.
