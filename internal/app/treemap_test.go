package app

import (
	"math"
	"testing"
)

func TestSquarifyKeepsProportions(t *testing.T) {
	items := []mapWeight{{Key: "api", Weight: 6}, {Key: "web", Weight: 3}, {Key: "docs", Weight: 1}, {Key: "empty", Weight: 0}}
	rects := squarify(items, mapRect{W: 200, H: 100})
	if rects[3].W != 0 || rects[3].H != 0 {
		t.Fatalf("empty entry took area: %+v", rects[3])
	}
	area := 0.0
	var largest float64
	for i, r := range rects[:3] {
		a := float64(r.W * r.H)
		area += a
		if i == 0 {
			largest = a
		}
		if a > largest+0.5 {
			t.Fatalf("rect %d area %f exceeds the first", i, a)
		}
	}
	if math.Abs(area-200*100) > 2 {
		t.Fatalf("areas sum to %f", area)
	}
	if rects[0].W*rects[0].H < rects[2].W*rects[2].H*5 {
		t.Fatalf("6:1 ratio collapsed: %+v", rects)
	}
	cells := []mapCell{{Path: "/a", X: 0, Y: 0, W: 10, H: 10}, {Path: "/b", X: 10, Y: 0, W: 5, H: 10}}
	if hitMap(cells, 3, 3) != "/a" || hitMap(cells, 12, 2) != "/b" || hitMap(cells, 30, 1) != "" {
		t.Fatalf("hit %q %q", hitMap(cells, 3, 3), hitMap(cells, 12, 2))
	}
}
