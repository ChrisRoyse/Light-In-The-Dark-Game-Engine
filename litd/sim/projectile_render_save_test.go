package sim

// #590 — ProjectileRender save/load round-trip FSV. The render statics for a
// mover projectile (Arc/guidance/Span + driving MoverID) are NOT hashed but
// ARE saved, so a game saved mid-flight reloads with its projectiles still
// drawing as arced billboards. SoT = the ProjRender columns + the published
// snapshot after a real SaveState -> LoadState into a fresh world.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestProjectileRenderSaveLoadFSV(t *testing.T) {
	mk := func() *World {
		w := NewWorld(Caps{Units: 8, Movers: 8, Projectiles: 8})
		if err := w.BindDamageMatrix(atkMatrix); err != nil {
			t.Fatal(err)
		}
		return w
	}
	w := mk()
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	victim := atkUnit(t, w, 1, xy(2000, 1000), 0)
	const arc = 48 * fixed.One
	body, ok := w.spawnMoverProjectile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Point: xy(2000, 1000), Speed: 60 * fixed.One,
		Arc: arc, GuidanceID: MissileGuidancePoint,
		Packet: DamagePacket{Source: shooter, Target: victim, Amount: 10 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn projectile")
	}
	w.Step() // mid-flight
	pr := w.ProjRender.Row(body)
	if pr == -1 {
		t.Fatal("no ProjRender record before save")
	}
	wantArc, wantGuid, wantSpan, wantMover := w.ProjRender.Arc[pr], w.ProjRender.Guidance[pr], w.ProjRender.Span[pr], w.ProjRender.Mover[pr]
	t.Logf("FSV pre-save: arc=%d guid=%d span=%d mover=%#x", wantArc.Floor(), wantGuid, wantSpan, uint32(wantMover))

	// SoT: ProjRender is NOT hashed — mutating Span must not move the state hash.
	reg := NewHashRegistry()
	var base, spanMut statehash.Snapshot
	w.HashState(reg, &base)
	w.ProjRender.Span[pr] = wantSpan + 5000
	w.HashState(reg, &spanMut)
	w.ProjRender.Span[pr] = wantSpan
	if spanMut.Top != base.Top {
		t.Fatal("ProjRender.Span entered the state hash — must be render-only")
	}

	// Save -> load into a fresh world.
	var buf bytes.Buffer
	if err := w.SaveState(&buf, 9); err != nil {
		t.Fatal(err)
	}
	loaded := mk()
	if err := loaded.LoadState(bytes.NewReader(buf.Bytes()), 9); err != nil {
		t.Fatal(err)
	}

	// SoT: the record round-trips and its mover resolves to a live mover.
	lr := loaded.ProjRender.Row(body)
	if lr == -1 {
		t.Fatal("ProjRender record lost across save/load — reloaded projectile would not render")
	}
	gotArc, gotGuid, gotSpan, gotMover := loaded.ProjRender.Arc[lr], loaded.ProjRender.Guidance[lr], loaded.ProjRender.Span[lr], loaded.ProjRender.Mover[lr]
	t.Logf("FSV post-load: arc=%d guid=%d span=%d mover=%#x", gotArc.Floor(), gotGuid, gotSpan, uint32(gotMover))
	if gotArc != wantArc || gotGuid != wantGuid || gotSpan != wantSpan || gotMover != wantMover {
		t.Fatalf("ProjRender mismatch: arc %d/%d guid %d/%d span %d/%d mover %#x/%#x",
			gotArc.Floor(), wantArc.Floor(), gotGuid, wantGuid, gotSpan, wantSpan, uint32(gotMover), uint32(wantMover))
	}
	if !loaded.Movers.Alive(gotMover) {
		t.Fatal("reloaded ProjRender.Mover does not resolve to a live mover")
	}

	// SoT: the reloaded world publishes the projectile as a billboard.
	loaded.publishSnapshot()
	saw := false
	for _, m := range loaded.Snaps.Curr().Missiles {
		if m.ID == body {
			saw = true
			if m.Arc != wantArc {
				t.Fatalf("reloaded billboard Arc=%d want %d", m.Arc.Floor(), wantArc.Floor())
			}
		}
	}
	if !saw {
		t.Fatal("reloaded projectile not published as a billboard")
	}

	// SoT: determinism continues — the saved+loaded world runs bit-identically
	// to the original from this tick (render-only ProjRender doesn't perturb it).
	var ha, hb statehash.Snapshot
	for i := 0; i < 5; i++ {
		w.Step()
		loaded.Step()
	}
	w.HashState(reg, &ha)
	loaded.HashState(reg, &hb)
	t.Logf("FSV continue-hash: original=%016x reloaded=%016x", ha.Top, hb.Top)
	if ha.Top != hb.Top {
		t.Fatal("saved+loaded world diverged from the original — projectile state not faithfully restored")
	}
}
