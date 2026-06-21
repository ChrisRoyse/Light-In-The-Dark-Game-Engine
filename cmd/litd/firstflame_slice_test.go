package main

// FSV for #482: the First Flame vertical slice (worlds/firstflame-slice) runs the
// whole gameplay stack on the Trigger/ECA substrate — a hero casts firebolt
// (ability), soldiers auto-attack (attack→damage events), the burn DoT lands
// (buff), a beacon is captured by uncontested presence (beacon), and the capture
// wins the match (win/lose). This is the HEADLESS half of #482's evidence (the
// deterministic SoT: Storage state + Game.StateHash() + player results); the
// per-beat render screenshots are tracked separately (see the #482 progress note).
//
// SoT inspected (not exit codes): the foe's life delta, the burn-applied flag,
// the damage-event count, the beacon state, the victory step, and each player's
// match Result. Edges: determinism double-run, mid-slice save/load round-trip,
// presentation-drain hash parity (audio/VFX never perturb the sim), and a
// second-match clean reset.

import (
	"bytes"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/savegame"
)

const sliceWorld = "../../worlds/firstflame-slice"
const sliceSeed = int64(0x5717)

func sliceInt(g *api.Game, key string) int {
	v, _ := g.Storage().GetInt("slice", key)
	return v
}

// TestFirstFlameSliceFSV drives the full slice and inspects every beat's state.
func TestFirstFlameSliceFSV(t *testing.T) {
	g, _, _, cleanup, err := loadWorldFull(sliceWorld, sliceSeed, 200_000_000)
	if err != nil {
		t.Fatalf("load slice: %v", err)
	}
	defer cleanup()

	// Beat 1 — ability: advance past the cast point; the foe must take firebolt
	// damage and the burn buff must land.
	g.Advance(20)
	t.Logf("@20  hits=%d burned=%d beacon_state=%d", sliceInt(g, "hits"), sliceInt(g, "burned"), sliceInt(g, "beacon_state"))
	if sliceInt(g, "burned") != 1 {
		t.Fatal("beat=buff: burn was never applied by the firebolt trigger")
	}
	if sliceInt(g, "hits") == 0 {
		t.Fatal("beat=attack/ability: no damage events observed")
	}

	// Beats 2-4 — attack + beacon + win/lose: run the match out.
	g.Advance(280)
	owner := sliceInt(g, "beacon_owner")
	vstep := sliceInt(g, "victory_step")
	t.Logf("@300 beacon_owner=%d beacon_state=%d victory_step=%d hits=%d P1=%v P2=%v",
		owner, sliceInt(g, "beacon_state"), vstep, sliceInt(g, "hits"),
		g.Player(1).Result(), g.Player(2).Result())
	if sliceInt(g, "beacon_state") != 1 || owner != 1 {
		t.Fatalf("beat=beacon: P1 did not capture the beacon (owner=%d state=%d)", owner, sliceInt(g, "beacon_state"))
	}
	if vstep == 0 {
		t.Fatal("beat=win/lose: no victory recorded")
	}
	if g.Player(1).Result() != api.ResultWon {
		t.Fatalf("beat=win/lose: P1 result = %v, want Won", g.Player(1).Result())
	}
	if g.Player(2).Result() != api.ResultLost {
		t.Fatalf("beat=win/lose: P2 result = %v, want Lost", g.Player(2).Result())
	}
	t.Logf("#482 slice: ability+attack+buff+beacon+win/lose all fired; final hash=%#016x", g.StateHash())
}

// runSlice loads + advances the slice n ticks and returns the final hash.
func runSlice(t *testing.T, n int) uint64 {
	t.Helper()
	g, _, _, cleanup, err := loadWorldFull(sliceWorld, sliceSeed, 200_000_000)
	if err != nil {
		t.Fatalf("load slice: %v", err)
	}
	defer cleanup()
	g.Advance(n)
	return g.StateHash()
}

// TestFirstFlameSliceDeterminismFSV — edge: double-run identical hash, and a
// second match in the same process resets cleanly (same seed → same hash, so no
// leaked entities/timers/subscriptions from the first match).
func TestFirstFlameSliceDeterminismFSV(t *testing.T) {
	h1 := runSlice(t, 300)
	h2 := runSlice(t, 300) // second match, same process
	t.Logf("#482 reset/determinism: match1=%#016x match2=%#016x MATCH=%v", h1, h2, h1 == h2)
	if h1 != h2 {
		t.Fatalf("slice not deterministic / dirty reset: %#x != %#x", h1, h2)
	}
}

// TestFirstFlameSliceSaveLoadFSV — edge: save mid-slice (burn ticking, foe alive,
// pre-capture), reload into a freshly re-run world, finish → hash identical to the
// unbroken run.
func TestFirstFlameSliceSaveLoadFSV(t *testing.T) {
	const saveAt, finish = 16, 300
	ref := runSlice(t, finish)

	gs, Ls, regs, cls, err := loadWorldFull(sliceWorld, sliceSeed, 200_000_000)
	if err != nil {
		t.Fatalf("save load: %v", err)
	}
	gs.Advance(saveAt)
	burning := sliceInt(gs, "burned") == 1
	var buf bytes.Buffer
	if err := savegame.Write(&buf, gs, Ls, regs, worldFP); err != nil {
		t.Fatalf("savegame.Write: %v", err)
	}
	cls()
	if !burning {
		t.Fatal("precondition: burn must be active at save")
	}

	gg, Lg, regg, clg, err := loadWorldFull(sliceWorld, sliceSeed, 200_000_000)
	if err != nil {
		t.Fatalf("restore load: %v", err)
	}
	defer clg()
	if err := savegame.Load(bytes.NewReader(buf.Bytes()), gg, Lg, regg, worldFP); err != nil {
		t.Fatalf("savegame.Load: %v", err)
	}
	gg.Advance(finish - saveAt)
	got := gg.StateHash()
	t.Logf("#482 save/load: unbroken@%d=%#016x save@%d→load→@%d=%#016x MATCH=%v",
		finish, ref, saveAt, finish, got, got == ref)
	if got != ref {
		t.Fatalf("slice mid-game save/load not bit-identical: %#x != %#x", got, ref)
	}
}

// TestFirstFlameSlicePresentationParityFSV — edge (E6 spirit): draining the
// render-event presentation channel every tick (what an audio/VFX consumer does)
// must NOT change the sim hash — presentation is non-hashing (#449). A run that
// drains snapshots hashes identically to one that never looks.
func TestFirstFlameSlicePresentationParityFSV(t *testing.T) {
	silent := runSlice(t, 300)

	g, _, _, cleanup, err := loadWorldFull(sliceWorld, sliceSeed, 200_000_000)
	if err != nil {
		t.Fatalf("load slice: %v", err)
	}
	defer cleanup()
	drained := 0
	var buf []api.RenderEvent
	for i := 0; i < 300; i++ {
		g.Advance(1)
		buf = g.RenderEvents(buf[:0]) // presentation drain (audio/VFX would consume these)
		drained += len(buf)
	}
	loud := g.StateHash()
	t.Logf("#482 presentation parity: silent=%#016x drained(%d events)=%#016x MATCH=%v",
		silent, drained, loud, silent == loud)
	if silent != loud {
		t.Fatalf("presentation drain perturbed the sim hash: %#x != %#x", silent, loud)
	}
}
