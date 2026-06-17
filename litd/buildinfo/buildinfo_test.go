package buildinfo

import (
	"strings"
	"testing"
)

// TestUnstampedIsDevNotPublishableFSV: a build with no -ldflags stamping (the
// test binary itself) reports version "dev" and is not publishable. SoT = the
// resolved Info from the actual (unstamped) package vars.
func TestUnstampedIsDevNotPublishableFSV(t *testing.T) {
	if version != "" {
		t.Skipf("package was stamped (version=%q) — unstamped invariant not testable here", version)
	}
	got := Get()
	t.Logf("FSV unstamped: %s (publishable=%v)", got, got.Publishable())
	if got.Version != "dev" {
		t.Errorf("unstamped Version = %q, want \"dev\"", got.Version)
	}
	if got.Publishable() {
		t.Error("a dev build must not be publishable")
	}
}

// TestPublishableRules pins the publishable predicate against known identities —
// known input, known output.
func TestPublishableRules(t *testing.T) {
	cases := []struct {
		in   Info
		want bool
		why  string
	}{
		{Info{Version: "v0.4.2", Commit: "abc123", Date: "2026-06-17T00:00:00Z"}, true, "stamped clean release"},
		{Info{Version: "dev", Commit: "abc123", Date: "x"}, false, "dev version"},
		{Info{Version: "", Commit: "abc123", Date: "x"}, false, "empty version"},
		{Info{Version: "v0.4.2", Commit: "abc123-dirty", Date: "x"}, false, "dirty tree"},
		{Info{Version: "v0.4.2", Commit: "unknown", Date: "x"}, false, "unknown commit"},
		{Info{Version: "v0.4.2", Commit: "", Date: "x"}, false, "empty commit"},
	}
	for _, c := range cases {
		if got := c.in.Publishable(); got != c.want {
			t.Errorf("Publishable(%+v) = %v, want %v (%s)", c.in, got, c.want, c.why)
		}
	}
}

// TestStringFormat: --version line is exactly `version commit date`.
func TestStringFormat(t *testing.T) {
	got := Info{Version: "v1.2.3", Commit: "deadbeef", Date: "2026-06-17T12:00:00Z"}.String()
	want := "v1.2.3 deadbeef 2026-06-17T12:00:00Z"
	if got != want {
		t.Errorf("String() = %q, want %q", got, want)
	}
	// A dev build's line names "dev" first.
	if dev := (Info{Version: "dev", Commit: "abc", Date: "unknown"}).String(); !strings.HasPrefix(dev, "dev ") {
		t.Errorf("dev String() = %q, want it to start with \"dev \"", dev)
	}
}
