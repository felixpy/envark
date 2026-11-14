// Package store keeps preferences and history beside any older Envark files.
package store

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"time"

	"github.com/felixpy/envark/internal/cleanup"
	"github.com/felixpy/envark/internal/safe"
)

const prefSchema = 2

// Preferences is the writable state of this version.
type Preferences struct {
	Theme          string   `json:"theme"`
	Roots          []string `json:"roots"`
	Excludes       []string `json:"excludes"`
	Protected      []string `json:"protected"`
	UseTrash       bool     `json:"useTrash"`
	AllowPermanent bool     `json:"allowPermanent"`
	ScanOnLaunch   bool     `json:"scanOnLaunch"`
}

// HistoryEntry is one finished or cancelled operation.
type HistoryEntry struct {
	ID     string               `json:"id"`
	At     time.Time            `json:"at"`
	Action string               `json:"action"`
	Note   string               `json:"note,omitempty"`
	Items  []cleanup.ItemResult `json:"items"`
}

// Loaded is what Open found, including migration notices.
type Loaded struct {
	Dir     string
	Prefs   Preferences
	History []HistoryEntry
	Notices []string
}

// DefaultDir is the Tauri data directory for dev.envark.desktop.
func DefaultDir() (string, error) {
	if dir := os.Getenv("ENVARK_DATA_DIR"); dir != "" {
		return safe.Absolute(dir)
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	switch runtime.GOOS {
	case "windows":
		if app := os.Getenv("APPDATA"); app != "" {
			return filepath.Join(app, "dev.envark.desktop"), nil
		}
		return filepath.Join(home, "AppData", "Roaming", "dev.envark.desktop"), nil
	case "darwin":
		return filepath.Join(home, "Library", "Application Support", "dev.envark.desktop"), nil
	default:
		if data := os.Getenv("XDG_DATA_HOME"); data != "" && os.Getenv("ENVARK_DATA_DIR") == "" {
			return filepath.Join(data, "dev.envark.desktop"), nil
		}
		return filepath.Join(home, ".local", "share", "dev.envark.desktop"), nil
	}
}

// Open reads preferences and history, migrating schema 1 settings once.
func Open(dir string) (Loaded, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return Loaded{}, err
	}
	loaded := Loaded{Dir: dir, Prefs: DefaultPreferences()}
	notices, err := migrate(dir, &loaded.Prefs)
	if err != nil {
		return Loaded{}, err
	}
	loaded.Notices = append(loaded.Notices, notices...)
	prefs, notice, err := readPrefs(filepath.Join(dir, "preferences.json"))
	if err != nil {
		return Loaded{}, err
	}
	if notice != "" {
		loaded.Notices = append(loaded.Notices, notice)
	}
	if prefs != nil {
		loaded.Prefs = *prefs
	}
	history, notice, err := readHistory(filepath.Join(dir, "history.json"))
	if err != nil {
		return Loaded{}, err
	}
	if notice != "" {
		loaded.Notices = append(loaded.Notices, notice)
	}
	loaded.History = history
	return loaded, nil
}

// DefaultPreferences is a new install: trash on, permanent deletion off,
// and no scan until the user chooses a folder.
func DefaultPreferences() Preferences {
	return Preferences{Theme: "system", UseTrash: true, Excludes: []string{".git", ".svn", ".hg"}}
}

// SavePreferences writes preferences.json without touching settings.json.
func SavePreferences(dir string, prefs Preferences) error {
	return writeJSON(filepath.Join(dir, "preferences.json"), prefSchema, prefs)
}

// SaveHistory writes history.json.
func SaveHistory(dir string, entries []HistoryEntry) error {
	if len(entries) > 200 {
		entries = entries[len(entries)-200:]
	}
	return writeJSON(filepath.Join(dir, "history.json"), 1, entries)
}

func migrate(dir string, prefs *Preferences) ([]string, error) {
	path := filepath.Join(dir, "settings.json")
	before, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(filepath.Join(dir, "preferences.json")); err == nil {
		return []string{"Existing preferences were kept. settings.json was not modified."}, nil
	}
	legacy, err := decodeLegacy(before)
	if err != nil {
		return []string{"settings.json could not be read and was left unchanged. New preferences use defaults."}, nil
	}
	next := DefaultPreferences()
	if legacy.Theme != "" {
		next.Theme = legacy.Theme
	}
	next.Roots = append([]string(nil), legacy.Roots...)
	if len(legacy.Excludes) > 0 {
		next.Excludes = append([]string(nil), legacy.Excludes...)
	}
	next.Protected = append([]string(nil), legacy.ProtectedProjects...)
	next.UseTrash = legacy.UseTrash
	next.ScanOnLaunch = legacy.ScanOnLaunch
	if err := SavePreferences(dir, next); err != nil {
		return nil, err
	}
	*prefs = next
	after, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if string(before) != string(after) {
		return nil, errors.New("migration changed settings.json")
	}
	notices := []string{"Imported roots, excludes, protected projects, theme, trash, and scan-on-launch from settings.json. The original file was not modified."}
	if legacy.Language != "" && legacy.Language != "en" {
		notices = append(notices, "The previous language preference was left in settings.json. This version's interface is English.")
	}
	return notices, nil
}

type legacySettings struct {
	Language          string   `json:"language"`
	Theme             string   `json:"theme"`
	ScanOnLaunch      bool     `json:"scanOnLaunch"`
	Roots             []string `json:"roots"`
	Excludes          []string `json:"excludes"`
	UseTrash          bool     `json:"useTrash"`
	ProtectedProjects []string `json:"protectedProjects"`
}

func decodeLegacy(body []byte) (legacySettings, error) {
	var wrapped struct {
		SchemaVersion *uint64        `json:"schemaVersion"`
		Data          legacySettings `json:"data"`
	}
	if err := json.Unmarshal(body, &wrapped); err != nil {
		return legacySettings{}, err
	}
	if wrapped.SchemaVersion != nil {
		if *wrapped.SchemaVersion != 1 {
			return legacySettings{}, fmt.Errorf("unsupported settings schema %d", *wrapped.SchemaVersion)
		}
		return wrapped.Data, nil
	}
	var plain legacySettings
	if err := json.Unmarshal(body, &plain); err != nil {
		return legacySettings{}, err
	}
	return plain, nil
}

func readPrefs(path string) (*Preferences, string, error) {
	body, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, "", nil
	}
	if err != nil {
		return nil, "", err
	}
	var wrapped struct {
		SchemaVersion uint64      `json:"schemaVersion"`
		Data          Preferences `json:"data"`
	}
	if err := json.Unmarshal(body, &wrapped); err != nil || wrapped.SchemaVersion != prefSchema {
		if err := quarantine(path); err != nil {
			return nil, "", err
		}
		return nil, "preferences.json could not be read. It was moved aside and defaults are in use.", nil
	}
	return &wrapped.Data, "", nil
}

func readHistory(path string) ([]HistoryEntry, string, error) {
	body, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, "", nil
	}
	if err != nil {
		return nil, "", err
	}
	var wrapped struct {
		SchemaVersion uint64         `json:"schemaVersion"`
		Data          []HistoryEntry `json:"data"`
	}
	if err := json.Unmarshal(body, &wrapped); err != nil || wrapped.SchemaVersion != 1 {
		if err := quarantine(path); err != nil {
			return nil, "", err
		}
		return nil, "history.json could not be read. It was moved aside.", nil
	}
	return wrapped.Data, "", nil
}

func quarantine(path string) error {
	dest := path + ".unreadable-" + time.Now().Format("20060102T150405")
	return os.Rename(path, dest)
}

func writeJSON(path string, schema uint64, data any) error {
	body, err := json.MarshalIndent(map[string]any{"schemaVersion": schema, "data": data}, "", "  ")
	if err != nil {
		return err
	}
	body = append(body, '\n')
	tmp, err := os.CreateTemp(filepath.Dir(path), ".envark-tmp-*")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName)
	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		return err
	}
	if _, err := tmp.Write(body); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmpName, path)
}
