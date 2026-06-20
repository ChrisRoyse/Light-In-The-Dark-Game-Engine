package audio

// #230↔#227 wiring FSV: the Manager play path (Handle → add) now routes every
// sound through the voice Allocator, so the 32-voice budget (24 world / 6 UI / 2
// stream) and the admission chain are actually ENFORCED — not just available as a
// standalone engine. SoT = Manager.Dump() (VoiceCount, per-slot voices, Culled,
// Dropped) after driving real AudioEvents with known geometry and known expected
// admission outcomes. Before this wiring, add() appended voices unbounded; these
// tests fail against that code.

import (
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// worldPartCount counts dumped voices whose slot falls in the world partition.
func partCount(s Snapshot, p Partition) int {
	lo, hi := bounds(p)
	n := 0
	for _, v := range s.Voices {
		if v.Slot >= lo && v.Slot < hi {
			n++
		}
	}
	return n
}

// playWorld fires a distinct positional world cue near the listener.
func playWorld(m *Manager, cue uint32, x float64, vol float64) {
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: cue, Volume: vol,
		HasPos: true, Pos: api.Vec2{X: x, Y: 0}, Channel: api.ChannelEffects,
	})
}

// Edge: world saturation — 30 distinct world cues into a 24-slot world partition.
// SoT: exactly 24 admitted (world partition full), 6 dropped, total never 32+.
func TestAdmissionWorldBudgetCapFSV(t *testing.T) {
	m := NewManager(nil)
	m.SetListener(Vec3{0, 0, 0})
	for i := 0; i < 30; i++ {
		playWorld(m, uint32(100+i), 100, 1) // distinct cues (no coalescing), same near distance
	}
	s := m.Dump()
	if got := partCount(s, PartitionWorld); got != WorldVoices {
		t.Fatalf("world voices = %d, want %d (budget must cap the partition)", got, WorldVoices)
	}
	if s.VoiceCount != WorldVoices {
		t.Fatalf("total voices = %d, want %d", s.VoiceCount, WorldVoices)
	}
	if s.Dropped != 30-WorldVoices {
		t.Fatalf("dropped = %d, want %d (excess equal-priority requests must drop)", s.Dropped, 30-WorldVoices)
	}
	t.Logf("FSV #230 wiring: 30 world plays → %d admitted (cap %d), %d dropped, total %d (never %d)",
		WorldVoices, WorldVoices, s.Dropped, s.VoiceCount, TotalVoices)
}

// Edge: the world battle must NOT starve UI feedback. Saturate world, then a UI
// click must still be admitted from the separate UI partition.
func TestAdmissionUINotStarvedByBattleFSV(t *testing.T) {
	m := NewManager(nil)
	m.SetListener(Vec3{0, 0, 0})
	for i := 0; i < 24; i++ {
		playWorld(m, uint32(200+i), 100, 1)
	}
	before := m.Dump()
	if partCount(before, PartitionWorld) != WorldVoices {
		t.Fatalf("setup: world not saturated (%d)", partCount(before, PartitionWorld))
	}
	// UI click on the UI channel during full world saturation.
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 999, Volume: 1, Channel: api.ChannelUI})
	after := m.Dump()
	if partCount(after, PartitionUI) != 1 {
		t.Fatalf("UI click NOT admitted during battle (ui voices=%d) — partition starved", partCount(after, PartitionUI))
	}
	t.Logf("FSV #230 wiring: world saturated at %d, UI click still admitted (ui partition = 1) — no starvation",
		WorldVoices)
}

// Edge: priority eviction on the real play path. A world partition full of
// low-priority (Ambient) voices must yield to a high-priority Alert via a steal.
func TestAdmissionPriorityStealFSV(t *testing.T) {
	// Table classifies one cue as a world Alert; the fills are unclassified → Ambient.
	const tbl = `
[[sound]]
cue = "world_alert"
domain = "world"
priority = "alert"
ogg = "sfx/alert.ogg"
`
	st, err := LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(tbl)}}, "audio/sounds.toml")
	if err != nil {
		t.Fatal(err)
	}
	m := NewManager(nil)
	m.SetSoundTable(st)
	m.SetListener(Vec3{0, 0, 0})

	const victimCue = 300
	playWorld(m, victimCue, 100, 1)      // slot 0 — an Ambient
	for i := 1; i < WorldVoices; i++ {    // fill the rest of the world partition
		playWorld(m, uint32(300+i), 100, 1)
	}
	full := m.Dump()
	if full.VoiceCount != WorldVoices {
		t.Fatalf("setup: want %d world voices, got %d", WorldVoices, full.VoiceCount)
	}

	// Fire the Alert (positional world) into the full partition → steal.
	alertID := api.CueID("world_alert")
	playWorld(m, alertID, 50, 1)
	after := m.Dump()

	if after.VoiceCount != WorldVoices {
		t.Fatalf("steal changed the count: %d, want %d (one in, one out)", after.VoiceCount, WorldVoices)
	}
	if after.Dropped != 0 {
		t.Fatalf("alert should STEAL, not drop: dropped=%d", after.Dropped)
	}
	// The alert must be present; a victim (slot 0's Ambient) must be gone.
	sawAlert, sawVictim := false, false
	for _, v := range after.Voices {
		if v.Cue == alertID {
			sawAlert = true
		}
		if v.Cue == victimCue {
			sawVictim = true
		}
	}
	if !sawAlert {
		t.Fatal("alert not admitted via steal")
	}
	if sawVictim {
		t.Fatal("weakest victim (slot 0 Ambient) was not evicted")
	}
	// Confirm the Manager assigns the alert cue the high priority that drove the steal.
	if p := classifyPrio(m, alertID); p != PrioAlert {
		t.Fatalf("alert cue classified priority %d, want PrioAlert(%d)", p, PrioAlert)
	}
	t.Logf("FSV #230 wiring: Alert into full world partition → stole the weakest Ambient (victim cue %d gone, alert in, count steady %d)",
		victimCue, after.VoiceCount)
}

// classifyPrio surfaces the priority the Manager would assign a cue (helper).
func classifyPrio(m *Manager, cue uint32) Priority {
	_, _, p := m.classify(cue, api.ChannelEffects)
	return p
}

// Edge: anti-machine-gun coalescing on the real play path. The same asset fired
// 10× collapses to MaxConcurrentPerAsset voices; the surplus merges (gain bump,
// capped, no restart), never adding voices.
func TestAdmissionCoalesceOnPlayPathFSV(t *testing.T) {
	m := NewManager(nil)
	m.SetListener(Vec3{0, 0, 0})
	const cue = 7
	for i := 0; i < 10; i++ {
		playWorld(m, cue, 100, 0.3) // same cue ⇒ same asset ⇒ coalesces past 3
	}
	s := m.Dump()
	if got := partCount(s, PartitionWorld); got != MaxConcurrentPerAsset {
		t.Fatalf("coalesce: %d world voices, want %d (surplus must merge, not add)", got, MaxConcurrentPerAsset)
	}
	// Exactly one voice carries the accumulated (capped) coalesce bump.
	bumped := 0
	for _, v := range s.Voices {
		if v.Gain > 0.3+1e-9 {
			bumped++
			if v.Gain > 0.3+CoalesceGainCap+1e-9 {
				t.Fatalf("coalesce gain not capped: %v > %v", v.Gain, 0.3+CoalesceGainCap)
			}
		}
	}
	if bumped != 1 {
		t.Fatalf("want exactly 1 bumped (merged-into) voice, got %d", bumped)
	}
	t.Logf("FSV #230 wiring: 10 identical fires → %d voices, 1 merged-in (gain bumped, capped at +%.2f), no restart",
		MaxConcurrentPerAsset, CoalesceGainCap)
}
