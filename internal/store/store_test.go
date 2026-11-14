package store

import (
	"os"
	"path/filepath"
	"testing"
)

func TestMigrationPreservesSettingsJSON(t *testing.T) {
	dir := t.TempDir()
	original := []byte("{\n  \"schemaVersion\": 1,\n  \"data\": {\n    \"language\": \"zh-TW\",\n    \"theme\": \"dark\",\n    \"scanOnLaunch\": false,\n    \"roots\": [\"/work/src\"],\n    \"excludes\": [\".git\"],\n    \"useTrash\": true,\n    \"protectedProjects\": [\"/work/src/app\"]\n  }\n}\n")
	if err := os.WriteFile(filepath.Join(dir, "settings.json"), original, 0o600); err != nil {
		t.Fatal(err)
	}
	loaded, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	after, err := os.ReadFile(filepath.Join(dir, "settings.json"))
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(original) {
		t.Fatalf("settings.json changed:\n%s", after)
	}
	if len(loaded.Prefs.Roots) != 1 || loaded.Prefs.Roots[0] != "/work/src" {
		t.Fatalf("roots: %+v", loaded.Prefs)
	}
	if len(loaded.Prefs.Protected) != 1 || loaded.Prefs.Theme != "dark" || !loaded.Prefs.UseTrash || loaded.Prefs.ScanOnLaunch {
		t.Fatalf("prefs: %+v", loaded.Prefs)
	}
	if loaded.Prefs.Language != "zh" {
		t.Fatalf("language: %+v", loaded.Prefs)
	}
	if len(loaded.Notices) < 2 {
		t.Fatal("expected the traditional-chinese notice")
	}
	again, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	if again.Prefs.Roots[0] != "/work/src" {
		t.Fatalf("reopen: %+v", again.Prefs)
	}
}
