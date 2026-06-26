package audio

// #428 parts 2/3 FSV: the Manager classifies a playing cue by its DATA-TABLE
// entry (domain → flat/3D + volume group), overriding the mixer channel and the
// position it was played at. SoT = the resolved Voice in Manager.Dump() (final
// domain/group/gain/pan), driven by synthetic AudioEvents with known geometry and
// known expected output. The discriminator: the SAME positional play on the
// Effects channel resolves as a 3D world voice WITHOUT the table, but as a flat UI
// voice WITH it — and routes to the UI volume group, not the channel-inferred
// World group.

import (
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func uiVoiceTable(t *testing.T) *SoundTable {
	t.Helper()
	const body = `
[[sound]]
cue = "voice_ack"
domain = "ui"
priority = "alert"
ogg = "ui/voice_ack.ogg"
`
	tbl, err := LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(body)}}, "audio/sounds.toml")
	if err != nil {
		t.Fatalf("LoadSoundTable: %v", err)
	}
	return tbl
}

// playAtEffects plays cue "voice_ack" on the EFFECTS channel at world (600,0) with
// the listener at the origin — far enough to attenuate and pan a 3D world voice.
func playAtEffects(m *Manager) {
	m.SetListener(Vec3{0, 0, 0})
	m.Handle(api.AudioEvent{
		Kind:    api.AudioPlayAt,
		Cue:     api.CueID("voice_ack"),
		Channel: api.ChannelEffects, // NOT the UI channel — inference would say World
		Pos:     api.Vec2{X: 600, Y: 0},
		Z:       0,
		Volume:  1,
	})
}

func TestSoundClassTableOverridesChannelAndPositionFSV(t *testing.T) {
	// Baseline — NO table: an Effects-channel positional play is a 3D WORLD voice,
	// attenuated (400/600) and panned (600/900) by geometry.
	base := NewManager(nil)
	playAtEffects(base)
	bv := base.Dump().Voices
	if len(bv) != 1 {
		t.Fatalf("baseline: %d voices, want 1", len(bv))
	}
	if bv[0].Domain != DomainWorld || bv[0].Group != GroupWorld {
		t.Fatalf("baseline domain/group = %d/%d, want World/World", bv[0].Domain, bv[0].Group)
	}
	if approx(bv[0].Pan, 0) || approx(bv[0].Gain, 1) {
		t.Fatalf("baseline should be positional (panned + attenuated): gain=%v pan=%v", bv[0].Gain, bv[0].Pan)
	}
	t.Logf("baseline (no table): World voice gain=%.4f pan=%.4f (3D)", bv[0].Gain, bv[0].Pan)

	// WITH table classifying voice_ack as UI: the SAME positional play on the SAME
	// Effects channel becomes a FLAT UI voice — gain at full, centered.
	m := NewManager(nil)
	m.SetSoundTable(uiVoiceTable(t))
	playAtEffects(m)
	v := m.Dump().Voices
	if len(v) != 1 {
		t.Fatalf("classified: %d voices, want 1", len(v))
	}
	if v[0].Domain != DomainUI || v[0].Group != GroupUI {
		t.Fatalf("classified domain/group = %d/%d, want UI/UI (table must override the Effects channel)", v[0].Domain, v[0].Group)
	}
	if !approx(v[0].Gain, 1.0) || !approx(v[0].Pan, 0) {
		t.Fatalf("classified UI voice not flat: gain=%v pan=%v, want gain=1 pan=0", v[0].Gain, v[0].Pan)
	}
	t.Logf("FSV #428: voice_ack on Effects@(600,0) → table classifies UI → flat gain=%.4f pan=%.4f", v[0].Gain, v[0].Pan)

	// Group routing teeth: the voice rides the Effects channel (channel inference
	// would put it in the World group), but the table routes it to the UI group.
	// Muting World must NOT touch it; muting UI must.
	m.SetGroupVolume(GroupWorld, 0)
	if g := m.Dump().Voices[0].Gain; !approx(g, 1.0) {
		t.Fatalf("muting World group changed the UI-classified voice (gain=%v) — group not table-routed", g)
	}
	m.SetGroupVolume(GroupUI, 0.5)
	if g := m.Dump().Voices[0].Gain; !approx(g, 0.5) {
		t.Fatalf("muting UI group to 0.5 did not halve the UI-classified voice (gain=%v)", g)
	}
	t.Logf("FSV #428: UI-classified voice on Effects channel follows the UI volume group, not World (mute World → unchanged; UI 0.5 → gain 0.5)")
}
