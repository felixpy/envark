// Package classify labels directories Envark is willing to review.
package classify

import "strings"

// Result is the cleanup policy for one entry.
type Result struct {
	Kind     string // source, file, link, other-volume, project-artifact, manager-cache, browser-cache, model-store
	Risk     string // none, low, caution
	Eligible bool
	Method   string // none, trash, command
	Title    string
	Reason   string
}

// File is an ordinary file. Individual files are not cleanup targets.
func File() Result {
	return Result{
		Kind:   "file",
		Risk:   "none",
		Method: "none",
		Title:  "File",
		Reason: "Individual files are shown so large ones can be found. Envark does not delete them from Space.",
	}
}

// Link is a symlink. Links are never cleanup targets.
func Link() Result {
	return Result{
		Kind:   "link",
		Risk:   "none",
		Method: "none",
		Title:  "Link",
		Reason: "Symlinks are not followed and are not removed.",
	}
}

// OtherVolume is a mount that was not entered.
func OtherVolume() Result {
	return Result{
		Kind:   "other-volume",
		Risk:   "none",
		Method: "none",
		Title:  "Other volume",
		Reason: "This directory is on another device. Its size was not measured and it was not entered.",
	}
}

// Source is a directory with no cleanup policy.
func Source() Result {
	return Result{
		Kind:   "source",
		Risk:   "none",
		Method: "none",
		Title:  "Directory",
		Reason: "Not a recognized build output or cache.",
	}
}

// Directory classifies a real directory from its name and nearby evidence.
// parentNames are the entries beside it. ownNames are its children.
func Directory(name string, parentNames, ownNames []string) Result {
	parent := set(parentNames)
	own := set(ownNames)
	switch name {
	case "node_modules":
		if parent["package.json"] {
			return artifact("low", "Node dependencies", "Dependencies beside package.json. Trash can restore them; the package manager can recreate them.")
		}
	case "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache":
		return artifact("low", "Python cache", "Generated Python cache. Trash can restore it.")
	case "target":
		if parent["Cargo.toml"] {
			return artifact("low", "Cargo output", "Build output beside Cargo.toml. Trash can restore it; cargo build can recreate it.")
		}
	case ".next", ".nuxt", ".turbo", ".parcel-cache":
		if parent["package.json"] {
			return artifact("low", "Framework cache", "Framework build cache beside package.json. Trash can restore it.")
		}
	case "dist", "build", "coverage":
		if parent["package.json"] {
			return artifact("caution", "Build output", "Output beside package.json. It may be something you publish. Trash can restore it.")
		}
	case ".venv", "venv":
		if own["pyvenv.cfg"] {
			return artifact("caution", "Virtualenv", "Python virtual environment. It may be the one a project is using. Trash can restore it.")
		}
	case ".gradle":
		if parent["build.gradle"] || parent["build.gradle.kts"] {
			return artifact("low", "Gradle project cache", "Project Gradle cache. Trash can restore it.")
		}
	case "vendor":
		if own["modules.txt"] {
			return artifact("caution", "Vendored modules", "Go vendor directory. Trash can restore it.")
		}
	}
	return Source()
}

func artifact(risk, title, reason string) Result {
	return Result{
		Kind:     "project-artifact",
		Risk:     risk,
		Eligible: true,
		Method:   "trash",
		Title:    title,
		Reason:   reason,
	}
}

func set(names []string) map[string]bool {
	out := make(map[string]bool, len(names))
	for _, name := range names {
		out[name] = true
	}
	return out
}

// Has reports whether names contain name.
func Has(names []string, name string) bool {
	for _, item := range names {
		if item == name {
			return true
		}
	}
	return false
}

// Location is the policy for a known cache path, independent of a project scan.
type Location struct {
	ID       string
	Title    string
	Kind     string
	Risk     string
	Eligible bool
	Method   string // trash or command or none
	Reason   string
}

// KnownLocation returns the policy for a catalog id.
func KnownLocation(id string) (Location, bool) {
	switch id {
	case "npm-cache":
		return Location{id, "npm cache", "manager-cache", "caution", true, "command", "npm cache clean --force empties npm's cache. It is not restorable from Trash, and Envark does not claim a byte count."}, true
	case "pnpm-store":
		return Location{id, "pnpm store", "manager-cache", "caution", true, "command", "pnpm store prune removes unreferenced packages. It is not restorable from Trash."}, true
	case "pip-cache":
		return Location{id, "pip cache", "manager-cache", "caution", true, "command", "python -m pip cache purge deletes pip's cache. It is not restorable from Trash."}, true
	case "go-build-cache":
		return Location{id, "Go build cache", "manager-cache", "caution", true, "command", "go clean -cache clears the build cache. The module cache is left alone."}, true
	case "go-mod-cache":
		return Location{id, "Go module cache", "manager-cache", "caution", false, "none", "The module cache can be the only copy of a module. Envark does not remove it."}, true
	case "cargo-registry-cache":
		return Location{id, "Cargo registry cache", "manager-cache", "caution", true, "trash", "Downloaded crate files. Trash can restore them. Installed binaries and git checkouts are not included."}, true
	case "cargo-git-db":
		return Location{id, "Cargo git database", "manager-cache", "caution", true, "trash", "Cargo's git object database. Checkouts are not included, because they can contain local changes."}, true
	case "cargo-registry-src":
		return Location{id, "Cargo registry sources", "manager-cache", "caution", false, "none", "Extracted crate sources are shown only. Envark does not remove them in this version."}, true
	case "cargo-git-checkouts":
		return Location{id, "Cargo git checkouts", "manager-cache", "caution", false, "none", "Checkouts can contain uncommitted changes. Envark does not remove them."}, true
	case "gradle-caches":
		return Location{id, "Gradle caches", "manager-cache", "caution", false, "none", "Gradle retention cleanup is not in this version. The directory is measured only."}, true
	case "maven-repository":
		return Location{id, "Maven repository", "manager-cache", "caution", false, "none", "A local Maven repository can hold unpublished artifacts. Envark does not remove it."}, true
	case "playwright-cache":
		return Location{id, "Playwright browsers", "browser-cache", "caution", true, "trash", "The Playwright browser cache directory only. Browser profiles are not a target."}, true
	case "puppeteer-cache":
		return Location{id, "Puppeteer browsers", "browser-cache", "caution", true, "trash", "The Puppeteer browser cache directory only. Browser profiles are not a target."}, true
	case "ollama-models":
		return Location{id, "Ollama models", "model-store", "caution", false, "none", "Model deletion is not in this version. The directory is measured only."}, true
	case "uv-cache":
		return Location{id, "uv cache", "manager-cache", "caution", false, "none", "uv's cache is measured only. Envark does not guess a safe purge command in this version."}, true
	case "bun-cache":
		return Location{id, "Bun install cache", "manager-cache", "caution", false, "none", "Bun's install cache is measured only in this version."}, true
	default:
		return Location{}, false
	}
}

// TitleRisk is a short risk label.
func TitleRisk(risk string) string {
	switch risk {
	case "low":
		return "Low"
	case "caution":
		return "Caution"
	default:
		return "None"
	}
}

// JoinReason joins sentences without duplicating punctuation.
func JoinReason(parts ...string) string {
	return strings.Join(parts, " ")
}
