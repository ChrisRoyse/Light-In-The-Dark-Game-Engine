package render

// FSV for the #313 render-side sound trigger. SoT = the audio Manager's voice
// table (Dump: per-voice domain/cue/pos/slot + Dropped) after firing known cues,
// plus the TriggerOutcome the trigger returns. X+X=Y: a known set of gameplay cues
// in → a known voice population / throttle decision out, read back from the
// Manager (null backend — headless accounting, the #227 degrade path).

import (
	"fmt"
	"strings"
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

// buildTrigger generates classification + sound-set tables for n unit types
// (u0..u{n-1}), wires a null-backend Manager, and returns a trigger over them.
func buildTrigger(t *testing.T, n int, throttleTicks uint32) (*SoundTrigger, *audio.Manager) {
	t.Helper()
	var cls, sets strings.Builder
	for i := 0; i < n; i++ {
		// classification: attack/death/under-attack are world; ready/ack/order are UI.
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_atk\"\ndomain=\"world\"\npriority=\"attackimpact\"\nogg=\"a%d.ogg\"\n", i, i)
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_die\"\ndomain=\"world\"\npriority=\"death\"\nogg=\"d%d.ogg\"\n", i, i)
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_rdy\"\ndomain=\"ui\"\npriority=\"ambient\"\nogg=\"r%d.ogg\"\n", i, i)
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_ack\"\ndomain=\"ui\"\npriority=\"ambient\"\nogg=\"k%d.ogg\"\n", i, i)
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_ord\"\ndomain=\"ui\"\npriority=\"ambient\"\nogg=\"o%d.ogg\"\n", i, i)
		fmt.Fprintf(&cls, "[[sound]]\ncue=\"u%d_warn\"\ndomain=\"world\"\npriority=\"alert\"\nogg=\"w%d.ogg\"\n", i, i)
		fmt.Fprintf(&sets, "[[unit]]\ntype=\"u%d\"\nattack=\"u%d_atk\"\ndeath=\"u%d_die\"\nready=\"u%d_rdy\"\nack=\"u%d_ack\"\norder_ack=\"u%d_ord\"\nunder_attack=\"u%d_warn\"\n",
			i, i, i, i, i, i, i)
	}
	classify, err := audio.LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(cls.String())}}, "audio/sounds.toml")
	if err != nil {
		t.Fatalf("classification: %v", err)
	}
	st, err := audio.LoadSoundSetTable(fstest.MapFS{"data/sounds/sets.toml": {Data: []byte(sets.String())}}, "data/sounds/sets.toml", classify)
	if err != nil {
		t.Fatalf("sound-set: %v", err)
	}
	mgr := audio.NewManager(nil)
	mgr.SetSoundTable(classify)
	mgr.SetListener(audio.Vec3{}) // listener at origin
	return NewSoundTrigger(mgr, st, throttleTicks), mgr
}

func worldVoices(s audio.Snapshot) int {
	n := 0
	for _, v := range s.Voices {
		if v.Domain == audio.DomainWorld {
			n++
		}
	}
	return n
}

// Edge 1 — world budget cap: 30 distinct unit types each die near the listener
// (distinct assets ⇒ no coalescing). The trigger feeds admission, which must cap
// the world partition at WorldVoices and drop the surplus.
func TestSoundTriggerWorldBudgetCapFSV(t *testing.T) {
	trig, mgr := buildTrigger(t, 30, 0)
	for i := 0; i < 30; i++ {
		out := trig.Fire(AudioCue{Category: audio.CatDeath, UnitType: fmt.Sprintf("u%d", i),
			Unit: uint32(i + 1), Pos: api.Vec2{X: float64(i), Y: 0}, Tick: 10})
		if out != CueRouted {
			t.Fatalf("death cue %d not routed: %v", i, out)
		}
	}
	s := mgr.Dump()
	if wv := worldVoices(s); wv != audio.WorldVoices {
		t.Fatalf("world voices = %d, want %d (budget must cap the trigger's output)", wv, audio.WorldVoices)
	}
	if s.Dropped != 30-audio.WorldVoices {
		t.Fatalf("dropped = %d, want %d", s.Dropped, 30-audio.WorldVoices)
	}
	t.Logf("FSV #313 budget: 30 distinct deaths → %d world voices admitted (cap), %d dropped",
		audio.WorldVoices, s.Dropped)
}

// Edge 2 — coalescing: the SAME unit type dies 10× (same death cue ⇒ same asset).
// Admission collapses it to MaxConcurrentPerAsset, never adding 10 voices.
func TestSoundTriggerCoalesceSameAssetFSV(t *testing.T) {
	trig, mgr := buildTrigger(t, 1, 0)
	for i := 0; i < 10; i++ {
		trig.Fire(AudioCue{Category: audio.CatDeath, UnitType: "u0", Unit: uint32(i + 1), Pos: api.Vec2{X: 5, Y: 0}, Tick: 10})
	}
	if wv := worldVoices(mgr.Dump()); wv != audio.MaxConcurrentPerAsset {
		t.Fatalf("world voices = %d, want %d (same-asset deaths must coalesce)", wv, audio.MaxConcurrentPerAsset)
	}
	t.Logf("FSV #313 coalesce: 10 identical deaths → %d voices (capped per asset)", audio.MaxConcurrentPerAsset)
}

// Edge 3 — under-attack throttle: a unit spammed with under-attack cues every tick
// emits at most one stinger per throttle window; a later cue past the window emits
// again. SoT = the TriggerOutcome sequence.
func TestSoundTriggerUnderAttackThrottleFSV(t *testing.T) {
	const window = 60
	trig, _ := buildTrigger(t, 1, window)
	routed, throttled := 0, 0
	for tick := uint32(100); tick <= 109; tick++ { // 10 consecutive ticks, same unit
		switch trig.Fire(AudioCue{Category: audio.CatUnderAttack, UnitType: "u0", Unit: 1, Pos: api.Vec2{X: 5}, Tick: tick}) {
		case CueRouted:
			routed++
		case CueThrottled:
			throttled++
		}
	}
	// One window later the stinger is allowed again.
	late := trig.Fire(AudioCue{Category: audio.CatUnderAttack, UnitType: "u0", Unit: 1, Pos: api.Vec2{X: 5}, Tick: 100 + window})
	if routed != 1 || throttled != 9 {
		t.Fatalf("within window: routed=%d throttled=%d, want 1/9", routed, throttled)
	}
	if late != CueRouted {
		t.Fatalf("cue one window later = %v, want routed", late)
	}
	// A DIFFERENT unit is independent — its first stinger is never throttled.
	if out := trig.Fire(AudioCue{Category: audio.CatUnderAttack, UnitType: "u0", Unit: 2, Pos: api.Vec2{X: 5}, Tick: 105}); out != CueRouted {
		t.Fatalf("other unit's first stinger = %v, want routed (throttle is per-unit)", out)
	}
	t.Logf("FSV #313 throttle: 10 spammed under-attacks → 1 stinger; +%dt later → 1 more; other unit independent", window)
}

// Edge 4 — domain routing + no-set: a death routes to a 3D world voice; a
// selection ack to a 2D UI voice; an unknown unit type produces no voice at all.
func TestSoundTriggerRoutingAndNoSetFSV(t *testing.T) {
	trig, mgr := buildTrigger(t, 1, 0)

	if out := trig.Fire(AudioCue{Category: audio.CatDeath, UnitType: "u0", Unit: 1, Pos: api.Vec2{X: 7, Y: 0}, Tick: 1}); out != CueRouted {
		t.Fatalf("death = %v, want routed", out)
	}
	if out := trig.Fire(AudioCue{Category: audio.CatAck, UnitType: "u0", Unit: 1, Tick: 1}); out != CueRouted {
		t.Fatalf("ack = %v, want routed", out)
	}
	// Unknown unit type: no set, no voice.
	if out := trig.Fire(AudioCue{Category: audio.CatDeath, UnitType: "zzz", Unit: 9, Pos: api.Vec2{X: 7}, Tick: 1}); out != CueNoSet {
		t.Fatalf("unknown type = %v, want no-set", out)
	}

	s := mgr.Dump()
	var sawWorld, sawUI bool
	for _, v := range s.Voices {
		switch v.Cue {
		case api.CueID("u0_die"):
			sawWorld = true
			if v.Domain != audio.DomainWorld || !v.HasPos {
				t.Fatalf("death voice domain=%d hasPos=%v, want world+positional", v.Domain, v.HasPos)
			}
		case api.CueID("u0_ack"):
			sawUI = true
			if v.Domain != audio.DomainUI || v.HasPos {
				t.Fatalf("ack voice domain=%d hasPos=%v, want ui+flat", v.Domain, v.HasPos)
			}
		}
	}
	if !sawWorld || !sawUI {
		t.Fatalf("missing routed voices: world=%v ui=%v", sawWorld, sawUI)
	}
	if len(s.Voices) != 2 {
		t.Fatalf("voice count = %d, want 2 (unknown type added none)", len(s.Voices))
	}
	t.Logf("FSV #313 routing: death→3D world voice, ack→2D UI voice, unknown type→no voice (total 2)")
}
