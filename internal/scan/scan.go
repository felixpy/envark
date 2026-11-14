// Package scan walks a directory the user chose and measures it.
package scan

import (
	"context"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/felixpy/envark/internal/classify"
	"github.com/felixpy/envark/internal/safe"
)

const (
	maxDepth     = 256
	maxErrors    = 100
	largeFileMin = 1 << 20
)

// Entry is one file or directory. Directory Logical and Allocated include
// descendants. Small files contribute to those totals and are not listed.
type Entry struct {
	Name        string
	Path        string
	Rel         string
	Dir         bool
	Link        bool
	OtherVolume bool
	Logical     uint64
	Allocated   uint64
	Files       uint64
	Dirs        uint64
	Skipped     uint64
	SmallFiles  uint64
	Complete    bool
	AllocExact  bool
	HardlinkDup bool
	Mod         time.Time
	Dev         uint64
	Ino         uint64
	Children    []*Entry
	Class       classify.Result
}

// Result is one completed walk. A cancelled walk returns no result.
type Result struct {
	Root          string
	Tree          *Entry
	Errors        []string
	Excluded      uint64
	Entries       int
	AllocExact    bool
	HardlinkAware bool
}

// Options controls one walk. A nil Excludes list uses .git, .svn, and .hg.
// An empty non-nil list excludes nothing. SameDevice, when set, replaces
// the device comparison; tests use it to force the other-volume path.
type Options struct {
	Excludes   []string
	Progress   func(path string, entries int)
	AfterEntry func()
	SameDevice func(dev uint64) bool
}

type fileStat struct {
	Dev, Ino, Alloc, Nlink uint64
	AllocExact             bool
	HardlinkKnown          bool
}

type walker struct {
	ctx           context.Context
	opt           Options
	exclude       map[string]bool
	seen          map[[2]uint64]struct{}
	errors        []string
	entries       int
	excluded      uint64
	rootDev       uint64
	allocExact    bool
	hardlinkKnown bool
}

// Scan measures root. Symlinks are not followed. The root itself must be a
// real directory. On cancellation the partial tree is discarded.
func Scan(ctx context.Context, root string, opt Options) (*Result, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	root, err := safe.RealDir(root)
	if err != nil {
		return nil, err
	}
	info, err := os.Lstat(root)
	if err != nil {
		return nil, err
	}
	st := statPath(root, info)
	w := &walker{
		ctx:           ctx,
		opt:           opt,
		exclude:       excludeSet(opt.Excludes),
		seen:          map[[2]uint64]struct{}{},
		rootDev:       st.Dev,
		allocExact:    st.AllocExact,
		hardlinkKnown: st.HardlinkKnown,
	}
	tree, err := w.walk(root, ".", info, st, 0)
	if err != nil {
		return nil, err
	}
	return &Result{
		Root:          root,
		Tree:          tree,
		Errors:        w.errors,
		Excluded:      w.excluded,
		Entries:       w.entries,
		AllocExact:    w.allocExact,
		HardlinkAware: w.hardlinkKnown,
	}, nil
}

func (w *walker) walk(path, rel string, info fs.FileInfo, st fileStat, depth int) (*Entry, error) {
	if err := w.ctx.Err(); err != nil {
		return nil, err
	}
	if w.opt.AfterEntry != nil {
		w.opt.AfterEntry()
	}
	if err := w.ctx.Err(); err != nil {
		return nil, err
	}
	w.entries++
	if w.opt.Progress != nil {
		w.opt.Progress(path, w.entries)
	}
	if !st.AllocExact {
		w.allocExact = false
	}
	if !st.HardlinkKnown {
		w.hardlinkKnown = false
	}
	entry := &Entry{
		Name:       info.Name(),
		Path:       path,
		Rel:        rel,
		Dir:        info.IsDir(),
		Link:       info.Mode()&os.ModeSymlink != 0,
		Allocated:  st.Alloc,
		Complete:   true,
		AllocExact: st.AllocExact,
		Mod:        info.ModTime(),
		Dev:        st.Dev,
		Ino:        st.Ino,
	}
	// Logical size is file length only. Directory and symlink metadata stay
	// in Allocated when the platform reports blocks.
	if !entry.Dir && !entry.Link {
		entry.Logical = uint64(max64(info.Size()))
	}
	if rel == "." {
		entry.Name = filepath.Base(path)
	}
	if entry.Link {
		entry.Class = classify.Link()
		entry.Dir = false
		return entry, nil
	}
	if !entry.Dir {
		entry.Class = classify.File()
		entry.Files = 1
		if st.HardlinkKnown && st.Nlink > 1 && st.Ino != 0 {
			key := [2]uint64{st.Dev, st.Ino}
			if _, ok := w.seen[key]; ok {
				entry.Logical = 0
				entry.Allocated = 0
				entry.HardlinkDup = true
				return entry, nil
			}
			w.seen[key] = struct{}{}
		}
		return entry, nil
	}
	if depth > 0 && !w.onVolume(st.Dev) {
		entry.OtherVolume = true
		entry.Logical = 0
		entry.Allocated = 0
		entry.Complete = false
		entry.Skipped = 1
		entry.Class = classify.OtherVolume()
		return entry, nil
	}
	if depth >= maxDepth {
		entry.Complete = false
		entry.Skipped = 1
		w.addError(path + ": depth limit")
		entry.Class = classify.Source()
		return entry, nil
	}
	children, err := os.ReadDir(path)
	if err != nil {
		entry.Complete = false
		entry.Skipped = 1
		entry.Logical = 0
		entry.Allocated = 0
		w.addError(path + ": " + err.Error())
		entry.Class = classify.Source()
		return entry, nil
	}
	names := make([]string, 0, len(children))
	var listed []*Entry
	for _, child := range children {
		if w.exclude[child.Name()] {
			w.excluded++
			continue
		}
		childPath := filepath.Join(path, child.Name())
		childRel := child.Name()
		if rel != "." {
			childRel = rel + "/" + child.Name()
		}
		info, err := os.Lstat(childPath)
		if err != nil {
			entry.Complete = false
			entry.Skipped++
			w.addError(childPath + ": " + err.Error())
			continue
		}
		st := statPath(childPath, info)
		got, err := w.walk(childPath, childRel, info, st, depth+1)
		if err != nil {
			return nil, err
		}
		names = append(names, child.Name())
		entry.Logical += got.Logical
		entry.Allocated += got.Allocated
		entry.Files += got.Files
		entry.Dirs += got.Dirs
		entry.Skipped += got.Skipped
		entry.SmallFiles += got.SmallFiles
		if !got.Complete {
			entry.Complete = false
		}
		if got.Dir {
			entry.Dirs++
		}
		if !got.Dir && !got.Link && !got.HardlinkDup && got.Logical < largeFileMin {
			entry.SmallFiles++
			continue
		}
		listed = append(listed, got)
	}
	sort.Slice(listed, func(i, j int) bool {
		if listed[i].Logical == listed[j].Logical {
			return listed[i].Name < listed[j].Name
		}
		return listed[i].Logical > listed[j].Logical
	})
	for _, child := range listed {
		own := childNames(child)
		if child.Link {
			child.Class = classify.Link()
		} else if child.OtherVolume {
			child.Class = classify.OtherVolume()
		} else if child.Dir {
			child.Class = classify.Directory(child.Name, names, own)
		} else {
			child.Class = classify.File()
		}
	}
	entry.Children = listed
	entry.Class = classify.Source()
	return entry, nil
}

func childNames(entry *Entry) []string {
	if entry == nil {
		return nil
	}
	names := make([]string, 0, len(entry.Children)+1)
	for _, child := range entry.Children {
		names = append(names, child.Name)
	}
	// Small files are omitted from Children. pyvenv.cfg is small, so a
	// virtualenv would lose its evidence. Read the names we already had
	// only when the directory is a candidate.
	if entry.Name == ".venv" || entry.Name == "venv" || entry.Name == "vendor" {
		if extra, err := os.ReadDir(entry.Path); err == nil {
			names = names[:0]
			for _, item := range extra {
				names = append(names, item.Name())
			}
		}
	}
	return names
}

func (w *walker) addError(msg string) {
	if len(w.errors) >= maxErrors {
		return
	}
	w.errors = append(w.errors, msg)
}

func (w *walker) onVolume(dev uint64) bool {
	if w.opt.SameDevice != nil {
		return w.opt.SameDevice(dev)
	}
	return dev == w.rootDev
}

func excludeSet(names []string) map[string]bool {
	if names == nil {
		names = []string{".git", ".svn", ".hg"}
	}
	out := make(map[string]bool, len(names))
	for _, name := range names {
		name = strings.TrimSpace(name)
		if name == "" || strings.ContainsAny(name, `/\`) {
			continue
		}
		out[name] = true
	}
	return out
}

func max64(n int64) int64 {
	if n < 0 {
		return 0
	}
	return n
}

// Find returns the entry with path, or nil.
func Find(entry *Entry, path string) *Entry {
	if entry == nil {
		return nil
	}
	if entry.Path == path {
		return entry
	}
	for _, child := range entry.Children {
		if got := Find(child, path); got != nil {
			return got
		}
	}
	return nil
}
