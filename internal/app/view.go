package app

import (
	"fmt"
	"strings"

	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/human"
	"github.com/felixpy/envark/internal/scan"
)

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
			ui.Text(c, "Runtimes and disk").FontSize(12).TextColor(theme.TextMuted)
		})
		page := a.Page
		if ui.Sidebar(c, &page, func() {
			ui.SidebarItem(c, "space", nil, "Space")
			ui.SidebarItem(c, "environments", nil, "Environments")
			ui.SidebarItem(c, "review", nil, "Review").Children(func() {
				n := len(a.Marks) + len(a.Extras) + len(a.Commands)
				if n > 0 {
					ui.Badge(c, fmt.Sprint(n))
				}
			})
			ui.SidebarItem(c, "history", nil, "History")
			ui.SidebarItem(c, "settings", nil, "Settings")
		}).Grow(1).Changed() {
			a.Page = page
		}
		if ui.Button(c, "Keyboard").Clicked() {
			a.Help = true
		}
	})
}

func (a *App) topnav(c *ui.Context) {
	ui.Row(c).Padding(8, 12).Gap(6).Children(func() {
		for _, item := range []struct{ id, label string }{
			{"space", "Space"},
			{"environments", "Environments"},
			{"review", "Review"},
			{"history", "History"},
			{"settings", "Settings"},
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
	heading(c, "Space", "File lengths and allocated blocks, for the folder you chose.")
	a.spaceToolbar(c, width)
	if a.Tree == nil && !a.Scanning {
		a.emptySpace(c)
		return
	}
	if a.Result != nil && !a.Result.Tree.Complete {
		banner(c, a.partialText(), c.Theme().Surface, c.Theme().Text)
	}
	if width < 980 {
		ui.Column(c).Grow(1).Gap(12).Children(func() {
			a.tree(c, width)
			a.detail(c)
		})
		return
	}
	ui.SplitPercent(c, &a.split, func() { a.tree(c, width) }, func() { a.detail(c) }).Grow(1)
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
		if ui.Button(c, "Add folder").Clicked() {
			a.pickRoot()
		}
		if ui.PrimaryButton(c, "Scan").Disabled(a.Scanning).Clicked() {
			a.startScan()
		}
		if ui.Button(c, "Cancel scan").Disabled(!a.Scanning).Clicked() {
			a.cancelScan()
		}
	}
	filters := func() {
		ui.SearchField(c, &a.Query).Placeholder("Filter by name").Width(180)
		ui.Segmented(c, &a.Filter, "All", "Reclaimable")
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
		ui.Text(c, "Choose a folder to scan").FontSize(18).FontWeight(620)
		ui.Text(c, "Envark measures only the folders you add. It does not walk your home directory until you say so.").FontSize(13).TextColor(theme.TextMuted).MaxWidth(460)
		if a.ScanErr != "" {
			ui.Text(c, a.ScanErr).FontSize(13).TextColor(theme.Danger)
		}
	})
}

func (a *App) partialText() string {
	msg := "Some paths could not be read. Sizes are incomplete."
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
		{Title: "Name"},
		{Title: "Kind", Width: kind},
		{Title: "Logical", Width: logical, Align: ui.End},
		{Title: "Allocated", Width: allocated, Align: ui.End},
		{Title: "Risk", Width: risk},
	}
	roots := []string{}
	if a.Tree != nil && a.visible[a.Tree.Path] {
		roots = []string{a.Tree.Path}
	}
	if len(roots) == 0 {
		ui.Text(c, "Nothing in this filter.").TextColor(c.Theme().TextMuted)
		return
	}
	a.outline.List.Selected = &a.row
	ui.OutlineTable(c, &a.outline, cols, roots, a.children, func(path string, col int) {
		n := a.index[path]
		if n == nil {
			return
		}
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
			ui.Text(c, n.Class.Title).SingleLine().TextColor(c.Theme().TextMuted)
		case 2:
			ui.Text(c, human.Bytes(n.Logical)).Font("monospace")
		case 3:
			label := human.Bytes(n.Allocated)
			if a.Result != nil && !a.Result.AllocExact {
				label = "n/a"
			}
			ui.Text(c, label).Font("monospace").TextColor(c.Theme().TextMuted)
		case 4:
			ui.Text(c, classify.TitleRisk(n.Class.Risk)).TextColor(riskColor(c.Theme(), n.Class.Risk))
		}
	}).Grow(1)
	if a.outline.Rows() > 0 {
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
		ui.Text(c, "Details").FontSize(13).FontWeight(620)
		if n == nil {
			ui.Text(c, "Select a row.").TextColor(theme.TextMuted)
			return
		}
		ui.Text(c, n.Name).FontSize(16).FontWeight(620)
		ui.Text(c, n.Path).Font("monospace").FontSize(12)
		ui.Text(c, n.Class.Reason).FontSize(13).TextColor(theme.TextMuted)
		ui.Textf(c, "Logical %s · Allocated %s · %d files", human.Bytes(n.Logical), human.Bytes(n.Allocated), n.Files).FontSize(12)
		if n.SmallFiles > 0 {
			ui.Textf(c, "%d smaller files are included in the size and are not listed on their own.", n.SmallFiles).FontSize(12).TextColor(theme.TextMuted)
		}
		if !n.Complete {
			ui.Text(c, "This subtree is incomplete.").FontSize(12).TextColor(theme.Warning)
		}
		if n.Class.Eligible {
			label := "Mark for review"
			if a.Marks[n.Path] {
				label = "Remove mark"
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
	ui.Text(c, "Largest entries here").FontSize(12).FontWeight(600)
	shown := n.Children
	if len(shown) > 6 {
		shown = shown[:6]
	}
	ui.Row(c).Height(10).Gap(2).Children(func() {
		var listed uint64
		for _, child := range shown {
			listed += child.Logical
		}
		for _, child := range shown {
			color := theme.Accent
			if child.Class.Eligible {
				color = theme.Warning
			}
			share := float32(child.Logical) / float32(n.Logical)
			if share < 0.02 {
				share = 0.02
			}
			ui.Box(c).Grow(share).Height(10).Radius(2).Background(color)
		}
		if n.Logical > listed {
			ui.Box(c).Grow(float32(n.Logical-listed) / float32(n.Logical)).Height(10).Radius(2).Background(theme.Border)
		}
	})
	for _, child := range shown {
		share := float64(child.Logical) / float64(n.Logical) * 100
		ui.Textf(c, "%s  %.0f%%  %s", child.Name, share, human.Bytes(child.Logical)).FontSize(12).SingleLine()
	}
}

func (a *App) environments(c *ui.Context) {
	heading(c, "Environments", "Read-only probes. Removal appears only for an allowlisted, non-default installation.")
	ui.Row(c).Gap(8).Children(func() {
		if ui.PrimaryButton(c, "Refresh").Disabled(a.EnvRunning).Clicked() {
			a.startDiscover()
		}
		if ui.Button(c, "Measure locations").Disabled(a.EnvRunning || len(a.Locations) == 0).Clicked() {
			a.measureLocations()
		}
	})
	if a.EnvRunning {
		ui.Text(c, "Working…").TextColor(c.Theme().TextMuted)
	}
	if a.EnvErr != "" {
		ui.Text(c, a.EnvErr).TextColor(c.Theme().Danger)
	}
	if len(a.Probes) == 0 {
		ui.Text(c, "No probe yet. Refresh reads the versions on this PATH.").TextColor(c.Theme().TextMuted)
	} else {
		a.probeTable(c)
	}
	ui.Text(c, "Known locations").FontSize(16).FontWeight(620)
	if len(a.Locations) == 0 {
		ui.Text(c, "Refresh also looks up cache locations. A missing tool stays missing; it is not shown as an empty cache.").TextColor(c.Theme().TextMuted)
		return
	}
	a.locationTable(c)
}

func (a *App) probeTable(c *ui.Context) {
	cols := []ui.TableColumn{
		{Title: "Ecosystem", Width: 110},
		{Title: "Version", Width: 160},
		{Title: "Manager", Width: 110},
		{Title: "Ownership", Width: 110},
		{Title: "Action"},
	}
	a.envList.Selected = &a.envRow
	ui.Table(c, &a.envList, cols, len(a.Probes), func(row, col int) {
		probe := a.Probes[row]
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
			ui.Text(c, emptyDash(probe.Manager)).SingleLine()
		case 3:
			ui.Text(c, emptyDash(probe.Ownership)).SingleLine()
		case 4:
			if probe.Removal != nil {
				if ui.Button(c.Key("probe:"+probe.Ecosystem+probe.Version), "Review removal").Clicked() {
					a.addProbe(probe)
				}
				return
			}
			ui.Text(c, probe.Unavailable).FontSize(12).TextColor(c.Theme().TextMuted)
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

func (a *App) locationTable(c *ui.Context) {
	cols := []ui.TableColumn{
		{Title: "Location", Width: 180},
		{Title: "Logical", Width: 100, Align: ui.End},
		{Title: "Status"},
	}
	a.locList.Selected = &a.locRow
	ui.Table(c, &a.locList, cols, len(a.Locations), func(row, col int) {
		loc := a.Locations[row]
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
				ui.Text(c, "Not present").TextColor(c.Theme().TextMuted)
				return
			}
			if loc.Err != "" && loc.Command == nil && !loc.Policy.Eligible {
				ui.Text(c, loc.Err).FontSize(12).TextColor(c.Theme().TextMuted)
				return
			}
			label := "Details"
			if loc.Policy.Eligible {
				label = "Add to review"
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
	heading(c, "Review", "Nothing is selected until you mark it. Nested directories count once. Manager commands do not add a byte total.")
	if len(resolved.Targets) == 0 && len(a.Commands) == 0 {
		ui.Text(c, "Nothing is marked. Mark a reclaimable directory in Space, or add a cache command from Environments.").TextColor(c.Theme().TextMuted)
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
			if ui.PrimaryButton(c, "Move to Trash").Disabled(a.Running || !a.Prefs.UseTrash).Clicked() {
				a.Confirm = "trash"
			}
			if ui.Button(c, "Delete permanently").Disabled(a.Running || !a.Prefs.AllowPermanent).Clicked() {
				a.Confirm = "permanent"
			}
		})
	}
	if len(a.Commands) > 0 {
		ui.Text(c, "Manager commands").FontSize(16).FontWeight(620)
		for _, cmd := range a.Commands {
			ui.Column(c).Padding(8, 10).Gap(2).Border(1, c.Theme().Border).Radius(8).Children(func() {
				ui.Text(c, cmd.Display()).Font("monospace").FontSize(12)
				ui.Text(c, cmd.Effect).FontSize(12).TextColor(c.Theme().TextMuted)
			})
		}
		on := a.Ack
		if ui.Checkbox(c, &on, "These commands cannot be restored from Trash.").Changed() {
			a.Ack = on
		}
		if ui.Button(c, "Run manager commands").Disabled(a.Running || !a.Ack).Clicked() {
			a.Confirm = "commands"
		}
	}
	for _, note := range resolved.Notes {
		ui.Text(c, note).FontSize(12).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) history(c *ui.Context) {
	heading(c, "History", "Each line is one item. A failed or cancelled item is not reported as removed.")
	if len(a.History) == 0 {
		ui.Text(c, "No operations yet.").TextColor(c.Theme().TextMuted)
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
				if ui.Button(c.Key("restore:"+item.TrashPath), "Restore").Clicked() {
					a.restore(item, root)
				}
			}
		})
	}
}

func (a *App) settings(c *ui.Context) {
	heading(c, "Settings", "Scan roots, name excludes, and protected projects are stored in this app's data directory. The previous settings.json file is left as it was.")
	ui.Text(c, "Scan roots").FontWeight(620)
	if len(a.Prefs.Roots) == 0 {
		ui.Text(c, "No folders yet.").TextColor(c.Theme().TextMuted)
	}
	for _, root := range a.Prefs.Roots {
		ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
			ui.Text(c, root).Font("monospace").FontSize(12).Grow(1)
			if ui.Button(c.Key("remove-root:"+root), "Remove").Clicked() {
				a.RemoveRoot(root)
			}
		})
	}
	ui.Text(c, "Excluded directory names").FontWeight(620)
	ui.TextArea(c, &a.ExcludeDraft).Height(80)
	if ui.Button(c, "Save excludes").Clicked() {
		a.saveExcludes()
	}
	ui.Text(c, "Protected projects").FontWeight(620)
	ui.Text(c, "Cleanup refuses these paths, their parents, and their contents.").FontSize(12).TextColor(c.Theme().TextMuted)
	for _, path := range a.Prefs.Protected {
		ui.Text(c, path).Font("monospace").FontSize(12)
	}
	ui.Row(c).Gap(8).Children(func() {
		ui.TextInput(c, &a.ProtectedDraft).Placeholder("Absolute path").Grow(1)
		if ui.Button(c, "Protect").Clicked() {
			a.addProtected()
		}
	})
	if ui.Checkbox(c, &a.Prefs.UseTrash, "Move files to Trash").Changed() {
		a.persist()
	}
	if ui.Checkbox(c, &a.Prefs.AllowPermanent, "Allow permanent deletion").Changed() {
		a.persist()
	}
	if ui.Checkbox(c, &a.Prefs.ScanOnLaunch, "Scan the first folder when Envark opens").Changed() {
		a.persist()
	}
	ui.Text(c, "Appearance").FontWeight(620)
	if ui.Select(c, &a.Prefs.Theme, []string{"system", "light", "dark"}).Changed() {
		a.persist()
		a.applyTheme()
	}
	ui.Text(c, "Data directory").FontWeight(620)
	ui.Text(c, a.Dir).Font("monospace").FontSize(12)
}

func (a *App) overlays(c *ui.Context) {
	if a.Help {
		ui.Modal(c, &a.Help, func() {
			ui.Column(c).Padding(16).Gap(8).MaxWidth(420).Children(func() {
				ui.Text(c, "Keyboard").FontSize(18).FontWeight(640)
				for _, line := range []string{
					"Ctrl/Cmd 1–5 switch pages",
					"Ctrl/Cmd R scans, or refreshes Environments",
					"Ctrl/Cmd F shows Space",
					"Escape cancels a scan",
					"Up and Down move in the table",
					"Left and Right collapse and expand a row",
				} {
					ui.Text(c, line).FontSize(13)
				}
			})
		})
	}
	open := a.Confirm != ""
	ui.Modal(c, &open, func() {
		ui.Column(c).Padding(16).Gap(10).MaxWidth(520).Children(func() {
			switch a.Confirm {
			case "permanent":
				ui.Text(c, "Delete permanently?").FontSize(18).FontWeight(640)
				ui.Text(c, "These directories will be deleted. Trash will not have them. This cannot be undone.").TextColor(c.Theme().Danger)
				a.confirmPaths(c)
				if ui.Button(c, "Delete permanently now").Clicked() {
					a.runFiles(true)
				}
			case "commands":
				ui.Text(c, "Run manager commands?").FontSize(18).FontWeight(640)
				ui.Text(c, "Each command runs as a separate argument list. Envark will not report bytes freed.").FontSize(13)
				for _, cmd := range a.Commands {
					ui.Text(c, cmd.Display()).Font("monospace").FontSize(12)
				}
				if ui.PrimaryButton(c, "Run commands").Clicked() {
					a.runCommands()
				}
			default:
				ui.Text(c, "Move to Trash?").FontSize(18).FontWeight(640)
				ui.Text(c, "The directories below move to Trash. You can restore them from History while the trash path still exists.").FontSize(13)
				a.confirmPaths(c)
				if ui.PrimaryButton(c, "Confirm move to Trash").Clicked() {
					a.runFiles(false)
				}
			}
			if ui.Button(c, "Back").Clicked() {
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
