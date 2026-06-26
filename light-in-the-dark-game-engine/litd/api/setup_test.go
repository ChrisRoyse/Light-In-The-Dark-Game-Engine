package litd

// FSV for the public NewGame setup path (#386). SoT = (1) the seeded sim PRNG's
// draw sequence read back through RandomInt — same seed reproduces, different
// seed diverges (the seed is actually wired, R-SIM-2); (2) a unit created on the
// NewGame-built world resolving Valid() through the handle codec (the game is
// functional); (3) the fail-closed edge on a negative capacity.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func drawSeq(g *Game, n int) []int {
	s := make([]int, n)
	for i := range s {
		s[i] = g.RandomInt(0, 1_000_000)
	}
	return s
}

func seqEqual(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestNewGameFSV(t *testing.T) {
	// Happy path.
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 42})
	if err != nil || g == nil {
		t.Fatalf("NewGame: g=%v err=%v", g, err)
	}

	// SoT 1: same seed reproduces the PRNG sequence.
	g2, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 42})
	a, b := drawSeq(g, 5), drawSeq(g2, 5)
	t.Logf("FSV seed-42 sequences: A=%v B=%v", a, b)
	if !seqEqual(a, b) {
		t.Fatalf("same seed produced different sequences: %v vs %v", a, b)
	}
	// SoT 1b: a different seed diverges (proves the seed is wired, not ignored).
	g3, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 99})
	c := drawSeq(g3, 5)
	t.Logf("FSV seed-99 sequence: %v", c)
	if seqEqual(a, c) {
		t.Fatalf("different seeds produced identical sequences (seed not wired): %v", a)
	}

	// SoT 2: the world is functional — bind a def, create a unit, resolve it.
	if !g.w.BindUnitDefs([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}) {
		t.Fatal("BindUnitDefs failed on NewGame world")
	}
	typ := g.UnitType("hfoo")
	if typ.IsZero() {
		t.Fatal(`UnitType("hfoo") null on NewGame world`)
	}
	u := g.CreateUnit(Player{idx: 1, g: g}, typ, Vec2{X: 64, Y: 64}, Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit on NewGame world returned invalid")
	}
	ref, ok := RefOf(u)
	if !ok {
		t.Fatal("RefOf on NewGame-created unit failed")
	}
	h, ok := g.Resolve(ref)
	if !ok || !h.Valid() {
		t.Fatalf("handle round-trip on NewGame world failed: ok=%v valid=%v", ok, h != nil && h.Valid())
	}
	t.Logf("FSV functional: unit created + handle resolves Valid on a NewGame-built world")

	// Edge: negative capacity fails closed (no silent clamp).
	if _, err := NewGame(GameOptions{MaxUnits: -1}); err == nil {
		t.Fatal("NewGame(MaxUnits:-1) must error")
	} else {
		t.Logf("FSV edge: negative cap rejected: %v", err)
	}
	// Edge: the zero value builds a valid default game.
	gd, err := NewGame(GameOptions{})
	if err != nil || gd == nil {
		t.Fatalf("NewGame(zero) must succeed, got g=%v err=%v", gd, err)
	}
	t.Logf("FSV edge: default (zero) options build a valid game")
}
