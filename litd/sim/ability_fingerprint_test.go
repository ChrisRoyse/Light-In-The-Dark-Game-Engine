package sim

// #596 FSV — ability content fingerprint. SoT = the fingerprint value and the
// LoadState join result (accept/reject). A book is registered into a world;
// the fingerprint and the cross-load gate are read directly.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// fpBook compiles a spec list into a fresh book and returns its fingerprint.
func fpBook(t *testing.T, srcs ...AbilitySpecSource) uint64 {
	t.Helper()
	bk := NewAbilityBook()
	for _, s := range srcs {
		spec, err := CompileAbilitySpec(s, interpResolver{})
		if err != nil {
			t.Fatalf("compile %q: %v", s.ID, err)
		}
		bk.RegisterSpec(spec)
	}
	return bk.Fingerprint()
}

func srcFireball(cd float64) AbilitySpecSource {
	return AbilitySpecSource{
		ID: "fireball", Name: "Fireball", CastType: "active", CastRange: 900, Cooldown: cd,
		OnCast: []OpSource{
			{Op: "spawn_projectile"},
			{Op: "attach_mover", Mover: "linear", Effects: "impact", Speed: 30, Range: 900, Radius: 64},
		},
	}
}

func srcBlink() AbilitySpecSource {
	return AbilitySpecSource{
		ID: "blink", Name: "Blink", CastType: "active", CastRange: 600,
		OnCast: []OpSource{{Op: "emit_event", Event: "ability.impact", Arg: 1}},
	}
}

// TestFPIdentical: two books built from byte-identical sources hash equal.
func TestFPIdentical(t *testing.T) {
	a := fpBook(t, srcFireball(6.0), srcBlink())
	b := fpBook(t, srcFireball(6.0), srcBlink())
	t.Logf("A=%016x B=%016x", a, b)
	if a != b {
		t.Fatalf("identical files differ: %016x != %016x", a, b)
	}
	if a == 0 {
		t.Fatal("fingerprint is zero (no content folded)")
	}
}

// TestFPOneNumberDiffers: a single changed cooldown changes the fingerprint.
func TestFPOneNumberDiffers(t *testing.T) {
	a := fpBook(t, srcFireball(6.0))
	b := fpBook(t, srcFireball(6.5)) // one differing number
	t.Logf("cd=6.0 -> %016x ; cd=6.5 -> %016x", a, b)
	if a == b {
		t.Fatalf("changed cooldown left fingerprint unchanged: %016x", a)
	}
}

// TestFPAddedAbility: adding an ability changes the fingerprint.
func TestFPAddedAbility(t *testing.T) {
	a := fpBook(t, srcFireball(6.0))
	b := fpBook(t, srcFireball(6.0), srcBlink())
	t.Logf("1 ability -> %016x ; 2 abilities -> %016x", a, b)
	if a == b {
		t.Fatalf("added ability left fingerprint unchanged: %016x", a)
	}
}

// TestFPReorderedOpsDiffers: reordering on_cast ops changes the fingerprint
// (order is behavior — spawn-then-move ≠ move-then-spawn).
func TestFPReorderedOpsDiffers(t *testing.T) {
	forward := AbilitySpecSource{ID: "x", OnCast: []OpSource{
		{Op: "spawn_projectile"},
		{Op: "attach_mover", Mover: "linear", Speed: 30, Range: 100},
	}}
	reversed := AbilitySpecSource{ID: "x", OnCast: []OpSource{
		{Op: "attach_mover", Mover: "linear", Speed: 30, Range: 100},
		{Op: "spawn_projectile"},
	}}
	a := fpBook(t, forward)
	b := fpBook(t, reversed)
	t.Logf("forward=%016x reversed=%016x", a, b)
	if a == b {
		t.Fatalf("reordered ops collide (order must matter): %016x", a)
	}
}

// TestFPReorderedSpecsDiffers: registration order of specs is part of the
// fingerprint (block indices and dispatch order depend on it).
func TestFPReorderedSpecsDiffers(t *testing.T) {
	a := fpBook(t, srcFireball(6.0), srcBlink())
	b := fpBook(t, srcBlink(), srcFireball(6.0))
	t.Logf("fb,blink=%016x ; blink,fb=%016x", a, b)
	if a == b {
		t.Fatalf("spec order collides: %016x", a)
	}
}

// TestFPJoinGateRejectsMismatch: peer A saves under its JoinFingerprint; peer
// B (one differing ability number) refuses the load. SoT = LoadState error.
func TestFPJoinGateRejectsMismatch(t *testing.T) {
	build := func(cd float64) *World {
		w := NewWorld(Caps{Units: 16})
		w.SetDataFingerprint(0xDA7A) // same data tables on both peers
		spec, err := CompileAbilitySpec(srcFireball(cd), interpResolver{})
		if err != nil {
			t.Fatal(err)
		}
		w.AbilityDefs.RegisterSpec(spec)
		return w
	}
	peerA := build(6.0)
	peerB := build(6.5) // identical except one ability number

	fpA := peerA.JoinFingerprint()
	fpB := peerB.JoinFingerprint()
	t.Logf("peerA JoinFP=%016x  peerB JoinFP=%016x", fpA, fpB)
	if fpA == fpB {
		t.Fatal("peers with differing ability files share a join fingerprint (would desync)")
	}

	// A serializes under its own fingerprint.
	var buf bytes.Buffer
	if err := peerA.SaveState(&buf, fpA); err != nil {
		t.Fatalf("save: %v", err)
	}
	// B loads expecting ITS fingerprint → rejected (fail-closed).
	freshB := NewWorld(Caps{Units: 16})
	freshB.SetDataFingerprint(0xDA7A)
	errB := freshB.LoadState(bytes.NewReader(buf.Bytes()), fpB)
	t.Logf("B load result: %v", errB)
	if errB == nil {
		t.Fatal("B accepted A's save despite a different ability file — silent desync")
	}

	// Control: a matching peer A' loads A's save under the SAME fingerprint.
	freshA := NewWorld(Caps{Units: 16})
	freshA.SetDataFingerprint(0xDA7A)
	if err := freshA.LoadState(bytes.NewReader(buf.Bytes()), fpA); err != nil {
		t.Fatalf("matching peer rejected a valid save: %v", err)
	}
	t.Logf("matching peer accepted (fp=%016x)", fpA)
	_ = fixed.One
}
