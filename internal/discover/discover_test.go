package discover

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDiscoveryAndUnavailableManagers(t *testing.T) {
	home := t.TempDir()
	bin := filepath.Join(home, "bin")
	if err := os.Mkdir(bin, 0o755); err != nil {
		t.Fatal(err)
	}
	write(t, filepath.Join(bin, "node"), "#!/bin/sh\nif [ \"$1\" = \"-v\" ]; then echo v22.14.0; exit 0; fi\nexit 1\n")
	write(t, filepath.Join(bin, "fnm"), "#!/bin/sh\nif [ \"$1\" = \"list\" ]; then printf '* v22.14.0 default\\n  v20.11.0\\n'; exit 0; fi\nexit 1\n")
	write(t, filepath.Join(bin, "python3"), "#!/bin/sh\necho 'Python 3.12.1'\n")
	write(t, filepath.Join(bin, "uv"), "#!/bin/sh\nif [ \"$1\" = \"python\" ]; then printf 'cpython-3.12.1-linux (default)\\ncpython-3.11.8-linux\\n'; exit 0; fi\nexit 1\n")
	write(t, filepath.Join(bin, "java"), "#!/bin/sh\necho 'openjdk version \"21.0.2\"' >&2\nexit 1\n")
	write(t, filepath.Join(bin, "rustc"), "#!/bin/sh\necho 'rustc 1.81.0'\n")
	write(t, filepath.Join(bin, "rustup"), "#!/bin/sh\nif [ \"$1\" = \"toolchain\" ]; then printf 'stable-x86_64-unknown-linux-gnu (active, default)\\nnightly-x86_64-unknown-linux-gnu\\n'; exit 0; fi\nexit 1\n")
	write(t, filepath.Join(bin, "go"), fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = \"version\" ]; then echo 'go version go1.27.4 linux/amd64'; exit 0; fi\nif [ \"$1\" = \"env\" ]; then echo '%s/sdk/go'; exit 0; fi\nexit 1\n", home))
	write(t, filepath.Join(bin, "npm"), fmt.Sprintf("#!/bin/sh\nif [ \"$1\" = \"config\" ]; then echo '%s/.npm'; exit 0; fi\nexit 1\n", home))
	if err := os.MkdirAll(filepath.Join(home, ".npm"), 0o755); err != nil {
		t.Fatal(err)
	}
	env := Env{Home: home, Path: bin}
	probes := Discover(context.Background(), env)
	fnmOld := findProbe(probes, "Node.js", "v20.11.0")
	if fnmOld == nil || fnmOld.Removal == nil || fnmOld.Removal.Args[1] != "v20.11.0" {
		t.Fatalf("fnm old: %+v", fnmOld)
	}
	fnmDefault := findProbe(probes, "Node.js", "v22.14.0")
	if fnmDefault == nil || fnmDefault.Removal != nil || fnmDefault.Unavailable == "" {
		t.Fatalf("fnm default: %+v", fnmDefault)
	}
	java := findProbe(probes, "Java", "21.0.2")
	if java == nil || java.Removal != nil || !strings.Contains(java.Unavailable, "not offered") {
		t.Fatalf("java: %+v", java)
	}
	nightly := findProbe(probes, "Rust", "nightly-x86_64-unknown-linux-gnu")
	if nightly == nil || nightly.Removal == nil {
		t.Fatalf("nightly: %+v", nightly)
	}
	stable := findProbe(probes, "Rust", "stable-x86_64-unknown-linux-gnu")
	if stable == nil || stable.Removal != nil {
		t.Fatalf("stable: %+v", stable)
	}
	pyOld := findProbe(probes, "Python", "3.11.8")
	if pyOld == nil || pyOld.Removal == nil {
		t.Fatalf("python: %+v", pyOld)
	}
	goProbe := findProbe(probes, "Go", "go1.27.4")
	if goProbe == nil || goProbe.Removal != nil || !strings.Contains(goProbe.Unavailable, "not offered") {
		t.Fatalf("go: %+v", goProbe)
	}
	locs := Locations(context.Background(), env)
	npm := findLocation(locs, "npm-cache")
	if npm == nil || npm.Command == nil || npm.Err != "" {
		t.Fatalf("npm cache: %+v", npm)
	}
	mod := findLocation(locs, "go-mod-cache")
	if mod == nil || mod.Policy.Eligible || mod.Command != nil {
		t.Fatalf("mod cache: %+v", mod)
	}
	maven := findLocation(locs, "maven-repository")
	if maven == nil || maven.Missing || maven.Policy.Eligible {
		// The directory does not exist, so it is missing and not eligible.
		if maven == nil || !maven.Missing || maven.Policy.Eligible {
			t.Fatalf("maven: %+v", maven)
		}
	}
}

func TestAbsentToolIsNotSuccess(t *testing.T) {
	env := Env{Home: t.TempDir(), Path: t.TempDir()}
	probes := Discover(context.Background(), env)
	for _, probe := range probes {
		if probe.Err == "" || probe.Unavailable == "" {
			t.Fatalf("present-looking probe: %+v", probe)
		}
		if probe.Removal != nil {
			t.Fatalf("removal without a binary: %+v", probe)
		}
	}
}

func write(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o755); err != nil {
		t.Fatal(err)
	}
}

func findProbe(probes []Probe, eco, version string) *Probe {
	for i := range probes {
		if probes[i].Ecosystem == eco && probes[i].Version == version {
			return &probes[i]
		}
	}
	return nil
}

func findLocation(locs []Location, id string) *Location {
	for i := range locs {
		if locs[i].ID == id {
			return &locs[i]
		}
	}
	return nil
}
