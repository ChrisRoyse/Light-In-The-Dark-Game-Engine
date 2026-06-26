package data

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Regression for #631: Speed/Range/Radius feed the integer swept-collision
// projection (litd/sim/mover_collision.go), which forms (Δ²)·l2 — at absurd
// magnitudes (≥~1.5M u/tick) that wraps int64 and returns a wrong (but
// deterministic) collision result. LowerAbilitySpec must reject such values at
// authoring (fail-closed), so the sim projection's overflow-safe invariant holds
// for every accepted spec. SoT = the returned error / the lowered struct fields.
func TestAbilityProjectionExtentFSV(t *testing.T) {
	mk := func(field string, v float64) AbilitySpecSource {
		op := OpSource{Op: "attach_mover", Mover: "linear"}
		switch field {
		case "speed":
			op.Speed = v
		case "range":
			op.Range = v
		case "radius":
			op.Radius = v
		}
		return AbilitySpecSource{ID: "ext", CastType: "active", OnCast: []OpSource{op}}
	}

	// Accepted: at the cap and at a realistic value — must lower cleanly and the
	// fixed-point SoT must equal the input (no false rejection).
	for _, ok := range []struct {
		field string
		v     float64
	}{
		{"speed", maxProjectionExtent}, // exactly at the boundary
		{"radius", maxProjectionExtent},
		{"range", maxProjectionExtent},
		{"speed", 600.0}, // real demo value
		{"radius", 64.0},
	} {
		lo, err := LowerAbilitySpec(mk(ok.field, ok.v))
		if err != nil {
			t.Fatalf("ACCEPT %s=%g rejected: %v", ok.field, ok.v, err)
		}
		var got fixed.F64
		switch ok.field {
		case "speed":
			got = lo.OnCast[0].Speed
		case "range":
			got = lo.OnCast[0].Range
		case "radius":
			got = lo.OnCast[0].Radius
		}
		want := fixed.F64(int64(ok.v) << 32)
		t.Logf("ACCEPT %s=%g -> lowered F64=%d (want %d)", ok.field, ok.v, int64(got), int64(want))
		if got != want {
			t.Fatalf("ACCEPT %s=%g lowered to %d, want %d", ok.field, ok.v, int64(got), int64(want))
		}
	}

	// Rejected: just over the cap, well over, and negative magnitude — all must
	// fail closed (non-nil error), never silently truncate or pass through.
	for _, bad := range []struct {
		field string
		v     float64
	}{
		{"speed", maxProjectionExtent + 1}, // boundary+1
		{"radius", 2 * maxProjectionExtent},
		{"range", 5 * maxProjectionExtent},
		{"speed", -2 * maxProjectionExtent}, // negative magnitude
		{"speed", 1.5e6},                    // the original #631 overflow trigger
	} {
		_, err := LowerAbilitySpec(mk(bad.field, bad.v))
		t.Logf("REJECT %s=%g -> err=%v", bad.field, bad.v, err)
		if err == nil {
			t.Fatalf("REJECT %s=%g passed lowering (fail-OPEN); sim projection would overflow (#631)", bad.field, bad.v)
		}
	}
}
