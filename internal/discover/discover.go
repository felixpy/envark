// Package discover asks installed tools what they are. It does not install,
// update, or remove them.
package discover

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/managers"
	"github.com/felixpy/envark/internal/safe"
)

// Runner executes one binary. Arguments are never joined into a shell.
type Runner func(ctx context.Context, bin string, args ...string) (string, string, error)

// Env is the filesystem view discovery is allowed to use.
type Env struct {
	Home  string
	Path  string
	Extra []string
	Run   Runner
	Look  func(name string) (string, error)
}

// Probe is one runtime or an explicit failure to inspect an ecosystem.
type Probe struct {
	Ecosystem   string
	Name        string
	Version     string
	Manager     string
	Ownership   string
	Path        string
	Active      bool
	Detail      string
	Unavailable string
	Removal     *managers.Command
	Err         string
}

// Location is a known cache directory or an unavailable command.
type Location struct {
	ID         string
	Title      string
	Path       string
	Policy     classify.Location
	Command    *managers.Command
	Missing    bool
	Err        string
	Logical    uint64
	Allocated  uint64
	Measured   bool
	Incomplete bool
}

// Discover probes Node.js, Python, Java, Rust, and Go.
func Discover(ctx context.Context, env Env) []Probe {
	env = env.normalized()
	var out []Probe
	out = append(out, nodeProbes(ctx, env)...)
	out = append(out, pythonProbes(ctx, env)...)
	out = append(out, javaProbe(ctx, env))
	out = append(out, rustProbes(ctx, env)...)
	out = append(out, goProbe(ctx, env))
	return out
}

// Locations resolves known cache paths. Missing tools are reported, not
// treated as empty successes.
func Locations(ctx context.Context, env Env) []Location {
	env = env.normalized()
	var out []Location
	out = append(out, commandLocation(ctx, env, "npm-cache", "npm", []string{"config", "get", "cache"}, true))
	out = append(out, commandLocation(ctx, env, "pnpm-store", "pnpm", []string{"store", "path"}, true))
	out = append(out, commandLocation(ctx, env, "pip-cache", "python3", []string{"-m", "pip", "cache", "dir"}, true))
	out = append(out, goEnvLocation(ctx, env, "go-build-cache", "GOCACHE"))
	out = append(out, goEnvLocation(ctx, env, "go-mod-cache", "GOMODCACHE"))
	out = append(out, homeLocation(env, "cargo-registry-cache", filepath.Join(".cargo", "registry", "cache")))
	out = append(out, homeLocation(env, "cargo-git-db", filepath.Join(".cargo", "git", "db")))
	out = append(out, homeLocation(env, "cargo-registry-src", filepath.Join(".cargo", "registry", "src")))
	out = append(out, homeLocation(env, "cargo-git-checkouts", filepath.Join(".cargo", "git", "checkouts")))
	out = append(out, homeLocation(env, "gradle-caches", filepath.Join(".gradle", "caches")))
	out = append(out, homeLocation(env, "maven-repository", filepath.Join(".m2", "repository")))
	out = append(out, homeLocation(env, "playwright-cache", filepath.Join(".cache", "ms-playwright")))
	out = append(out, homeLocation(env, "puppeteer-cache", filepath.Join(".cache", "puppeteer")))
	out = append(out, homeLocation(env, "ollama-models", filepath.Join(".ollama", "models")))
	out = append(out, homeLocation(env, "uv-cache", filepath.Join(".cache", "uv")))
	out = append(out, homeLocation(env, "bun-cache", filepath.Join(".bun", "install", "cache")))
	return out
}

func (e Env) normalized() Env {
	if e.Run == nil {
		e.Run = defaultRun
	}
	if e.Look == nil {
		path := e.Path
		e.Look = func(name string) (string, error) { return Look(path, name) }
	}
	return e
}

func nodeProbes(ctx context.Context, env Env) []Probe {
	node, err := env.Look("node")
	if err != nil {
		return []Probe{{Ecosystem: "Node.js", Name: "node", Unavailable: "Node.js was not found on PATH.", Err: "not installed"}}
	}
	version, _, runErr := env.Run(ctx, node, "-v")
	version = firstLine(version)
	manager, ownership := classifyPath(node, env.Home)
	base := Probe{Ecosystem: "Node.js", Name: "node", Version: version, Manager: manager, Ownership: ownership, Path: node, Active: true}
	if runErr != nil {
		base.Err = runErr.Error()
		base.Unavailable = "The node binary did not report a version."
		return []Probe{base}
	}
	fnm, ferr := env.Look("fnm")
	if ferr != nil {
		base.Unavailable = removalReason(manager, true)
		return []Probe{base}
	}
	list, _, lerr := env.Run(ctx, fnm, "list")
	if lerr != nil {
		base.Detail = "fnm list failed: " + lerr.Error()
		base.Unavailable = "fnm is installed but its version list could not be read."
		return []Probe{base}
	}
	rows := parseVersionLines(list, true)
	if len(rows) == 0 {
		base.Detail = "fnm list did not contain a version Envark understands."
		base.Unavailable = "fnm's version list could not be parsed, so removal is unavailable."
		return []Probe{base}
	}
	var out []Probe
	for _, row := range rows {
		item := Probe{
			Ecosystem: "Node.js", Name: "node", Version: row.version, Manager: "fnm",
			Ownership: "manager", Path: node, Active: row.active, Detail: "Listed by fnm.",
		}
		if row.active {
			item.Unavailable = "This is the active or default Node.js. Envark will not remove it."
		} else if cmd, err := managers.FnmUninstall(fnm, row.version); err != nil {
			item.Unavailable = err.Error()
		} else {
			item.Removal = &cmd
		}
		out = append(out, item)
	}
	return out
}

func pythonProbes(ctx context.Context, env Env) []Probe {
	bin, err := env.Look("python3")
	if err != nil {
		bin, err = env.Look("python")
	}
	if err != nil {
		return []Probe{{Ecosystem: "Python", Name: "python", Unavailable: "Python was not found on PATH.", Err: "not installed"}}
	}
	version, _, runErr := env.Run(ctx, bin, "--version")
	manager, ownership := classifyPath(bin, env.Home)
	base := Probe{Ecosystem: "Python", Name: filepath.Base(bin), Version: firstLine(version), Manager: manager, Ownership: ownership, Path: bin, Active: true}
	if runErr != nil {
		base.Err = runErr.Error()
		base.Unavailable = "Python did not report a version."
		return []Probe{base}
	}
	uv, uerr := env.Look("uv")
	if uerr != nil {
		base.Unavailable = removalReason(manager, false)
		return []Probe{base}
	}
	list, _, lerr := env.Run(ctx, uv, "python", "list")
	if lerr != nil {
		base.Detail = "uv python list failed: " + lerr.Error()
		base.Unavailable = "uv is installed but its Python list could not be read."
		return []Probe{base}
	}
	rows := parseVersionLines(list, false)
	if len(rows) == 0 {
		base.Unavailable = "uv's Python list could not be parsed, so removal is unavailable."
		return []Probe{base}
	}
	var out []Probe
	for _, row := range rows {
		item := Probe{Ecosystem: "Python", Name: "python", Version: row.version, Manager: "uv", Ownership: "manager", Path: bin, Active: row.active, Detail: "Listed by uv."}
		if row.active {
			item.Unavailable = "This is the active or default Python. Envark will not remove it."
		} else if cmd, err := managers.UvPythonUninstall(uv, row.version); err != nil {
			item.Unavailable = err.Error()
		} else {
			item.Removal = &cmd
		}
		out = append(out, item)
	}
	return out
}

func javaProbe(ctx context.Context, env Env) Probe {
	bin, err := env.Look("java")
	if err != nil {
		return Probe{Ecosystem: "Java", Name: "java", Unavailable: "Java was not found on PATH.", Err: "not installed"}
	}
	_, stderr, runErr := env.Run(ctx, bin, "-version")
	version := quotedVersion(stderr)
	if version == "" {
		version = firstLine(stderr)
	}
	manager, ownership := classifyPath(bin, env.Home)
	probe := Probe{Ecosystem: "Java", Name: "java", Version: version, Manager: manager, Ownership: ownership, Path: bin, Active: true, Unavailable: "Java removal is not offered. Use the manager or package that installed it."}
	if runErr != nil && version == "" {
		probe.Err = runErr.Error()
	}
	return probe
}

func rustProbes(ctx context.Context, env Env) []Probe {
	bin, err := env.Look("rustc")
	if err != nil {
		return []Probe{{Ecosystem: "Rust", Name: "rustc", Unavailable: "Rust was not found on PATH.", Err: "not installed"}}
	}
	version, _, runErr := env.Run(ctx, bin, "--version")
	manager, ownership := classifyPath(bin, env.Home)
	base := Probe{Ecosystem: "Rust", Name: "rustc", Version: firstLine(version), Manager: manager, Ownership: ownership, Path: bin, Active: true}
	if runErr != nil {
		base.Err = runErr.Error()
		base.Unavailable = "rustc did not report a version."
		return []Probe{base}
	}
	rustup, rerr := env.Look("rustup")
	if rerr != nil {
		base.Unavailable = "rustup was not found, so toolchain removal is unavailable."
		return []Probe{base}
	}
	list, _, lerr := env.Run(ctx, rustup, "toolchain", "list")
	if lerr != nil {
		base.Unavailable = "rustup toolchain list failed, so removal is unavailable."
		base.Detail = lerr.Error()
		return []Probe{base}
	}
	var out []Probe
	for _, line := range strings.Split(list, "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		name := fields[0]
		if strings.ContainsAny(name, `/\`) {
			continue
		}
		active := strings.Contains(line, "default") || strings.Contains(line, "active")
		item := Probe{Ecosystem: "Rust", Name: name, Version: name, Manager: "rustup", Ownership: "manager", Path: bin, Active: active, Detail: "Listed by rustup."}
		if active {
			item.Unavailable = "This Rust toolchain is active or default. Envark will not remove it."
		} else if cmd, err := managers.RustupUninstall(rustup, name); err != nil {
			item.Unavailable = err.Error()
		} else {
			item.Removal = &cmd
		}
		out = append(out, item)
	}
	if len(out) == 0 {
		base.Unavailable = "rustup returned no toolchain Envark understands."
		return []Probe{base}
	}
	return out
}

func goProbe(ctx context.Context, env Env) Probe {
	bin, err := env.Look("go")
	if err != nil {
		return Probe{Ecosystem: "Go", Name: "go", Unavailable: "Go was not found on PATH.", Err: "not installed"}
	}
	raw, _, runErr := env.Run(ctx, bin, "version")
	version := goVersion(firstLine(raw))
	root, _, _ := env.Run(ctx, bin, "env", "GOROOT")
	manager, ownership := classifyPath(firstLine(root), env.Home)
	if manager == "unknown" {
		manager, ownership = classifyPath(bin, env.Home)
	}
	probe := Probe{
		Ecosystem: "Go", Name: "go", Version: version, Manager: manager, Ownership: ownership,
		Path: firstLine(root), Active: true,
		Unavailable: "Go removal is not offered. Remove it with the installer that placed GOROOT.",
		Detail:      bin,
	}
	if probe.Path == "" {
		probe.Path = bin
	}
	if runErr != nil {
		probe.Err = runErr.Error()
	}
	return probe
}

func commandLocation(ctx context.Context, env Env, id, binName string, args []string, wantPath bool) Location {
	policy, _ := classify.KnownLocation(id)
	loc := Location{ID: id, Title: policy.Title, Policy: policy}
	bin, err := env.Look(binName)
	if err != nil && binName == "python3" {
		bin, err = env.Look("python")
	}
	if err != nil {
		loc.Err = binName + " was not found. Envark will not guess a cache directory."
		loc.Policy.Eligible = false
		return loc
	}
	stdout, stderr, runErr := env.Run(ctx, bin, args...)
	if runErr != nil {
		loc.Err = strings.TrimSpace(firstLine(stderr + stdout + " " + runErr.Error()))
		loc.Policy.Eligible = false
		return loc
	}
	if wantPath {
		path := firstLine(stdout)
		if !filepath.IsAbs(path) || strings.ContainsAny(path, "\r\n") {
			loc.Err = "the tool did not return an absolute path"
			loc.Policy.Eligible = false
			return loc
		}
		loc.Path = filepath.Clean(path)
		if _, err := os.Lstat(loc.Path); err != nil {
			loc.Missing = true
			loc.Policy.Eligible = false
		}
	}
	loc.Command = commandFor(id, bin)
	if loc.Command == nil && policy.Method == "command" {
		loc.Policy.Eligible = false
		if loc.Err == "" {
			loc.Err = "no allowlisted command for this binary"
		}
	}
	if policy.Method != "command" {
		loc.Command = nil
	}
	return loc
}

func goEnvLocation(ctx context.Context, env Env, id, key string) Location {
	policy, _ := classify.KnownLocation(id)
	loc := Location{ID: id, Title: policy.Title, Policy: policy}
	bin, err := env.Look("go")
	if err != nil {
		loc.Err = "go was not found. Envark will not guess a cache directory."
		loc.Policy.Eligible = false
		return loc
	}
	stdout, stderr, runErr := env.Run(ctx, bin, "env", key)
	if runErr != nil {
		loc.Err = strings.TrimSpace(stderr + runErr.Error())
		loc.Policy.Eligible = false
		return loc
	}
	path := firstLine(stdout)
	if !filepath.IsAbs(path) {
		loc.Err = "go env did not return an absolute path"
		loc.Policy.Eligible = false
		return loc
	}
	loc.Path = filepath.Clean(path)
	if _, err := os.Lstat(loc.Path); err != nil {
		loc.Missing = true
		loc.Policy.Eligible = false
	}
	if id == "go-build-cache" {
		if cmd, err := managers.GoCleanCache(bin); err == nil {
			loc.Command = &cmd
		}
	}
	return loc
}

func homeLocation(env Env, id, rel string) Location {
	policy, _ := classify.KnownLocation(id)
	loc := Location{ID: id, Title: policy.Title, Policy: policy}
	if env.Home == "" {
		loc.Err = "home directory is unknown"
		loc.Policy.Eligible = false
		return loc
	}
	path := filepath.Join(env.Home, rel)
	loc.Path = path
	info, err := os.Lstat(path)
	if err != nil {
		loc.Missing = true
		loc.Policy.Eligible = false
		return loc
	}
	if info.Mode()&os.ModeSymlink != 0 {
		loc.Err = "the cache path is a symlink, so it is not a cleanup target"
		loc.Policy.Eligible = false
	}
	return loc
}

func commandFor(id, bin string) *managers.Command {
	var (
		cmd managers.Command
		err error
	)
	switch id {
	case "npm-cache":
		cmd, err = managers.NpmCacheClean(bin)
	case "pnpm-store":
		cmd, err = managers.PnpmStorePrune(bin)
	case "pip-cache":
		cmd, err = managers.PipCachePurge(bin)
	default:
		return nil
	}
	if err != nil {
		return nil
	}
	return &cmd
}

type versionRow struct {
	version string
	active  bool
}

var (
	nodeVersionPattern = regexp.MustCompile(`v?\d+\.\d+\.\d+(?:[-.][A-Za-z0-9][A-Za-z0-9.]*)?`)
	pyVersionPattern   = regexp.MustCompile(`\d+\.\d+(?:\.\d+)?`)
)

func parseVersionLines(text string, node bool) []versionRow {
	re := pyVersionPattern
	if node {
		re = nodeVersionPattern
	}
	var out []versionRow
	seen := map[string]bool{}
	for _, line := range strings.Split(text, "\n") {
		active := strings.Contains(line, "default") || strings.Contains(line, "active") || strings.HasPrefix(strings.TrimSpace(line), "*")
		for _, field := range strings.Fields(line) {
			field = strings.Trim(field, "*,()")
			found := re.FindString(field)
			if found != "" && !seen[found] {
				seen[found] = true
				out = append(out, versionRow{version: found, active: active})
				break
			}
		}
	}
	return out
}

func classifyPath(path, home string) (manager, ownership string) {
	p := strings.ToLower(filepath.ToSlash(path))
	switch {
	case strings.Contains(p, "/fnm/"):
		return "fnm", "manager"
	case strings.Contains(p, "/.nvm/") || strings.Contains(p, "/nvm/"):
		return "nvm", "manager"
	case strings.Contains(p, "/volta/"):
		return "volta", "manager"
	case strings.Contains(p, "/mise/"):
		return "mise", "manager"
	case strings.Contains(p, "/.sdkman/"):
		return "sdkman", "manager"
	case strings.Contains(p, "/.rustup/") || strings.Contains(p, "/rustup/"):
		return "rustup", "manager"
	case strings.Contains(p, "/uv/") || strings.Contains(p, "/.local/share/uv/"):
		return "uv", "manager"
	case strings.Contains(p, "/homebrew/") || strings.Contains(p, "/cellar/"):
		return "homebrew", "manager"
	case strings.Contains(p, "/.asdf/"):
		return "asdf", "manager"
	}
	if isSystemPath(p) {
		return "system", "system"
	}
	if home != "" {
		if h, err := safe.Absolute(home); err == nil && safe.Within(h, path) && path != h {
			return "unknown", "user"
		}
	}
	return "unknown", "unknown"
}

func isSystemPath(slash string) bool {
	switch runtime.GOOS {
	case "windows":
		return strings.HasPrefix(slash, "c:/windows") || strings.HasPrefix(slash, "c:/program files")
	default:
		return strings.HasPrefix(slash, "/usr/") || strings.HasPrefix(slash, "/bin/") || strings.HasPrefix(slash, "/system/")
	}
}

func removalReason(manager string, node bool) string {
	switch manager {
	case "system":
		return "This installation is owned by the system. Envark will not change it."
	case "nvm":
		return "nvm is a shell function. Envark does not run nvm uninstall."
	case "mise", "asdf", "volta", "homebrew", "sdkman":
		return "This manager keeps its own uninstall. Envark does not remove its runtimes in this version."
	case "unknown":
		return "Envark cannot tell which manager owns this installation, so removal is unavailable."
	default:
		if node {
			return "Removal is unavailable for this Node.js installation."
		}
		return "Removal is unavailable for this installation."
	}
}

func goVersion(line string) string {
	if v := regexp.MustCompile(`\bgo\d+\.\d+(?:\.\d+)?\b`).FindString(line); v != "" {
		return v
	}
	return line
}

func firstLine(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.IndexAny(s, "\r\n"); i >= 0 {
		s = s[:i]
	}
	return strings.TrimSpace(s)
}

func quotedVersion(s string) string {
	start := strings.Index(s, `"`)
	if start < 0 {
		return ""
	}
	rest := s[start+1:]
	end := strings.Index(rest, `"`)
	if end < 0 {
		return ""
	}
	return rest[:end]
}

// Look finds name on pathEnv. It rejects names that are paths.
func Look(pathEnv, name string) (string, error) {
	if name == "" || strings.ContainsAny(name, `/\`) {
		return "", errors.New("refusing to look up a path")
	}
	exts := []string{""}
	if runtime.GOOS == "windows" {
		exts = []string{"", ".exe", ".cmd"}
	}
	for _, dir := range filepath.SplitList(pathEnv) {
		if dir == "" {
			continue
		}
		for _, ext := range exts {
			candidate := filepath.Join(dir, name+ext)
			info, err := os.Stat(candidate)
			if err != nil || info.IsDir() {
				continue
			}
			if info.Mode()&0o111 == 0 && runtime.GOOS != "windows" {
				continue
			}
			return candidate, nil
		}
	}
	return "", fmt.Errorf("%s was not found", name)
}

func defaultRun(ctx context.Context, bin string, args ...string) (string, string, error) {
	if !filepath.IsAbs(bin) {
		return "", "", errors.New("binary must be absolute")
	}
	cmd := exec.CommandContext(ctx, bin, args...)
	cmd.Stdin = nil
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}
