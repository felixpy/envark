// Package app is Envark's desktop state and the actions behind it.
package app

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/cleanup"
	"github.com/felixpy/envark/internal/discover"
	"github.com/felixpy/envark/internal/i18n"
	"github.com/felixpy/envark/internal/managers"
	"github.com/felixpy/envark/internal/mark"
	"github.com/felixpy/envark/internal/safe"
	"github.com/felixpy/envark/internal/scan"
	"github.com/felixpy/envark/internal/store"
)

// Config points the app at a data directory and, for tests, a home and PATH.
type Config struct {
	DataDir string
	Home    string
	Path    string
}

// App is the window's state.
type App struct {
	Dir     string
	Home    string
	PathEnv string
	Prefs   store.Preferences
	Notices []string
	Notice  string
	Page    string
	Lang    string
	Help    bool
	Confirm string

	Tree         *scan.Entry
	ScanRoot     string
	Result       *scan.Result
	Scanning     bool
	Progress     string
	ScanErr      string
	Previous     bool
	Query        string
	Filter       int
	Marks        map[string]bool
	Selected     string
	row          int
	outline      ui.OutlineState[string]
	MapDir       string
	MapHover     string
	wantPath     string
	lastMap      []mapCell
	drillRun     uint64
	shownBackend string
	Paint        Renderer
	index        map[string]*scan.Entry
	visible      map[string]bool
	split        float32

	Probes       []discover.Probe
	Locations    []discover.Location
	Catalog      []discover.CatalogItem
	CatalogNotes []string
	EnvErr       string
	EnvRunning   bool
	PinDir       string
	PinVersion   string
	envRow       int
	envList      ui.ListState
	locRow       int
	locList      ui.ListState
	catRow       int
	catList      ui.ListState

	Extras   []mark.Target
	Commands []managers.Command
	Ack      bool

	History  []store.HistoryEntry
	histRow  int
	histList ui.ListState
	Running  bool

	ExcludeDraft   string
	ProtectedDraft string
	cancel         context.CancelFunc
	cancelEnv      context.CancelFunc
	cancelRun      context.CancelFunc
	catalogGen     uint64
	win            *mygo.Window
}

// Open loads preferences and history. It does not scan or probe until asked,
// unless a saved preference still requests a launch scan.
func Open(cfg Config) (*App, error) {
	dir := cfg.DataDir
	if dir == "" {
		var err error
		dir, err = store.DefaultDir()
		if err != nil {
			return nil, err
		}
	}
	home := cfg.Home
	if home == "" {
		h, err := os.UserHomeDir()
		if err != nil {
			return nil, err
		}
		home = h
	}
	pathEnv := cfg.Path
	if pathEnv == "" {
		pathEnv = os.Getenv("PATH")
	}
	loaded, err := store.Open(dir)
	if err != nil {
		return nil, err
	}
	app := &App{
		Dir:          dir,
		Home:         home,
		PathEnv:      pathEnv,
		Prefs:        loaded.Prefs,
		Notices:      loaded.Notices,
		History:      loaded.History,
		Page:         "space",
		Lang:         i18n.Resolve(loaded.Prefs.Language, os.Getenv("LC_ALL"), os.Getenv("LC_MESSAGES"), os.Getenv("LANG")),
		Marks:        map[string]bool{},
		index:        map[string]*scan.Entry{},
		visible:      map[string]bool{},
		split:        68,
		ExcludeDraft: strings.Join(loaded.Prefs.Excludes, "\n"),
	}
	if len(app.Prefs.Roots) > 0 {
		app.ScanRoot = app.Prefs.Roots[0]
	}
	if app.Prefs.ScanOnLaunch && app.ScanRoot != "" {
		app.startScan()
	}
	return app, nil
}

// Attach connects the window used for dialogs and background updates.
func (a *App) Attach(win *mygo.Window) { a.win = win }

func (a *App) env() discover.Env {
	return discover.Env{Home: a.Home, Path: a.PathEnv}
}

func (a *App) persist() {
	if err := store.SavePreferences(a.Dir, a.Prefs); err != nil {
		a.Notice = err.Error()
	}
}

func (a *App) applyLanguage() {
	a.Lang = i18n.Resolve(a.Prefs.Language, os.Getenv("LC_ALL"), os.Getenv("LC_MESSAGES"), os.Getenv("LANG"))
}

// ApplyTheme follows the saved appearance once the window exists.
func (a *App) ApplyTheme() { a.applyTheme() }

// Show switches the visible page.
func (a *App) Show(page string) {
	if a.win == nil {
		a.Page = page
		return
	}
	a.win.Update(func() { a.Page = page })
}

// ShowHelp opens the keyboard dialog.
func (a *App) ShowHelp() {
	if a.win == nil {
		a.Help = true
		return
	}
	a.win.Update(func() { a.Help = true })
}

// PickRoot asks for a folder when a window is attached.
func (a *App) PickRoot() { a.pickRoot() }

// Scan starts a scan of the selected root.
func (a *App) Scan() {
	if a.win == nil {
		a.startScan()
		return
	}
	a.win.Update(func() { a.startScan() })
}

func (a *App) applyTheme() {
	if a.win == nil {
		return
	}
	switch a.Prefs.Theme {
	case "dark":
		mygo.Theme.SetSource(mygo.ThemeDark)
	case "light":
		mygo.Theme.SetSource(mygo.ThemeLight)
	default:
		mygo.Theme.SetSource(mygo.ThemeSystem)
	}
}

// AddRoot remembers a real directory and selects it. It does not scan.
func (a *App) AddRoot(path string) {
	path, err := safe.RealDir(path)
	if err != nil {
		a.Notice = err.Error()
		return
	}
	if safe.System(path) || safe.IsHome(path, a.Home) {
		a.Notice = "Choose a project or work folder. The home directory and system locations are not scan roots."
		return
	}
	for _, have := range a.Prefs.Roots {
		if have == path {
			a.ScanRoot = path
			return
		}
	}
	a.Prefs.Roots = append(a.Prefs.Roots, path)
	a.ScanRoot = path
	a.persist()
	a.Notice = ""
}

// RemoveRoot drops a saved root. It does not delete the directory.
func (a *App) RemoveRoot(path string) {
	var next []string
	for _, have := range a.Prefs.Roots {
		if have != path {
			next = append(next, have)
		}
	}
	a.Prefs.Roots = next
	if a.ScanRoot == path {
		a.ScanRoot = ""
		if len(next) > 0 {
			a.ScanRoot = next[0]
		}
	}
	a.persist()
}

func (a *App) startScan() {
	if a.Scanning {
		return
	}
	root := a.ScanRoot
	if root == "" {
		a.Notice = "Choose a folder to scan. Envark does not walk your home directory on its own."
		return
	}
	checked, err := safe.RealDir(root)
	if err != nil {
		a.ScanErr = err.Error()
		return
	}
	ctx, cancel := context.WithCancel(context.Background())
	a.cancel = cancel
	a.Scanning = true
	a.ScanErr = ""
	a.Progress = "Starting scan"
	excludes := append([]string(nil), a.Prefs.Excludes...)
	run := func() {
		res, err := scan.Scan(ctx, checked, scan.Options{
			Excludes: excludes,
			Progress: func(path string, n int) {
				if a.win == nil || n%100 != 0 {
					return
				}
				a.win.Update(func() {
					a.Progress = fmt.Sprintf("%d entries · %s", n, path)
				})
			},
		})
		apply := func() {
			a.Scanning = false
			a.cancel = nil
			a.Progress = ""
			if err != nil {
				a.ScanErr = err.Error()
				a.Previous = a.Tree != nil
				if a.Tree != nil {
					a.Notice = "Scan stopped. The previous result is still on screen."
				}
				return
			}
			a.setTree(res)
		}
		if a.win == nil {
			apply()
			return
		}
		a.win.Update(apply)
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) cancelScan() {
	if a.cancel != nil {
		a.cancel()
	}
}

func (a *App) setTree(res *scan.Result) {
	a.Result = res
	a.Tree = res.Tree
	a.Previous = false
	a.Marks = map[string]bool{}
	a.index = map[string]*scan.Entry{}
	indexTree(res.Tree, a.index)
	a.outline.Open.Clear()
	if res.Tree != nil {
		a.openToReclaimable(res.Tree)
		a.outline.Open.Add(res.Tree.Path)
		a.Selected = res.Tree.Path
		a.row = 0
	}
}

// openToReclaimable expands ancestors of cleanup targets so they are visible
// without a hunt through collapsed rows.
func (a *App) openToReclaimable(n *scan.Entry) bool {
	if n == nil {
		return false
	}
	hit := n.Class.Eligible
	for _, child := range n.Children {
		if a.openToReclaimable(child) {
			hit = true
		}
	}
	if hit && n.Dir {
		a.outline.Open.Add(n.Path)
	}
	return hit
}

func indexTree(n *scan.Entry, m map[string]*scan.Entry) {
	if n == nil {
		return
	}
	m[n.Path] = n
	for _, child := range n.Children {
		indexTree(child, m)
	}
}

func (a *App) paintVisible() {
	a.visible = map[string]bool{}
	if a.Tree == nil {
		return
	}
	var visit func(*scan.Entry) bool
	visit = func(n *scan.Entry) bool {
		child := false
		for _, ch := range n.Children {
			if visit(ch) {
				child = true
			}
		}
		self := a.queryMatch(n) && (a.Filter == 0 || n.Class.Eligible)
		if self || child {
			a.visible[n.Path] = true
			return true
		}
		return false
	}
	visit(a.Tree)
}

func (a *App) queryMatch(n *scan.Entry) bool {
	if strings.TrimSpace(a.Query) == "" {
		return true
	}
	q := strings.ToLower(strings.TrimSpace(a.Query))
	return strings.Contains(strings.ToLower(n.Name), q) || strings.Contains(strings.ToLower(n.Rel), q)
}

func (a *App) children(path string) []string {
	n := a.index[path]
	if n == nil {
		return nil
	}
	var out []string
	for _, child := range n.Children {
		if a.visible[child.Path] {
			out = append(out, child.Path)
		}
	}
	return out
}

func (a *App) toggle(path string) {
	n := a.index[path]
	if n == nil || !n.Class.Eligible {
		a.Notice = "That row is not a cleanup target."
		return
	}
	if a.Marks[path] {
		delete(a.Marks, path)
		return
	}
	a.Marks[path] = true
	a.Notice = ""
}

func (a *App) reviewTargets() (mark.Resolved, []string) {
	var root string
	if a.Result != nil {
		root = a.Result.Root
	}
	resolved := mark.Resolve(a.Tree, root, a.Marks, a.Prefs.Protected, a.Home)
	merged, notes := mark.Absorb(append(append([]mark.Target{}, resolved.Targets...), a.Extras...))
	resolved.Targets = merged
	resolved.Logical = 0
	for _, target := range merged {
		resolved.Logical += target.Logical
	}
	resolved.Notes = append(resolved.Notes, notes...)
	return resolved, notes
}

func (a *App) startDiscover() {
	if a.EnvRunning {
		return
	}
	a.EnvRunning = true
	a.EnvErr = ""
	a.catalogGen++
	env := a.env()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	a.cancelEnv = cancel
	run := func() {
		probes := discover.Discover(ctx, env)
		locs := discover.Locations(ctx, env)
		// Read the context before releasing it. Update can run after this
		// function returns, and cancelling first would look like a user cancel.
		stopped := ctx.Err()
		apply := func() {
			a.EnvRunning = false
			a.cancelEnv = nil
			if stopped != nil {
				a.EnvErr = "Version check cancelled."
				return
			}
			a.Probes = probes
			a.Locations = locs
		}
		if a.win == nil {
			apply()
		} else {
			a.win.Update(apply)
		}
		cancel()
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) fetchRemote() {
	if a.EnvRunning {
		return
	}
	a.EnvRunning = true
	a.EnvErr = ""
	a.catalogGen++
	env := a.env()
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	a.cancelEnv = cancel
	run := func() {
		items, notes, err := discover.Remote(ctx, env)
		stopped := ctx.Err()
		apply := func() {
			a.EnvRunning = false
			a.cancelEnv = nil
			if stopped != nil {
				a.Catalog = nil
				a.EnvErr = "Version check cancelled."
				return
			}
			a.Catalog = items
			a.CatalogNotes = notes
			if err != nil {
				a.EnvErr = err.Error()
			}
		}
		if a.win == nil {
			apply()
		} else {
			a.win.Update(apply)
		}
		cancel()
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) cancelEnvironment() {
	if a.cancelEnv != nil {
		a.cancelEnv()
	}
}

func (a *App) measureLocations() {
	if a.EnvRunning {
		return
	}
	a.EnvRunning = true
	locs := append([]discover.Location(nil), a.Locations...)
	run := func() {
		ctx := context.Background()
		for i := range locs {
			if locs[i].Path == "" || locs[i].Missing {
				continue
			}
			res, err := scan.Scan(ctx, locs[i].Path, scan.Options{Excludes: []string{}})
			if err != nil {
				locs[i].Err = err.Error()
				continue
			}
			locs[i].Logical = res.Tree.Logical
			locs[i].Allocated = res.Tree.Allocated
			locs[i].Measured = true
			locs[i].Incomplete = !res.Tree.Complete
		}
		apply := func() {
			a.EnvRunning = false
			a.Locations = locs
		}
		if a.win == nil {
			apply()
			return
		}
		a.win.Update(apply)
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) addProbe(probe discover.Probe) {
	if probe.Removal == nil {
		a.Notice = probe.Unavailable
		return
	}
	a.addCommand(*probe.Removal)
}

func (a *App) addLocation(loc discover.Location) {
	if loc.Policy.Method == "command" {
		if loc.Command == nil {
			a.Notice = firstText(loc.Err, loc.Policy.Reason, "This command is unavailable.")
			return
		}
		a.addCommand(*loc.Command)
		return
	}
	if !loc.Policy.Eligible || loc.Path == "" {
		a.Notice = firstText(loc.Err, loc.Policy.Reason, "This location is not a cleanup target.")
		return
	}
	res, err := scan.Scan(context.Background(), loc.Path, scan.Options{Excludes: []string{}})
	if err != nil {
		a.Notice = err.Error()
		return
	}
	parent := filepath.Dir(loc.Path)
	a.Extras = append(a.Extras, mark.Target{
		Path: loc.Path, Rel: loc.Path, Root: parent,
		Logical: res.Tree.Logical, Allocated: res.Tree.Allocated,
		Dev: res.Tree.Dev, Ino: res.Tree.Ino, ModUnix: res.Tree.Mod.UnixNano(),
		ClassKind: loc.Policy.Kind, ClassRisk: loc.Policy.Risk,
		Title: loc.Policy.Title, Reason: loc.Policy.Reason,
	})
	a.Page = "review"
}

func (a *App) runFiles(permanent bool) {
	if a.Running {
		return
	}
	resolved, _ := a.reviewTargets()
	if len(resolved.Targets) == 0 {
		a.Notice = "There is nothing to remove."
		return
	}
	if permanent && !a.Prefs.AllowPermanent {
		a.Notice = "Permanent deletion is off in Settings."
		return
	}
	if !permanent && !a.Prefs.UseTrash {
		a.Notice = "Trash is off. Turn it back on, or enable permanent deletion and confirm that separately."
		return
	}
	a.Running = true
	targets := append([]mark.Target(nil), resolved.Targets...)
	protected := append([]string(nil), a.Prefs.Protected...)
	excludes := append([]string(nil), a.Prefs.Excludes...)
	home := a.Home
	run := func() {
		items := cleanup.Execute(context.Background(), targets, permanent, protected, home, excludes)
		note := cleanup.Summary(items)
		apply := func() {
			a.Running = false
			a.Confirm = ""
			a.record(actionName(permanent), note, items)
			a.Marks = map[string]bool{}
			a.Extras = nil
			a.Notice = note
			if a.ScanRoot != "" {
				a.startScan()
			}
		}
		if a.win == nil {
			apply()
			return
		}
		a.win.Update(apply)
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) addCommand(cmd managers.Command) {
	cmd.Revision = a.catalogGen
	for _, have := range a.Commands {
		if have.ID == cmd.ID && sameArgs(have.Args, cmd.Args) && have.Dir == cmd.Dir {
			a.Page = "review"
			return
		}
	}
	a.Commands = append(a.Commands, cmd)
	a.Page = "review"
	a.Notice = ""
}

func (a *App) reviewPin() {
	dir := strings.TrimSpace(a.PinDir)
	abs, err := safe.Absolute(dir)
	if err != nil || safe.System(abs) {
		a.Notice = "A project pin needs a real project directory, not a system location."
		return
	}
	info, statErr := os.Lstat(abs)
	if statErr != nil || info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		a.Notice = "A project pin needs a real directory."
		return
	}
	uv, err := discover.Look(a.PathEnv, "uv")
	if err != nil {
		a.Notice = "uv was not found. Envark will not write a Python pin."
		return
	}
	cmd, err := managers.UvPythonPin(uv, strings.TrimSpace(a.PinVersion), abs)
	if err != nil {
		a.Notice = err.Error()
		return
	}
	a.addCommand(cmd)
}

func (a *App) runCommands() {
	if a.Running {
		return
	}
	if !a.Ack {
		a.Notice = "Confirm that manager commands cannot be restored from Trash."
		return
	}
	cmds := append([]managers.Command(nil), a.Commands...)
	if len(cmds) == 0 {
		a.Notice = "There is no manager command in the review."
		return
	}
	a.Running = true
	ctx, cancel := context.WithCancel(context.Background())
	a.cancelRun = cancel
	run := func() {
		items := a.executePlans(ctx, cmds)
		note := cleanup.Summary(items)
		apply := func() {
			a.Running = false
			a.cancelRun = nil
			a.Confirm = ""
			a.Ack = false
			a.Commands = nil
			a.record("command", note, items)
			a.Notice = note
		}
		if a.win == nil {
			apply()
			return
		}
		a.win.Update(apply)
	}
	if a.win == nil {
		run()
		return
	}
	go run()
}

func (a *App) executePlans(ctx context.Context, cmds []managers.Command) []cleanup.ItemResult {
	generation := a.catalogGen
	var items []cleanup.ItemResult
	for _, cmd := range cmds {
		item := cleanup.ItemResult{Action: "command", Path: cmd.Display()}
		if cmd.Revision != generation {
			item.Outcome = "failed"
			item.Error = "This plan is stale. Refresh versions and review the command again."
			items = append(items, item)
			continue
		}
		if ctx.Err() != nil {
			item.Outcome = "cancelled"
			item.Error = ctx.Err().Error()
			items = append(items, item)
			continue
		}
		stdout, stderr, err := managers.Run(ctx, cmd)
		item.Detail = strings.TrimSpace(stdout + "\n" + stderr)
		if err != nil {
			if ctx.Err() != nil {
				item.Outcome = "cancelled"
				item.Error = ctx.Err().Error()
			} else {
				item.Outcome = "failed"
				item.Error = err.Error()
			}
		} else {
			item.Outcome = "completed"
		}
		items = append(items, item)
	}
	return items
}

func (a *App) restore(item cleanup.ItemResult, root string) {
	if item.TrashPath == "" {
		a.Notice = "This item has no trash path. Restore it from the system trash if it is still there."
		return
	}
	if err := cleanup.Restore(item.TrashPath, item.Path, root, a.Prefs.Protected); err != nil {
		a.Notice = err.Error()
		return
	}
	a.record("restore", "Restored "+item.Path, []cleanup.ItemResult{{
		Path: item.Path, Action: "restore", Outcome: "completed", TrashPath: item.TrashPath,
	}})
	a.Notice = "Restored " + item.Path
}

func (a *App) record(action, note string, items []cleanup.ItemResult) {
	a.History = append(a.History, store.HistoryEntry{
		ID: fmt.Sprintf("%d", time.Now().UnixNano()), At: time.Now(), Action: action, Note: note, Items: items,
	})
	if err := store.SaveHistory(a.Dir, a.History); err != nil {
		a.Notice = err.Error()
	}
}

func (a *App) saveExcludes() {
	var names []string
	for _, line := range strings.Split(a.ExcludeDraft, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		if strings.ContainsAny(line, `/\`) || line == "." || line == ".." {
			a.Notice = "Excludes are single directory names, such as .git."
			return
		}
		names = append(names, line)
	}
	a.Prefs.Excludes = names
	a.persist()
	a.Notice = "Excludes saved. The next scan uses them."
}

func (a *App) addProtected() {
	path := strings.TrimSpace(a.ProtectedDraft)
	abs, err := safe.Absolute(path)
	if err != nil {
		a.Notice = "A protected project must be an absolute path."
		return
	}
	if info, statErr := os.Lstat(abs); statErr == nil {
		if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
			a.Notice = "A protected project must be a real directory."
			return
		}
	}
	for _, have := range a.Prefs.Protected {
		if have == abs {
			return
		}
	}
	a.Prefs.Protected = append(a.Prefs.Protected, abs)
	a.ProtectedDraft = ""
	a.persist()
}

func (a *App) pickRoot() {
	if a.win == nil {
		a.Notice = "Folder selection needs the desktop window."
		return
	}
	win := a.win
	go func() {
		paths, err := mygo.Dialog.Open(mygo.OpenDialogOptions{
			Parent: win, Title: "Choose a folder to scan", Directory: true,
		})
		if err != nil || len(paths) == 0 {
			return
		}
		win.Update(func() {
			a.AddRoot(paths[0])
		})
	}()
}

func actionName(permanent bool) string {
	if permanent {
		return "permanent"
	}
	return "trash"
}

func sameArgs(a, b []string) bool {
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

func firstText(values ...string) string {
	for _, value := range values {
		if strings.TrimSpace(value) != "" {
			return value
		}
	}
	return ""
}
