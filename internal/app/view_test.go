package app

import (
	"bytes"
	"context"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/discover"
	"github.com/felixpy/envark/internal/managers"
	"github.com/felixpy/envark/internal/scan"
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
	tt := ui.NewTester(app.View, 720, 900)
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
	root := variedFixture(t)
	app.AddRoot(root)
	app.Scan()
	webModules := filepath.Join(root, "web", "node_modules")
	app.Marks[webModules] = true
	app.Selected = filepath.Join(root, "api")
	app.wantPath = app.Selected
	saveShot(t, app, dir, "space.png")
	app.Page = "review"
	saveShot(t, app, dir, "review.png")
	app.Page = "environments"
	app.Probes = []discover.Probe{
		{Ecosystem: "Node.js", Version: "v22.14.0", Manager: "fnm", Scope: "default", Ownership: "manager", Unavailable: "This is the active or default Node.js. Envark will not remove it."},
		{Ecosystem: "Node.js", Version: "v20.11.0", Manager: "fnm", Scope: "installed", Ownership: "manager"},
		{Ecosystem: "Python", Version: "3.12.1", Manager: "uv", Scope: "default", Ownership: "manager", Unavailable: "This is the active or default Python. Envark will not remove it."},
		{Ecosystem: "Java", Version: "21.0.2", Manager: "system", Scope: "installed", Ownership: "system", Unavailable: "Java removal is not offered. Use the manager or package that installed it."},
	}
	if cmd, err := managers.FnmUninstall("/usr/bin/fnm", "v20.11.0"); err == nil {
		app.Probes[1].Removal = &cmd
	}
	app.Catalog = []discover.CatalogItem{{
		Ecosystem: "Node.js", Manager: "fnm", Version: "v22.13.0", Scope: "available",
	}}
	if cmd, err := managers.FnmInstall("/usr/bin/fnm", "v22.13.0"); err == nil {
		app.Catalog[0].Install = &cmd
		app.Catalog[0].Hint = cmd.Hint
	}
	saveShot(t, app, dir, "environments.png")
	app.Page = "settings"
	saveShotSize(t, app, dir, "settings.png", 1280, 1200)
	app.Page = "space"
	app.Lang = "zh"
	tt := ui.NewTester(app.View, 720, 900)
	saveImage(t, tt, filepath.Join(dir, "space-zh-narrow.png"))
	app.Page = "environments"
	tt.Frame()
	saveImage(t, tt, filepath.Join(dir, "environments-zh-narrow.png"))
}

func variedFixture(t *testing.T) string {
	t.Helper()
	root := filepath.Join(t.TempDir(), "workspace")
	dirs := []string{
		filepath.Join(root, "web", "node_modules", "leftpad"),
		filepath.Join(root, "web", ".next"),
		filepath.Join(root, "web", "assets"),
		filepath.Join(root, "api", "target"),
		filepath.Join(root, "archive"),
		filepath.Join(root, "mobile", "build"),
		filepath.Join(root, "vendor", "modules"),
		filepath.Join(root, "lab", "cache", "a", "b"),
		filepath.Join(root, "design"),
		filepath.Join(root, "docs"),
		filepath.Join(root, "tools"),
		filepath.Join(root, "notes"),
		filepath.Join(root, "scripts"),
		filepath.Join(root, "config"),
		filepath.Join(root, "locked"),
	}
	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	writeBytes(t, filepath.Join(root, "web", "package.json"), []byte("{}\n"))
	writeBytes(t, filepath.Join(root, "api", "Cargo.toml"), []byte("[package]\nname='api'\n"))
	writeBytes(t, filepath.Join(root, "web", "assets", "clip.bin"), bytes.Repeat([]byte("w"), 3<<20))
	writeBytes(t, filepath.Join(root, "web", "node_modules", "leftpad", "index.js"), bytes.Repeat([]byte("n"), 2<<20))
	writeBytes(t, filepath.Join(root, "web", ".next", "cache.bin"), bytes.Repeat([]byte("x"), 2<<20))
	writeBytes(t, filepath.Join(root, "api", "target", "lib.rlib"), bytes.Repeat([]byte("r"), 4<<20))
	writeBytes(t, filepath.Join(root, "api", "photo.bin"), bytes.Repeat([]byte("p"), 5<<20/2))
	writeBytes(t, filepath.Join(root, "archive", "disk.img"), bytes.Repeat([]byte("a"), 5<<20))
	writeBytes(t, filepath.Join(root, "mobile", "build", "app.bin"), bytes.Repeat([]byte("m"), 5<<20/2))
	writeBytes(t, filepath.Join(root, "vendor", "modules", "lib.bin"), bytes.Repeat([]byte("v"), 3<<20/2))
	writeBytes(t, filepath.Join(root, "lab", "cache", "a", "b", "deep.bin"), bytes.Repeat([]byte("l"), 3<<20/2))
	writeBytes(t, filepath.Join(root, "design", "board.bin"), bytes.Repeat([]byte("d"), 5<<20/4))
	writeBytes(t, filepath.Join(root, "docs", "guide.bin"), bytes.Repeat([]byte("g"), 1<<20+100))
	writeBytes(t, filepath.Join(root, "tools", "cli.bin"), bytes.Repeat([]byte("t"), 800<<10))
	writeBytes(t, filepath.Join(root, "notes", "readme.txt"), bytes.Repeat([]byte("n"), 180<<10))
	writeBytes(t, filepath.Join(root, "scripts", "run.sh"), bytes.Repeat([]byte("s"), 40<<10))
	writeBytes(t, filepath.Join(root, "config", "app.toml"), bytes.Repeat([]byte("c"), 4<<10))
	if err := os.Symlink("docs", filepath.Join(root, "alias")); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(filepath.Join(root, "locked"), 0); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chmod(filepath.Join(root, "locked"), 0o755) })
	return root
}

func saveShot(t *testing.T, app *App, dir, name string) {
	t.Helper()
	saveShotSize(t, app, dir, name, 1280, 860)
}

func saveShotSize(t *testing.T, app *App, dir, name string, w, h int) {
	t.Helper()
	tt := ui.NewTester(app.View, w, h)
	tt.Frame()
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

func TestFieldShaderPaintsCellInk(t *testing.T) {
	app := newTestApp(t)
	started := time.Now()
	view := func(c *ui.Context) {
		ui.Box(c).Size(80, 40).Draw(func(p *ui.Painter, r ui.Rect) {
			app.drawFieldShader(p, r, []mapCell{{
				Path: "/api", Name: "api", Kind: "low", X: r.X, Y: r.Y, W: r.W, H: r.H, Logical: 10,
			}}, 0)
			app.Paint.GPU = p.Accelerated()
		})
	}
	img := ui.NewTester(view, 80, 40).Image()
	elapsed := time.Since(started)
	r, g, b := pix(img, 40, 20)
	t.Logf("shader sample 80x40 took %s; center %d,%d,%d; accelerated=%v", elapsed, r, g, b, app.Paint.GPU)
	if r < 190 || g < 120 || g > 160 || b > 90 {
		t.Fatalf("shader did not paint the reclaimable ink: %d,%d,%d", r, g, b)
	}
}

func TestChineseNarrowLayout(t *testing.T) {
	app := newTestApp(t)
	app.Prefs.Language = "zh"
	app.applyLanguage()
	app.Page = "environments"
	tt := ui.NewTester(app.View, 720, 900)
	for _, text := range []string{"环境", "检查可用版本", "项目钉选"} {
		if !tt.HasText(text) {
			t.Fatalf("missing %q in %v", text, tt.Texts())
		}
	}
	app.Page = "settings"
	tt.Frame()
	if !tt.HasText("简体中文") || !tt.HasText("减少动效") || !tt.HasText("着色器样本") {
		t.Fatalf("settings: %v", tt.Texts())
	}
	app.Page = "space"
	tt.Frame()
	if !tt.HasText("空间") || !tt.HasText("选择要扫描的文件夹") {
		t.Fatalf("space: %v", tt.Texts())
	}
}

func TestSelectedRowContrast(t *testing.T) {
	app := newTestApp(t)
	root := filepath.Join(t.TempDir(), "work")
	if err := os.MkdirAll(root, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "notes.txt"), bytes.Repeat([]byte("n"), 1<<20+10), 0o644); err != nil {
		t.Fatal(err)
	}
	app.AddRoot(root)
	app.Scan()
	tt := ui.NewTester(app.View, 1100, 760)
	if muted := grayOnAccent(tt.Image()); muted > 40 {
		t.Fatalf("selected row still draws gray text on the accent: %d pixels", muted)
	}
}

func TestVariedSpaceFixture(t *testing.T) {
	app := newTestApp(t)
	root := variedFixture(t)
	app.AddRoot(root)
	app.Scan()
	if app.Tree == nil || len(app.Tree.Children) < 10 {
		t.Fatalf("children: %#v", app.Tree)
	}
	if app.Tree.Children[0].Name != "web" || app.Tree.Children[1].Name != "api" {
		names := make([]string, len(app.Tree.Children))
		for i, child := range app.Tree.Children {
			names[i] = child.Name
		}
		t.Fatalf("size order: %v", names)
	}
	for _, name := range []string{"archive", "mobile", "lab", "design", "docs"} {
		if childNamed(app.Tree, name) == nil {
			t.Fatalf("missing sibling %s", name)
		}
	}
	docsEntry := childNamed(app.Tree, "docs")
	if docsEntry == nil || len(docsEntry.Children) == 0 || docsEntry.Children[0].Name != "guide.bin" {
		t.Fatal("large file was not listed")
	}
	lab := childNamed(app.Tree, "lab")
	if lab == nil || childNamed(lab, "cache") == nil {
		t.Fatal("deep directory was not kept")
	}
	if os.Geteuid() != 0 {
		if app.Result == nil || len(app.Result.Errors) == 0 || app.Result.Tree.Complete {
			t.Fatalf("permission failure was not recorded: %+v", app.Result)
		}
	}
	tt := ui.NewTester(app.View, 1280, 860)
	for _, text := range []string{"web", "api", "reclaimable", "node_modules", "Hard links once"} {
		if !tt.HasText(text) {
			t.Fatalf("missing %q in %v", text, tt.Texts())
		}
	}
	if tt.HasText("Map: CPU rectangles") || tt.HasText("Map: not drawn yet") {
		t.Fatal("renderer diagnostics belong on Settings")
	}
	if os.Geteuid() != 0 && !tt.HasText("Some paths could not be read") {
		t.Fatal("permission banner missing")
	}
	if err := tt.Click("web"); err != nil {
		t.Fatal(err)
	}
	if app.Selected == "" || !strings.HasSuffix(app.Selected, "web") {
		t.Fatalf("navigation selected %q", app.Selected)
	}
	app.Prefs.ReduceMotion = true
	app.drill(filepath.Join(root, "web"))
	tt.Frame()
	if !strings.HasSuffix(app.MapDir, "web") {
		t.Fatalf("drill with reduced motion: %s", app.MapDir)
	}
}

func TestExecutePlansReportsStaleCancelAndFailure(t *testing.T) {
	app := newTestApp(t)
	app.catalogGen = 4
	dir := t.TempDir()
	logPath := filepath.Join(dir, "args.txt")
	okBin := filepath.Join(dir, "fnm")
	badBin := filepath.Join(dir, "uv")
	writeBytes(t, okBin, []byte("#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$ENVARK_ARGS\"\nexit 0\n"))
	writeBytes(t, badBin, []byte("#!/bin/sh\nexit 1\n"))
	if err := os.Chmod(okBin, 0o755); err != nil || os.Chmod(badBin, 0o755) != nil {
		t.Fatal(err)
	}
	t.Setenv("ENVARK_ARGS", logPath)
	install, err := managers.FnmInstall(okBin, "v22.13.0")
	if err != nil {
		t.Fatal(err)
	}
	install.Revision = 4
	stale, err := managers.FnmDefault(okBin, "v20.11.0")
	if err != nil {
		t.Fatal(err)
	}
	stale.Revision = 3
	fail, err := managers.UvPythonInstall(badBin, "3.11")
	if err != nil {
		t.Fatal(err)
	}
	fail.Revision = 4
	items := app.executePlans(context.Background(), []managers.Command{stale, install, fail})
	if items[0].Outcome != "failed" || !strings.Contains(items[0].Error, "stale") {
		t.Fatalf("stale: %+v", items[0])
	}
	if items[1].Outcome != "completed" {
		t.Fatalf("current: %+v", items[1])
	}
	if items[2].Outcome != "failed" {
		t.Fatalf("failure: %+v", items[2])
	}
	logged, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(logged), "install") || strings.Contains(string(logged), "default") {
		t.Fatalf("argv log: %s", logged)
	}
	sleepDir := filepath.Join(dir, "sleep")
	if err := os.Mkdir(sleepDir, 0o755); err != nil {
		t.Fatal(err)
	}
	sleepBin := filepath.Join(sleepDir, "fnm")
	flag := filepath.Join(dir, "started")
	writeBytes(t, sleepBin, []byte("#!/bin/sh\ntouch \"$ENVARK_FLAG\"\nsleep 30\n"))
	if err := os.Chmod(sleepBin, 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ENVARK_FLAG", flag)
	slow, err := managers.FnmInstall(sleepBin, "v22.13.0")
	if err != nil {
		t.Fatal(err)
	}
	slow.Revision = 4
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	stopped := app.executePlans(ctx, []managers.Command{slow})
	if stopped[0].Outcome != "cancelled" {
		t.Fatalf("cancel: %+v", stopped[0])
	}
	if _, err := os.Stat(flag); err == nil {
		t.Fatal("cancelled install started")
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
	app.Prefs.Language = "en"
	app.Lang = "en"
	return app
}

func writeBytes(t *testing.T, path string, body []byte) {
	t.Helper()
	if err := os.WriteFile(path, body, 0o644); err != nil {
		t.Fatal(err)
	}
}

func childNamed(entry *scan.Entry, name string) *scan.Entry {
	if entry == nil {
		return nil
	}
	for _, child := range entry.Children {
		if child.Name == name {
			return child
		}
	}
	return nil
}

func grayOnAccent(img *image.RGBA) int {
	if img == nil {
		return 0
	}
	b := img.Bounds()
	n := 0
	for y := b.Min.Y; y < b.Max.Y; y++ {
		for x := b.Min.X + 1; x < b.Max.X-1; x++ {
			r, g, bl := pix(img, x, y)
			if !grayish(r, g, bl) && !greenish(r, g, bl) && !amber(r, g, bl) {
				continue
			}
			hits, light := 0, 0
			for _, d := range []int{-2, -1, 1, 2} {
				if accentish(pix(img, x+d, y)) || accentish(pix(img, x, y+d)) {
					hits++
				}
				if cream(pix(img, x+d, y)) || cream(pix(img, x, y+d)) {
					light++
				}
			}
			if hits >= 2 && light == 0 {
				n++
			}
		}
	}
	return n
}

func pix(img *image.RGBA, x, y int) (int, int, int) {
	b := img.Bounds()
	if x < b.Min.X || y < b.Min.Y || x >= b.Max.X || y >= b.Max.Y {
		return 0, 0, 0
	}
	i := img.PixOffset(x, y)
	return int(img.Pix[i]), int(img.Pix[i+1]), int(img.Pix[i+2])
}

func cream(rgb ...int) bool {
	if len(rgb) != 3 {
		return false
	}
	return rgb[0] > 210 && rgb[1] > 200 && rgb[2] > 190
}

func accentish(r, g, b int) bool {
	return r > 20 && r < 55 && g > 32 && g < 70 && b > 28 && b < 64 && g+8 >= r
}

func grayish(r, g, b int) bool {
	return abs(r-111) < 16 && abs(g-103) < 16 && abs(b-94) < 16
}

func greenish(r, g, b int) bool {
	return g > r+20 && g > b && g > 80 && r < 100 && b < 130
}

func amber(r, g, b int) bool {
	return r > 140 && r < 190 && g > 70 && g < 140 && b < 80
}

func abs(n int) int {
	if n < 0 {
		return -n
	}
	return n
}

func logicalOf(app *App) uint64 {
	if app.Tree == nil {
		return 0
	}
	return app.Tree.Logical
}
