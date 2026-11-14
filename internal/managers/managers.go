// Package managers builds the only process arguments Envark will run.
package managers

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/felixpy/envark/internal/safe"
)

// Command is one allowlisted invocation. Validate rejects anything the
// constructors did not produce, including a later edit of Args.
type Command struct {
	ID           string
	Bin          string
	Args         []string
	Dir          string
	Effect       string
	Hint         string
	Use          string
	Irreversible bool
	// Revision is the catalog generation that produced this plan. A later
	// refresh makes it stale.
	Revision uint64
}

const (
	HintInstall = "The install runs in a separate process. It does not change PATH in the terminal that is already open. Open a new terminal, or run the manager's use command there."
	HintDefault = "This changes the version new shells start with. It does not change the terminal that is already open. In that terminal, run the use command, or open a new one."
	HintPin     = "This writes .python-version in the chosen directory. It does not change PATH in the terminal that is already open."
)

var (
	toolchainName = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._+-]{0,80}$`)
	nodeVersion   = regexp.MustCompile(`^v?\d+\.\d+\.\d+(?:[-.][A-Za-z0-9][A-Za-z0-9.]*)?$`)
	pyVersion     = regexp.MustCompile(`^\d+\.\d+(?:\.\d+)?$`)
)

// RustupUninstall removes one non-default toolchain by name.
func RustupUninstall(bin, toolchain string) (Command, error) {
	if err := executable(bin, "rustup"); err != nil {
		return Command{}, err
	}
	if !toolchainName.MatchString(toolchain) || toolchain == "stable" {
		return Command{}, fmt.Errorf("refusing rustup toolchain %q", toolchain)
	}
	return Command{
		ID:           "rustup-uninstall",
		Bin:          bin,
		Args:         []string{"toolchain", "uninstall", toolchain},
		Effect:       "rustup deletes that toolchain. The stable toolchain name is refused here. This is not undone from Trash.",
		Irreversible: true,
	}, nil
}

// FnmUninstall removes one Node.js version fnm installed.
func FnmUninstall(bin, version string) (Command, error) {
	if err := executable(bin, "fnm"); err != nil {
		return Command{}, err
	}
	if !nodeVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing fnm version %q", version)
	}
	return Command{
		ID:           "fnm-uninstall",
		Bin:          bin,
		Args:         []string{"uninstall", version},
		Effect:       "fnm uninstalls that Node.js version. This is not undone from Trash.",
		Irreversible: true,
	}, nil
}

// FnmInstall downloads one Node.js version. It does not switch the open shell.
func FnmInstall(bin, version string) (Command, error) {
	if err := executable(bin, "fnm"); err != nil {
		return Command{}, err
	}
	if !nodeVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing fnm version %q", version)
	}
	return Command{
		ID:           "fnm-install",
		Bin:          bin,
		Args:         []string{"install", version},
		Effect:       "fnm downloads and installs that Node.js version. Trash cannot undo the download.",
		Hint:         HintInstall,
		Use:          "fnm use " + version,
		Irreversible: true,
	}, nil
}

// FnmDefault sets fnm's default for new shells. It does not change the open shell.
func FnmDefault(bin, version string) (Command, error) {
	if err := executable(bin, "fnm"); err != nil {
		return Command{}, err
	}
	if !nodeVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing fnm version %q", version)
	}
	return Command{
		ID:           "fnm-default",
		Bin:          bin,
		Args:         []string{"default", version},
		Effect:       "fnm records that version as the default for new shells.",
		Hint:         HintDefault,
		Use:          "fnm use " + version,
		Irreversible: true,
	}, nil
}

// UvPythonUninstall removes one Python version uv installed.
func UvPythonUninstall(bin, version string) (Command, error) {
	if err := executable(bin, "uv"); err != nil {
		return Command{}, err
	}
	if !pyVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing uv python version %q", version)
	}
	return Command{
		ID:           "uv-python-uninstall",
		Bin:          bin,
		Args:         []string{"python", "uninstall", version},
		Effect:       "uv removes that Python version. This is not undone from Trash.",
		Irreversible: true,
	}, nil
}

// UvPythonInstall downloads one Python version. It does not switch the open shell.
func UvPythonInstall(bin, version string) (Command, error) {
	if err := executable(bin, "uv"); err != nil {
		return Command{}, err
	}
	if !pyVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing uv python version %q", version)
	}
	return Command{
		ID:           "uv-python-install",
		Bin:          bin,
		Args:         []string{"python", "install", version},
		Effect:       "uv downloads and installs that Python version. Trash cannot undo the download.",
		Hint:         HintInstall,
		Irreversible: true,
	}, nil
}

// UvPythonPin writes .python-version in dir. dir must be a real directory.
func UvPythonPin(bin, version, dir string) (Command, error) {
	if err := executable(bin, "uv"); err != nil {
		return Command{}, err
	}
	if !pyVersion.MatchString(version) {
		return Command{}, fmt.Errorf("refusing uv python version %q", version)
	}
	if err := projectDir(dir); err != nil {
		return Command{}, err
	}
	return Command{
		ID:           "uv-python-pin",
		Bin:          bin,
		Args:         []string{"python", "pin", version},
		Dir:          dir,
		Effect:       "uv writes .python-version in the chosen directory. The file is the project pin.",
		Hint:         HintPin,
		Irreversible: true,
	}, nil
}

// NpmCacheClean is npm's own cache purge.
func NpmCacheClean(bin string) (Command, error) {
	if err := executable(bin, "npm"); err != nil {
		return Command{}, err
	}
	return Command{
		ID:           "npm-cache-clean",
		Bin:          bin,
		Args:         []string{"cache", "clean", "--force"},
		Effect:       "npm empties its cache. Envark does not delete a directory itself and does not report a byte count.",
		Irreversible: true,
	}, nil
}

// PipCachePurge runs pip's purge through an explicit Python.
func PipCachePurge(python string) (Command, error) {
	if err := executable(python, "python", "python3"); err != nil {
		return Command{}, err
	}
	return Command{
		ID:           "pip-cache-purge",
		Bin:          python,
		Args:         []string{"-m", "pip", "cache", "purge"},
		Effect:       "pip deletes its download cache. Envark does not report a byte count.",
		Irreversible: true,
	}, nil
}

// GoCleanCache clears the build cache and not the module cache.
func GoCleanCache(bin string) (Command, error) {
	if err := executable(bin, "go"); err != nil {
		return Command{}, err
	}
	return Command{
		ID:           "go-clean-cache",
		Bin:          bin,
		Args:         []string{"clean", "-cache"},
		Effect:       "go clean -cache removes build cache objects. The module cache is not part of this command.",
		Irreversible: true,
	}, nil
}

// PnpmStorePrune asks pnpm to drop unreferenced store files.
func PnpmStorePrune(bin string) (Command, error) {
	if err := executable(bin, "pnpm"); err != nil {
		return Command{}, err
	}
	return Command{
		ID:           "pnpm-store-prune",
		Bin:          bin,
		Args:         []string{"store", "prune"},
		Effect:       "pnpm removes unreferenced packages from its store. Envark does not report a byte count.",
		Irreversible: true,
	}, nil
}

// Validate checks the binary and the exact argument vector.
func (c Command) Validate() error {
	base := strings.TrimSuffix(strings.ToLower(filepath.Base(c.Bin)), ".exe")
	base = strings.TrimSuffix(base, ".cmd")
	switch c.ID {
	case "rustup-uninstall":
		if base != "rustup" || len(c.Args) != 3 || c.Args[0] != "toolchain" || c.Args[1] != "uninstall" || !toolchainName.MatchString(c.Args[2]) || c.Args[2] == "stable" {
			return errors.New("rustup command does not match the allowlist")
		}
	case "fnm-uninstall":
		if base != "fnm" || len(c.Args) != 2 || c.Args[0] != "uninstall" || !nodeVersion.MatchString(c.Args[1]) {
			return errors.New("fnm command does not match the allowlist")
		}
	case "uv-python-uninstall":
		if base != "uv" || len(c.Args) != 3 || c.Args[0] != "python" || c.Args[1] != "uninstall" || !pyVersion.MatchString(c.Args[2]) {
			return errors.New("uv command does not match the allowlist")
		}
	case "fnm-install":
		if base != "fnm" || len(c.Args) != 2 || c.Args[0] != "install" || !nodeVersion.MatchString(c.Args[1]) {
			return errors.New("fnm command does not match the allowlist")
		}
	case "fnm-default":
		if base != "fnm" || len(c.Args) != 2 || c.Args[0] != "default" || !nodeVersion.MatchString(c.Args[1]) {
			return errors.New("fnm command does not match the allowlist")
		}
	case "uv-python-install":
		if base != "uv" || len(c.Args) != 3 || c.Args[0] != "python" || c.Args[1] != "install" || !pyVersion.MatchString(c.Args[2]) {
			return errors.New("uv command does not match the allowlist")
		}
	case "uv-python-pin":
		if base != "uv" || len(c.Args) != 3 || c.Args[0] != "python" || c.Args[1] != "pin" || !pyVersion.MatchString(c.Args[2]) {
			return errors.New("uv command does not match the allowlist")
		}
		if err := projectDir(c.Dir); err != nil {
			return err
		}
	case "npm-cache-clean":
		if base != "npm" || !same(c.Args, []string{"cache", "clean", "--force"}) {
			return errors.New("npm command does not match the allowlist")
		}
	case "pip-cache-purge":
		if (base != "python" && base != "python3") || !same(c.Args, []string{"-m", "pip", "cache", "purge"}) {
			return errors.New("pip command does not match the allowlist")
		}
	case "go-clean-cache":
		if base != "go" || !same(c.Args, []string{"clean", "-cache"}) {
			return errors.New("go command does not match the allowlist")
		}
	case "pnpm-store-prune":
		if base != "pnpm" || !same(c.Args, []string{"store", "prune"}) {
			return errors.New("pnpm command does not match the allowlist")
		}
	default:
		return errors.New("unknown manager command")
	}
	if !filepath.IsAbs(c.Bin) || strings.ContainsAny(c.Bin, "\r\n") {
		return errors.New("manager binary must be an absolute path")
	}
	if c.ID != "uv-python-pin" && c.Dir != "" {
		return errors.New("this command does not take a working directory")
	}
	for _, arg := range c.Args {
		if strings.ContainsAny(arg, "\r\n;&|$`") {
			return fmt.Errorf("refusing argument %q", arg)
		}
	}
	return nil
}

// Display is the argv as separate quoted arguments, not a shell line.
func (c Command) Display() string {
	parts := []string{filepath.Base(c.Bin)}
	for _, arg := range c.Args {
		parts = append(parts, fmt.Sprintf("%q", arg))
	}
	return strings.Join(parts, " ")
}

// Run executes the command from an empty directory. It never invokes a shell.
func Run(ctx context.Context, c Command) (string, string, error) {
	if err := c.Validate(); err != nil {
		return "", "", err
	}
	dir := c.Dir
	if dir == "" {
		temp, err := os.MkdirTemp("", "envark-cmd-")
		if err != nil {
			return "", "", err
		}
		defer os.RemoveAll(temp)
		dir = temp
	}
	cmd := exec.CommandContext(ctx, c.Bin, c.Args...)
	cmd.Dir = dir
	cmd.Stdin = nil
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

func executable(bin string, names ...string) error {
	if !filepath.IsAbs(bin) {
		return fmt.Errorf("binary must be absolute, got %q", bin)
	}
	base := strings.TrimSuffix(strings.ToLower(filepath.Base(bin)), ".exe")
	base = strings.TrimSuffix(base, ".cmd")
	for _, name := range names {
		if base == name {
			return nil
		}
	}
	return fmt.Errorf("refusing binary named %s", filepath.Base(bin))
}

func projectDir(dir string) error {
	if !filepath.IsAbs(dir) || strings.ContainsAny(dir, "\r\n") {
		return errors.New("project directory must be absolute")
	}
	info, err := os.Lstat(dir)
	if err != nil {
		return fmt.Errorf("project directory: %w", err)
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return errors.New("project directory must be a real directory")
	}
	if safe.System(dir) {
		return errors.New("project directory must not be a system location")
	}
	return nil
}

func same(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
