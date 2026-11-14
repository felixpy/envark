package managers

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestConstructorsRejectInjection(t *testing.T) {
	bin := filepath.Join(t.TempDir(), "rustup")
	if _, err := RustupUninstall(bin, "nightly;rm -rf /"); err == nil {
		t.Fatal("metacharacters accepted")
	}
	if _, err := RustupUninstall(bin, "stable"); err == nil {
		t.Fatal("stable toolchain accepted")
	}
	if _, err := RustupUninstall("rustup", "nightly"); err == nil {
		t.Fatal("relative binary accepted")
	}
	if _, err := FnmUninstall(filepath.Join(t.TempDir(), "fnm"), "v20.0.0 && uname"); err == nil {
		t.Fatal("fnm injection accepted")
	}
	if _, err := NpmCacheClean(filepath.Join(t.TempDir(), "bash")); err == nil {
		t.Fatal("npm name mismatch accepted")
	}
	cmd, err := NpmCacheClean(filepath.Join(t.TempDir(), "npm"))
	if err != nil {
		t.Fatal(err)
	}
	cmd.Args = append(cmd.Args, "--extra")
	if err := cmd.Validate(); err == nil {
		t.Fatal("mutated npm args validated")
	}
}

func TestRunUsesSeparateArguments(t *testing.T) {
	dir := t.TempDir()
	logPath := filepath.Join(dir, "args.txt")
	bin := filepath.Join(dir, "fnm")
	script := "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > " + shellQuote(logPath) + "\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	cmd, err := FnmUninstall(bin, "v20.11.0")
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := Run(context.Background(), cmd); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(string(body)), "\n")
	if len(lines) != 3 || lines[1] != "uninstall" || lines[2] != "v20.11.0" {
		t.Fatalf("args = %#v", lines)
	}
}

func TestMissingBinaryIsNotSuccess(t *testing.T) {
	cmd, err := GoCleanCache(filepath.Join(t.TempDir(), "go"))
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := Run(context.Background(), cmd); err == nil {
		t.Fatal("missing binary reported success")
	}
}

func TestInstallDefaultAndPin(t *testing.T) {
	dir := t.TempDir()
	fnm := filepath.Join(dir, "fnm")
	if err := os.WriteFile(fnm, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if _, err := FnmInstall(fnm, "v22;id"); err == nil {
		t.Fatal("install injection accepted")
	}
	install, err := FnmInstall(fnm, "v22.13.0")
	if err != nil || install.Args[0] != "install" || install.Hint == "" {
		t.Fatalf("install: %+v %v", install, err)
	}
	def, err := FnmDefault(fnm, "v22.13.0")
	if err != nil || def.Args[0] != "default" {
		t.Fatalf("default: %+v %v", def, err)
	}
	uv := filepath.Join(dir, "uv")
	if err := os.WriteFile(uv, []byte("#!/bin/sh\nif [ \"$1\" = python ] && [ \"$2\" = pin ]; then printf '%s\\n' \"$3\" > .python-version; exit 0; fi\nexit 1\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	project := filepath.Join(dir, "proj")
	if err := os.Mkdir(project, 0o755); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "link")
	if err := os.Symlink(project, link); err != nil {
		t.Fatal(err)
	}
	if _, err := UvPythonPin(uv, "3.12", link); err == nil {
		t.Fatal("symlink project accepted")
	}
	pin, err := UvPythonPin(uv, "3.12", project)
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := Run(context.Background(), pin); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(project, ".python-version"))
	if err != nil || strings.TrimSpace(string(body)) != "3.12" {
		t.Fatalf("pin file: %q %v", body, err)
	}
}

func TestRunHonorsCancellation(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "fnm")
	flag := filepath.Join(dir, "started")
	script := "#!/bin/sh\ntouch " + shellQuote(flag) + "\nsleep 30\n"
	if err := os.WriteFile(bin, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	cmd, err := FnmInstall(bin, "v20.11.0")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, _, err := Run(ctx, cmd); err == nil {
		t.Fatal("cancelled command reported success")
	}
	if _, err := os.Stat(flag); err == nil {
		t.Fatal("cancelled command started")
	}
}

func shellQuote(path string) string {
	return "'" + strings.ReplaceAll(path, "'", `'\''`) + "'"
}
