package audio

// #231 FSV. SoT = the resolved (domain, gain, pan, culled) for scripted emitters
// at known positions, read from the manager dump / the pure Resolve* functions.
// World = 3D attenuated + cullable; UI = 2D flat, full volume, never culled.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// World gain falls monotonically with distance and is zero (culled) beyond the
// max audible radius. Emitters at camera center / inside ref / past ref / past max.
func TestWorldDistanceMonotonicFSV(t *testing.T) {
	dists := []float64{0, ReferenceDistance, ReferenceDistance + 200, MaxAudibleDistance - 1, MaxAudibleDistance + 1}
	var prev = 2.0
	for _, d := range dists {
		r := ResolveWorld(Vec3{X: d}, Vec3{}, 1.0, 1.0)
		if d > MaxAudibleDistance {
			if !r.Culled {
				t.Fatalf("d=%v beyond max audible must cull, got gain=%v", d, r.Gain)
			}
			t.Logf("FSV #231 world d=%v → CULLED", d)
			continue
		}
		if r.Culled {
			t.Fatalf("d=%v within max audible must not cull", d)
		}
		if r.Gain > prev+eps {
			t.Fatalf("gain not monotonic: d=%v gain=%v rose above previous %v", d, r.Gain, prev)
		}
		if d <= ReferenceDistance && !approx(r.Gain, 1.0) {
			t.Fatalf("within reference distance gain must be full (1.0), d=%v got %v", d, r.Gain)
		}
		t.Logf("FSV #231 world d=%v → gain=%.4f", d, r.Gain)
		prev = r.Gain
	}
}

// Edge 1: a world sound 3 screens away is culled; the SAME sound classified UI
// plays at full volume regardless of distance.
func TestWorldCulledButUIExemptFSV(t *testing.T) {
	far := Vec3{X: 3 * ReferenceDistance * 2} // well past max audible
	w := ResolveWorld(far, Vec3{}, 1.0, 1.0)
	if !w.Culled {
		t.Fatalf("far world sound must be culled, got gain=%v", w.Gain)
	}
	ui := ResolveFlat(1.0, 1.0) // UI ignores position entirely
	if ui.Culled || !approx(ui.Gain, 1.0) || !approx(ui.Pan, 0) {
		t.Fatalf("UI sound must be flat full-volume centered, got %+v", ui)
	}
	t.Logf("FSV #231 domain split: far world CULLED; same-position UI → gain=1.0 pan=0 (heard regardless of camera)")
}

// Edge 2: hard-left vs hard-right world emitter → pan sign flips.
func TestWorldPanSignFSV(t *testing.T) {
	left := ResolveWorld(Vec3{X: -PanWidth}, Vec3{}, 1, 1)
	right := ResolveWorld(Vec3{X: PanWidth}, Vec3{}, 1, 1)
	if left.Pan >= 0 || right.Pan <= 0 {
		t.Fatalf("pan must flip sign across the listener: left=%v right=%v", left.Pan, right.Pan)
	}
	t.Logf("FSV #231 pan: hard-left=%.2f hard-right=%.2f (sign flips)", left.Pan, right.Pan)
}

// Channel→domain classification: UI channel is the UI domain; the rest are World.
func TestDomainClassificationFSV(t *testing.T) {
	if DomainOf(api.ChannelUI) != DomainUI {
		t.Fatal("ChannelUI must classify as the UI domain")
	}
	for _, ch := range []api.SoundChannel{api.ChannelEffects, api.ChannelVoice, api.ChannelAmbient, api.ChannelMusic} {
		if DomainOf(ch) != DomainWorld {
			t.Fatalf("channel %d must classify as World domain", ch)
		}
	}
	if GroupOf(api.ChannelMusic) != GroupMusic || GroupOf(api.ChannelAmbient) != GroupAmbience {
		t.Fatalf("music and ambience channels must have separate groups, got music=%d ambience=%d", GroupOf(api.ChannelMusic), GroupOf(api.ChannelAmbient))
	}
	t.Logf("FSV #231 classify: UI→UI; Effects/Voice/Ambient/Music→World")
}

// Edge 4: setting the World volume group to 0 silences world voices but leaves a
// UI voice untouched — verified through the live Manager (SoT = dump gains).
func TestVolumeGroupIndependenceFSV(t *testing.T) {
	m := NewManager(nil)
	// A world voice near the listener (within reference distance → full gain) and a
	// UI voice (flat). Both at requested volume 1.0.
	m.Handle(api.AudioEvent{Kind: api.AudioPlayAt, Cue: 1, Volume: 1, HasPos: true, Pos: api.Vec2{X: 10}, Channel: api.ChannelEffects})
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 2, Volume: 1, Channel: api.ChannelUI})
	if w, _ := voiceByCue(m.Dump(), 1); !approx(w.Gain, 1.0) {
		t.Fatalf("world voice near listener must start at full gain, got %v", w.Gain)
	}

	m.SetGroupVolume(GroupWorld, 0) // mute the World group only
	s := m.Dump()
	world, _ := voiceByCue(s, 1)
	ui, _ := voiceByCue(s, 2)
	if !approx(world.Gain, 0) {
		t.Fatalf("World group=0 must silence the world voice, got %v", world.Gain)
	}
	if !approx(ui.Gain, 1.0) {
		t.Fatalf("World group=0 must NOT affect the UI voice, got %v", ui.Gain)
	}
	t.Logf("FSV #231 groups: World group 1.0→0 silences world (gain %v) but UI unaffected (gain %v)", world.Gain, ui.Gain)
}
