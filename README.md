# Envark

**A home for your dev tools.** See what is installed, understand what takes up space, and keep your development machine tidy.

[![Release](https://img.shields.io/github/v/release/felixpy/envark)](https://github.com/felixpy/envark/releases/latest)
[![Build](https://github.com/felixpy/envark/actions/workflows/build.yml/badge.svg?branch=main&event=push)](https://github.com/felixpy/envark/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Envark brings runtimes, global tools, project artifacts, and caches into one desktop application for **Windows, macOS, and Ubuntu**. Spend less time piecing together CLI output and more time building.

[Download](https://github.com/felixpy/envark/releases/latest) · [Report a bug](https://github.com/felixpy/envark/issues/new?template=bug_report.yml) · [Suggest a feature](https://github.com/felixpy/envark/issues/new?template=feature_request.yml) · [Contribute](CONTRIBUTING.md)

## What you can do

- **🧰 Get your tools in order.** Inspect Node.js, Python, Java, Rust, and Go runtimes alongside their global tools. Install supported managers from their official sources, then manage runtime versions and tools in the app.
- **🌳 Make room between projects.** Scan Git repositories and their linked worktrees, sort by activity or artifact size, and find generated dependencies and build outputs left behind by inactive workspaces.
- **🧹 Understand your caches.** Review global caches and their reported sizes before using each tool's supported cleanup command. Shared caches stay under their owner's control.
- **🤖 See the bigger downloads.** Install and update Ollama, start its local service, and manage downloaded models. Inspect and clean existing Puppeteer / Playwright browser caches.
- **🛡️ Review before you act.** Preview cleanup targets, protect projects, and send eligible artifacts to the system trash by default. Edit tool configuration with backups and revisit operation results in local activity history.

Available actions depend on the detected tool manager and installation ownership. Online package checks are opt-in; unknown versions and sizes are shown explicitly.

## Download

Get installers and release notes from the [latest release](https://github.com/felixpy/envark/releases/latest).

| Platform | Architecture  | Package               |
| -------- | ------------- | --------------------- |
| Windows  | x64           | `.exe` installer      |
| macOS    | Apple Silicon | `.dmg`                |
| macOS    | Intel         | `.dmg`                |
| Ubuntu   | x64           | `.deb` or `.AppImage` |

Envark is under active development. Current installers are unsigned; every published release includes `SHA256SUMS` to verify downloads.

## How space is measured

Envark reports the logical size of discovered files. Shared storage, hard links, and files still referenced by other projects can make the space actually reclaimed smaller than the displayed size. Cleanup previews show the target and available action before execution.

Settings, inventory, activity, and configuration backups are stored locally on your computer.

## Development

Use an existing Rust toolchain **1.99 or newer**, **Node.js 24.15 or newer in the Node 24 series**, and **pnpm 11.19.0**. Install the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
pnpm install --frozen-lockfile
pnpm prepare
pnpm desktop
```

For a frontend-only preview, use `pnpm dev`. Native inspection and operations require the desktop application.

Built with Tauri, Rust, React, and TypeScript. The Rust core owns discovery and operations; the interface consumes provider capabilities so new ecosystems can fit the existing pages.

See [CONTRIBUTING.md](CONTRIBUTING.md) for architecture, validation, commit conventions, and the release process.

## Join in

Bug reports, ideas, translations, and focused pull requests are welcome. Start with an [issue](https://github.com/felixpy/envark/issues) or a [good first issue](https://github.com/felixpy/envark/issues?q=is%3Aissue%20is%3Aopen%20label%3A%22good%20first%20issue%22).

Read the [contribution guide](CONTRIBUTING.md) and [support guide](SUPPORT.md). Report vulnerabilities privately using the [security policy](SECURITY.md).

Envark is available under the [MIT License](LICENSE).
