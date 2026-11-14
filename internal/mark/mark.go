// Package mark turns user selections into a review list without
// double-counting nested directories.
package mark

import (
	"fmt"
	"path/filepath"

	"github.com/felixpy/envark/internal/safe"
	"github.com/felixpy/envark/internal/scan"
)

// Target is one directory the user may remove or trash.
type Target struct {
	Path      string
	Rel       string
	Root      string
	Logical   uint64
	Allocated uint64
	Dev       uint64
	Ino       uint64
	ModUnix   int64
	ClassKind string
	ClassRisk string
	Title     string
	Reason    string
}

// Resolved is the review list plus explanations for marks that were not kept.
type Resolved struct {
	Targets []Target
	Notes   []string
	Logical uint64
}

// Resolve keeps the highest marked directory that is eligible. Marks inside
// it are reported and are not added again. protected paths and anything that
// contains or sits inside them are refused.
func Resolve(tree *scan.Entry, root string, marks map[string]bool, protected []string, home string) Resolved {
	var out Resolved
	if tree == nil {
		return out
	}
	var walk func(n *scan.Entry, absorbed bool)
	walk = func(n *scan.Entry, absorbed bool) {
		if n == nil {
			return
		}
		if absorbed {
			if marks[n.Path] {
				out.Notes = append(out.Notes, fmt.Sprintf("%s is inside a selected directory and is not counted again", n.Path))
			}
			for _, child := range n.Children {
				walk(child, true)
			}
			return
		}
		if marks[n.Path] {
			if why := refuse(n, root, protected, home); why != "" {
				out.Notes = append(out.Notes, why)
				for _, child := range n.Children {
					walk(child, false)
				}
				return
			}
			out.Targets = append(out.Targets, Target{
				Path:      n.Path,
				Rel:       n.Rel,
				Root:      root,
				Logical:   n.Logical,
				Allocated: n.Allocated,
				Dev:       n.Dev,
				Ino:       n.Ino,
				ModUnix:   n.Mod.UnixNano(),
				ClassKind: n.Class.Kind,
				ClassRisk: n.Class.Risk,
				Title:     n.Class.Title,
				Reason:    n.Class.Reason,
			})
			out.Logical += n.Logical
			for _, child := range n.Children {
				walk(child, true)
			}
			return
		}
		for _, child := range n.Children {
			walk(child, false)
		}
	}
	walk(tree, false)
	return out
}

func refuse(n *scan.Entry, root string, protected []string, home string) string {
	if n.Path == root {
		return "The scan root is never removed."
	}
	if !n.Dir || n.Link || n.OtherVolume {
		return n.Path + " is not a directory Envark can remove."
	}
	if !n.Class.Eligible || n.Class.Method != "trash" {
		return n.Path + " is not a recognized cleanup target."
	}
	if safe.System(n.Path) {
		return n.Path + " is in a protected system location."
	}
	if home != "" && safe.IsHome(n.Path, home) {
		return "The home directory is never removed."
	}
	if safe.CoversProtected(n.Path, protected) {
		return n.Path + " overlaps a protected project."
	}
	if !safe.Within(root, n.Path) {
		return n.Path + " is outside the scan root."
	}
	return ""
}

// Outside reports whether path is not root and not inside root.
func Outside(root, path string) bool {
	return !safe.Within(root, path)
}

// Base is the last element of path, for labels.
func Base(path string) string { return filepath.Base(path) }

// Absorb drops duplicate paths and paths that sit inside another target so
// nested selections are not counted twice.
func Absorb(targets []Target) (kept []Target, notes []string) {
	for _, target := range targets {
		if target.Path == "" {
			continue
		}
		dup := false
		inside := false
		for _, other := range targets {
			if other.Path == target.Path {
				continue
			}
			if safe.Within(other.Path, target.Path) {
				inside = true
			}
		}
		for _, have := range kept {
			if have.Path == target.Path {
				dup = true
			}
		}
		if dup {
			continue
		}
		if inside {
			notes = append(notes, target.Path+" is inside another selected directory and is not counted again")
			continue
		}
		kept = append(kept, target)
	}
	return kept, notes
}
