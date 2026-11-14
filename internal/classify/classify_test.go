package classify

import "testing"

func TestDirectoryEvidence(t *testing.T) {
	if got := Directory("node_modules", []string{"package.json"}, nil); !got.Eligible || got.Risk != "low" {
		t.Fatalf("node_modules: %+v", got)
	}
	if got := Directory("node_modules", nil, nil); got.Eligible {
		t.Fatal("node_modules without package.json was eligible")
	}
	if got := Directory("target", []string{"README"}, nil); got.Eligible {
		t.Fatal("target without Cargo.toml was eligible")
	}
	if got := Directory("target", []string{"Cargo.toml"}, nil); !got.Eligible {
		t.Fatal("cargo target was not eligible")
	}
	if got := Directory("dist", []string{"package.json"}, nil); got.Risk != "caution" || !got.Eligible {
		t.Fatalf("dist: %+v", got)
	}
	if got := Directory(".venv", nil, []string{"pyvenv.cfg"}); got.Risk != "caution" {
		t.Fatalf("venv: %+v", got)
	}
	if got := Directory("src", []string{"package.json"}, nil); got.Eligible {
		t.Fatal("src was eligible")
	}
}

func TestKnownLocationRefusal(t *testing.T) {
	mod, ok := KnownLocation("go-mod-cache")
	if !ok || mod.Eligible {
		t.Fatalf("module cache: %+v", mod)
	}
	maven, ok := KnownLocation("maven-repository")
	if !ok || maven.Eligible {
		t.Fatalf("maven: %+v", maven)
	}
	npm, ok := KnownLocation("npm-cache")
	if !ok || npm.Method != "command" || !npm.Eligible {
		t.Fatalf("npm: %+v", npm)
	}
	if _, ok := KnownLocation("nope"); ok {
		t.Fatal("unknown location accepted")
	}
}
