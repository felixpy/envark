package app

import (
	"fmt"
	"strings"

	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/human"
	"github.com/felixpy/envark/internal/i18n"
	"github.com/felixpy/envark/internal/scan"
)

func (a *App) t(s string) string { return i18n.T(a.Lang, s) }

func (a *App) tone(c *ui.Context, selected bool, text string, normal ui.Color) ui.Element {
	el := ui.Text(c, text).SingleLine()
	if selected {
		el.TextColor(c.Theme().AccentText)
		return el
	}
	if normal != (ui.Color{}) {
		el.TextColor(normal)
	}
	return el
}

// View builds one frame of the window.
func (a *App) View(c *ui.Context) {
	a.shortcuts(c)
	a.paintVisible()
	theme := c.Theme()
	width, _ := c.Size()
	ui.Column(c).Fill().Background(theme.Background).Children(func() {
		if width < 760 {
			a.topnav(c)
		}
		ui.Row(c).Grow(1).AlignItems(ui.Stretch).Children(func() {
			if width >= 760 {
				a.sidebar(c)
			}
			ui.Column(c).Grow(1).Padding(20, 22).Gap(12).Children(func() {
				a.banners(c)
				if a.Page == "space" || a.Page == "" {
					a.space(c, width)
					return
				}
				ui.Scroll(c).Grow(1).Children(func() {
					ui.Column(c).Gap(12).Children(func() {
						switch a.Page {
						case "environments":
							a.environments(c)
						case "review":
							a.review(c)
						case "history":
							a.history(c)
						case "settings":
							a.settings(c)
						}
					})
				})
			})
		})
	})
	a.overlays(c)
}

func (a *App) shortcuts(c *ui.Context) {
	if a.Confirm != "" || a.Help {
		return
	}
	if c.Shortcut(ui.Cmd, ui.Key1) {
		a.Page = "space"
	}
	if c.Shortcut(ui.Cmd, ui.Key2) {
		a.Page = "environments"
	}
	if c.Shortcut(ui.Cmd, ui.Key3) {
		a.Page = "review"
	}
	if c.Shortcut(ui.Cmd, ui.Key4) {
		a.Page = "history"
	}
	if c.Shortcut(ui.Cmd, ui.Key5) {
		a.Page = "settings"
	}
	if c.Shortcut(ui.Cmd, ui.KeyR) {
		if a.Page == "environments" {
			a.startDiscover()
		} else if a.Page == "space" {
			a.startScan()
		}
	}
	if c.Shortcut(ui.Cmd, ui.KeyF) {
		a.Page = "space"
	}
	if c.Shortcut(0, ui.KeyEscape) && a.Scanning {
		a.cancelScan()
	}
}

func (a *App) sidebar(c *ui.Context) {
	theme := c.Theme()
	ui.Column(c).Width(216).Padding(16, 12).Gap(12).Background(theme.Surface).Children(func() {
		ui.Column(c).Gap(2).Children(func() {
			ui.Text(c, "Envark").FontSize(18).FontWeight(650)
			ui.Text(c, a.t("Runtimes and disk")).FontSize(12).TextColor(theme.TextMuted)
		})
		page := a.Page
		if ui.Sidebar(c, &page, func() {
			ui.SidebarItem(c, "space", nil, a.t("Space"))
			ui.SidebarItem(c, "environments", nil, a.t("Environments"))
			ui.SidebarItem(c, "review", nil, a.t("Review")).Children(func() {
				n := len(a.Marks) + len(a.Extras) + len(a.Commands)
				if n > 0 {
					ui.Badge(c, fmt.Sprint(n))
				}
			})
			ui.SidebarItem(c, "history", nil, a.t("History"))
			ui.SidebarItem(c, "settings", nil, a.t("Settings"))
		}).Grow(1).Changed() {
			a.Page = page
		}
		if ui.Button(c, a.t("Keyboard")).Clicked() {
			a.Help = true
		}
	})
}

func (a *App) topnav(c *ui.Context) {
	ui.Row(c).Padding(8, 12).Gap(6).Children(func() {
		for _, item := range []struct{ id, label string }{
			{"space", a.t("Space")},
			{"environments", a.t("Environments")},
			{"review", a.t("Review")},
			{"history", a.t("History")},
			{"settings", a.t("Settings")},
		} {
			label := item.label
			if item.id == a.Page {
				if ui.PrimaryButton(c.Key("nav:"+item.id), label).Clicked() {
					a.Page = item.id
				}
				continue
			}
			if ui.Button(c.Key("nav:"+item.id), label).Clicked() {
				a.Page = item.id
			}
		}
	})
}

func (a *App) banners(c *ui.Context) {
	theme := c.Theme()
	for _, notice := range a.Notices {
		banner(c, notice, theme.Surface, theme.Text)
	}
	if a.Notice != "" {
		banner(c, a.Notice, theme.Surface, theme.Text)
	}
}

func banner(c *ui.Context, text string, bg, fg ui.Color) {
	ui.Column(c).Padding(10, 12).Radius(8).Background(bg).Border(1, c.Theme().Border).Children(func() {
		ui.Text(c, text).FontSize(13).TextColor(fg)
	})
}

func heading(c *ui.Context, title, sub string) {
	theme := c.Theme()
	ui.Column(c).Gap(4).Children(func() {
		ui.Text(c, title).FontSize(22).FontWeight(640)
		if sub != "" {
			ui.Text(c, sub).FontSize(13).TextColor(theme.TextMuted)
		}
	})
}

func (a *App) space(c *ui.Context, width float32) {
	heading(c, a.t("Space"), a.t("File lengths and allocated blocks, for the folder you chose."))
	a.spaceToolbar(c, width)
	if a.Tree == nil && !a.Scanning {
		a.emptySpace(c)
		return
	}
	if a.Result != nil && !a.Result.Tree.Complete {
		banner(c, a.partialText(), c.Theme().Surface, c.Theme().Text)
	}
	a.followSelection()
	if width < 980 {
		ui.Column(c).Grow(1).Gap(12).Children(func() {
			a.field(c)
			a.detail(c)
			a.tree(c, width)
		})
		return
	}
	ui.Row(c).Grow(1).Gap(12).AlignItems(ui.Stretch).Children(func() {
		ui.Column(c).Grow(1).Gap(8).Children(func() {
			a.field(c)
			a.tree(c, width)
		})
		ui.Column(c).Width(300).Children(func() { a.detail(c) })
	})
}

func (a *App) spaceToolbar(c *ui.Context, width float32) {
	actions := func() {
		min := float32(220)
		if width < 1100 {
			min = 160
		}
		if len(a.Prefs.Roots) > 0 {
			ui.Select(c, &a.ScanRoot, a.Prefs.Roots).MinWidth(min)
		}
		if ui.Button(c, a.t("Add folder")).Clicked() {
			a.pickRoot()
		}
		if ui.PrimaryButton(c, a.t("Scan")).Disabled(a.Scanning).Clicked() {
			a.startScan()
		}
		if ui.Button(c, a.t("Cancel scan")).Disabled(!a.Scanning).Clicked() {
			a.cancelScan()
		}
	}
	filters := func() {
		ui.SearchField(c, &a.Query).Placeholder(a.t("Filter by name")).Width(180)
		ui.Segmented(c, &a.Filter, a.t("All"), a.t("Reclaimable"))
	}
	if width < 1100 {
		ui.Column(c).Gap(8).Children(func() {
			ui.Row(c).Gap(8).AlignItems(ui.Center).Children(actions)
			ui.Row(c).Gap(8).AlignItems(ui.Center).Children(filters)
		})
	} else {
		ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
			actions()
			ui.Spacer(c).Grow(1)
			filters()
		})
	}
	if a.Scanning {
		ui.Text(c, "Scanning. "+a.Progress).FontSize(13).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) emptySpace(c *ui.Context) {
	theme := c.Theme()
	ui.Column(c).Grow(1).Padding(28).Gap(8).AlignItems(ui.Center).Justify(ui.Center).Children(func() {
		ui.Text(c, a.t("Choose a folder to scan")).FontSize(18).FontWeight(620)
		ui.Text(c, a.t("Envark measures only the folders you add. It does not walk your home directory until you say so.")).FontSize(13).TextColor(theme.TextMuted).MaxWidth(460)
		if a.ScanErr != "" {
			ui.Text(c, a.ScanErr).FontSize(13).TextColor(theme.Danger)
		}
	})
}

func (a *App) partialText() string {
	msg := a.t("Some paths could not be read. Sizes are incomplete.")
	if a.Result != nil && len(a.Result.Errors) > 0 {
		msg += " " + a.Result.Errors[0]
		if len(a.Result.Errors) > 1 {
			msg += fmt.Sprintf(" (+%d more)", len(a.Result.Errors)-1)
		}
	}
	return msg
}

func (a *App) tree(c *ui.Context, width float32) {
	kind, logical, allocated, risk := float32(210), float32(100), float32(110), float32(90)
	if width < 1100 {
		kind, logical, allocated, risk = 150, 84, 92, 72
	}
	cols := []ui.TableColumn{
		{Title: a.t("Name")},
		{Title: a.t("Kind"), Width: kind},
		{Title: a.t("Logical"), Width: logical, Align: ui.End},
		{Title: a.t("Allocated"), Width: allocated, Align: ui.End},
		{Title: a.t("Risk"), Width: risk},
	}
	roots := []string{}
	if a.Tree != nil && a.visible[a.Tree.Path] {
		roots = []string{a.Tree.Path}
	}
	if len(roots) == 0 {
		ui.Text(c, a.t("Nothing in this filter.")).TextColor(c.Theme().TextMuted)
		return
	}
	a.outline.List.Selected = &a.row
	ui.OutlineTable(c, &a.outline, cols, roots, a.children, func(path string, col int) {
		n := a.index[path]
		if n == nil {
			return
		}
		selected := a.pathSelected(path)
		switch col {
		case 0:
			if n.Class.Eligible {
				on := a.Marks[path]
				if ui.Checkbox(c.Key("mark:"+path), &on, n.Name).Changed() {
					if on {
						a.Marks[path] = true
					} else {
						delete(a.Marks, path)
					}
				}
			} else {
				ui.Text(c, n.Name).SingleLine()
			}
		case 1:
			a.tone(c, selected, a.t(n.Class.Title), c.Theme().TextMuted)
		case 2:
			ui.Text(c, human.Bytes(n.Logical)).Font("monospace")
		case 3:
			label := human.Bytes(n.Allocated)
			if a.Result != nil && !a.Result.AllocExact {
				label = "n/a"
			}
			a.tone(c, selected, label, c.Theme().TextMuted)
		case 4:
			a.tone(c, selected, a.t(classify.TitleRisk(n.Class.Risk)), riskColor(c.Theme(), n.Class.Risk))
		}
	}).Height(220)
	if a.outline.Rows() > 0 {
		if a.wantPath != "" {
			for i := 0; i < a.outline.Rows(); i++ {
				if a.outline.Item(i) == a.wantPath {
					a.row = i
					break
				}
			}
			a.wantPath = ""
		}
		if a.row < 0 || a.row >= a.outline.Rows() {
			a.row = 0
		}
		if item := a.outline.Item(a.row); item != "" {
			a.Selected = item
		}
	}
	if a.Result != nil {
		note := fmt.Sprintf("%s logical · %s allocated · %d files. Hard links count once. Symlinks are not followed. Other volumes are not entered.", human.Bytes(a.Tree.Logical), human.Bytes(a.Tree.Allocated), a.Tree.Files)
		if !a.Result.AllocExact {
			note += " · allocated size is not reported separately on this system"
		}
		if a.Result.Excluded > 0 {
			note += fmt.Sprintf(" · %d excluded directories omitted", a.Result.Excluded)
		}
		ui.Text(c, note).FontSize(12).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) detail(c *ui.Context) {
	theme := c.Theme()
	n := a.index[a.Selected]
	ui.Column(c).Padding(4, 8).Gap(8).Children(func() {
		ui.Text(c, a.t("Details")).FontSize(13).FontWeight(620)
		if n == nil {
			ui.Text(c, a.t("Select a row.")).TextColor(theme.TextMuted)
			return
		}
		ui.Text(c, n.Name).FontSize(16).FontWeight(620)
		ui.Text(c, n.Path).Font("monospace").FontSize(12)
		ui.Text(c, a.t(n.Class.Reason)).FontSize(13).TextColor(theme.TextMuted)
		ui.Textf(c, "Logical %s · Allocated %s · %d files", human.Bytes(n.Logical), human.Bytes(n.Allocated), n.Files).FontSize(12)
		if n.SmallFiles > 0 {
			ui.Textf(c, "%d smaller files are included in the size and are not listed on their own.", n.SmallFiles).FontSize(12).TextColor(theme.TextMuted)
		}
		if !n.Complete {
			ui.Text(c, a.t("This subtree is incomplete.")).FontSize(12).TextColor(theme.Warning)
		}
		if n.Class.Eligible {
			label := a.t("Mark for review")
			if a.Marks[n.Path] {
				label = a.t("Remove mark")
			}
			if ui.Button(c, label).Clicked() {
				a.toggle(n.Path)
			}
		}
		a.proportion(c, n)
	})
}

func (a *App) proportion(c *ui.Context, n *scan.Entry) {
	if n == nil || !n.Dir || n.Logical == 0 || len(n.Children) == 0 {
		return
	}
	theme := c.Theme()
	ui.Text(c, a.t("Largest first, by logical size")).FontSize(12).FontWeight(600)
	shown := n.Children
	if len(shown) > 8 {
		shown = shown[:8]
	}
	largest := shown[0].Logical
	for _, child := range shown[1:] {
		if child.Logical > largest {
			largest = child.Logical
		}
	}
	if largest == 0 {
		largest = 1
	}
	for _, child := range shown {
		width := 4
		if child.Logical > 0 {
			width = int(child.Logical * 220 / largest)
			if width < 4 {
				width = 4
			}
		}
		color := theme.Accent
		if child.Class.Eligible {
			color = theme.Warning
		}
		kind := child.Class.Title
		ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
			ui.Box(c).Width(float32(width)).Height(10).Radius(2).Background(color)
			ui.Textf(c, "%s  %s  %s", child.Name, human.Bytes(child.Logical), kind).FontSize(12).SingleLine()
		})
	}
}

func (a *App) environments(c *ui.Context) {
	heading(c, a.t("Environments"), a.t("Installed versions, and the manager that owns them. Install and default changes are offered for fnm and uv only."))
	ui.Row(c).Gap(8).Children(func() {
		if ui.PrimaryButton(c, a.t("Refresh")).Disabled(a.EnvRunning).Clicked() {
			a.startDiscover()
		}
		if ui.Button(c, a.t("Check available versions")).Disabled(a.EnvRunning).Clicked() {
			a.fetchRemote()
		}
		if ui.Button(c, a.t("Cancel")).Disabled(!a.EnvRunning).Clicked() {
			a.cancelEnvironment()
		}
		if ui.Button(c, a.t("Measure locations")).Disabled(a.EnvRunning || len(a.Locations) == 0).Clicked() {
			a.measureLocations()
		}
	})
	ui.Text(c, a.t("Envark runs the manager in a separate process. That does not change the terminal that is already open.")).FontSize(12).TextColor(c.Theme().TextMuted)
	ui.Text(c, a.t("Only fnm and uv can install or change a default here. nvm, mise, asdf, volta, sdkman, Homebrew, rustup, Go, and Java stay unsupported.")).FontSize(12).TextColor(c.Theme().TextMuted)
	if a.EnvRunning {
		ui.Text(c, a.t("Working…")).TextColor(c.Theme().TextMuted)
	}
	if a.EnvErr != "" {
		ui.Text(c, a.t(a.EnvErr)).TextColor(c.Theme().Danger)
	}
	if len(a.Probes) == 0 {
		ui.Text(c, a.t("No probe yet. Refresh reads the versions on this PATH.")).TextColor(c.Theme().TextMuted)
	} else {
		a.scopeRail(c)
		a.probeTable(c)
	}
	a.catalog(c)
	ui.Text(c, a.t("Known locations")).FontSize(16).FontWeight(620)
	if len(a.Locations) == 0 {
		ui.Text(c, a.t("Refresh also looks up cache locations. A missing tool stays missing; it is not shown as an empty cache.")).TextColor(c.Theme().TextMuted)
		return
	}
	a.locationTable(c)
}

func (a *App) probeTable(c *ui.Context) {
	cols := []ui.TableColumn{
		{Title: a.t("Ecosystem"), Width: 100},
		{Title: a.t("Version"), Width: 130},
		{Title: a.t("Scope"), Width: 90},
		{Title: a.t("Manager"), Width: 90},
		{Title: a.t("Action")},
	}
	a.envList.Selected = &a.envRow
	ui.Table(c, &a.envList, cols, len(a.Probes), func(row, col int) {
		probe := a.Probes[row]
		selected := row == a.envRow
		switch col {
		case 0:
			ui.Text(c, probe.Ecosystem).SingleLine()
		case 1:
			text := probe.Version
			if text == "" {
				text = "—"
			}
			ui.Text(c, text).Font("monospace").SingleLine()
		case 2:
			a.tone(c, selected, a.t(emptyDash(probe.Scope)), c.Theme().TextMuted)
		case 3:
			ui.Text(c, emptyDash(probe.Manager)).SingleLine()
		case 4:
			if probe.Removal != nil {
				if ui.Button(c.Key("probe:"+probe.Ecosystem+probe.Version), a.t("Review removal")).Clicked() {
					a.addProbe(probe)
				}
				return
			}
			a.tone(c, selected, a.t(probe.Unavailable), c.Theme().TextMuted)
		}
	}).Height(float32(56 + len(a.Probes)*42))
	if len(a.Probes) > 0 {
		if a.envRow < 0 || a.envRow >= len(a.Probes) {
			a.envRow = 0
		}
		probe := a.Probes[a.envRow]
		ui.Text(c, probe.Path).Font("monospace").FontSize(12)
		if probe.Err != "" {
			ui.Text(c, probe.Err).FontSize(12).TextColor(c.Theme().Danger)
		}
		if probe.Detail != "" {
			ui.Text(c, probe.Detail).FontSize(12).TextColor(c.Theme().TextMuted)
		}
	}
}

func (a *App) catalog(c *ui.Context) {
	ui.Text(c, a.t("Available versions")).FontSize(16).FontWeight(620)
	ui.Text(c, a.t("Check available versions asks fnm and uv. It does not install anything.")).FontSize(12).TextColor(c.Theme().TextMuted)
	for _, note := range a.CatalogNotes {
		ui.Text(c, a.t(note)).FontSize(12)
	}
	if len(a.Catalog) == 0 {
		a.pinForm(c)
		return
	}
	cols := []ui.TableColumn{
		{Title: a.t("Ecosystem"), Width: 100},
		{Title: a.t("Version"), Width: 120},
		{Title: a.t("Scope"), Width: 90},
		{Title: a.t("Manager"), Width: 80},
		{Title: a.t("Action")},
	}
	a.catList.Selected = &a.catRow
	ui.Table(c, &a.catList, cols, len(a.Catalog), func(row, col int) {
		item := a.Catalog[row]
		selected := row == a.catRow
		switch col {
		case 0:
			ui.Text(c, item.Ecosystem).SingleLine()
		case 1:
			ui.Text(c, item.Version).Font("monospace").SingleLine()
		case 2:
			a.tone(c, selected, a.t(item.Scope), c.Theme().TextMuted)
		case 3:
			ui.Text(c, item.Manager).SingleLine()
		case 4:
			acted := false
			if item.Install != nil && ui.Button(c.Key("inst:"+item.Manager+item.Version), a.t("Review install")).Clicked() {
				a.addCommand(*item.Install)
				acted = true
			}
			if item.MakeDefault != nil && ui.Button(c.Key("def:"+item.Manager+item.Version), a.t("Make default")).Clicked() {
				a.addCommand(*item.MakeDefault)
				acted = true
			}
			if item.Removal != nil && ui.Button(c.Key("rm:"+item.Manager+item.Version), a.t("Review removal")).Clicked() {
				a.addCommand(*item.Removal)
				acted = true
			}
			if !acted && item.Install == nil && item.MakeDefault == nil && item.Removal == nil {
				a.tone(c, selected, a.t(item.Hint), c.Theme().TextMuted)
			}
		}
	}).Height(float32(56 + len(a.Catalog)*42))
	a.pinForm(c)
}

func (a *App) pinForm(c *ui.Context) {
	ui.Text(c, a.t("Project pin")).FontSize(16).FontWeight(620)
	ui.Text(c, a.t("uv python pin writes .python-version in a directory you choose. It does not change an open terminal.")).FontSize(12).TextColor(c.Theme().TextMuted)
	ui.Row(c).Gap(8).Children(func() {
		ui.TextInput(c, &a.PinDir).Placeholder(a.t("Project directory")).Grow(1)
		ui.TextInput(c, &a.PinVersion).Placeholder(a.t("Python version")).Width(140)
		if ui.Button(c, a.t("Review pin")).Clicked() {
			a.reviewPin()
		}
	})
}

func (a *App) locationTable(c *ui.Context) {
	cols := []ui.TableColumn{
		{Title: a.t("Location"), Width: 180},
		{Title: a.t("Logical"), Width: 100, Align: ui.End},
		{Title: a.t("Status")},
	}
	a.locList.Selected = &a.locRow
	ui.Table(c, &a.locList, cols, len(a.Locations), func(row, col int) {
		loc := a.Locations[row]
		selected := row == a.locRow
		switch col {
		case 0:
			ui.Text(c, loc.Title).SingleLine()
		case 1:
			text := "—"
			if loc.Measured {
				text = human.Bytes(loc.Logical)
			}
			ui.Text(c, text).Font("monospace")
		case 2:
			if loc.Missing {
				a.tone(c, selected, a.t("Not present"), c.Theme().TextMuted)
				return
			}
			if loc.Err != "" && loc.Command == nil && !loc.Policy.Eligible {
				a.tone(c, selected, loc.Err, c.Theme().TextMuted)
				return
			}
			label := a.t("Details")
			if loc.Policy.Eligible {
				label = a.t("Add to review")
			}
			if ui.Button(c.Key("loc:"+loc.ID), label).Clicked() {
				if loc.Policy.Eligible {
					a.addLocation(loc)
				} else {
					a.Notice = firstText(loc.Err, loc.Policy.Reason)
				}
			}
		}
	}).Height(float32(56 + len(a.Locations)*42))
}

func (a *App) review(c *ui.Context) {
	resolved, _ := a.reviewTargets()
	heading(c, a.t("Review"), a.t("Nothing is selected until you mark it. Nested directories count once. Manager commands do not add a byte total."))
	if len(resolved.Targets) == 0 && len(a.Commands) == 0 {
		ui.Text(c, a.t("Nothing is marked. Mark a reclaimable directory in Space, or add a command from Environments.")).TextColor(c.Theme().TextMuted)
		return
	}
	if len(resolved.Targets) > 0 {
		ui.Textf(c, "%d directories · %s logical. This is the reviewed size, not a promise of free disk.", len(resolved.Targets), human.Bytes(resolved.Logical)).FontSize(13)
		for _, target := range resolved.Targets {
			ui.Column(c).Padding(8, 10).Gap(2).Border(1, c.Theme().Border).Radius(8).Children(func() {
				ui.Text(c, target.Title+" · "+classify.TitleRisk(target.ClassRisk)).FontWeight(600)
				ui.Text(c, target.Path).Font("monospace").FontSize(12)
				ui.Text(c, target.Reason).FontSize(12).TextColor(c.Theme().TextMuted)
				ui.Text(c, "Trash can restore this directory. "+human.Bytes(target.Logical)+" logical.").FontSize(12)
			})
		}
		ui.Row(c).Gap(8).Children(func() {
			if ui.PrimaryButton(c, a.t("Move to Trash")).Disabled(a.Running || !a.Prefs.UseTrash).Clicked() {
				a.Confirm = "trash"
			}
			if ui.Button(c, a.t("Delete permanently")).Disabled(a.Running || !a.Prefs.AllowPermanent).Clicked() {
				a.Confirm = "permanent"
			}
		})
	}
	if len(a.Commands) > 0 {
		ui.Text(c, a.t("Manager commands")).FontSize(16).FontWeight(620)
		for _, cmd := range a.Commands {
			ui.Column(c).Padding(8, 10).Gap(2).Border(1, c.Theme().Border).Radius(8).Children(func() {
				ui.Text(c, cmd.Display()).Font("monospace").FontSize(12)
				ui.Text(c, cmd.Effect).FontSize(12).TextColor(c.Theme().TextMuted)
				if cmd.Hint != "" {
					ui.Text(c, a.t(cmd.Hint)).FontSize(12)
				}
				if cmd.Use != "" {
					ui.Text(c, cmd.Use).Font("monospace").FontSize(12)
				}
			})
		}
		on := a.Ack
		if ui.Checkbox(c, &on, a.t("These commands cannot be restored from Trash.")).Changed() {
			a.Ack = on
		}
		if ui.Button(c, a.t("Run manager commands")).Disabled(a.Running || !a.Ack).Clicked() {
			a.Confirm = "commands"
		}
		if ui.Button(c, a.t("Cancel")).Disabled(!a.Running).Clicked() {
			if a.cancelRun != nil {
				a.cancelRun()
			}
		}
	}
	for _, note := range resolved.Notes {
		ui.Text(c, note).FontSize(12).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) history(c *ui.Context) {
	heading(c, a.t("History"), a.t("Each line is one item. A failed or cancelled item is not reported as removed."))
	if len(a.History) == 0 {
		ui.Text(c, a.t("No operations yet.")).TextColor(c.Theme().TextMuted)
		return
	}
	labels := make([]string, len(a.History))
	for i, entry := range a.History {
		labels[i] = entry.At.Format("2006-01-02 15:04:05") + "  " + entry.Action
	}
	a.histList.Selected = &a.histRow
	ui.List(c, &a.histList, len(labels), func(i int) {
		ui.Text(c, labels[i]).SingleLine()
	}).Height(160)
	if a.histRow < 0 || a.histRow >= len(a.History) {
		a.histRow = len(a.History) - 1
	}
	entry := a.History[a.histRow]
	ui.Text(c, entry.Note).FontSize(13)
	for _, item := range entry.Items {
		ui.Column(c).Gap(2).Padding(6, 0).Children(func() {
			ui.Textf(c, "%s · %s", item.Outcome, emptyDash(item.Path)).FontSize(13)
			if item.Error != "" {
				ui.Text(c, item.Error).FontSize(12).TextColor(c.Theme().Danger)
			}
			if item.Detail != "" {
				ui.Text(c, item.Detail).FontSize(12).TextColor(c.Theme().TextMuted)
			}
			if item.Outcome == "removed" && item.TrashPath != "" {
				root := ""
				if a.Result != nil {
					root = a.Result.Root
				}
				if root == "" {
					root = parentOf(item.Path)
				}
				if ui.Button(c.Key("restore:"+item.TrashPath), a.t("Restore")).Clicked() {
					a.restore(item, root)
				}
			}
		})
	}
}

func (a *App) settings(c *ui.Context) {
	heading(c, a.t("Settings"), a.t("Scan roots, excludes, protected projects, and language are stored in this app's data directory. The previous settings.json file is left as it was."))
	ui.Text(c, a.t("Scan roots")).FontWeight(620)
	if len(a.Prefs.Roots) == 0 {
		ui.Text(c, a.t("No folders yet.")).TextColor(c.Theme().TextMuted)
	}
	for _, root := range a.Prefs.Roots {
		ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
			ui.Text(c, root).Font("monospace").FontSize(12).Grow(1)
			if ui.Button(c.Key("remove-root:"+root), a.t("Remove")).Clicked() {
				a.RemoveRoot(root)
			}
		})
	}
	ui.Text(c, a.t("Excluded directory names")).FontWeight(620)
	ui.TextArea(c, &a.ExcludeDraft).Height(80)
	if ui.Button(c, a.t("Save excludes")).Clicked() {
		a.saveExcludes()
	}
	ui.Text(c, a.t("Protected projects")).FontWeight(620)
	ui.Text(c, a.t("Cleanup refuses these paths, their parents, and their contents.")).FontSize(12).TextColor(c.Theme().TextMuted)
	for _, path := range a.Prefs.Protected {
		ui.Text(c, path).Font("monospace").FontSize(12)
	}
	ui.Row(c).Gap(8).Children(func() {
		ui.TextInput(c, &a.ProtectedDraft).Placeholder(a.t("Absolute path")).Grow(1)
		if ui.Button(c, a.t("Protect")).Clicked() {
			a.addProtected()
		}
	})
	if ui.Checkbox(c, &a.Prefs.UseTrash, a.t("Move files to Trash")).Changed() {
		a.persist()
	}
	if ui.Checkbox(c, &a.Prefs.AllowPermanent, a.t("Allow permanent deletion")).Changed() {
		a.persist()
	}
	if ui.Checkbox(c, &a.Prefs.ScanOnLaunch, a.t("Scan the first folder when Envark opens")).Changed() {
		a.persist()
	}
	ui.Text(c, a.t("Language")).FontWeight(620)
	ui.Text(c, a.t("system follows the operating system. en is English. zh is Simplified Chinese.")).FontSize(12).TextColor(c.Theme().TextMuted)
	if ui.Select(c, &a.Prefs.Language, []string{"system", "en", "zh"}).Changed() {
		a.applyLanguage()
		a.persist()
	}
	ui.Text(c, a.t("Appearance")).FontWeight(620)
	if ui.Select(c, &a.Prefs.Theme, []string{"system", "light", "dark"}).Changed() {
		a.persist()
		a.applyTheme()
	}
	if ui.Checkbox(c, &a.Prefs.ReduceMotion, a.t("Reduce motion")).Changed() {
		a.persist()
	}
	ui.Text(c, a.t("Renderer")).FontWeight(620)
	ui.Text(c, a.rendererLine()).FontSize(12).TextColor(c.Theme().TextMuted)
	a.shaderSample(c)
	ui.Text(c, a.t("Data directory")).FontWeight(620)
	ui.Text(c, a.Dir).Font("monospace").FontSize(12)
}

func (a *App) overlays(c *ui.Context) {
	if a.Help {
		ui.Modal(c, &a.Help, func() {
			ui.Column(c).Padding(16).Gap(8).MaxWidth(420).Children(func() {
				ui.Text(c, a.t("Keyboard")).FontSize(18).FontWeight(640)
				for _, line := range []string{
					"Ctrl/Cmd 1–5 switch pages",
					"Ctrl/Cmd R scans, or refreshes Environments",
					"Ctrl/Cmd F shows Space",
					"Escape cancels a scan",
					"Up and Down move in the table",
					"Left and Right collapse and expand a row",
				} {
					ui.Text(c, a.t(line)).FontSize(13)
				}
			})
		})
	}
	open := a.Confirm != ""
	ui.Modal(c, &open, func() {
		ui.Column(c).Padding(16).Gap(10).MaxWidth(520).Children(func() {
			switch a.Confirm {
			case "permanent":
				ui.Text(c, a.t("Delete permanently?")).FontSize(18).FontWeight(640)
				ui.Text(c, a.t("These directories will be deleted. Trash will not have them. This cannot be undone.")).TextColor(c.Theme().Danger)
				a.confirmPaths(c)
				if ui.Button(c, a.t("Delete permanently now")).Clicked() {
					a.runFiles(true)
				}
			case "commands":
				ui.Text(c, a.t("Run manager commands?")).FontSize(18).FontWeight(640)
				ui.Text(c, a.t("Each command runs as a separate argument list. Envark will not report bytes freed.")).FontSize(13)
				for _, cmd := range a.Commands {
					ui.Text(c, cmd.Display()).Font("monospace").FontSize(12)
					if cmd.Hint != "" {
						ui.Text(c, a.t(cmd.Hint)).FontSize(12)
					}
				}
				if ui.PrimaryButton(c, a.t("Run commands")).Clicked() {
					a.runCommands()
				}
			default:
				ui.Text(c, a.t("Move to Trash?")).FontSize(18).FontWeight(640)
				ui.Text(c, a.t("The directories below move to Trash. You can restore them from History while the trash path still exists.")).FontSize(13)
				a.confirmPaths(c)
				if ui.PrimaryButton(c, a.t("Confirm move to Trash")).Clicked() {
					a.runFiles(false)
				}
			}
			if ui.Button(c, a.t("Back")).Clicked() {
				a.Confirm = ""
			}
		})
	})
	if !open && a.Confirm != "" {
		a.Confirm = ""
	}
}

func (a *App) confirmPaths(c *ui.Context) {
	resolved, _ := a.reviewTargets()
	limit := resolved.Targets
	extra := 0
	if len(limit) > 12 {
		extra = len(limit) - 12
		limit = limit[:12]
	}
	for _, target := range limit {
		ui.Text(c, target.Path).Font("monospace").FontSize(12)
	}
	if extra > 0 {
		ui.Textf(c, "and %d more", extra).FontSize(12)
	}
}

func (a *App) pathSelected(path string) bool {
	sel := a.outline.List.Selected
	if sel == nil || *sel < 0 || *sel >= a.outline.Rows() {
		return false
	}
	return a.outline.Item(*sel) == path
}

func riskColor(theme *ui.Theme, risk string) ui.Color {
	switch risk {
	case "low":
		return theme.Success
	case "caution":
		return theme.Warning
	default:
		return theme.TextMuted
	}
}

func emptyDash(value string) string {
	if strings.TrimSpace(value) == "" {
		return "—"
	}
	return value
}

func parentOf(path string) string {
	slash := strings.LastIndex(path, "/")
	backslash := strings.LastIndex(path, `\`)
	i := slash
	if backslash > i {
		i = backslash
	}
	if i > 0 {
		return path[:i]
	}
	return path
}
