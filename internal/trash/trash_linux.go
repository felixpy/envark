//go:build linux

package trash

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

func move(path string) (string, error) {
	root, err := trashRoot()
	if err != nil {
		return "", err
	}
	files := filepath.Join(root, "files")
	infoDir := filepath.Join(root, "info")
	dest, err := unique(files, filepath.Base(path))
	if err != nil {
		return "", err
	}
	if err := os.MkdirAll(infoDir, 0o700); err != nil {
		return "", err
	}
	if err := os.Rename(path, dest); err != nil {
		if errors.Is(err, syscall.EXDEV) {
			return "", ErrCrossDevice
		}
		return "", err
	}
	infoPath := filepath.Join(infoDir, filepath.Base(dest)+".trashinfo")
	body := "[Trash Info]\nPath=" + urlEscape(path) + "\nDeletionDate=" + time.Now().Format("2006-01-02T15:04:05") + "\n"
	if err := os.WriteFile(infoPath, []byte(body), 0o600); err != nil {
		// The file is already in the trash. Surface the info failure but
		// keep the move, and still return the destination.
		return dest, fmt.Errorf("moved to trash, info file: %w", err)
	}
	return dest, nil
}

func trashRoot() (string, error) {
	if Dir != "" {
		return Dir, nil
	}
	if data := os.Getenv("XDG_DATA_HOME"); data != "" {
		return filepath.Join(data, "Trash"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".local", "share", "Trash"), nil
}

func urlEscape(path string) string {
	var b strings.Builder
	for _, r := range path {
		if r > 32 && r < 127 && r != '%' && r != '#' && r != '?' && r != ';' {
			b.WriteRune(r)
			continue
		}
		for _, c := range []byte(string(r)) {
			fmt.Fprintf(&b, "%%%02X", c)
		}
	}
	return b.String()
}
