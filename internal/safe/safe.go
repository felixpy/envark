// Package safe checks filesystem paths before a scan or a deletion.
package safe

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// ErrUnsafe is returned for a path Envark will not scan or remove.
var ErrUnsafe = errors.New("unsafe path")

// Absolute returns a cleaned absolute path. Relative paths, NUL, and empty
// input are refused.
func Absolute(path string) (string, error) {
	if path == "" || strings.ContainsRune(path, 0) {
		return "", fmt.Errorf("%w: empty or NUL path", ErrUnsafe)
	}
	if !filepath.IsAbs(path) {
		return "", fmt.Errorf("%w: %s is not absolute", ErrUnsafe, path)
	}
	return filepath.Clean(path), nil
}

// Within reports whether path is root or a descendant, using lexical
// components only. Callers that delete must also reject symlinks below root.
func Within(root, path string) bool {
	root, err1 := Absolute(root)
	path, err2 := Absolute(path)
	if err1 != nil || err2 != nil {
		return false
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return false
	}
	if rel == "." {
		return true
	}
	return rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}

// SymlinkBelow reports whether any component from root down to path is a
// symlink. Components above root are ignored so a linked ancestor such as
// macOS /tmp does not, by itself, reject a real directory inside the root.
func SymlinkBelow(root, path string) error {
	root, err := Absolute(root)
	if err != nil {
		return err
	}
	path, err = Absolute(path)
	if err != nil {
		return err
	}
	if !Within(root, path) {
		return fmt.Errorf("%w: %s is outside %s", ErrUnsafe, path, root)
	}
	info, err := os.Lstat(root)
	if err != nil {
		return err
	}
	if info.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("%w: scan root %s is a symlink", ErrUnsafe, root)
	}
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return err
	}
	cur := root
	if rel != "." {
		for _, part := range strings.Split(rel, string(filepath.Separator)) {
			if part == "" || part == "." || part == ".." {
				return fmt.Errorf("%w: refused component %q", ErrUnsafe, part)
			}
			cur = filepath.Join(cur, part)
			info, err = os.Lstat(cur)
			if err != nil {
				return err
			}
			if info.Mode()&os.ModeSymlink != 0 {
				return fmt.Errorf("%w: %s is a symlink", ErrUnsafe, cur)
			}
		}
	}
	return nil
}

// ResolvedInside resolves symlinks and requires path to land inside root.
func ResolvedInside(root, path string) error {
	if err := SymlinkBelow(root, path); err != nil {
		return err
	}
	resolvedRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return fmt.Errorf("%w: resolve root: %v", ErrUnsafe, err)
	}
	resolvedPath, err := filepath.EvalSymlinks(path)
	if err != nil {
		return fmt.Errorf("%w: resolve path: %v", ErrUnsafe, err)
	}
	if !Within(resolvedRoot, resolvedPath) {
		return fmt.Errorf("%w: %s resolves outside %s", ErrUnsafe, path, root)
	}
	return nil
}

// System reports whether path is a protected operating-system location or
// the filesystem root.
func System(path string) bool {
	path, err := Absolute(path)
	if err != nil {
		return true
	}
	if path == filepath.Clean("/") || isDriveRoot(path) {
		return true
	}
	for _, prefix := range systemPrefixes() {
		if path == prefix || strings.HasPrefix(path, prefix+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

func isDriveRoot(path string) bool {
	if runtime.GOOS != "windows" {
		return false
	}
	volume := filepath.VolumeName(path)
	return volume != "" && path == volume+`\`
}

func systemPrefixes() []string {
	switch runtime.GOOS {
	case "windows":
		return []string{
			`C:\Windows`,
			`C:\Program Files`,
			`C:\Program Files (x86)`,
			`C:\ProgramData`,
		}
	case "darwin":
		return []string{"/System", "/bin", "/sbin", "/usr", "/private/etc", "/Library", "/Applications"}
	default:
		return []string{
			"/bin", "/boot", "/dev", "/etc", "/lib", "/lib32", "/lib64", "/libx32",
			"/proc", "/root", "/run", "/sbin", "/snap", "/sys", "/usr",
			"/var/lib", "/var/run",
		}
	}
}

// CoversProtected reports whether target is a protected path, sits inside
// one, or contains one.
func CoversProtected(target string, protected []string) bool {
	target, err := Absolute(target)
	if err != nil {
		return true
	}
	for _, item := range protected {
		item, err = Absolute(item)
		if err != nil {
			continue
		}
		if target == item || strings.HasPrefix(target, item+string(filepath.Separator)) || strings.HasPrefix(item, target+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

// RealDir requires path to be an existing directory and not itself a symlink.
func RealDir(path string) (string, error) {
	path, err := Absolute(path)
	if err != nil {
		return "", err
	}
	info, err := os.Lstat(path)
	if err != nil {
		return "", err
	}
	if info.Mode()&os.ModeSymlink != 0 {
		return "", fmt.Errorf("%w: %s is a symlink", ErrUnsafe, path)
	}
	if !info.IsDir() {
		return "", fmt.Errorf("%w: %s is not a directory", ErrUnsafe, path)
	}
	return path, nil
}

// HomeDir is the user's home directory, or a non-empty override.
func HomeDir(override string) (string, error) {
	if override != "" {
		return Absolute(override)
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return Absolute(home)
}

// IsHome reports whether path is the home directory itself.
func IsHome(path, home string) bool {
	path, err1 := Absolute(path)
	home, err2 := Absolute(home)
	if err1 != nil || err2 != nil {
		return false
	}
	return path == home
}

// DirMode is the permission check used when a directory cannot be read.
func DirMode(mode fs.FileMode) bool { return mode.IsDir() }
