package scan

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestScanAccuracyLinksAndExcludes(t *testing.T) {
	root := t.TempDir()
	proj := filepath.Join(root, "proj")
	if err := os.MkdirAll(filepath.Join(proj, "node_modules", "pkg"), 0o755); err != nil {
		t.Fatal(err)
	}
	payload := bytesOf(4096)
	if err := os.WriteFile(filepath.Join(proj, "package.json"), []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(proj, "node_modules", "pkg", "index.js"), payload, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(proj, "main.go"), []byte("package main\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	keep := filepath.Join(root, "keep.txt")
	if err := os.WriteFile(keep, payload, 0o644); err != nil {
		t.Fatal(err)
	}
	dup := filepath.Join(root, "keep-link.txt")
	if err := os.Link(keep, dup); err != nil {
		t.Skip("hard links are not supported:", err)
	}
	outside := t.TempDir()
	if err := os.WriteFile(filepath.Join(outside, "secret.txt"), bytesOf(99999), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	git := filepath.Join(root, ".git")
	if err := os.Mkdir(git, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(git, "big"), bytesOf(50000), 0o644); err != nil {
		t.Fatal(err)
	}

	got, err := Scan(context.Background(), root, Options{})
	if err != nil {
		t.Fatal(err)
	}
	// package.json 3, index.js 4096, main.go 13, keep.txt 4096. The hard
	// link and the symlink target and .git must not add bytes.
	const want = 3 + 4096 + 13 + 4096
	if got.Tree.Logical != want {
		t.Fatalf("logical = %d, want %d", got.Tree.Logical, want)
	}
	if got.Tree.Allocated < got.Tree.Logical && got.AllocExact {
		t.Fatalf("allocated %d < logical %d", got.Tree.Allocated, got.Tree.Logical)
	}
	if got.Excluded != 1 {
		t.Fatalf("excluded = %d, want 1", got.Excluded)
	}
	node := Find(got.Tree, filepath.Join(proj, "node_modules"))
	if node == nil || !node.Class.Eligible {
		t.Fatalf("node_modules class: %+v", node)
	}
	if Find(got.Tree, filepath.Join(root, "escape")) == nil {
		t.Fatal("symlink was omitted")
	}
	escape := Find(got.Tree, filepath.Join(root, "escape"))
	if escape.Logical != 0 && escape.Class.Kind != "link" && !escape.Link {
		// symlink size may be the target string length, which is not 99999
	}
	if escape.Logical >= 99999 {
		t.Fatalf("symlink followed, logical %d", escape.Logical)
	}
	if !got.HardlinkAware && runtime.GOOS != "windows" {
		t.Fatal("expected hardlink accounting")
	}
}

func TestUnreadableDirectoryIsPartial(t *testing.T) {
	if os.Geteuid() == 0 {
		t.Skip("root can read mode 000 directories")
	}
	root := t.TempDir()
	secret := filepath.Join(root, "secret")
	if err := os.Mkdir(secret, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(secret, "hidden.txt"), []byte("nope"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(secret, 0); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(secret, 0o755) })
	if err := os.WriteFile(filepath.Join(root, "visible.txt"), []byte("yes"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Scan(context.Background(), root, Options{})
	if err != nil {
		t.Fatal(err)
	}
	if got.Tree.Complete {
		t.Fatal("scan was marked complete")
	}
	if got.Tree.Logical != 3 {
		t.Fatalf("logical = %d, want 3", got.Tree.Logical)
	}
	if got.Tree.Skipped == 0 || len(got.Errors) == 0 {
		t.Fatalf("skipped=%d errors=%v", got.Tree.Skipped, got.Errors)
	}
}

func TestCancellationDiscardsTheWalk(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "a.txt"), []byte("a"), 0o644); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	_, err := Scan(ctx, root, Options{AfterEntry: func() {
		cancel()
	}})
	if err == nil {
		t.Fatal("cancelled scan returned a result")
	}
	if err != context.Canceled {
		t.Fatalf("err = %v", err)
	}
}

func TestOtherVolumeIsNotEntered(t *testing.T) {
	root := t.TempDir()
	nested := filepath.Join(root, "mnt", "data")
	if err := os.MkdirAll(nested, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(nested, "disk.bin"), bytesOf(8192), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "local.txt"), []byte("ab"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Scan(context.Background(), root, Options{SameDevice: func(uint64) bool { return false }})
	if err != nil {
		t.Fatal(err)
	}
	if got.Tree.Logical != 2 {
		t.Fatalf("logical = %d, want 2", got.Tree.Logical)
	}
	mnt := Find(got.Tree, filepath.Join(root, "mnt"))
	if mnt == nil || !mnt.OtherVolume || mnt.Complete {
		t.Fatalf("mount entry: %+v", mnt)
	}
	if len(mnt.Children) != 0 {
		t.Fatal("other volume was entered")
	}
}

func bytesOf(n int) []byte {
	b := make([]byte, n)
	for i := range b {
		b[i] = 'x'
	}
	return b
}
