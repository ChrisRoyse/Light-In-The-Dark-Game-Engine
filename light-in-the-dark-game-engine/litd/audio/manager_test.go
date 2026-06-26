package audio

// #227 FSV. SoT = Manager.Dump() (the same snapshot renderdemo -dump serializes).
// Every case feeds synthetic api.AudioEvents with KNOWN geometry and asserts the
// exact resolved gain/pan/voice-count in the dump — never a return value. The null
// backend runs the full accounting path, so these results equal a real device's.

import (
	"encoding/json"
	"math"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

const eps = 1e-9

func approx(a, b float64) bool { return math.Abs(a-b) < eps }

func voiceByCue(s Snapshot, cue uint32) (Voice, bool) {
	for _, v := range s.Voices {
		if v.Cue == cue {
			return v, true
		}
	}
	return Voice{}, false
}

// Edge 1 (empty / device absence): nil backend → null, fresh state, no crash.
func TestNewManagerNullFallbackFSV(t *testing.T) {
	m := NewManager(nil)
	s := m.Dump()
	if s.Backend != "null" {
		t.Fatalf("device-absent manager must select null backend, got %q", s.Backend)
	}
	if s.VoiceCount != 0 || len(s.Voices) != 0 {
		t.Fatalf("fresh manager must have 0 voices, got %d", s.VoiceCount)
	}
	if (s.Listener != Vec3{}) {
		t.Fatalf("fresh listener must be origin, got %+v", s.Listener)
	}
	for i, cv := range s.ChannelVol {
		if !approx(cv, 1.0) {
			t.Fatalf("channel %d must start at unity, got %v", i, cv)
		}
	}
	t.Logf("FSV #227 empty: backend=%s voices=%d listener=%+v channels=%v", s.Backend, s.VoiceCount, s.Listener, s.ChannelVol)
}

// Non-positional play: centered, channel-scaled, no rolloff. X+X: vol 0.5 → gain 0.5.
func TestPlayNonPositionalFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 42, Volume: 0.5, Channel: api.ChannelEffects})
	s := m.Dump()
	v, ok := voiceByCue(s, 42)
	if !ok {
		t.Fatal("play did not create a voice")
	}
	if !approx(v.Gain, 0.5) || !approx(v.Pan, 0) || v.HasPos {
		t.Fatalf("non-positional: want gain=0.5 pan=0 hasPos=false, got gain=%v pan=%v hasPos=%v", v.Gain, v.Pan, v.HasPos)
	}
	if !approx(v.Pitch, 1.0) {
		t.Fatalf("pitch must default to 1.0, got %v", v.Pitch)
	}
	t.Logf("FSV #227 non-positional: cue=42 gain=%v pan=%v pitch=%v", v.Gain, v.Pan, v.Pitch)
}

// Positional play with known geometry: listener origin, source at (PanWidth,0,0).
// pan = 900/900 = 1.0 (full right); dist=900 > ReferenceDistance(400) → inverse-
// distance atten = 400/900 ≈ 0.444; gain = 0.444 (domain model, #231).
func TestPlayAtSpatialMathFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlayAt, Cue: 1, Volume: 1, HasPos: true, Pos: api.Vec2{X: PanWidth, Y: 0}, Channel: api.ChannelEffects})
	v, _ := voiceByCue(m.Dump(), 1)
	wantGain := ReferenceDistance / PanWidth // inverse-distance clamped
	if !approx(v.Pan, 1.0) {
		t.Fatalf("source at +PanWidth must pan full right (1.0), got %v", v.Pan)
	}
	if !approx(v.Gain, wantGain) {
		t.Fatalf("dist=%v gain: want %v, got %v", PanWidth, wantGain, v.Gain)
	}
	t.Logf("FSV #227 spatial: pos=(%v,0) pan=%v gain=%v (expected %v)", PanWidth, v.Pan, v.Gain, wantGain)
}

// Edge 2 (the issue's explicit case): a fixed emitter, listener panned left→right,
// dumped pan flips sign. Source at x=300. Listener 0 → pan +0.333; listener 600 →
// pan -0.333. SoT = voice.Pan before vs after SetListener.
func TestListenerPanFlipFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlayAt, Cue: 5, Volume: 1, HasPos: true, Pos: api.Vec2{X: 300, Y: 0}})
	before, _ := voiceByCue(m.Dump(), 5)
	if before.Pan <= 0 {
		t.Fatalf("emitter right of listener must pan right (>0), got %v", before.Pan)
	}
	m.SetListener(Vec3{X: 600})
	after, _ := voiceByCue(m.Dump(), 5)
	if after.Pan >= 0 {
		t.Fatalf("after panning listener past the emitter, pan must flip negative, got %v", after.Pan)
	}
	if math.Signbit(before.Pan) == math.Signbit(after.Pan) {
		t.Fatalf("pan sign did not flip: before=%v after=%v", before.Pan, after.Pan)
	}
	t.Logf("FSV #227 pan flip: emitter x=300; listener 0→pan %+.3f, listener 600→pan %+.3f (sign flipped)", before.Pan, after.Pan)
}

// Edge max+1 (out of falloff): source beyond MaxAudibleDistance is CULLED — it
// never enters the voice table (#231 distance cull), and the culled counter ticks.
func TestDistanceFalloffClampFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlayAt, Cue: 9, Volume: 1, HasPos: true, Pos: api.Vec2{X: FalloffRadius + 800, Y: 0}})
	s := m.Dump()
	if _, ok := voiceByCue(s, 9); ok {
		t.Fatal("source beyond max audible must be culled (no voice), but a voice exists")
	}
	if s.VoiceCount != 0 || s.Culled != 1 {
		t.Fatalf("want 0 voices and culled=1, got voices=%d culled=%d", s.VoiceCount, s.Culled)
	}
	t.Logf("FSV #227 cull: pos x=%v (> %v) → culled (0 voices, culled=%d)", FalloffRadius+800, FalloffRadius, s.Culled)
}

// Stop removes every voice for a cue. SoT = voice count before/after.
func TestStopRemovesVoicesFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 1, Volume: 1})
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 2, Volume: 1})
	if n := m.Dump().VoiceCount; n != 2 {
		t.Fatalf("want 2 voices before stop, got %d", n)
	}
	m.Handle(api.AudioEvent{Kind: api.AudioStop, Cue: 1})
	s := m.Dump()
	if s.VoiceCount != 1 {
		t.Fatalf("want 1 voice after stopping cue 1, got %d", s.VoiceCount)
	}
	if _, ok := voiceByCue(s, 1); ok {
		t.Fatal("stopped cue 1 must be gone from the voice table")
	}
	t.Logf("FSV #227 stop: 2 voices → stop cue 1 → %d voice (cue 2 remains)", s.VoiceCount)
}

// Channel master re-resolves live voices. Play on effects at vol 1, then set the
// effects channel to 0.5 → the active voice's gain halves. SoT = voice.Gain delta.
func TestChannelVolumeRetuneFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 3, Volume: 1, Channel: api.ChannelEffects})
	if v, _ := voiceByCue(m.Dump(), 3); !approx(v.Gain, 1.0) {
		t.Fatalf("want gain 1.0 at unity channel, got %v", v.Gain)
	}
	m.Handle(api.AudioEvent{Kind: api.AudioSetChannelVolume, Channel: api.ChannelEffects, Volume: 0.5})
	v, _ := voiceByCue(m.Dump(), 3)
	if !approx(v.Gain, 0.5) {
		t.Fatalf("after channel→0.5, want voice gain 0.5, got %v", v.Gain)
	}
	if !approx(m.Dump().ChannelVol[api.ChannelEffects], 0.5) {
		t.Fatalf("channel master must read 0.5")
	}
	t.Logf("FSV #227 channel: effects 1.0→0.5 halves live voice gain 1.0→%v", v.Gain)
}

// SetVolume retunes a live cue. Play vol 1 → set 0.3 → gain 0.3.
func TestSetVolumeRetuneFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 8, Volume: 1})
	m.Handle(api.AudioEvent{Kind: api.AudioSetVolume, Cue: 8, Volume: 0.3})
	v, _ := voiceByCue(m.Dump(), 8)
	if !approx(v.Gain, 0.3) {
		t.Fatalf("SetVolume 0.3 must set gain 0.3, got %v", v.Gain)
	}
	t.Logf("FSV #227 setvolume: cue 8 gain 1.0→%v", v.Gain)
}

// Determinism (acceptance §9.1, "audio on == audio off"): the SAME event sequence
// run twice on a null backend yields byte-identical JSON dumps. Because nothing
// here touches the sim, this IS the audio-on/off state-identity guarantee.
func TestDeterministicDumpFSV(t *testing.T) {
	seq := func() Snapshot {
		m := NewManager(nil)
		m.SetListener(Vec3{X: 100, Y: 50})
		m.Handle(api.AudioEvent{Kind: api.AudioPlayAt, Cue: 1, Volume: 1, HasPos: true, Pos: api.Vec2{X: 400, Y: 50}})
		m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 2, Volume: 0.7, Channel: api.ChannelUI})
		m.Handle(api.AudioEvent{Kind: api.AudioPlayMusic, Cue: 3, Volume: 0.9})
		m.Handle(api.AudioEvent{Kind: api.AudioSetChannelVolume, Channel: api.ChannelMusic, Volume: 0.4})
		return m.Dump()
	}
	a, _ := json.Marshal(seq())
	b, _ := json.Marshal(seq())
	if string(a) != string(b) {
		t.Fatalf("audio dump not deterministic:\n A=%s\n B=%s", a, b)
	}
	t.Logf("FSV #227 determinism: identical JSON dump across two runs (%d bytes) — audio-on==audio-off", len(a))
}

func TestPlayMusicRoutesToStreamPartitionFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlayMusic, Cue: 77})
	s := m.Dump()
	v, ok := voiceByCue(s, 77)
	if !ok {
		t.Fatal("AudioPlayMusic did not create a voice")
	}
	if api.SoundChannel(v.Channel) != api.ChannelMusic || v.Group != GroupMusic || v.Slot < MusicStreamSlot || v.Slot > AmbienceStreamSlot {
		t.Fatalf("music voice must route to music stream slot/group, got %+v", v)
	}
	if !approx(v.Gain, 1.0) || partCount(s, PartitionStream) != 1 {
		t.Fatalf("music default gain/partition wrong: gain=%v streamCount=%d dump=%+v", v.Gain, partCount(s, PartitionStream), s)
	}
	m.Handle(api.AudioEvent{Kind: api.AudioSetChannelVolume, Channel: api.ChannelMusic, Volume: 0.4})
	v, _ = voiceByCue(m.Dump(), 77)
	if !approx(v.Gain, 0.4) {
		t.Fatalf("music channel volume must retune stream voice to 0.4, got %v", v.Gain)
	}
	t.Logf("FSV #314 manager: music cue=77 channel=%d slot=%d streamCount=%d gain %.1f→%.1f",
		v.Channel, v.Slot, partCount(m.Dump(), PartitionStream), 1.0, v.Gain)
}

func TestStopMusicStopsOnlyMusicChannelFSV(t *testing.T) {
	m := NewManager(nil)
	m.Handle(api.AudioEvent{Kind: api.AudioPlayMusic, Cue: 11})
	m.Handle(api.AudioEvent{Kind: api.AudioPlay, Cue: 22, Volume: 1, Channel: api.ChannelAmbient})
	before := m.Dump()
	if before.VoiceCount != 2 || partCount(before, PartitionStream) != 2 {
		t.Fatalf("setup must occupy both stream slots, got %+v", before)
	}
	m.Handle(api.AudioEvent{Kind: api.AudioStopMusic})
	after := m.Dump()
	if after.VoiceCount != 1 || partCount(after, PartitionStream) != 1 {
		t.Fatalf("StopMusic should leave ambience stream only, got %+v", after)
	}
	if _, ok := voiceByCue(after, 11); ok {
		t.Fatal("music cue remained after StopMusic")
	}
	amb, ok := voiceByCue(after, 22)
	if !ok || api.SoundChannel(amb.Channel) != api.ChannelAmbient || amb.Group != GroupAmbience {
		t.Fatalf("ambience cue should remain on ambience group, got %+v ok=%v", amb, ok)
	}
	t.Logf("FSV #314 stop: before stream voices=%d, StopMusic -> music gone and ambience cue=%d remains group=%d",
		partCount(before, PartitionStream), amb.Cue, amb.Group)
}
