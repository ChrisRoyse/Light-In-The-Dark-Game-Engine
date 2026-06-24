package data

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Manual FSV (#628): synthetic known inputs → known fixed-point outputs, read
// straight off the lowered struct (the SoT), plus the R-ABL-2 rejection edges.
func TestLowerAbilitySpecFSV(t *testing.T) {
	// Happy path: known floats → known fixed-point.
	// CastRange 5.0 wu  -> 5 * 2^32 = 21474836480
	// Cooldown  2.0 s   -> 2000ms / 50ms = 40 ticks
	// Speed     600 wu  -> 600 * 2^32
	// AngVel    90 deg  -> 90 * 65536/360 = 16384 BAM
	src := AbilitySpecSource{
		ID: "fsv", CastType: "active", CastRange: 5.0, ManaCost: 25, Cooldown: 2.0,
		OnCast: []OpSource{{Op: "attach_mover", Mover: "linear", Speed: 600.0, AngVel: 90.0, Decay: 700}},
	}
	lo, err := LowerAbilitySpec(src)
	if err != nil {
		t.Fatalf("happy path errored: %v", err)
	}
	t.Logf("FSV lowered: CastRange=%d Cooldown=%d ManaCost=%d | op Speed=%d AngVel=%d Decay=%d",
		int64(lo.CastRange), lo.Cooldown, lo.ManaCost,
		int64(lo.OnCast[0].Speed), int64(lo.OnCast[0].AngVel), lo.OnCast[0].Decay)
	if lo.CastRange != 5*fixed.One {
		t.Fatalf("CastRange = %d, want %d (5.0 wu)", int64(lo.CastRange), int64(5*fixed.One))
	}
	if lo.Cooldown != 40 {
		t.Fatalf("Cooldown = %d ticks, want 40 (2.0s / 50ms)", lo.Cooldown)
	}
	if lo.ManaCost != 25 {
		t.Fatalf("ManaCost = %d, want 25", lo.ManaCost)
	}
	if lo.OnCast[0].Speed != 600*fixed.One {
		t.Fatalf("Speed = %d, want %d (600 wu)", int64(lo.OnCast[0].Speed), int64(600*fixed.One))
	}
	if lo.OnCast[0].AngVel != 16384 {
		t.Fatalf("AngVel = %d BAM, want 16384 (90 deg)", int64(lo.OnCast[0].AngVel))
	}
	if lo.OnCast[0].Decay != 700 {
		t.Fatalf("Decay = %d, want 700", lo.OnCast[0].Decay)
	}

	// R-ABL-2 rejection edges — each must fail closed (non-nil error).
	edges := []struct {
		name string
		src  AbilitySpecSource
	}{
		{"negative mana", AbilitySpecSource{ID: "e", ManaCost: -1}},
		{"NaN castRange", AbilitySpecSource{ID: "e", CastRange: nan()}},
		{"Inf cooldown", AbilitySpecSource{ID: "e", Cooldown: inf()}},
		{"negative duration", AbilitySpecSource{ID: "e", Cooldown: -2.0}},
		{"out-of-fixed-range speed", AbilitySpecSource{ID: "e", OnCast: []OpSource{{Op: "x", Speed: 1e15}}}},
		{"decay out of range", AbilitySpecSource{ID: "e", OnCast: []OpSource{{Op: "x", Decay: 1001}}}},
	}
	for _, e := range edges {
		_, err := LowerAbilitySpec(e.src)
		t.Logf("FSV edge %q -> err=%v", e.name, err)
		if err == nil {
			t.Fatalf("edge %q: expected fail-closed rejection, got nil error", e.name)
		}
	}
}

func nan() float64 { z := 0.0; return z / z }
func inf() float64 { z := 0.0; return 1.0 / z }
