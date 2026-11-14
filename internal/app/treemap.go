package app

import "math"

// mapRect is one rectangle in the space map, in DIPs.
type mapRect struct {
	X, Y, W, H float32
}

// mapWeight is a sized entry. Weight is logical bytes, never a guess.
type mapWeight struct {
	Key    string
	Weight float64
}

// squarify lays weights into bounds. Areas match the weights. A zero
// weight is left out; the caller keeps that entry in the list instead.
func squarify(items []mapWeight, bounds mapRect) []mapRect {
	var weights []float64
	var total float64
	for _, item := range items {
		if item.Weight <= 0 || math.IsNaN(item.Weight) || math.IsInf(item.Weight, 0) {
			weights = append(weights, 0)
			continue
		}
		weights = append(weights, item.Weight)
		total += item.Weight
	}
	out := make([]mapRect, len(items))
	if total <= 0 || bounds.W <= 1 || bounds.H <= 1 {
		return out
	}
	area := float64(bounds.W * bounds.H)
	sized := make([]float64, 0, len(weights))
	index := make([]int, 0, len(weights))
	for i, w := range weights {
		if w <= 0 {
			continue
		}
		sized = append(sized, w/total*area)
		index = append(index, i)
	}
	placed := squarifyAreas(sized, bounds)
	for i, at := range index {
		if i < len(placed) {
			out[at] = placed[i]
		}
	}
	return out
}

func squarifyAreas(areas []float64, bounds mapRect) []mapRect {
	var out []mapRect
	var row []float64
	rest := append([]float64(nil), areas...)
	for len(rest) > 0 {
		side := shortest(bounds)
		if len(row) == 0 || worst(append(row, rest[0]), side) <= worst(row, side) {
			row = append(row, rest[0])
			rest = rest[1:]
			if len(rest) == 0 {
				bounds = placeRow(row, bounds, &out)
			}
			continue
		}
		bounds = placeRow(row, bounds, &out)
		row = nil
	}
	return out
}

func shortest(r mapRect) float64 {
	if r.W < r.H {
		return float64(r.W)
	}
	return float64(r.H)
}

func worst(row []float64, side float64) float64 {
	if len(row) == 0 || side <= 0 {
		return math.Inf(1)
	}
	sum := 0.0
	maxV, minV := row[0], row[0]
	for _, v := range row {
		sum += v
		if v > maxV {
			maxV = v
		}
		if v < minV {
			minV = v
		}
	}
	if sum == 0 || minV == 0 {
		return math.Inf(1)
	}
	sum2 := sum * sum
	side2 := side * side
	return math.Max(side2*maxV/sum2, sum2/(side2*minV))
}

func placeRow(row []float64, bounds mapRect, out *[]mapRect) mapRect {
	sum := 0.0
	for _, v := range row {
		sum += v
	}
	if bounds.W >= bounds.H {
		width := float32(sum / float64(bounds.H))
		if width > bounds.W {
			width = bounds.W
		}
		y := bounds.Y
		for _, v := range row {
			h := float32(0)
			if width > 0 {
				h = float32(v / float64(width))
			}
			*out = append(*out, mapRect{X: bounds.X, Y: y, W: width, H: h})
			y += h
		}
		return mapRect{X: bounds.X + width, Y: bounds.Y, W: bounds.W - width, H: bounds.H}
	}
	height := float32(sum / float64(bounds.W))
	if height > bounds.H {
		height = bounds.H
	}
	x := bounds.X
	for _, v := range row {
		w := float32(0)
		if height > 0 {
			w = float32(v / float64(height))
		}
		*out = append(*out, mapRect{X: x, Y: bounds.Y, W: w, H: height})
		x += w
	}
	return mapRect{X: bounds.X, Y: bounds.Y + height, W: bounds.W, H: bounds.H - height}
}

func hitMap(cells []mapCell, x, y float32) string {
	for i := len(cells) - 1; i >= 0; i-- {
		c := cells[i]
		if c.W <= 0 || c.H <= 0 {
			continue
		}
		if x >= c.X && y >= c.Y && x < c.X+c.W && y < c.Y+c.H {
			return c.Path
		}
	}
	return ""
}
