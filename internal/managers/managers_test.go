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

func shellQuote(path string) string {
	return "'" + strings.ReplaceAll(path, "'", `'\''`) + "'"
}
