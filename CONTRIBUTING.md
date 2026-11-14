# Contributing to Envark

Envark is a Go desktop app on [MyGo](https://github.com/egoist/mygo) native UI. Source, comments, identifiers, commit messages, and technical docs stay in English.

## Setup

Install Go 1.27.4. On Linux, install GTK 3. macOS and Windows native UI do not need a webview; this repository has not been run on those systems, so do not treat a cross-compile as proof.

```sh
go test ./...
go vet ./...
gofmt -l .
go build -o envark .
```

`gofmt -l .` should print nothing. Format with `gofmt -w .`.

## Product boundary

Keep the three workflows complete: Space, Environments, and Review. A new provider should show up as a probe or a known location with an explicit unavailable reason, not as a new page and not as a button that pretends an action succeeded.

Filesystem walks and process execution stay off the UI thread. Bound the work, honor cancellation, and report a partial failure as a failure. Do not invent a successful result.

Subprocesses use argument arrays. Never interpolate user input into a shell command. Manager actions are constructed by the allowlist in `internal/managers` and revalidated before `exec.Command`. A missing binary is an error.

## Cleanup

Scan and discovery are read-only. A destructive operation is planned, then revalidated: the path must still resolve inside the approved root, must not be a symlink, must still be the same directory identity, and must still measure the same logical size. A mismatch fails that item and the rest continue.

Refuse the scan root, the home directory, system locations, protected projects, and any target that contains a protected project. Reject symlink escapes below the scan root.

Project artifacts go to the platform trash. Shared caches use the manager's own command. Permanent deletion is off unless Settings enables it, and it still requires a second confirmation. Do not fall back from a cross-device trash failure to an irreversible delete.

Do not run cleanup, install, upgrade, or uninstall against a real home directory, cache, project, runtime, Docker data, or the host system. Tests and manual checks use a temporary directory. `ENVARK_DATA_DIR`, `ENVARK_HOME`, `ENVARK_PATH`, and `ENVARK_TRASH_DIR` exist for that.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/). One logical change per commit, with the tests that belong to it. Subject line, imperative, no trailing period, at most 100 characters:

```
feat(scan): count hard links once per scan
fix(cleanup): fail a stale target without deleting its sibling
```

Do not commit credentials, personal paths, environment snapshots, downloaded toolchains, or review screenshots. Screenshots for a pull request go in the pull request body as attachments, not in the repository.

## Pull requests

Describe the behavior change, the commands you ran, and the platforms you did not run. For a cleanup or process change, say how ownership, path checks, and recovery work.
