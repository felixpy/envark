package app

import (
	"bytes"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/discover"
	"github.com/felixpy/envark/internal/trash"
)

func TestEmptySpaceAndNarrowLayout(t *testing.T) {
	app := newTestApp(t)
	tt := ui.NewTester(app.View, 1100, 760)
	if !tt.HasText("Choose a folder to scan") {
		t.Fatalf("texts: %v", tt.Texts())
	}
	tt.SetSize(720, 760)
	tt.Frame()
	if !tt.HasText("Choose a folder to scan") || !tt.HasText("Space") {
		t.Fatalf("narrow texts: %v", tt.Texts())
	}
}

func TestScanMarkReviewAndTrash(t *testing.T) {
	app := newTestApp(t)
	root := t.TempDir()
	proj := filepath.Join(root, "proj")
	modules := filepath.Join(proj, "node_modules")
	if err := os.MkdirAll(filepath.Join(modules, "pkg"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(proj, "package.json"), []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(modules, "pkg", "index.js"), bytes.Repeat([]byte("x"), 2048), 0o644); err != nil {
		t.Fatal(err)
	}
	app.AddRoot(root)
	app.Scan()
	if app.Tree == nil || app.Tree.Logical != 3+2048 {
		t.Fatalf("logical %v tree %#v", logicalOf(app), app.Tree)
	}
	tt := ui.NewTester(app.View, 1200, 800)
	if !tt.HasText("node_modules") {
		t.Fatalf("texts: %v", tt.Texts())
	}
	if err := tt.Click("node_modules"); err != nil {
		t.Fatal(err)
	}
	if !app.Marks[modules] {
		t.Fatal("node_modules was not marked")
	}
	if err := tt.Click("Review"); err != nil {
		t.Fatal(err)
	}
	tt.Frame()
	if !tt.HasText(modules) {
		t.Fatalf("review texts: %v", tt.Texts())
	}
	if err := tt.Click("Move to Trash"); err != nil {
		t.Fatal(err)
	}
	if err := tt.Click("Confirm move to Trash"); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(modules); !os.IsNotExist(err) {
		t.Fatal("node_modules still exists", err)
	}
	if _, err := os.Stat(filepath.Join(proj, "package.json")); err != nil {
		t.Fatal("package.json was removed", err)
	}
	if !strings.Contains(app.Notice, "1 moved or deleted") {
		t.Fatalf("notice: %s", app.Notice)
	}
	app.Page = "history"
	tt.Frame()
	if !tt.HasText("removed") {
		t.Fatalf("history texts: %v", tt.Texts())
	}
}

func TestUnavailableProbeIsNotASuccess(t *testing.T) {
	app := newTestApp(t)
	app.Page = "environments"
	app.Probes = []discover.Probe{{
		Ecosystem: "Java", Name: "java", Version: "21.0.2", Manager: "system", Ownership: "system",
		Unavailable: "Java removal is not offered. Use the manager or package that installed it.",
	}}
	tt := ui.NewTester(app.View, 1100, 760)
	if !tt.HasText("Java removal is not offered. Use the manager or package that installed it.") {
		t.Fatalf("texts: %v", tt.Texts())
	}
	if strings.Contains(strings.Join(tt.Texts(), "\n"), "Review removal") {
		t.Fatal("unavailable probe offered removal")
	}
}

func TestDiscoverMissingTools(t *testing.T) {
	app := newTestApp(t)
	app.Page = "environments"
	app.startDiscover()
	tt := ui.NewTester(app.View, 1100, 760)
	if !tt.HasText("Node.js was not found on PATH.") {
		t.Fatalf("texts: %v", tt.Texts())
	}
	for _, probe := range app.Probes {
		if probe.Removal != nil {
			t.Fatalf("removal without a tool: %+v", probe)
		}
	}
}

func TestScreenshots(t *testing.T) {
	dir := os.Getenv("ENVARK_SHOT_DIR")
	if dir == "" {
		t.Skip()
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	app := newTestApp(t)
	root := filepath.Join(t.TempDir(), "work")
	proj := filepath.Join(root, "api")
	if err := os.MkdirAll(filepath.Join(proj, "node_modules", "leftpad"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(proj, "target"), 0o755); err != nil {
		t.Fatal(err)
	}
	_ = os.WriteFile(filepath.Join(proj, "package.json"), []byte("{}\n"), 0o644)
	_ = os.WriteFile(filepath.Join(proj, "Cargo.toml"), []byte("[package]\nname='api'\n"), 0o644)
	_ = os.WriteFile(filepath.Join(proj, "node_modules", "leftpad", "index.js"), bytes.Repeat([]byte("a"), 3000), 0o644)
	_ = os.WriteFile(filepath.Join(proj, "target", "lib.rlib"), bytes.Repeat([]byte("b"), 8000), 0o644)
	_ = os.WriteFile(filepath.Join(proj, "main.go"), []byte("package main\n"), 0o644)
	app.AddRoot(root)
	app.Scan()
	app.Marks[filepath.Join(proj, "node_modules")] = true
	saveShot(t, app, dir, "space.png")
	app.Page = "review"
	saveShot(t, app, dir, "review.png")
	app.Page = "environments"
	app.startDiscover()
	saveShot(t, app, dir, "environments.png")
	app.Page = "space"
	tt := ui.NewTester(app.View, 720, 900)
	saveImage(t, tt, filepath.Join(dir, "space-narrow.png"))
}

func saveShot(t *testing.T, app *App, dir, name string) {
	t.Helper()
	tt := ui.NewTester(app.View, 1280, 800)
	saveImage(t, tt, filepath.Join(dir, name))
}

func saveImage(t *testing.T, tt *ui.Tester, path string) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if err := png.Encode(f, tt.Image()); err != nil {
		t.Fatal(err)
	}
}

func newTestApp(t *testing.T) *App {
	t.Helper()
	trash.Dir = t.TempDir()
	t.Cleanup(func() { trash.Dir = "" })
	app, err := Open(Config{DataDir: t.TempDir(), Home: t.TempDir(), Path: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	return app
}

func logicalOf(app *App) uint64 {
	if app.Tree == nil {
		return 0
	}
	return app.Tree.Logical
}
