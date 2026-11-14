package app

import (
	"fmt"
	"time"

	"github.com/egoist/mygo/ui"
	"github.com/egoist/mygo/ui/shader"

	"github.com/felixpy/envark/internal/human"
	"github.com/felixpy/envark/internal/scan"
)

// fieldShader paints the space map from the real cell rectangles. It does
// not invent sizes: a pixel takes the color of the cell that contains it.
var fieldShader = shader.MustCompile(`package main

var Cells [12]vec4
var Ink [12]vec4
var Flag [12]float
var CellCount int
var Sweep float

func Fragment(dst vec4, pos vec2) vec4 {
	paper := vec4(0.965, 0.957, 0.941, 1)
	hit := -1
	for i := 0; i < 12; i++ {
		if i >= CellCount {
			continue
		}
		r := Cells[i]
		if pos.x >= r.x && pos.y >= r.y && pos.x < r.x+r.z && pos.y < r.y+r.w {
			hit = i
		}
	}
	if hit < 0 {
		return paper
	}
	r := Cells[hit]
	inset := float(4)
	if pos.x < r.x+inset || pos.y < r.y+inset || pos.x > r.x+r.z-inset || pos.y > r.y+r.w-inset {
		return paper
	}
	col := Ink[hit].rgb
	edge := min(min(pos.x-(r.x+inset), (r.x+r.z-inset)-pos.x), min(pos.y-(r.y+inset), (r.y+r.w-inset)-pos.y))
	flag := Flag[hit]
	if flag == 1 {
		col = mix(col, vec3(0.1, 0.09, 0.08), 0.08)
	}
	if (flag == 2 || flag == 4) && edge < 2.4 {
		col = vec3(0.11, 0.098, 0.086)
	}
	if flag == 3 || flag == 4 {
		band := mod(pos.x+pos.y, 7)
		if band < 1.2 {
			col = mix(col, vec3(0.45, 0.28, 0.18), 0.45)
		}
	}
	if Sweep > 0 {
		at := pos.x / max(rectSize().x, float(1))
		if abs(at-Sweep) < 0.012 {
			col = mix(col, vec3(0.11, 0.098, 0.086), 0.5)
		}
	}
	return vec4(col, 1)
}
`)

// scopeShader paints one runtime's state. The mark is a fixed size; it is
// not a disk measurement.
var scopeShader = shader.MustCompile(`package main

var Mode float
var Fill vec4

func Fragment(dst vec4, pos vec2) vec4 {
	size := rectSize()
	c := pos - size*0.5
	d := length(c) / max(min(size.x, size.y)*0.5, float(1))
	ink := Fill.rgb
	if Mode < 0.5 {
		if d > 0.72 && d < 0.98 {
			return vec4(ink, 1)
		}
		return vec4(0, 0, 0, 0)
	}
	if d > 0.98 {
		return vec4(0, 0, 0, 0)
	}
	if Mode > 1.5 && d < 0.34 {
		return vec4(0.11, 0.098, 0.086, 1)
	}
	return vec4(ink, 1)
}
`)

const (
	fieldPaper = "#f6f4f0"
	inkDir     = "#d9d3c7"
	inkFile    = "#c5d0c6"
	inkLink    = "#d4d4d8"
	inkLow     = "#ead9b0"
	inkCaution = "#e4c4b8"
	inkText    = "#1c1917"
)

// mapCell is one visible block of the current directory.
type mapCell struct {
	Path       string
	Name       string
	Logical    uint64
	Kind       string
	Incomplete bool
	Rest       int
	Ink        ui.Color
	X, Y, W, H float32
}

// Renderer is what painted the space map on the last frame.
type Renderer struct {
	Backend string
	GPU     bool
	Shader  bool
}

func (a *App) mapNode() *scan.Entry {
	if a.Tree == nil {
		return nil
	}
	if a.MapDir != "" {
		if n := a.index[a.MapDir]; n != nil {
			return n
		}
	}
	return a.Tree
}

func (a *App) followSelection() {
	if a.Tree == nil {
		return
	}
	if a.MapDir == "" {
		a.MapDir = a.Tree.Path
	}
	sel := a.index[a.Selected]
	node := a.index[a.MapDir]
	if sel == nil || node == nil {
		return
	}
	if sel.Path == node.Path {
		return
	}
	for _, child := range node.Children {
		if child.Path == sel.Path {
			return
		}
	}
	parent := parentOf(sel.Path)
	if a.index[parent] != nil {
		a.MapDir = parent
		return
	}
	a.MapDir = a.Tree.Path
}

func (a *App) drill(path string) {
	n := a.index[path]
	if n == nil || !n.Dir || n.Link || len(n.Children) == 0 {
		return
	}
	a.MapDir = path
	a.Selected = path
	a.wantPath = path
	a.drillRun++
}

func (a *App) mapUp() {
	node := a.mapNode()
	if node == nil || a.Tree == nil || node.Path == a.Tree.Path {
		return
	}
	parent := parentOf(node.Path)
	if a.index[parent] == nil {
		parent = a.Tree.Path
	}
	a.MapDir = parent
	a.Selected = node.Path
	a.wantPath = node.Path
	a.drillRun++
}

func (a *App) selectPath(path string) {
	if path == "" || a.index[path] == nil {
		return
	}
	a.Selected = path
	a.wantPath = path
}

func (a *App) field(c *ui.Context) {
	node := a.mapNode()
	if node == nil {
		return
	}
	a.crumbs(c, node)
	ui.Row(c).Gap(8).Children(func() {
		if ui.Button(c, a.t("Up")).Disabled(a.Tree == nil || node.Path == a.Tree.Path).Clicked() {
			a.mapUp()
		}
		if ui.Button(c, a.t("Open")).Disabled(!a.canDrill(a.Selected)).Clicked() {
			a.drill(a.Selected)
		}
	})
	zoom := float32(1)
	box := ui.Box(c).MinHeight(280).Grow(1).Background(ui.Hex(fieldPaper)).Radius(8).Focusable()
	if a.drillRun > 0 {
		zoom = box.Keyframes(a.drillRun, []ui.Keyframe{
			{Value: 0.94},
			{At: 180 * time.Millisecond, Value: 1},
		}, ui.KeyframeOptions{Run: a.drillRun})
	}
	box.Draw(func(p *ui.Painter, r ui.Rect) {
		a.paintField(p, r, node, zoom)
		if a.shownBackend != a.Paint.Backend {
			a.shownBackend = a.Paint.Backend
			c.Invalidate()
		}
	})
	if box.Hovered() {
		x, y, over := box.PointerPosition()
		if over {
			a.MapHover = hitMap(a.lastMap, x, y)
		}
	} else if a.MapHover != "" {
		a.MapHover = ""
	}
	if box.Clicked() || box.DoubleClicked() {
		x, y, _ := box.PointerPosition()
		path := hitMap(a.lastMap, x, y)
		if box.DoubleClicked() && a.canDrill(path) {
			a.drill(path)
		} else if path != "" {
			a.selectPath(path)
		}
	}
	ui.Text(c, a.t("Area is logical size. A warmer cell has more reclaimable bytes inside it.")).FontSize(12).TextColor(c.Theme().TextMuted)
	ui.Text(c, a.rendererLine()).FontSize(12).TextColor(c.Theme().TextMuted)
}

func (a *App) canDrill(path string) bool {
	n := a.index[path]
	return n != nil && n.Dir && !n.Link && len(n.Children) > 0
}

func (a *App) crumbs(c *ui.Context, node *scan.Entry) {
	ui.Row(c).Gap(4).Children(func() {
		cur := node
		var chain []*scan.Entry
		for cur != nil {
			chain = append(chain, cur)
			if a.Tree != nil && cur.Path == a.Tree.Path {
				break
			}
			parent := a.index[parentOf(cur.Path)]
			if parent == nil || parent == cur {
				break
			}
			cur = parent
		}
		for i := len(chain) - 1; i >= 0; i-- {
			entry := chain[i]
			label := entry.Name
			if i != len(chain)-1 {
				ui.Text(c, "/").TextColor(c.Theme().TextMuted)
			}
			if ui.Button(c.Key("crumb:"+entry.Path), label).Clicked() {
				a.MapDir = entry.Path
				a.Selected = entry.Path
				a.wantPath = entry.Path
				a.drillRun++
			}
		}
	})
}

func (a *App) paintField(p *ui.Painter, r ui.Rect, node *scan.Entry, zoom float32) {
	cells := a.cellsFor(node, r)
	a.Paint.GPU = p.Accelerated()
	shaderOK := p.Accelerated() || r.W*r.H <= 24000
	a.Paint.Shader = shaderOK && len(cells) > 0
	if a.Paint.GPU && a.Paint.Shader {
		a.Paint.Backend = "gpu-shader"
	} else if a.Paint.Shader {
		a.Paint.Backend = "cpu-shader"
	} else {
		a.Paint.Backend = "cpu-rectangles"
	}
	if zoom > 0 && zoom < 1 {
		inset := (1 - zoom) * 0.5
		r = ui.Rect{X: r.X + r.W*inset, Y: r.Y + r.H*inset, W: r.W * zoom, H: r.H * zoom}
		cells = a.cellsFor(node, r)
	}
	sweep := float32(0)
	if a.Scanning && !a.Prefs.ReduceMotion && p.Accelerated() {
		sweep = float32(p.Now().UnixNano()%1_000_000_000) / 1_000_000_000
		p.AnimationFrameRate(30)
	}
	if a.Paint.Shader {
		a.drawFieldShader(p, r, cells, sweep)
	} else {
		a.drawFieldRects(p, cells)
	}
	a.lastMap = relativeCells(cells, r)
	for _, cell := range cells {
		if cell.W < 72 || cell.H < 32 {
			continue
		}
		p.Text(cell.X+8, cell.Y+8, cell.Name, 13, ui.Hex(inkText))
		p.Text(cell.X+8, cell.Y+26, human.Bytes(cell.Logical), 11, ui.Hex("#44403c"))
	}
}

func (a *App) cellsFor(node *scan.Entry, r ui.Rect) []mapCell {
	if node == nil {
		return nil
	}
	var sized []*scan.Entry
	for _, child := range node.Children {
		if child.Logical > 0 {
			sized = append(sized, child)
		}
	}
	const maxCells = 12
	restN := 0
	var restLogical uint64
	if len(sized) > maxCells {
		for _, child := range sized[maxCells-1:] {
			restN++
			restLogical += child.Logical
		}
		sized = sized[:maxCells-1]
	}
	items := make([]mapWeight, 0, len(sized)+1)
	cells := make([]mapCell, 0, len(sized)+1)
	for _, child := range sized {
		items = append(items, mapWeight{Key: child.Path, Weight: float64(child.Logical)})
		cells = append(cells, mapCell{
			Path: child.Path, Name: child.Name, Logical: child.Logical,
			Kind: cellKind(child), Incomplete: !child.Complete, Ink: cellInk(child),
		})
	}
	if restN > 0 {
		items = append(items, mapWeight{Key: node.Path + "/*rest", Weight: float64(restLogical)})
		cells = append(cells, mapCell{
			Name: fmt.Sprintf("%d more", restN), Logical: restLogical, Kind: "dir", Rest: restN, Ink: ui.Hex(inkDir),
		})
	}
	rects := squarify(items, mapRect{X: r.X, Y: r.Y, W: r.W, H: r.H})
	for i := range cells {
		if i >= len(rects) {
			break
		}
		cells[i].X, cells[i].Y, cells[i].W, cells[i].H = rects[i].X, rects[i].Y, rects[i].W, rects[i].H
	}
	return cells
}

func cellKind(e *scan.Entry) string {
	if e == nil {
		return "dir"
	}
	if e.Link {
		return "link"
	}
	switch e.Class.Risk {
	case "caution":
		return "caution"
	case "low":
		return "low"
	}
	if !e.Dir {
		return "file"
	}
	return "dir"
}

func cellInk(e *scan.Entry) ui.Color {
	switch cellKind(e) {
	case "file":
		return ui.Hex(inkFile)
	case "link":
		return ui.Hex(inkLink)
	case "low":
		return ui.Hex(inkLow)
	case "caution":
		return ui.Hex(inkCaution)
	default:
		return mixInk(ui.Hex(inkDir), ui.Hex(inkLow), reclaimShare(e))
	}
}

func reclaimShare(e *scan.Entry) float32 {
	if e == nil || e.Logical == 0 {
		return 0
	}
	if e.Class.Eligible {
		return 1
	}
	var marked uint64
	var walk func(*scan.Entry)
	walk = func(n *scan.Entry) {
		if n.Class.Eligible {
			marked += n.Logical
			return
		}
		for _, child := range n.Children {
			walk(child)
		}
	}
	walk(e)
	if marked > e.Logical {
		marked = e.Logical
	}
	return float32(marked) / float32(e.Logical)
}

func mixInk(a, b ui.Color, t float32) ui.Color {
	if t < 0 {
		t = 0
	}
	if t > 1 {
		t = 1
	}
	m := 1 - t
	return ui.Color{
		R: uint8(float32(a.R)*m + float32(b.R)*t),
		G: uint8(float32(a.G)*m + float32(b.G)*t),
		B: uint8(float32(a.B)*m + float32(b.B)*t),
		A: 255,
	}
}

func inkOf(kind string) ui.Color {
	switch kind {
	case "file":
		return ui.Hex(inkFile)
	case "link":
		return ui.Hex(inkLink)
	case "low":
		return ui.Hex(inkLow)
	case "caution":
		return ui.Hex(inkCaution)
	default:
		return ui.Hex(inkDir)
	}
}

func relativeCells(cells []mapCell, r ui.Rect) []mapCell {
	out := append([]mapCell(nil), cells...)
	for i := range out {
		out[i].X -= r.X
		out[i].Y -= r.Y
	}
	return out
}

func (a *App) drawFieldShader(p *ui.Painter, r ui.Rect, cells []mapCell, sweep float32) {
	var cellsU, inkU [12 * 4]float32
	var flagU [12]float32
	n := len(cells)
	if n > 12 {
		n = 12
	}
	for i := 0; i < n; i++ {
		cell := cells[i]
		base := i * 4
		cellsU[base] = cell.X - r.X
		cellsU[base+1] = cell.Y - r.Y
		cellsU[base+2] = cell.W
		cellsU[base+3] = cell.H
		col := cell.Ink
		if col.A == 0 {
			col = inkOf(cell.Kind)
		}
		inkU[base] = float32(col.R) / 255
		inkU[base+1] = float32(col.G) / 255
		inkU[base+2] = float32(col.B) / 255
		inkU[base+3] = 1
		flag := float32(0)
		if cell.Path != "" && cell.Path == a.MapHover {
			flag = 1
		}
		if cell.Path != "" && cell.Path == a.Selected && cell.Rest == 0 {
			flag = 2
		}
		if cell.Incomplete {
			if flag == 2 {
				flag = 4
			} else {
				flag = 3
			}
		}
		flagU[i] = flag
	}
	fieldShader.Draw(p, r, shader.Options{Uniforms: map[string]any{
		"Cells":     cellsU[:],
		"Ink":       inkU[:],
		"Flag":      flagU[:],
		"CellCount": n,
		"Sweep":     sweep,
	}})
}

func (a *App) drawFieldRects(p *ui.Painter, cells []mapCell) {
	for _, cell := range cells {
		if cell.W < 1 || cell.H < 1 {
			continue
		}
		pad := float32(4)
		rect := ui.Rect{X: cell.X + pad, Y: cell.Y + pad, W: cell.W - pad*2, H: cell.H - pad*2}
		if rect.W < 2 || rect.H < 2 {
			rect = ui.Rect{X: cell.X, Y: cell.Y, W: cell.W, H: cell.H}
		}
		col := cell.Ink
		if col.A == 0 {
			col = inkOf(cell.Kind)
		}
		p.Fill(rect, col, 4)
		if cell.Path == a.Selected && cell.Rest == 0 {
			p.Stroke(rect, ui.Hex(inkText), 4, 2)
		}
		if cell.Incomplete {
			p.StrokeDashed(ui.Rect{X: cell.X + 4, Y: cell.Y + 4, W: cell.W - 8, H: cell.H - 8}, ui.Hex("#9a3412"), 3, 1)
		}
	}
}

func (a *App) rendererLine() string {
	switch a.Paint.Backend {
	case "gpu-shader":
		return a.t("Map: GPU shader. Cell area is logical size.")
	case "cpu-shader":
		return a.t("Map: shader interpreted on the CPU. Cell area is logical size. This process has no GPU renderer.")
	case "cpu-rectangles":
		return a.t("Map: CPU rectangles. The same cells are drawn by the shader when a GPU renderer is present. Cell area is logical size.")
	default:
		return a.t("Map: not drawn yet.")
	}
}

func (a *App) scopeMark(c *ui.Context, key, mode string) {
	fill := ui.Hex(inkDir)
	modeN := float32(0)
	switch mode {
	case "installed":
		fill = ui.Hex(inkFile)
		modeN = 1
	case "default":
		fill = ui.Hex(inkLow)
		modeN = 2
	case "missing":
		fill = ui.Hex("#a8a29e")
	}
	ui.Box(c.Key(key)).Size(18, 18).Draw(func(p *ui.Painter, r ui.Rect) {
		if p.Accelerated() || r.W*r.H <= 800 {
			scopeShader.Draw(p, r, shader.Options{Uniforms: map[string]any{"Mode": modeN, "Fill": fill}})
			if !a.Paint.GPU {
				a.Paint.GPU = p.Accelerated()
			}
			return
		}
		p.Fill(r, fill, r.W/2)
	})
}

func (a *App) scopeRail(c *ui.Context) {
	if len(a.Probes) == 0 {
		return
	}
	ui.Text(c, a.t("Scope marks are state, not disk size.")).FontSize(12).TextColor(c.Theme().TextMuted)
	ui.Row(c).Gap(10).Children(func() {
		for _, probe := range a.Probes {
			mode := probe.Scope
			if mode == "" {
				mode = "missing"
			}
			ui.Row(c).Gap(6).AlignItems(ui.Center).Children(func() {
				a.scopeMark(c, "scope:"+probe.Ecosystem+probe.Version+mode, mode)
				ui.Text(c, probe.Ecosystem+" "+a.t(mode)).FontSize(12).SingleLine()
			})
		}
	})
}

func (a *App) shaderSample(c *ui.Context) {
	ui.Text(c, a.t("Shader sample")).FontWeight(620)
	ui.Text(c, a.t("A small map drawn by the shader, including on the CPU interpreter. The main map uses it when a GPU renderer is present or the area stays small.")).FontSize(12).TextColor(c.Theme().TextMuted)
	ui.Box(c).Size(220, 88).Background(ui.Hex(fieldPaper)).Radius(8).Draw(func(p *ui.Painter, r ui.Rect) {
		demo := []mapCell{
			{Name: "api", Kind: "low", X: r.X, Y: r.Y, W: r.W * 0.5, H: r.H, Logical: 6, Path: "/api"},
			{Name: "web", Kind: "file", X: r.X + r.W*0.5, Y: r.Y, W: r.W * 0.3, H: r.H, Logical: 3, Path: "/web"},
			{Name: "docs", Kind: "caution", X: r.X + r.W*0.8, Y: r.Y, W: r.W * 0.2, H: r.H, Logical: 1, Path: "/docs"},
		}
		demo[0].Path = a.Selected
		if a.Selected == "" {
			demo[0].Path = "/api"
		}
		a.drawFieldShader(p, r, demo, 0)
		if !p.Accelerated() {
			a.Paint.Backend = "cpu-shader"
		} else {
			a.Paint.Backend = "gpu-shader"
			a.Paint.GPU = true
		}
		a.Paint.Shader = true
	})
}
