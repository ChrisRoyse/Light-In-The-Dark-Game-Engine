package semver

// FSV for the single shared engine-version range matcher (#180). The matcher is
// "exact and total": every (version, range) pair is compatible or incompatible,
// never unknown. SoT = the printed (version, range, verdict) table, with the
// boundary cases the issue calls out — lower bound inclusive, exclusive upper
// bound incompatible — plus the malformed-range rejection.

import "testing"

func TestVersionRange(t *testing.T) {
	cases := []struct {
		version string
		rng     string
		want    bool
		note    string
	}{
		// The canonical world range ">=0.1.0 <0.2.0" across its boundaries.
		{"0.1.0", ">=0.1.0 <0.2.0", true, "exact lower bound is INCLUSIVE"},
		{"0.0.9", ">=0.1.0 <0.2.0", false, "just below lower bound"},
		{"0.1.9", ">=0.1.0 <0.2.0", true, "max-1 (still under exclusive upper)"},
		{"0.2.0", ">=0.1.0 <0.2.0", false, "exact exclusive UPPER bound is incompatible"},
		{"0.1.5", ">=0.1.0 <0.2.0", true, "interior"},
		// Wildcard admits everything.
		{"9.9.9", "*", true, "wildcard"},
		{"0.0.0", "*", true, "wildcard admits zero"},
		// Single comparators.
		{"1.2.3", "=1.2.3", true, "exact equal"},
		{"1.2.4", "=1.2.3", false, "exact unequal"},
		{"1.2.3", "1.2.3", true, "bare version means ="},
		{"2.0.0", ">=1.0.0", true, "open upper"},
		{"0.9.0", ">=1.0.0", false, "below open lower"},
		{"1.0.0", "<=1.0.0", true, "inclusive upper boundary"},
		// Component ordering: minor/patch precedence.
		{"0.10.0", ">=0.9.0 <1.0.0", true, "minor 10 > minor 9 numerically, not lexically"},
		{"0.2.0", ">=0.10.0", false, "0.2 < 0.10 numerically"},
		// Malformed versions never satisfy a concrete range (fail-closed).
		{"1.2", ">=1.0.0", false, "two-component version is malformed"},
		{"1.2.3.4", ">=1.0.0", false, "four-component version is malformed"},
		{"v1.2.3", ">=1.0.0", false, "leading v is not part of the grammar"},
		{"", ">=1.0.0", false, "empty version"},
		{"1.2.x", ">=1.0.0", false, "non-numeric component"},
	}
	for _, c := range cases {
		got := Satisfies(c.version, c.rng)
		verdict := "INCOMPATIBLE"
		if got {
			verdict = "compatible"
		}
		t.Logf("FSV (%-8s, %-16s) -> %-12s [%s]", c.version, c.rng, verdict, c.note)
		if got != c.want {
			t.Errorf("Satisfies(%q, %q) = %v, want %v (%s)", c.version, c.rng, got, c.want, c.note)
		}
	}
}

func TestValidRange(t *testing.T) {
	valid := []string{"*", "1.2.3", "=1.2.3", ">=0.1.0 <0.2.0", ">0.0.1", "<=9.9.9", ">=1.0.0 <=2.0.0 =1.5.0"}
	invalid := []string{"", "  ", "1.2", "1.2.3.4", "v1.0.0", ">=1.0", "~1.0.0", "^1.0.0", ">= 1.0.0", "1.2.x", "abc"}
	for _, r := range valid {
		if !ValidRange(r) {
			t.Errorf("ValidRange(%q) = false, want true", r)
		} else {
			t.Logf("FSV valid range accepted: %q", r)
		}
	}
	for _, r := range invalid {
		if ValidRange(r) {
			t.Errorf("ValidRange(%q) = true, want false (malformed must be rejected at intake)", r)
		} else {
			t.Logf("FSV malformed range rejected: %q", r)
		}
	}
}

// TestMalformedRangeNeverSatisfied: a range that fails ValidRange must also be
// unsatisfiable — the two predicates can never disagree such that a malformed
// range silently admits a build.
func TestMalformedRangeNeverSatisfied(t *testing.T) {
	for _, r := range []string{">= 1.0.0", "~1.0.0", "1.2", "abc", ">=1.0"} {
		if ValidRange(r) {
			t.Fatalf("%q should be invalid", r)
		}
		if Satisfies("1.5.0", r) {
			t.Errorf("Satisfies(1.5.0, %q) = true for a malformed range — must fail closed", r)
		}
	}
	t.Logf("FSV: malformed ranges are both invalid AND unsatisfiable (no false compat)")
}
