package cleanup

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/felixpy/envark/internal/mark"
	"github.com/felixpy/envark/internal/scan"
	"github.com/felixpy/envark/internal/trash"
)

func TestPreviewRevalidateAndPartialTrash(t *testing.T) {
	trash.Dir = t.TempDir()
	t.Cleanup(func() { trash.Dir = "" })
	root := t.TempDir()
	keep := filepath.Join(root, "keep")
	stale := filepath.Join(root, "stale")
	for _, dir := range []string{keep, stale} {
		if err := os.Mkdir(dir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "data.txt"), []byte("1234"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	measured, err := scan.Scan(context.Background(), root, scan.Options{Excludes: []string{}})
	if err != nil {
		t.Fatal(err)
	}
	targets := []mark.Target{targetOf(measured.Tree, keep, root), targetOf(measured.Tree, stale, root)}
	if err := os.WriteFile(filepath.Join(stale, "data.txt"), []byte("12345"), 0o644); err != nil {
		t.Fatal(err)
	}
	got := Execute(context.Background(), targets, false, nil, "", []string{})
	if len(got) != 2 {
		t.Fatalf("results: %+v", got)
	}
	if got[0].Outcome != "removed" || got[0].TrashPath == "" {
		t.Fatalf("keep: %+v", got[0])
	}
	if got[1].Outcome != "failed" {
		t.Fatalf("stale: %+v", got[1])
	}
	if _, err := os.Stat(stale); err != nil {
		t.Fatal("stale directory was removed", err)
	}
	if _, err := os.Stat(keep); !os.IsNotExist(err) {
		t.Fatal("keep directory still exists")
	}
	if err := Restore(got[0].TrashPath, keep, root, nil); err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(filepath.Join(keep, "data.txt"))
	if err != nil || string(body) != "1234" {
		t.Fatalf("restored %q %v", body, err)
	}
}

func TestSymlinkTargetIsRejected(t *testing.T) {
	trash.Dir = t.TempDir()
	t.Cleanup(func() { trash.Dir = "" })
	root := t.TempDir()
	outside := t.TempDir()
	payload := filepath.Join(outside, "node_modules")
	if err := os.Mkdir(payload, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(payload, "index.js"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(root, "link")
	if err := os.Symlink(payload, link); err != nil {
		t.Fatal(err)
	}
	target := mark.Target{Path: link, Root: root, Logical: 1}
	got := Execute(context.Background(), []mark.Target{target}, false, nil, "", nil)
	if got[0].Outcome != "failed" {
		t.Fatalf("result: %+v", got[0])
	}
	if _, err := os.Stat(filepath.Join(payload, "index.js")); err != nil {
		t.Fatal(err)
	}
}

func TestCancellationLeavesTheRest(t *testing.T) {
	root := t.TempDir()
	dir := filepath.Join(root, "mod")
	if err := os.Mkdir(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	got := Execute(ctx, []mark.Target{{Path: dir, Root: root}}, false, nil, "", nil)
	if len(got) != 1 || got[0].Outcome != "cancelled" {
		t.Fatalf("result: %+v", got)
	}
	if _, err := os.Stat(dir); err != nil {
		t.Fatal(err)
	}
}

func targetOf(tree *scan.Entry, path, root string) mark.Target {
	n := scan.Find(tree, path)
	return mark.Target{
		Path: path, Root: root, Logical: n.Logical, Allocated: n.Allocated,
		Dev: n.Dev, Ino: n.Ino, ModUnix: n.Mod.UnixNano(),
	}
}
