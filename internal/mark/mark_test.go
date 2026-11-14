package mark

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/scan"
)

func entry(path string, logical uint64, class classify.Result, children ...*scan.Entry) *scan.Entry {
	return &scan.Entry{
		Name:     filepath.Base(path),
		Path:     path,
		Dir:      true,
		Logical:  logical,
		Mod:      time.Unix(10, 0),
		Class:    class,
		Children: children,
	}
}

func TestNestedMarksCountTheParentOnce(t *testing.T) {
	root := t.TempDir()
	proj := filepath.Join(root, "proj")
	modules := filepath.Join(proj, "node_modules")
	inner := filepath.Join(modules, "pkg", "node_modules")
	tree := entry(root, 300, classify.Source(),
		entry(proj, 300, classify.Source(),
			entry(modules, 200, classify.Directory("node_modules", []string{"package.json"}, nil),
				entry(inner, 50, classify.Directory("node_modules", []string{"package.json"}, nil)),
			),
		),
	)
	// The synthetic children are not linked as the real scanner would, but
	// Resolve only needs the tree passed in.
	tree.Children[0].Children[0].Children = []*scan.Entry{
		entry(inner, 50, classify.Directory("node_modules", []string{"package.json"}, nil)),
	}
	got := Resolve(tree, root, map[string]bool{modules: true, inner: true}, nil, "")
	if len(got.Targets) != 1 || got.Targets[0].Path != modules {
		t.Fatalf("targets: %+v", got.Targets)
	}
	if got.Logical != 200 {
		t.Fatalf("logical = %d, want 200", got.Logical)
	}
	if len(got.Notes) != 1 {
		t.Fatalf("notes: %v", got.Notes)
	}
}

func TestProtectedAndRootAreRefused(t *testing.T) {
	root := t.TempDir()
	proj := filepath.Join(root, "proj")
	modules := filepath.Join(proj, "node_modules")
	tree := entry(root, 10, classify.Source(),
		entry(modules, 10, classify.Directory("node_modules", []string{"package.json"}, nil)),
	)
	got := Resolve(tree, root, map[string]bool{root: true, modules: true}, []string{proj}, "")
	if len(got.Targets) != 0 {
		t.Fatalf("targets: %+v", got.Targets)
	}
	if len(got.Notes) < 2 {
		t.Fatalf("notes: %v", got.Notes)
	}
}
