package render

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Snapshot interpolation — the one-way sim→render bridge (PRD R-SIM-1, §4.1;
// batching-and-draw-calls.md §6.3; M4 deliverable 1).
//
// The sim advances at a fixed 20 Hz; the renderer runs faster. Each frame the
// renderer alpha-lerps entity transforms between the previous and current sim
// snapshot, so motion looks smooth without the sim ever knowing the frame rate.
// The bridge is one-way and read-only: this file imports only litd/fixed (the
// leaf fixed-point type), never litd/sim — the import-graph check that forbids
// render→sim-for-mutation stays trivially green.
//
// Snapshots are SoA and slot-aligned: index i is the same entity in prev and
// curr. Spawn/teleport/death are marked per slot so they snap instead of
// sliding — a teleported unit must not glide across the map for a frame.

const fixedOne = float64(int64(1) << 32)

// fixedToF32 converts a sim 32.32 fixed-point scalar to a render float32. This
// is the only place sim coordinates cross into render space; the fixed value
// remains the source of truth.
func fixedToF32(v fixed.F64) float32 { return float32(float64(v) / fixedOne) }

// Snapshot is one 20 Hz sim frame's entity transforms, SoA and slot-indexed.
// Present[i] marks slot i as a live entity this frame; Snap[i] marks it as
// spawn/teleport/death — render takes the current value without interpolating.
type Snapshot struct {
	Tick    uint64
	X       []fixed.F64
	Y       []fixed.F64
	Z       []fixed.F64
	Facing  []fixed.F64
	Present []bool
	Snap    []bool
}

// Len returns the slot count of the snapshot (by the Present column).
func (s *Snapshot) Len() int { return len(s.Present) }

// InterpBuffer holds the render-rate interpolated transforms, SoA float32.
// Reused across frames; grown only when the slot count rises, so steady-state
// interpolation allocates nothing (R-GC-3).
type InterpBuffer struct {
	X      []float32
	Y      []float32
	Z      []float32
	Facing []float32
	Active []bool
	n      int
}

// Len returns the number of slots the last Interpolate filled.
func (b *InterpBuffer) Len() int { return b.n }

// ensure grows the buffers to at least n slots. Grows by reslice/allocation
// only when capacity is exceeded — once warmed to the peak entity count, steady
// state never reallocates.
func (b *InterpBuffer) ensure(n int) {
	if cap(b.X) >= n {
		b.X = b.X[:n]
		b.Y = b.Y[:n]
		b.Z = b.Z[:n]
		b.Facing = b.Facing[:n]
		b.Active = b.Active[:n]
		return
	}
	b.X = make([]float32, n)
	b.Y = make([]float32, n)
	b.Z = make([]float32, n)
	b.Facing = make([]float32, n)
	b.Active = make([]bool, n)
}

// Interpolate fills dst by alpha-lerping prev→curr per slot. curr is
// authoritative for which slots are live. A slot snaps to its current value
// (no interpolation) when it is flagged Snap, when it is absent from prev
// (a spawn), or when alpha is outside (0,1). alpha is clamped to [0,1].
//
// Lerp is done in render float32 after the fixed→float boundary conversion;
// the sim fixed-point values remain the truth this is checked against. Zero
// allocations once dst is warmed to the slot count.
func Interpolate(dst *InterpBuffer, prev, curr *Snapshot, alpha float32) {
	if alpha < 0 {
		alpha = 0
	} else if alpha > 1 {
		alpha = 1
	}
	n := curr.Len()
	dst.ensure(n)
	dst.n = n
	prevN := prev.Len()
	for i := 0; i < n; i++ {
		if !curr.Present[i] {
			dst.Active[i] = false
			continue
		}
		dst.Active[i] = true
		cx, cy, cz, cf := fixedToF32(curr.X[i]), fixedToF32(curr.Y[i]), fixedToF32(curr.Z[i]), fixedToF32(curr.Facing[i])
		// Snap: spawn/teleport/death, no prev counterpart, or degenerate alpha.
		if curr.Snap[i] || i >= prevN || !prev.Present[i] {
			dst.X[i], dst.Y[i], dst.Z[i], dst.Facing[i] = cx, cy, cz, cf
			continue
		}
		px, py, pz, pf := fixedToF32(prev.X[i]), fixedToF32(prev.Y[i]), fixedToF32(prev.Z[i]), fixedToF32(prev.Facing[i])
		dst.X[i] = px + (cx-px)*alpha
		dst.Y[i] = py + (cy-py)*alpha
		dst.Z[i] = pz + (cz-pz)*alpha
		dst.Facing[i] = pf + (cf-pf)*alpha
	}
}
