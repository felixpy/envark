// Package cleanup revalidates review targets and then trash or deletes them.
package cleanup

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/felixpy/envark/internal/human"
	"github.com/felixpy/envark/internal/mark"
	"github.com/felixpy/envark/internal/safe"
	"github.com/felixpy/envark/internal/scan"
	"github.com/felixpy/envark/internal/trash"
)

// ItemResult is one path or command outcome.
type ItemResult struct {
	Path      string `json:"path,omitempty"`
	Action    string `json:"action"`
	Outcome   string `json:"outcome"`
	Logical   uint64 `json:"logical,omitempty"`
	Error     string `json:"error,omitempty"`
	TrashPath string `json:"trashPath,omitempty"`
	Detail    string `json:"detail,omitempty"`
}

// Execute trashes or permanently deletes targets. A failure or a stale
// target does not stop the rest. Cancellation finishes the current item's
// decision and marks the remainder cancelled. Permanent deletion is used
// only when permanent is true; there is no fallback from a trash error.
func Execute(ctx context.Context, targets []mark.Target, permanent bool, protected []string, home string, excludes []string) []ItemResult {
	if ctx == nil {
		ctx = context.Background()
	}
	out := make([]ItemResult, 0, len(targets))
	for i, target := range targets {
		if err := ctx.Err(); err != nil {
			for _, rest := range targets[i:] {
				out = append(out, ItemResult{Path: rest.Path, Action: actionName(permanent), Outcome: "cancelled", Error: err.Error()})
			}
			break
		}
		out = append(out, one(ctx, target, permanent, protected, home, excludes))
	}
	return out
}

func one(ctx context.Context, target mark.Target, permanent bool, protected []string, home string, excludes []string) ItemResult {
	action := actionName(permanent)
	if err := revalidate(ctx, target, protected, home, excludes); err != nil {
		return ItemResult{Path: target.Path, Action: action, Outcome: "failed", Error: err.Error()}
	}
	if permanent {
		if err := os.RemoveAll(target.Path); err != nil {
			return ItemResult{Path: target.Path, Action: action, Outcome: "failed", Error: err.Error()}
		}
		return ItemResult{
			Path:    target.Path,
			Action:  action,
			Outcome: "removed",
			Logical: target.Logical,
			Detail:  "Permanently deleted. Logical bytes are the reviewed size, not a measurement of free disk.",
		}
	}
	trashed, err := trash.Move(target.Path)
	if err != nil {
		return ItemResult{Path: target.Path, Action: action, Outcome: "failed", Error: err.Error()}
	}
	return ItemResult{
		Path:      target.Path,
		Action:    action,
		Outcome:   "removed",
		Logical:   target.Logical,
		TrashPath: trashed,
		Detail:    "Moved to Trash. Logical bytes are the reviewed size, not a measurement of free disk.",
	}
}

func revalidate(ctx context.Context, target mark.Target, protected []string, home string, excludes []string) error {
	if err := safe.ResolvedInside(target.Root, target.Path); err != nil {
		return err
	}
	if target.Path == target.Root {
		return errors.New("refusing the scan root")
	}
	if safe.System(target.Path) || (home != "" && safe.IsHome(target.Path, home)) || safe.CoversProtected(target.Path, protected) {
		return errors.New("path became protected")
	}
	info, err := os.Lstat(target.Path)
	if err != nil {
		return fmt.Errorf("stale target: %w", err)
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return errors.New("stale target: not the reviewed directory")
	}
	// Measure again before the move. A size change means the tree raced.
	measured, err := scan.Scan(ctx, target.Path, scan.Options{Excludes: excludes})
	if err != nil {
		return err
	}
	if !measured.Tree.Complete {
		return errors.New("stale target: the directory could not be fully remeasured")
	}
	if measured.Tree.Logical != target.Logical {
		return fmt.Errorf("stale target: logical size changed from %d to %d", target.Logical, measured.Tree.Logical)
	}
	st := measured.Tree
	if st.Dev != target.Dev || st.Ino != target.Ino {
		return errors.New("stale target: directory identity changed")
	}
	return nil
}

func actionName(permanent bool) string {
	if permanent {
		return "permanent"
	}
	return "trash"
}

// RemovedLogical sums logical bytes of items that were actually removed.
func RemovedLogical(items []ItemResult) uint64 {
	var n uint64
	for _, item := range items {
		if item.Outcome == "removed" {
			n += item.Logical
		}
	}
	return n
}

// Summary describes a finished batch without calling it freed disk.
func Summary(items []ItemResult) string {
	var removed, failed, cancelled, completed int
	for _, item := range items {
		switch item.Outcome {
		case "removed":
			removed++
		case "completed":
			completed++
		case "failed":
			failed++
		case "cancelled":
			cancelled++
		}
	}
	n := RemovedLogical(items)
	return fmt.Sprintf("%d moved or deleted, %d commands finished, %d failed, %d cancelled. Logical bytes of moved or deleted items: %s (%d bytes). This is not a measurement of free disk.", removed, completed, failed, cancelled, human.Bytes(n), n)
}

// Encode is a compact JSON form for logs in tests.
func Encode(items []ItemResult) string {
	b, _ := json.Marshal(items)
	return string(b)
}

// Restore puts one trashed directory back when its parent is still inside
// the approved root and is not protected.
func Restore(trashed, original, root string, protected []string) error {
	if trashed == "" {
		return errors.New("this item has no trash path; restore it from the system trash yourself")
	}
	parent := filepath.Dir(original)
	if err := safe.ResolvedInside(root, parent); err != nil {
		return err
	}
	if !safe.Within(root, original) || original == root {
		return errors.New("original path escapes the approved root")
	}
	if safe.System(original) || safe.CoversProtected(original, protected) {
		return errors.New("refusing to restore into a protected path")
	}
	return trash.Restore(trashed, original)
}

// Now is a tiny clock helper for history stamps.
func Now() time.Time { return time.Now() }
