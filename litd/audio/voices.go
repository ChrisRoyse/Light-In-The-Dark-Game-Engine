package audio

// Voice budget + admission control (#230, audio.md §5 R-AUD-1.4/1.5, §9.3–9.4).
//
// A hard 32-voice budget on a PREALLOCATED, fixed source array, partitioned
// 24 world / 6 UI / 2 stream. Sources are never created/destroyed mid-match and
// Admit performs ZERO heap allocations at steady state (R-GC-1) — it only reads
// and mutates the fixed slot array and returns a value Decision.
//
// The WC3-style admission chain, in order:
//  1. distance cull — positional request beyond MaxAudible is dropped;
//  2. duplicate coalescing — at most MaxConcurrentPerAsset (3) concurrent
//     instances of one asset in a partition; the excess merges into the newest
//     instance with a capped gain bump and NO restart (anti-machine-gun);
//  3. priority eviction — when the partition is full, the lowest-priority active
//     voice is stolen (with a FadeMs fade); equal priority → the closer event
//     wins; a request that can't beat any victim is dropped;
//  4. silent drop.
//
// Admission reads interpolated render state (listener, positions) only; it never
// writes anything the sim reads (R-AUD-1). This file is the pure decision engine;
// the per-asset PRIORITY values are supplied by the caller (data-driven sound sets
// are #313) and audible playback of the decisions is #228 — neither is needed to
// verify the admission logic itself.

// Partition is one of the three fixed source pools.
type Partition uint8

const (
	PartitionWorld  Partition = iota // positional gameplay sounds
	PartitionUI                      // non-positional UI feedback
	PartitionStream                  // music + ambience streams
)

// Partition sizes — sum to the 32-voice hardware budget.
const (
	WorldVoices  = 24
	UIVoices     = 6
	StreamVoices = 2
	TotalVoices  = WorldVoices + UIVoices + StreamVoices // 32
)

// Priority orders eviction: higher wins. Alert > AbilityCast > Death >
// AttackImpact > Footstep/Ambient (audio.md §9.3).
type Priority uint8

const (
	PrioAmbient Priority = iota // Footstep / Ambient — lowest
	PrioAttackImpact
	PrioDeath
	PrioAbilityCast
	PrioAlert // highest
)

// Admission tunables.
const (
	// MaxConcurrentPerAsset caps simultaneous instances of one asset per partition.
	MaxConcurrentPerAsset = 3
	// RetriggerWindowMs is the window within which a re-fired asset merges into the
	// existing instance instead of restarting it.
	RetriggerWindowMs = 50
	// FadeMs is the steal fade applied to an evicted voice (device-side).
	FadeMs = 5
	// CoalesceGainBump is the additive gain a coalesced (merged) hit adds to the
	// existing instance, capped at CoalesceGainCap total bump.
	CoalesceGainBump = 0.15
	CoalesceGainCap  = 0.45
)

// Outcome is what Admit decided.
type Outcome uint8

const (
	Admitted       Outcome = iota // got a fresh slot
	Coalesced                     // merged into an existing same-asset slot
	Stolen                        // evicted a lower-priority voice for this slot
	CulledDistance                // dropped: beyond MaxAudible
	Dropped                       // dropped: partition full, could not beat a victim
)

func (o Outcome) String() string {
	switch o {
	case Admitted:
		return "admitted"
	case Coalesced:
		return "coalesced"
	case Stolen:
		return "stolen"
	case CulledDistance:
		return "culled-distance"
	case Dropped:
		return "dropped"
	default:
		return "?"
	}
}

// VoiceRequest is one admission candidate. Asset identifies the sound for
// coalescing (distinct from Cue, which may carry per-instance variation).
type VoiceRequest struct {
	Cue       uint32
	Asset     uint32
	Partition Partition
	Priority  Priority
	Pos       Vec3
	HasPos    bool
	Volume    float64
	TimeMs    int64 // logical event time, for the retrigger window
}

// Decision is the (alloc-free) result of Admit. Slot is the assigned/target slot
// index, or -1; Victim is the evicted slot index, or -1.
type Decision struct {
	Outcome  Outcome
	Slot     int
	Victim   int
	GainBump float64 // for Coalesced: the (capped) bump applied to the existing voice
}

// vslot is one preallocated source.
type vslot struct {
	active  bool
	req     VoiceRequest
	gain    float64 // current gain (grows on coalesce, capped)
	bump    float64 // accumulated coalesce bump
	startMs int64
}

// Allocator is the fixed-budget voice pool with admission control.
type Allocator struct {
	slots      [TotalVoices]vslot
	listener   Vec3
	maxAudible float64
}

// NewAllocator builds the pool. maxAudible is the distance cull radius (use
// FalloffRadius for parity with the mixer).
func NewAllocator(maxAudible float64) *Allocator {
	return &Allocator{maxAudible: maxAudible}
}

// SetListener updates the cull/closer-wins reference point.
func (a *Allocator) SetListener(p Vec3) { a.listener = p }

// bounds returns the [lo,hi) slot range backing a partition.
func bounds(p Partition) (lo, hi int) {
	switch p {
	case PartitionWorld:
		return 0, WorldVoices
	case PartitionUI:
		return WorldVoices, WorldVoices + UIVoices
	default: // PartitionStream
		return WorldVoices + UIVoices, TotalVoices
	}
}

// ActiveIn counts active voices in a partition (FSV/observability).
func (a *Allocator) ActiveIn(p Partition) int {
	lo, hi := bounds(p)
	n := 0
	for i := lo; i < hi; i++ {
		if a.slots[i].active {
			n++
		}
	}
	return n
}

// ActiveTotal counts all active voices.
func (a *Allocator) ActiveTotal() int {
	n := 0
	for i := range a.slots {
		if a.slots[i].active {
			n++
		}
	}
	return n
}

// Slot exposes a slot's current request + gain for FSV (active only).
func (a *Allocator) Slot(i int) (req VoiceRequest, gain float64, active bool) {
	s := &a.slots[i]
	return s.req, s.gain, s.active
}

// Release frees a slot (voice finished). No-op if already free.
func (a *Allocator) Release(slot int) {
	if slot >= 0 && slot < len(a.slots) {
		a.slots[slot] = vslot{}
	}
}

// dist2 returns squared distance from the listener (avoids a sqrt on the hot path).
func (a *Allocator) dist2(p Vec3) float64 {
	d := p.sub(a.listener)
	return d.X*d.X + d.Y*d.Y + d.Z*d.Z
}

// Admit runs the admission chain for req and returns the decision. Zero-alloc:
// operates only on the fixed slot array.
func (a *Allocator) Admit(req VoiceRequest) Decision {
	// (1) Distance cull.
	if req.HasPos && a.dist2(req.Pos) > a.maxAudible*a.maxAudible {
		return Decision{Outcome: CulledDistance, Slot: -1, Victim: -1}
	}

	lo, hi := bounds(req.Partition)

	// (2) Duplicate coalescing: count concurrent same-asset; merge the excess into
	// the newest same-asset instance with a capped gain bump (never restart).
	concurrent := 0
	newest := -1
	var newestStart int64 = -1 << 62
	for i := lo; i < hi; i++ {
		s := &a.slots[i]
		if s.active && s.req.Asset == req.Asset {
			concurrent++
			if s.startMs >= newestStart {
				newestStart, newest = s.startMs, i
			}
		}
	}
	if concurrent >= MaxConcurrentPerAsset && newest >= 0 {
		if !withinRetriggerWindow(req.TimeMs, newestStart) {
			return Decision{Outcome: Dropped, Slot: -1, Victim: -1}
		}
		s := &a.slots[newest]
		bump := CoalesceGainBump
		if s.bump+bump > CoalesceGainCap {
			bump = CoalesceGainCap - s.bump
		}
		if bump < 0 {
			bump = 0
		}
		s.bump += bump
		s.gain = clamp(s.gain+bump, 0, 1)
		// startMs unchanged → no restart.
		return Decision{Outcome: Coalesced, Slot: newest, Victim: -1, GainBump: bump}
	}

	// (3a) Free slot in partition?
	for i := lo; i < hi; i++ {
		if !a.slots[i].active {
			a.place(i, req)
			return Decision{Outcome: Admitted, Slot: i, Victim: -1}
		}
	}

	// (3b) Partition full: find the weakest victim (lowest priority; ties → farthest).
	victim := a.weakest(lo, hi)
	v := &a.slots[victim]
	if req.Priority > v.req.Priority ||
		(req.Priority == v.req.Priority && a.closerThan(req, v.req)) {
		a.place(victim, req)
		return Decision{Outcome: Stolen, Slot: victim, Victim: victim}
	}

	// (4) Silent drop.
	return Decision{Outcome: Dropped, Slot: -1, Victim: -1}
}

// withinRetriggerWindow reports whether now is close enough to newestStart to
// merge instead of starting/restarting another same-asset instance. A backwards
// timestamp is treated as inside the window, which keeps the allocator
// conservative when callers supply a non-wall-clock monotonic sequence.
func withinRetriggerWindow(now, newestStart int64) bool {
	if now < newestStart {
		return true
	}
	return now-newestStart <= RetriggerWindowMs
}

// weakest returns the index of the lowest-priority active voice in [lo,hi);
// ties broken by greatest distance from the listener (steal the farthest).
func (a *Allocator) weakest(lo, hi int) int {
	best := lo
	for i := lo + 1; i < hi; i++ {
		if a.lessImportant(i, best) {
			best = i
		}
	}
	return best
}

// lessImportant reports whether slot i is a better eviction victim than slot j.
func (a *Allocator) lessImportant(i, j int) bool {
	si, sj := &a.slots[i], &a.slots[j]
	if si.req.Priority != sj.req.Priority {
		return si.req.Priority < sj.req.Priority
	}
	// Equal priority: the farther voice is the better victim.
	return a.dist2(si.req.Pos) > a.dist2(sj.req.Pos)
}

// closerThan reports whether request r is closer to the listener than the active
// request o (used for equal-priority contests). Non-positional counts as closest.
func (a *Allocator) closerThan(r, o VoiceRequest) bool {
	rd := 0.0
	if r.HasPos {
		rd = a.dist2(r.Pos)
	}
	od := 0.0
	if o.HasPos {
		od = a.dist2(o.Pos)
	}
	return rd < od
}

// place installs req into slot i.
func (a *Allocator) place(i int, req VoiceRequest) {
	a.slots[i] = vslot{
		active:  true,
		req:     req,
		gain:    clamp(req.Volume, 0, 1),
		startMs: req.TimeMs,
	}
}
