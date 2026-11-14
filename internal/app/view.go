package app

import (
	"fmt"
	"strings"

	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/human"
	"github.com/felixpy/envark/internal/i18n"
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
	a.applyChrome(c)
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
		banner(c, a.t(notice), theme.Surface, theme.Text)
	}
	if a.Notice != "" {
		banner(c, a.t(a.Notice), theme.Surface, theme.Text)
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
	a.spaceChrome(c, width)
	if a.Tree == nil && !a.Scanning {
		a.emptySpace(c)
		return
	}
	if a.Result != nil && !a.Result.Tree.Complete {
		banner(c, a.partialText(), c.Theme().Surface, c.Theme().Text)
	}
	a.followSelection()
	if width < 980 {
		ui.Scroll(c).Grow(1).Children(func() {
			ui.Column(c).Gap(10).MaxWidth(contentWidth(c)).Children(func() {
				a.contextBar(c)
				a.field(c, 380)
				a.detail(c)
				a.tree(c, width, 320)
			})
		})
		return
	}
	ui.Column(c).Grow(1).Gap(10).Children(func() {
		a.contextBar(c)
		ui.Row(c).Grow(1).Gap(12).AlignItems(ui.Stretch).Children(func() {
			a.field(c, 420)
			ui.Column(c).Width(320).Gap(8).Children(func() {
				a.detail(c)
				a.tree(c, width, 0)
			})
		})
	})
}

func (a *App) spaceChrome(c *ui.Context, width float32) {
	ui.Row(c).Gap(10).AlignItems(ui.Center).Wrap().Children(func() {
		ui.Text(c, a.t("Space")).FontSize(20).FontWeight(650)
		if width >= 1100 {
			ui.Spacer(c).Grow(1)
		}
		a.spaceActions(c, width)
	})
	if a.Scanning {
		ui.Text(c, a.t("Scanning.")+" "+a.Progress).FontSize(12).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) spaceActions(c *ui.Context, width float32) {
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
			ui.Row(c).Gap(8).AlignItems(ui.Center).Wrap().Children(actions)
			ui.Row(c).Gap(8).AlignItems(ui.Center).Wrap().Children(filters)
		})
		return
	}
	ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
		actions()
		filters()
	})
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

func (a *App) contextBar(c *ui.Context) {
	node := a.mapNode()
	if node == nil {
		return
	}
	theme := c.Theme()
	sel := a.index[a.Selected]
	if sel == nil {
		sel = node
	}
	ui.Column(c).Padding(10, 12).Gap(8).Radius(10).Background(theme.Surface).Children(func() {
		ui.Row(c).Gap(8).AlignItems(ui.Center).Wrap().Children(func() {
			a.crumbs(c, node)
			ui.Spacer(c).Grow(1)
			if ui.Button(c, a.t("Up")).Disabled(a.Tree == nil || node.Path == a.Tree.Path).Clicked() {
				a.mapUp()
			}
			if ui.Button(c, a.t("Open")).Disabled(!a.canDrill(a.Selected)).Clicked() {
				a.drill(a.Selected)
			}
			if sel.Class.Eligible {
				label := a.t("Mark for review")
				if a.Marks[sel.Path] {
					label = a.t("Remove mark")
				}
				if ui.PrimaryButton(c, label).Clicked() {
					a.toggle(sel.Path)
				}
			}
		})
		ui.Row(c).Gap(10).AlignItems(ui.Center).Wrap().Children(func() {
			ui.Text(c, sel.Name).FontSize(16).FontWeight(640)
			ui.Text(c, human.Bytes(sel.Logical)+" "+a.t("logical")).Font("monospace").FontSize(13)
			label, warm := a.reclaimLabel(sel)
			color := theme.TextMuted
			if warm {
				color = ui.Hex("#6a3d16")
			}
			ui.Text(c, label).FontSize(13).TextColor(color)
			if a.MapHover != "" && a.MapHover != sel.Path {
				if hover := a.index[a.MapHover]; hover != nil {
					ui.Text(c, hover.Name+"  "+human.Bytes(hover.Logical)).FontSize(12).TextColor(theme.TextMuted)
				}
			}
		})
	})
}

func (a *App) tree(c *ui.Context, width, height float32) {
	rail := height <= 0
	// The side rail and the narrow page keep the columns that fit.
	// Kind and allocated size stay in the detail panel, so a 720px
	// window does not ellipsize "Allocated" or a long classification.
	compact := !rail && width < 980
	kind, logical, allocated, risk := float32(120), float32(96), float32(104), float32(72)
	cols := []ui.TableColumn{{Title: a.t("Name")}, {Title: a.t("Logical"), Width: logical, Align: ui.End}}
	if compact {
		cols = append(cols, ui.TableColumn{Title: a.t("Risk"), Width: risk})
	} else if !rail {
		cols = []ui.TableColumn{
			{Title: a.t("Name")},
			{Title: a.t("Kind"), Width: kind},
			{Title: a.t("Logical"), Width: logical, Align: ui.End},
			{Title: a.t("Allocated"), Width: allocated, Align: ui.End},
			{Title: a.t("Risk"), Width: risk},
		}
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
	table := ui.OutlineTable(c, &a.outline, cols, roots, a.children, func(path string, col int) {
		n := a.index[path]
		if n == nil {
			return
		}
		selected := a.pathSelected(path)
		logicalCol, kindCol, allocatedCol, riskCol := 1, -1, -1, -1
		if compact {
			riskCol = 2
		} else if !rail {
			kindCol, logicalCol, allocatedCol, riskCol = 1, 2, 3, 4
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
		case logicalCol:
			ui.Text(c, human.Bytes(n.Logical)).Font("monospace")
		default:
			if col == kindCol {
				a.tone(c, selected, a.t(n.Class.Title), c.Theme().TextMuted)
			} else if col == allocatedCol {
				label := human.Bytes(n.Allocated)
				if a.Result != nil && !a.Result.AllocExact {
					label = "n/a"
				}
				a.tone(c, selected, label, c.Theme().TextMuted)
			} else if col == riskCol {
				a.tone(c, selected, a.t(classify.TitleRisk(n.Class.Risk)), riskColor(c.Theme(), n.Class.Risk))
			}
		}
	})
	if height <= 0 {
		table.Grow(1).MinHeight(180)
	} else {
		table.Height(height)
	}
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
	if a.Result != nil && a.Tree != nil {
		note := a.t("Hard links once. Links are not followed.")
		if !a.Result.AllocExact {
			note += " " + a.t("Allocated size is not separate on this system.")
		}
		if a.Result.Excluded > 0 {
			note += fmt.Sprintf(" %d excluded.", a.Result.Excluded)
		}
		ui.Text(c, note).FontSize(12).TextColor(c.Theme().TextMuted)
	}
}

func (a *App) detail(c *ui.Context) {
	theme := c.Theme()
	n := a.index[a.Selected]
	ui.Column(c).Padding(12).Gap(6).Radius(10).Background(theme.Surface).MaxWidth(contentWidth(c)).Children(func() {
		ui.Text(c, a.t("Details")).FontSize(11).FontWeight(620).TextColor(theme.TextMuted)
		if n == nil {
			ui.Text(c, a.t("Select a row.")).TextColor(theme.TextMuted)
			return
		}
		ui.Text(c, n.Name).FontSize(15).FontWeight(640)
		ui.Text(c, n.Path).Font("monospace").FontSize(12).TextColor(theme.TextMuted)
		ui.Text(c, fmt.Sprintf("%s %s · %s %s · %s", a.t("Logical"), human.Bytes(n.Logical), a.t("Allocated"), human.Bytes(n.Allocated), fmt.Sprintf(a.t("%d files"), n.Files))).FontSize(12)
		label, warm := a.reclaimLabel(n)
		color := theme.TextMuted
		if warm {
			color = ui.Hex("#6a3d16")
		}
		ui.Text(c, label).FontSize(13).TextColor(color)
		if reason := a.t(n.Class.Reason); reason != "" {
			ui.Text(c, reason).FontSize(13)
		}
		if n.SmallFiles > 0 {
			ui.Text(c, fmt.Sprintf(a.t("%d smaller files are included in the size and are not listed on their own."), n.SmallFiles)).FontSize(12).TextColor(theme.TextMuted)
		}
		if !n.Complete {
			ui.Text(c, a.t("This subtree is incomplete.")).FontSize(12).TextColor(theme.Warning)
		}
	})
}

func (a *App) environments(c *ui.Context) {
	theme := c.Theme()
	heading(c, a.t("Environments"), a.t("fnm and uv only. This does not change the open terminal."))
	ui.Row(c).Gap(8).Wrap().Children(func() {
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
	if a.EnvRunning {
		ui.Text(c, a.t("Working…")).TextColor(theme.TextMuted)
	}
	if a.EnvErr != "" {
		ui.Text(c, a.t(a.EnvErr)).TextColor(theme.Danger)
	}
	ui.Text(c, a.t("Installed")).FontSize(13).FontWeight(640)
	if len(a.Probes) == 0 {
		ui.Text(c, a.t("No probe yet. Refresh reads the versions on this PATH.")).TextColor(theme.TextMuted)
	} else {
		if a.envRow < 0 || a.envRow >= len(a.Probes) {
			a.envRow = 0
		}
		ui.Row(c).Gap(8).Wrap().Children(func() {
			for i := range a.Probes {
				a.runtimeCard(c, i)
			}
		})
		a.runtimeDetail(c, a.envRow)
	}
	a.catalog(c)
	ui.Text(c, a.t("Known locations")).FontSize(13).FontWeight(640)
	if len(a.Locations) == 0 {
		ui.Text(c, a.t("Refresh also looks up cache locations. A missing tool stays missing; it is not shown as an empty cache.")).TextColor(theme.TextMuted)
		return
	}
	a.locationTable(c)
}

func (a *App) runtimeCard(c *ui.Context, i int) {
	probe := a.Probes[i]
	theme := c.Theme()
	selected := i == a.envRow
	border := theme.Border
	width := float32(1)
	if selected {
		border = theme.Accent
		width = 2
	}
	version := probe.Version
	if version == "" {
		version = "—"
	}
	card := ui.Column(c.Key("runtime:"+probe.Ecosystem+probe.Version+probe.Manager)).MinWidth(210).Grow(1).MaxWidth(360).Padding(12).Gap(4).Radius(10).Background(theme.Surface).Border(width, border).Focusable()
	card.Children(func() {
		ui.Text(c, probe.Ecosystem).FontSize(12).TextColor(theme.TextMuted)
		ui.Text(c, version).Font("monospace").FontSize(18).FontWeight(640)
		ui.Text(c, a.t(emptyDash(probe.Scope))+" · "+emptyDash(probe.Manager)).FontSize(12)
		if probe.Removal != nil {
			if ui.Button(c.Key("probe:"+probe.Ecosystem+probe.Version), a.t("Review removal")).Clicked() {
				a.envRow = i
				a.addProbe(probe)
			}
			return
		}
		ui.Text(c, a.t("Kept")).FontSize(12).TextColor(theme.TextMuted)
	})
	if card.Clicked() || card.Focused() {
		a.envRow = i
	}
}

func (a *App) runtimeDetail(c *ui.Context, i int) {
	if i < 0 || i >= len(a.Probes) {
		return
	}
	probe := a.Probes[i]
	theme := c.Theme()
	ui.Column(c).Padding(12).Gap(6).Radius(10).Background(theme.Surface).MaxWidth(contentWidth(c)).Children(func() {
		ui.Text(c, probe.Ecosystem+" "+emptyDash(probe.Version)).FontSize(14).FontWeight(620)
		if probe.Path != "" {
			ui.Text(c, probe.Path).Font("monospace").FontSize(12).TextColor(theme.TextMuted)
		}
		if probe.Unavailable != "" {
			ui.Text(c, a.t(probe.Unavailable)).FontSize(13)
		}
		if probe.Err != "" {
			ui.Text(c, probe.Err).FontSize(12).TextColor(theme.Danger)
		}
		if probe.Detail != "" {
			ui.Text(c, a.t(probe.Detail)).FontSize(12).TextColor(theme.TextMuted)
		}
	})
}

func (a *App) catalog(c *ui.Context) {
	theme := c.Theme()
	ui.Text(c, a.t("Available versions")).FontSize(13).FontWeight(640)
	ui.Text(c, a.t("Lists versions. It does not install.")).FontSize(12).TextColor(theme.TextMuted)
	for _, note := range a.CatalogNotes {
		ui.Text(c, a.t(note)).FontSize(12)
	}
	if len(a.Catalog) == 0 {
		ui.Text(c, a.t("Nothing listed yet.")).FontSize(12).TextColor(theme.TextMuted)
	} else {
		ui.Row(c).Gap(8).Wrap().Children(func() {
			for i := range a.Catalog {
				a.versionCard(c, i)
			}
		})
	}
	a.pinForm(c)
}

func (a *App) versionCard(c *ui.Context, i int) {
	item := a.Catalog[i]
	theme := c.Theme()
	ui.Column(c.Key("ver:"+item.Manager+item.Version)).MinWidth(210).Grow(1).MaxWidth(360).Padding(12).Gap(6).Radius(10).Background(theme.Surface).Border(1, theme.Border).Children(func() {
		ui.Text(c, item.Ecosystem+" · "+a.t(item.Scope)).FontSize(12).TextColor(theme.TextMuted)
		ui.Text(c, item.Version).Font("monospace").FontSize(16).FontWeight(620)
		ui.Text(c, item.Manager).FontSize(12)
		ui.Row(c).Gap(6).Wrap().Children(func() {
			if item.Install != nil && ui.Button(c.Key("inst:"+item.Manager+item.Version), a.t("Review install")).Clicked() {
				a.addCommand(*item.Install)
			}
			if item.MakeDefault != nil && ui.Button(c.Key("def:"+item.Manager+item.Version), a.t("Make default")).Clicked() {
				a.addCommand(*item.MakeDefault)
			}
			if item.Removal != nil && ui.Button(c.Key("rm:"+item.Manager+item.Version), a.t("Review removal")).Clicked() {
				a.addCommand(*item.Removal)
			}
		})
		if item.Install == nil && item.MakeDefault == nil && item.Removal == nil && item.Hint != "" {
			ui.Text(c, a.t(item.Hint)).FontSize(12).TextColor(theme.TextMuted)
		}
	})
}

func (a *App) pinForm(c *ui.Context) {
	theme := c.Theme()
	ui.Column(c).Padding(12).Gap(8).Radius(10).Background(theme.Surface).Border(1, theme.Border).MaxWidth(contentWidth(c)).Children(func() {
		ui.Text(c, a.t("Project pin")).FontSize(13).FontWeight(640)
		ui.Text(c, a.t("Writes .python-version in the directory below. The open terminal stays as it is.")).FontSize(12).TextColor(theme.TextMuted)
		ui.TextInput(c, &a.PinDir).Placeholder(a.t("Project directory"))
		ui.TextInput(c, &a.PinVersion).Placeholder(a.t("Python version"))
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
				a.tone(c, selected, a.t("Note"), c.Theme().TextMuted)
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
	if a.locRow >= 0 && a.locRow < len(a.Locations) {
		loc := a.Locations[a.locRow]
		if loc.Err != "" {
			ui.Text(c, loc.Err).FontSize(12).TextColor(c.Theme().TextMuted)
		} else if loc.Path != "" {
			ui.Text(c, loc.Path).Font("monospace").FontSize(12).TextColor(c.Theme().TextMuted)
		}
	}
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
				ui.Text(c, a.t(item.Detail)).FontSize(12).TextColor(c.Theme().TextMuted)
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

func (a *App) applyChrome(c *ui.Context) {
	base := c.Theme()
	if base == nil {
		return
	}
	next := graphiteTheme(base.Dark)
	if base.FontSize > 0 {
		next.FontSize = base.FontSize
	}
	next.Font = base.Font
	c.SetTheme(next)
}

func contentWidth(c *ui.Context) float32 {
	w, _ := c.Size()
	inset := float32(56)
	if w >= 760 {
		inset += 216
	}
	if w-inset < 300 {
		return 300
	}
	return w - inset
}

func graphiteTheme(dark bool) *ui.Theme {
	if dark {
		t := ui.DarkTheme()
		t.Background = ui.Hex("#141311")
		t.Surface = ui.Hex("#221f1c")
		t.SurfaceHover = ui.Hex("#2c2824")
		t.SurfacePressed = ui.Hex("#38332e")
		t.Border = ui.Hex("#3a342e")
		t.Text = ui.Hex("#f3efe8")
		t.TextMuted = ui.Hex("#a39990")
		t.Accent = ui.Hex("#e4d2b0")
		t.AccentHover = ui.Hex("#efe3c8")
		t.AccentPressed = ui.Hex("#d3be96")
		t.AccentText = ui.Hex("#1c1a17")
		t.InactiveAccent = t.Accent
		t.Danger = ui.Hex("#e07a72")
		t.Warning = ui.Hex("#e0b15a")
		t.Success = ui.Hex("#8fbf9a")
		t.Selection = ui.RGBA(228, 210, 176, 0.28)
		t.Focus = ui.RGBA(228, 210, 176, 0.7)
		t.Radius = 8
		return t
	}
	t := ui.LightTheme()
	t.Background = ui.Hex("#f3f1ec")
	t.Surface = ui.Hex("#e7e2d8")
	t.SurfaceHover = ui.Hex("#dbd6cb")
	t.SurfacePressed = ui.Hex("#d0cbc0")
	t.Border = ui.Hex("#d5cfc3")
	t.Text = ui.Hex("#1c1a17")
	t.TextMuted = ui.Hex("#6f675e")
	t.Accent = ui.Hex("#24312c")
	t.AccentHover = ui.Hex("#1b2622")
	t.AccentPressed = ui.Hex("#141c19")
	t.AccentText = ui.Hex("#f6f3ec")
	t.InactiveAccent = t.Accent
	t.Danger = ui.Hex("#9f3a32")
	t.Warning = ui.Hex("#9a6230")
	t.Success = ui.Hex("#3d6b4f")
	t.Selection = ui.RGBA(36, 49, 44, 0.16)
	t.Focus = ui.RGBA(36, 49, 44, 0.55)
	t.Radius = 8
	return t
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
