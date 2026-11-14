// Package trash moves a path into the platform trash without following links.
package trash

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// ErrCrossDevice means the trash directory is on another filesystem.
// Callers must not fall back to permanent deletion.
var ErrCrossDevice = errors.New("trash is on another filesystem")

// Dir overrides the trash root. Tests set it. An empty value uses the
// platform default.
var Dir string

// Move puts path in the trash and returns the trashed path when the
// platform exposes one. Windows recycle does not return a path.
func Move(path string) (string, error) {
	path = filepath.Clean(path)
	info, err := os.Lstat(path)
	if err != nil {
		return "", err
	}
	if info.Mode()&os.ModeSymlink != 0 {
		return "", fmt.Errorf("refusing to trash symlink %s", path)
	}
	return move(path)
}

// Restore moves a previously trashed path back to original when original
// does not already exist.
func Restore(trashed, original string) error {
	if trashed == "" || original == "" {
		return errors.New("restore needs the trash path and the original path")
	}
	if _, err := os.Lstat(original); err == nil {
		return fmt.Errorf("refusing to overwrite %s", original)
	}
	parent := filepath.Dir(original)
	info, err := os.Lstat(parent)
	if err != nil {
		return err
	}
	if info.Mode()&os.ModeSymlink != 0 || !info.IsDir() {
		return fmt.Errorf("restore parent is not a real directory")
	}
	return os.Rename(trashed, original)
}

func unique(dir, name string) (string, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	candidate := filepath.Join(dir, name)
	if _, err := os.Lstat(candidate); errors.Is(err, os.ErrNotExist) {
		return candidate, nil
	}
	ext := filepath.Ext(name)
	stem := strings.TrimSuffix(name, ext)
	for i := 2; i < 1000; i++ {
		candidate = filepath.Join(dir, fmt.Sprintf("%s %d%s", stem, i, ext))
		if _, err := os.Lstat(candidate); errors.Is(err, os.ErrNotExist) {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("no free trash name for %s", name)
}
