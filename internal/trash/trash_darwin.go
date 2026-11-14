//go:build darwin

package trash

import (
	"errors"
	"os"
	"path/filepath"
	"syscall"
)

func move(path string) (string, error) {
	dir := Dir
	if dir == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		dir = filepath.Join(home, ".Trash")
	}
	dest, err := unique(dir, filepath.Base(path))
	if err != nil {
		return "", err
	}
	if err := os.Rename(path, dest); err != nil {
		if errors.Is(err, syscall.EXDEV) {
			return "", ErrCrossDevice
		}
		return "", err
	}
	return dest, nil
}
