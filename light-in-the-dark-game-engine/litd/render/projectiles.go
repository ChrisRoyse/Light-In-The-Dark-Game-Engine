package render

// Missile billboard builder (#309, R-GC-2).
//
// Missiles publish to the render snapshot as a separate surface
// (sim.Snapshot.Missiles) — ground position, facing, presentation arc height,
// and guidance kind. They draw as camera-facing billboards riding a parabola,
// not as unit models. This builder turns the per-frame missile set into the
// billboard draw list: a fixed-capacity, reused backing slice (zero steady-state
// alloc, data-only — the render frame draws the active set, same posture as the
// impact and aura pools). It owns no GL and imports no sim: the driver converts
// the snapshot's fixed-point values to float at the seam and hands plain inputs
// in, keeping render ⊥ sim.
//
// The arc is presentation-only — the missile's flight is straight (the sim moves
// it in the ground plane); the height is a parabola the renderer paints on top,
// peaking at Arc when the missile is half-way to impact.

// MaxProjectiles caps the billboard draw list. Spell-heavy fights throw many
// missiles at once; over the cap the newest are dropped and BuildInto reports the
// count so the drop is never silent.
const MaxProjectiles = 256

// MissileBillboardInput is one missile's per-frame render input, in float world
// space (the driver has already converted from sim fixed-point). Progress is the
// flight fraction in [0,1] (0 = just launched, 1 = at impact) that places the
// missile on its arc; the driver supplies it (clamped here defensively).
type MissileBillboardInput struct {
	Key      uint32  // missile entity id (stable identity for interpolation)
	GroundX  float32 // world ground position (the missile's sim XZ)
	GroundZ  float32
	Arc      float32 // parabola peak height (presentation only)
	Progress float32 // flight fraction [0,1]
	Facing   float32 // sprite orientation (radians)
	Guidance uint16  // MissileGuidance* kind — selects the sprite
}

// MissileBillboard is one resolved billboard to draw: world position with the arc
// height baked into Y, plus facing and guidance for sprite selection.
type MissileBillboard struct {
	Key      uint32
	X, Y, Z  float32
	Facing   float32
	Guidance uint16
}

// ProjectileBillboards builds the missile billboard draw list each frame into a
// reused backing slice. The zero value is not usable — call NewProjectileBillboards.
type ProjectileBillboards struct {
	active  []MissileBillboard
	dropped int
}

// NewProjectileBillboards preallocates the backing slice to MaxProjectiles so
// BuildInto never grows it.
func NewProjectileBillboards() *ProjectileBillboards {
	return &ProjectileBillboards{active: make([]MissileBillboard, 0, MaxProjectiles)}
}

// ArcHeight returns the presentation height for a missile at flight fraction
// progress: a parabola that is 0 at launch and impact and peaks at arc when
// progress is 0.5 — h = arc * 4 * p * (1-p). progress is clamped to [0,1]; a
// non-positive arc yields a flat (ground) path.
func ArcHeight(arc, progress float32) float32 {
	if progress < 0 {
		progress = 0
	} else if progress > 1 {
		progress = 1
	}
	if arc <= 0 {
		return 0
	}
	return arc * 4 * progress * (1 - progress)
}

// BuildInto resolves every missile input into a billboard (ground position +
// arc height in Y), filling the active list. Beyond MaxProjectiles the extra
// missiles are dropped; the dropped count is recorded (Dropped) so the truncation
// is observable. Allocation-free once warmed. Returns the number built.
func (p *ProjectileBillboards) BuildInto(inputs []MissileBillboardInput) int {
	p.active = p.active[:0]
	p.dropped = 0
	for i := range inputs {
		if len(p.active) == MaxProjectiles {
			p.dropped = len(inputs) - i
			break
		}
		in := &inputs[i]
		p.active = append(p.active, MissileBillboard{
			Key:      in.Key,
			X:        in.GroundX,
			Y:        ArcHeight(in.Arc, in.Progress),
			Z:        in.GroundZ,
			Facing:   in.Facing,
			Guidance: in.Guidance,
		})
	}
	return len(p.active)
}

// Active returns this frame's billboard draw list (owned by the builder, valid
// until the next BuildInto).
func (p *ProjectileBillboards) Active() []MissileBillboard { return p.active }

// Count returns the number of billboards built this frame.
func (p *ProjectileBillboards) Count() int { return len(p.active) }

// Dropped returns how many missiles were dropped this frame for exceeding
// MaxProjectiles (0 in the normal case).
func (p *ProjectileBillboards) Dropped() int { return p.dropped }
