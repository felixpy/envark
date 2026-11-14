package safe

import (
	"os"
	"path/filepath"
	"testing"
)

func TestAbsoluteRejectsRelativeAndNul(t *testing.T) {
	if _, err := Absolute("rel"); err == nil {
		t.Fatal("relative path accepted")
	}
	if _, err := Absolute("/tmp/a\x00b"); err == nil {
		t.Fatal("NUL path accepted")
	}
}

func TestWithinAndSymlinkEscape(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	inside := filepath.Join(root, "proj")
	if err := os.Mkdir(inside, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(inside, "file.txt"), []byte("ok"), 0o644); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(root, "escape")
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	if !Within(root, inside) {
		t.Fatal("inside path was not contained")
	}
	if err := SymlinkBelow(root, filepath.Join(inside, "file.txt")); err != nil {
		t.Fatal(err)
	}
	if err := SymlinkBelow(root, link); err == nil {
		t.Fatal("symlink directory was accepted")
	}
	nested := filepath.Join(link, "file.txt")
	if err := ResolvedInside(root, nested); err == nil {
		t.Fatal("path through a symlink was accepted")
	}
	if err := ResolvedInside(root, outside); err == nil {
		t.Fatal("outside path was accepted")
	}
}

func TestProtectedAndSystem(t *testing.T) {
	root := t.TempDir()
	project := filepath.Join(root, "app")
	artifact := filepath.Join(project, "node_modules")
	if err := os.MkdirAll(artifact, 0o755); err != nil {
		t.Fatal(err)
	}
	if !CoversProtected(artifact, []string{project}) {
		t.Fatal("artifact inside a protected project was allowed")
	}
	if !CoversProtected(root, []string{project}) {
		t.Fatal("parent of a protected project was allowed")
	}
	if CoversProtected(filepath.Join(root, "other"), []string{project}) {
		t.Fatal("unrelated path was treated as protected")
	}
	if !System("/") {
		t.Fatal("filesystem root was not system")
	}
	if System(root) {
		t.Fatal("temp dir was treated as system")
	}
}
