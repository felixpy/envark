package i18n

import "testing"

func TestResolve(t *testing.T) {
	if got := Resolve("en", "zh_CN.UTF-8"); got != "en" {
		t.Fatalf("explicit english: %s", got)
	}
	if got := Resolve("system", "zh_CN.UTF-8"); got != "zh" {
		t.Fatalf("system chinese: %s", got)
	}
	if got := Resolve("system", "C"); got != "en" {
		t.Fatalf("system english: %s", got)
	}
	if got := Resolve("zh-TW"); got != "zh" {
		t.Fatalf("traditional maps to simplified catalog: %s", got)
	}
	if got := T("zh", "Space"); got != "空间" {
		t.Fatalf("catalog: %s", got)
	}
	if got := T("zh", "not in the catalog"); got != "not in the catalog" {
		t.Fatalf("fallback: %s", got)
	}
	if got := T("en", "Space"); got != "Space" {
		t.Fatalf("english: %s", got)
	}
}
