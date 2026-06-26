package render

// Animation clip driver (#149; PRD R-AST-3; milestones.md M4 deliverable 2;
// fog-of-war doc §2.3 corpse fade).
//
// The sim decides each unit's state; the renderer picks and advances the
// contractual clip (Idle / Walk / Attack / Death). This file is that driver —
// the deterministic clip-selection and time-advance state machine, per entity
// in preallocated SoA arrays (0 allocs/frame). The actual skeletal sampling of
// the chosen clip at the advanced time is the GLB-skinning step wired at the
// render-graph boundary; this driver hands it (clip, time, fade) per slot.
//
// Rules (R-AST-3): a state change switches clip and restarts it at frame 0 (no
// T-pose); Idle/Walk loop; Attack and Death play once and clamp at the last
// frame; Death then ramps a corpse-fade scalar 1→0; Attack reports the impact
// frame from clip metadata. Off-screen (pre-culled) units skip the time advance
// entirely and resume at their preserved phase on re-entry — never a T-pose. A
// missing contractual clip is a validator error upstream; the driver never
// silently substitutes.

// ClipID identifies a contractual animation clip.
type ClipID uint8

const (
	ClipIdle ClipID = iota
	ClipWalk
	ClipAttack
	ClipDeath
	clipCount
)

// SimAnimState is the sim-side action state the driver maps to a clip.
type SimAnimState uint8

const (
	StateIdle SimAnimState = iota
	StateMove
	StateAttack
	StateDead
)

// clipForState maps a sim state to its contractual clip.
func clipForState(s SimAnimState) ClipID {
	switch s {
	case StateMove:
		return ClipWalk
	case StateAttack:
		return ClipAttack
	case StateDead:
		return ClipDeath
	default:
		return ClipIdle
	}
}

// ClipMeta is per-clip metadata from the unit's data tables.
type ClipMeta struct {
	Duration   float32 // seconds; <=0 means a degenerate clip (treated as a single held frame)
	Loop       bool    // Idle/Walk loop; Attack/Death play once
	ImpactTime float32 // for Attack: seconds into the clip the hit lands
}

// ClipSet is the contractual clip metadata indexed by ClipID.
type ClipSet [clipCount]ClipMeta

// AnimDriver holds per-entity animation state in slot-aligned SoA arrays,
// matching the snapshot/sync slot layout. Reused across frames; grown only when
// the slot count rises, so steady-state Update allocates nothing.
type AnimDriver struct {
	clip     []ClipID
	time     []float32
	fade     []float32 // corpse-fade scalar, 1 normally, ramps to 0 after Death
	state    []SimAnimState
	inited   []bool
	impacted []bool // set the frame an Attack clip crosses its impact time
	n        int
}

// NewAnimDriver preallocates a driver for capacity slots.
func NewAnimDriver(capacity int) *AnimDriver {
	if capacity < 0 {
		capacity = 0
	}
	return &AnimDriver{
		clip:     make([]ClipID, capacity),
		time:     make([]float32, capacity),
		fade:     make([]float32, capacity),
		state:    make([]SimAnimState, capacity),
		inited:   make([]bool, capacity),
		impacted: make([]bool, capacity),
	}
}

func (d *AnimDriver) ensure(n int) {
	if cap(d.clip) >= n {
		d.clip = d.clip[:n]
		d.time = d.time[:n]
		d.fade = d.fade[:n]
		d.state = d.state[:n]
		d.inited = d.inited[:n]
		d.impacted = d.impacted[:n]
		return
	}
	d.clip = make([]ClipID, n)
	d.time = make([]float32, n)
	d.fade = make([]float32, n)
	d.state = make([]SimAnimState, n)
	d.inited = make([]bool, n)
	d.impacted = make([]bool, n)
}

// Update advances every slot's animation by dt. states is the per-slot sim
// state (authoritative slot count); visible marks slots on screen (culled slots
// skip the time advance and skinning); clips is the unit's contractual metadata;
// fadeTime is the corpse-fade duration in seconds. Zero allocations once warmed.
func (d *AnimDriver) Update(states []SimAnimState, visible []bool, dt float32, clips ClipSet, fadeTime float32) {
	n := len(states)
	d.ensure(n)
	d.n = n
	if fadeTime <= 0 {
		fadeTime = 1
	}
	for i := 0; i < n; i++ {
		d.impacted[i] = false
		st := states[i]
		// Transition (or first sight): switch clip, restart at frame 0. Done even
		// when culled so a re-entering unit shows the correct clip, not a T-pose.
		if !d.inited[i] || d.state[i] != st {
			d.clip[i] = clipForState(st)
			d.time[i] = 0
			d.fade[i] = 1
			d.state[i] = st
			d.inited[i] = true
		}
		// Culled: skip the advance/skinning; phase is preserved for re-entry.
		if i < len(visible) && !visible[i] {
			continue
		}
		meta := clips[d.clip[i]]
		prev := d.time[i]
		d.time[i] += dt
		if meta.Loop {
			if meta.Duration > 0 {
				for d.time[i] >= meta.Duration {
					d.time[i] -= meta.Duration
				}
			} else {
				d.time[i] = 0
			}
			continue
		}
		// Play-once clip: clamp at the last frame.
		if meta.Duration > 0 && d.time[i] > meta.Duration {
			d.time[i] = meta.Duration
		}
		switch d.clip[i] {
		case ClipAttack:
			// Impact fires the frame the clip time crosses ImpactTime.
			if meta.ImpactTime > 0 && prev < meta.ImpactTime && d.time[i] >= meta.ImpactTime {
				d.impacted[i] = true
			}
		case ClipDeath:
			// After the death clip ends, ramp the corpse fade scalar to 0. Only
			// the slice of dt spent *past* the clip end counts, so the frame the
			// clip finishes does not over-consume fade with its whole dt.
			raw := prev + dt
			if meta.Duration <= 0 {
				d.fade[i] -= dt / fadeTime
			} else if raw > meta.Duration {
				over := raw - meta.Duration // prev is clamped ≤ Duration, so over ≤ dt
				d.fade[i] -= over / fadeTime
			}
			if d.fade[i] < 0 {
				d.fade[i] = 0
			}
		}
	}
}

// Len returns the slot count the last Update filled.
func (d *AnimDriver) Len() int { return d.n }

// Clip returns the active clip for slot i.
func (d *AnimDriver) Clip(i int) ClipID { return d.clip[i] }

// Time returns the active clip time (seconds) for slot i.
func (d *AnimDriver) Time(i int) float32 { return d.time[i] }

// Fade returns the corpse-fade scalar for slot i (1 = opaque, 0 = faded out).
func (d *AnimDriver) Fade(i int) float32 { return d.fade[i] }

// Impacted reports whether slot i's Attack clip crossed its impact frame this
// Update (the frame to apply the hit / spawn the effect).
func (d *AnimDriver) Impacted(i int) bool { return d.impacted[i] }
