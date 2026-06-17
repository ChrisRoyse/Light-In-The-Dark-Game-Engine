package render

import (
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/terrain"
	"github.com/g3n/engine/math32"
)

func TestSelectionPoolAcquireReleaseFSV(t *testing.T) {
	p := NewSelectionDecalPool(4)
	if p.Cap() != 4 || p.ActiveCount() != 0 {
		t.Fatalf("fresh pool cap=%d active=%d want 4,0", p.Cap(), p.ActiveCount())
	}
	ids := []int{}
	for i := 0; i < 4; i++ {
		id, ok := p.Acquire(math32.Vector2{X: float32(i)}, 32, RelSelf)
		if !ok {
			t.Fatalf("acquire %d failed in a pool of 4", i)
		}
		ids = append(ids, id)
	}
	if p.ActiveCount() != 4 {
		t.Fatalf("active=%d want 4", p.ActiveCount())
	}
	if _, ok := p.Acquire(math32.Vector2{}, 32, RelSelf); ok {
		t.Fatal("acquire past capacity must fail (fail-closed)")
	}
	p.Release(ids[1])
	if p.ActiveCount() != 3 {
		t.Fatalf("after release active=%d want 3", p.ActiveCount())
	}
	reid, ok := p.Acquire(math32.Vector2{X: 99}, 16, RelEnemy)
	if !ok || reid != ids[1] {
		t.Fatalf("freed slot %d not reused (got %d ok=%v)", ids[1], reid, ok)
	}
	t.Logf("FSV pool acquire/deny/release/reuse: cap=4 active=%d reused slot=%d", p.ActiveCount(), reid)
}

func TestRelationshipTintFSV(t *testing.T) {
	cases := []struct {
		rel  Relationship
		want RGBA
	}{
		{RelSelf, RGBA{0, 1, 0, 1}},
		{RelAlly, RGBA{0, 0.6, 1, 1}},
		{RelEnemy, RGBA{1, 0, 0, 1}},
		{RelNeutral, RGBA{1, 0.85, 0, 1}},
	}
	for _, c := range cases {
		got := RelationshipTint(c.rel)
		t.Logf("FSV tint rel=%d -> %+v", c.rel, got)
		if got != c.want {
			t.Fatalf("rel %d tint=%+v want %+v", c.rel, got, c.want)
		}
	}
}

// TestSelectionDrapeSlopeFSV — corners draped on a known slope get the exact
// surface heights (corner-draped, X+X=Y). Slope h(x,z) = 0.5x + 2z.
func TestSelectionDrapeSlopeFSV(t *testing.T) {
	h := func(x, z float32) float32 { return 0.5*x + 2*z }
	p := NewSelectionDecalPool(2)
	const cx, cz, r = 100, 50, 10
	id, _ := p.Acquire(math32.Vector2{X: cx, Y: cz}, r, RelSelf)
	p.Drape(h)
	d := p.At(id)
	want := [4]float32{h(cx-r, cz-r), h(cx+r, cz-r), h(cx+r, cz+r), h(cx-r, cz+r)}
	t.Logf("FSV drape corners got=%v want=%v", d.CornerY, want)
	if d.CornerY != want {
		t.Fatalf("draped corners %v want %v", d.CornerY, want)
	}
}

// TestSelectionDrapeRealMapFSV — drape on the real test64 terrain via the #81
// height sampler; corner heights must equal the sampler's surface.
func TestSelectionDrapeRealMapFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../.."), "data/maps/test64")
	if err != nil {
		t.Fatalf("load test64: %v", err)
	}
	s := terrain.NewMapHeightSampler(m)
	hf := func(x, z float32) float32 { h, _ := s.SampleHeight(x, z); return h }
	p := NewSelectionDecalPool(1)
	const cx, cz, r = 0, 0, 64
	id, _ := p.Acquire(math32.Vector2{X: cx, Y: cz}, r, RelSelf)
	p.Drape(hf)
	d := p.At(id)
	want := [4]float32{hf(cx-r, cz-r), hf(cx+r, cz-r), hf(cx+r, cz+r), hf(cx-r, cz+r)}
	t.Logf("FSV realmap drape corners=%v", d.CornerY)
	if d.CornerY != want {
		t.Fatalf("realmap draped corners %v want %v", d.CornerY, want)
	}
}

func TestSelectionDrapeZeroAllocFSV(t *testing.T) {
	h := func(x, z float32) float32 { return x + z }
	p := NewSelectionDecalPool(64)
	for i := 0; i < 64; i++ {
		p.Acquire(math32.Vector2{X: float32(i * 10)}, 8, RelAlly)
	}
	p.Drape(h) // warm
	allocs := testing.AllocsPerRun(200, func() { p.Drape(h) })
	t.Logf("FSV drape allocs/op=%v (64 decals)", allocs)
	if allocs != 0 {
		t.Fatalf("Drape allocates %v/op, want 0", allocs)
	}
}
