package sim

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #594 — AbilitySpec compile/validate, fail-closed. SoT = the compiled
// spec fields and the reject errors.

// fakeResolver resolves a fixed set of names; everything else is unknown.
type fakeResolver struct{}

func (fakeResolver) EffectListByName(n string) (data.EffectList, bool) {
	if n == "fireball_impact" {
		return data.EffectList{Off: 0, Len: 2}, true
	}
	return data.EffectList{}, false
}
func (fakeResolver) EventKindByName(n string) (uint16, bool) {
	if n == "ability.impact" {
		return 80, true
	}
	return 0, false
}
func (fakeResolver) MoverKindByName(n string) (MoverKind, bool) {
	switch n {
	case "linear":
		return MoverLinear, true
	case "arc":
		return MoverArc, true
	}
	return 0, false
}
func (fakeResolver) KeyID(n string) uint32 { return 7 }

func validFireball() data.AbilitySpecSource {
	return data.AbilitySpecSource{
		ID: "fireball", Name: "Fireball", CastType: "active", Indicator: "line",
		CastRange: 900, ManaCost: 75, Cooldown: 6.0,
		Precast: 0.3, CastPoint: 0.0, Backswing: 0.4,
		OnCast: []data.OpSource{
			{Op: "attach_mover", Mover: "linear", Speed: 30, Range: 900, Radius: 64, Pierce: 1},
			{Op: "run_effects", Effects: "fireball_impact"},
			{Op: "emit_event", Event: "ability.impact", Arg: 1},
		},
	}
}

func TestAbilityCompileValid(t *testing.T) {
	spec, err := compileSrc(validFireball(), fakeResolver{})
	if err != nil {
		t.Fatalf("valid spec rejected: %v", err)
	}
	// SoT: compiled fields. cooldown 6s → 120 ticks (TickMS 50); precast 0.3s → 6.
	if spec.Cooldown != 120 || spec.Precast != 6 || spec.Backswing != 8 {
		t.Fatalf("timing ticks: cd=%d pre=%d bs=%d, want 120/6/8", spec.Cooldown, spec.Precast, spec.Backswing)
	}
	if spec.CastType != CastActive || spec.Indicator != IndicatorLine {
		t.Fatalf("cast type/indicator wrong: %d/%d", spec.CastType, spec.Indicator)
	}
	if spec.CastRange != 900*fixed.One {
		t.Fatalf("cast_range = %d, want 900", spec.CastRange)
	}
	if len(spec.OnCast) != 3 || spec.OnCast[0].Kind != OpAttachMover || spec.OnCast[0].MoverKind != MoverLinear {
		t.Fatalf("on_cast ops wrong: %+v", spec.OnCast)
	}
	if spec.OnCast[0].Speed != 30*fixed.One || spec.OnCast[1].EffectList.Len != 2 || spec.OnCast[2].EventKind != 80 {
		t.Fatalf("op refs unresolved: %+v", spec.OnCast)
	}
}

func TestAbilityRejectUnknownOp(t *testing.T) {
	src := validFireball()
	src.OnCast = append(src.OnCast, data.OpSource{Op: "teleport_to_moon"})
	_, err := compileSrc(src, fakeResolver{})
	if err == nil || !strings.Contains(err.Error(), "unknown op") {
		t.Fatalf("expected unknown-op reject, got %v", err)
	}
}

func TestAbilityRejectMissingEffectRef(t *testing.T) {
	src := validFireball()
	src.OnCast[1].Effects = "nonexistent_list"
	_, err := compileSrc(src, fakeResolver{})
	if err == nil || !strings.Contains(err.Error(), "unknown effect list") {
		t.Fatalf("expected missing-effect reject, got %v", err)
	}
}

func TestAbilityRejectOutOfRangeCooldown(t *testing.T) {
	src := validFireball()
	src.Cooldown = 1e9 // seconds → way past uint16 ticks
	_, err := compileSrc(src, fakeResolver{})
	if err == nil || !strings.Contains(err.Error(), "tick limit") {
		t.Fatalf("expected cooldown out-of-range reject, got %v", err)
	}
	src2 := validFireball()
	src2.Cooldown = -1
	if _, err := compileSrc(src2, fakeResolver{}); err == nil {
		t.Fatal("expected negative-cooldown reject")
	}
}

func TestAbilityRejectPrecisionLoss(t *testing.T) {
	// A value with more precision than 2^-32 can represent exactly: use a
	// NaN/Inf (definitely unrepresentable) and a huge magnitude.
	src := validFireball()
	src.CastRange = 1e30 // out of fixed range
	if _, err := compileSrc(src, fakeResolver{}); err == nil {
		t.Fatal("expected out-of-fixed-range reject for cast_range 1e30")
	}
}

func TestAbilityRejectMissingEventName(t *testing.T) {
	src := validFireball()
	src.OnCast[2].Event = "" // emit_event with no event
	_, err := compileSrc(src, fakeResolver{})
	if err == nil || !strings.Contains(err.Error(), "emit_event needs") {
		t.Fatalf("expected emit_event reject, got %v", err)
	}
}
