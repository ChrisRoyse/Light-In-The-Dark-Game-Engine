package audio

// #230 FSV. SoT = the Allocator slot array (active counts, per-slot priority /
// gain / bump) + the Decision returned per Admit. Synthetic VoiceRequests with
// known priority/position/asset/time; assert exactly which are admitted, coalesced,
// stolen, culled, dropped. Zero device, zero PCM — pure admission logic.

import (
	"testing"
)

// world builds a positional world-partition footstep request near the listener.
func footstep(asset uint32, x float64) VoiceRequest {
	return VoiceRequest{Cue: asset, Asset: asset, Partition: PartitionWorld, Priority: PrioAmbient,
		HasPos: true, Pos: Vec3{X: x}, Volume: 1, TimeMs: 0}
}

// Edge 1: 40 simultaneous identical impacts → exactly 3 admitted, 37 coalesced;
// world active == 3; the merged gain bump is capped (not 37×0.15).
func TestVolleyCoalesceFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	const N = 40
	admitted, coalesced := 0, 0
	for i := 0; i < N; i++ {
		d := a.Admit(VoiceRequest{Cue: 1, Asset: 7, Partition: PartitionWorld,
			Priority: PrioAttackImpact, HasPos: true, Pos: Vec3{}, Volume: 0.5, TimeMs: 0})
		switch d.Outcome {
		case Admitted:
			admitted++
		case Coalesced:
			coalesced++
		default:
			t.Fatalf("hit %d: unexpected outcome %s", i, d.Outcome)
		}
	}
	if admitted != MaxConcurrentPerAsset || coalesced != N-MaxConcurrentPerAsset {
		t.Fatalf("40-volley: want %d admitted / %d coalesced, got %d / %d", MaxConcurrentPerAsset, N-MaxConcurrentPerAsset, admitted, coalesced)
	}
	if got := a.ActiveIn(PartitionWorld); got != MaxConcurrentPerAsset {
		t.Fatalf("world active: want %d, got %d", MaxConcurrentPerAsset, got)
	}
	// The merged instance's bump is capped; gain = 0.5 + 0.45 = 0.95.
	var bumped int = -1
	for i := 0; i < WorldVoices; i++ {
		if _, _, act := a.Slot(i); act && a.slots[i].bump > 0 {
			bumped = i
		}
	}
	if bumped < 0 {
		t.Fatal("no coalesced (bumped) instance found")
	}
	if b := a.slots[bumped].bump; b > CoalesceGainCap+eps {
		t.Fatalf("coalesce bump not capped: %v > %v", b, CoalesceGainCap)
	}
	if g := a.slots[bumped].gain; g < 0.94 || g > 0.96 {
		t.Fatalf("merged gain: want ~0.95 (0.5+cap), got %v", g)
	}
	t.Logf("FSV #230 volley: 40 impacts → %d admitted, %d coalesced; world=%d; merged bump=%.2f (cap %.2f) gain=%.2f",
		admitted, coalesced, a.ActiveIn(PartitionWorld), a.slots[bumped].bump, CoalesceGainCap, a.slots[bumped].gain)
}

// Edge 2: UI click during a saturated world — the UI partition is separate, so a
// full battle never starves UI feedback.
func TestPartitionIsolationFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	for i := 0; i < WorldVoices+5; i++ { // overfill world with distinct assets
		a.Admit(footstep(uint32(i+100), float64(i)))
	}
	if got := a.ActiveIn(PartitionWorld); got != WorldVoices {
		t.Fatalf("world must saturate at %d, got %d", WorldVoices, got)
	}
	d := a.Admit(VoiceRequest{Cue: 1, Asset: 1, Partition: PartitionUI, Priority: PrioAlert, Volume: 1})
	if d.Outcome != Admitted {
		t.Fatalf("UI click during full battle must be admitted (separate partition), got %s", d.Outcome)
	}
	if a.ActiveIn(PartitionUI) != 1 || a.ActiveIn(PartitionWorld) != WorldVoices {
		t.Fatalf("partitions leaked: ui=%d world=%d", a.ActiveIn(PartitionUI), a.ActiveIn(PartitionWorld))
	}
	t.Logf("FSV #230 isolation: world saturated at %d, UI click still admitted (ui=%d)", WorldVoices, a.ActiveIn(PartitionUI))
}

// Edge 3: an Alert at a full world budget steals the lowest-priority voice.
func TestPriorityEvictionFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	for i := 0; i < WorldVoices; i++ { // fill with low-priority footsteps
		a.Admit(footstep(uint32(i+200), float64(i)))
	}
	d := a.Admit(VoiceRequest{Cue: 9, Asset: 9, Partition: PartitionWorld, Priority: PrioAlert,
		HasPos: true, Pos: Vec3{X: 1}, Volume: 1, TimeMs: 0})
	if d.Outcome != Stolen {
		t.Fatalf("Alert at full world must steal a voice, got %s", d.Outcome)
	}
	if d.Victim < 0 || d.Victim >= WorldVoices {
		t.Fatalf("victim slot %d outside world partition", d.Victim)
	}
	if req, _, _ := a.Slot(d.Slot); req.Priority != PrioAlert {
		t.Fatalf("stolen slot must now hold the Alert, got priority %d", req.Priority)
	}
	if a.ActiveIn(PartitionWorld) != WorldVoices {
		t.Fatalf("world count must stay at %d after a steal, got %d", WorldVoices, a.ActiveIn(PartitionWorld))
	}
	t.Logf("FSV #230 eviction: Alert stole slot %d (was Ambient); world stays %d, never 25+", d.Victim, a.ActiveIn(PartitionWorld))
}

// Edge 4: equal-priority contest → the closer event wins; a farther equal-priority
// request loses (dropped).
func TestEqualPriorityCloserWinsFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	for i := 0; i < WorldVoices; i++ { // all footsteps at x=1000 (far, but audible)
		a.Admit(footstep(uint32(i+300), 1000))
	}
	// A closer footstep (x=10) — equal priority — must steal the farthest.
	near := a.Admit(footstep(99001, 10))
	if near.Outcome != Stolen {
		t.Fatalf("closer equal-priority footstep must steal, got %s", near.Outcome)
	}
	// A farther footstep (x=1100) — equal priority, not closer than any victim — drops.
	far := a.Admit(footstep(99002, 1100))
	if far.Outcome != Dropped {
		t.Fatalf("farther equal-priority footstep must drop, got %s", far.Outcome)
	}
	t.Logf("FSV #230 closer-wins: near(x=10) stole slot %d; far(x=1100) dropped", near.Victim)
}

// Edge 5: a request beyond MaxAudible is distance-culled, consuming no slot.
func TestDistanceCullFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	d := a.Admit(footstep(1, FalloffRadius+800)) // x=2000 > 1200
	if d.Outcome != CulledDistance {
		t.Fatalf("out-of-range request must be distance-culled, got %s", d.Outcome)
	}
	if a.ActiveTotal() != 0 {
		t.Fatalf("culled request must consume no slot, active=%d", a.ActiveTotal())
	}
	t.Logf("FSV #230 cull: x=%v (> %v) culled, 0 slots used", FalloffRadius+800, FalloffRadius)
}

// Budget invariant: under heavy overfill of every partition, the total never
// exceeds 32 and world never exceeds 24.
func TestBudgetNeverExceededFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	for i := 0; i < 200; i++ {
		a.Admit(footstep(uint32(i), float64(i%50)))
		a.Admit(VoiceRequest{Cue: uint32(i), Asset: uint32(i), Partition: PartitionUI, Priority: PrioAttackImpact, Volume: 1})
		a.Admit(VoiceRequest{Cue: uint32(i), Asset: uint32(i), Partition: PartitionStream, Priority: PrioAttackImpact, Volume: 1})
	}
	if w := a.ActiveIn(PartitionWorld); w > WorldVoices {
		t.Fatalf("world exceeded budget: %d > %d", w, WorldVoices)
	}
	if tot := a.ActiveTotal(); tot > TotalVoices {
		t.Fatalf("total exceeded budget: %d > %d", tot, TotalVoices)
	}
	t.Logf("FSV #230 budget: after 600 admits — world=%d (≤%d) ui=%d (≤%d) stream=%d (≤%d) total=%d (≤%d)",
		a.ActiveIn(PartitionWorld), WorldVoices, a.ActiveIn(PartitionUI), UIVoices,
		a.ActiveIn(PartitionStream), StreamVoices, a.ActiveTotal(), TotalVoices)
}

// Edge 5 (R-GC-1): the dispatch path allocates zero at full 32-voice load.
func TestAdmitZeroAllocFSV(t *testing.T) {
	a := NewAllocator(FalloffRadius)
	for i := 0; i < WorldVoices; i++ {
		a.Admit(footstep(uint32(i+400), float64(i%20)))
	}
	// At full load, repeated Admit either coalesces, steals, or drops — all of which
	// must be allocation-free (fixed slot array, value Decision).
	steal := VoiceRequest{Cue: 1, Asset: 777, Partition: PartitionWorld, Priority: PrioAlert, HasPos: true, Pos: Vec3{X: 5}, Volume: 1}
	if n := testing.AllocsPerRun(200, func() { a.Admit(steal) }); n != 0 {
		t.Fatalf("Admit must be zero-alloc at full load, got %v allocs/op", n)
	}
	t.Logf("FSV #230 R-GC-1: Admit at full 32-voice load = 0 allocs/op")
}
