package litd

// Sound surface (#244, sound-and-music.md; public-api-design.md §2 row 17).
// Audio is presentation-only and MUST NOT feed back into the sim (R-AUD-1):
// every verb here is sim-inert. In the headless build (no audio sink) the
// verbs validate + clamp their arguments and then no-op; the render driver
// (or a test) installs a sink via Game.OnAudio to actually hear them. Because
// nothing here touches the sim, a scripted scenario hashes identically with
// audio on or off — that identity IS the determinism guarantee.

import "hash/fnv"

// AudioEventKind tags an AudioEvent.
type AudioEventKind uint8

// The Audio* values tag the audio operations emitted as AudioEvents to the
// presentation sink.
const (
	AudioPlay AudioEventKind = iota
	AudioPlayAt
	AudioPlayOn
	AudioStop
	AudioSetVolume
	AudioSetPitch
	AudioPlayMusic
	AudioStopMusic
	AudioSetMusicVolume
	AudioSetChannelVolume
)

// SoundChannel is a mix group whose master volume is set independently.
type SoundChannel uint8

// The Channel* values name the independently-mixed sound channels (see
// SoundChannel).
const (
	ChannelEffects SoundChannel = iota
	ChannelMusic
	ChannelAmbient
	ChannelUI
	ChannelVoice
)

// AudioEvent is one resolved presentation request handed to the audio sink.
// Volumes are already clamped to [0,1]; positions/targets are populated only
// for the spatial variants. It carries no sim state.
type AudioEvent struct {
	// Kind selects which audio action this event describes (play/stop/channel/music).
	Kind AudioEventKind
	Cue  uint32 // Sound.id / music cue hash; 0 for channel/stop-music
	// Volume is the playback volume in [0,1].
	Volume float64
	// Pitch is the playback pitch multiplier (1.0 = unshifted).
	Pitch float64
	// HasPos reports whether Pos and Z carry a world position (a 3D sound)
	// rather than a positionless 2D UI sound.
	HasPos bool
	// Pos is the world position of a 3D sound (valid when HasPos).
	Pos Vec2
	// Z is the world height of a 3D sound (valid when HasPos).
	Z float64
	// Target is the unit the sound is attached to, if any (zero handle = none).
	Target Unit
	// Channel is the mixer channel/domain the sound plays on.
	Channel SoundChannel
}

// OnAudio installs the presentation sink for the sound/music surface. Passing
// nil (the default) restores headless no-op behavior. The sim is never
// consulted or mutated by audio, so installing a sink cannot affect the state
// hash.
func (g *Game) OnAudio(f func(AudioEvent)) {
	if g != nil {
		g.onAudio = f
	}
}

// emitAudio forwards a resolved event to the sink if one is installed.
func (g *Game) emitAudio(ev AudioEvent) {
	if g != nil && g.onAudio != nil {
		g.onAudio(ev)
	}
}

func clamp01(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

// CreateSound resolves a sound cue (e.g. "Spells/footman-attack") to a
// playable handle. The cue maps to an .ogg via the data tables at render time;
// the returned handle is a stable reference (the zero Sound for an empty cue).
// JASS: CreateSound, CreateSoundFilenameWithLabel, CreateSoundFromLabel
func (g *Game) CreateSound(cue string) Sound {
	if g == nil || cue == "" {
		return Sound{}
	}
	h := fnv.New32a()
	_, _ = h.Write([]byte(cue))
	id := h.Sum32()
	if id == 0 {
		id = 1 // keep the handle valid even on the (vanishing) zero hash
	}
	return Sound{id: id, g: g}
}

// Play starts the sound on the effects channel (non-positional). No-op on an
// invalid handle. JASS: StartSound.
// JASS: PlaySound, PlaySoundBJ, StartSound
func (s Sound) Play() {
	if !s.valid("Sound.Play") {
		return
	}
	s.g.emitAudio(AudioEvent{Kind: AudioPlay, Cue: s.id, Volume: 1})
}

// PlayAt starts the sound at a world position with height z. 3D positional
// playback consults the visibility grid at render time (no audio intel through
// fog, R-AUD-1) — that gating lives in the sink, not the sim. No-op on an
// invalid handle. JASS: the XY/Loc positional play variants collapse here.
// JASS: PlaySoundAtPointBJ, SetSoundPosition, SetSoundPositionLocBJ
func (s Sound) PlayAt(pos Vec2, z float64) {
	if !s.valid("Sound.PlayAt") {
		return
	}
	s.g.emitAudio(AudioEvent{Kind: AudioPlayAt, Cue: s.id, Volume: 1, HasPos: true, Pos: pos, Z: z})
}

// PlayOn attaches the sound to a unit. No-op on an invalid handle or unit.
// JASS: AttachSoundToUnit, AttachSoundToUnitBJ, PlaySoundOnUnitBJ
func (s Sound) PlayOn(u Unit) {
	if !s.valid("Sound.PlayOn") {
		return
	}
	s.g.emitAudio(AudioEvent{Kind: AudioPlayOn, Cue: s.id, Volume: 1, Target: u})
}

// Stop stops the sound. No-op on an invalid handle. JASS: StopSound.
// JASS: StopSound, StopSoundBJ
func (s Sound) Stop() {
	if !s.valid("Sound.Stop") {
		return
	}
	s.g.emitAudio(AudioEvent{Kind: AudioStop, Cue: s.id})
}

// SetVolume sets the sound's volume on a 0..1 scale (clamped). The JASS
// 0..127 / percent forms collapse onto this float. No-op on an invalid handle.
// JASS: SetSoundVolume, SetSoundVolumeBJ
func (s Sound) SetVolume(v float64) {
	if !s.valid("Sound.SetVolume") {
		return
	}
	s.g.emitAudio(AudioEvent{Kind: AudioSetVolume, Cue: s.id, Volume: clamp01(v)})
}

// SetPitch sets the sound's pitch multiplier (clamped to (0, 4]; 1 = normal).
// No-op on an invalid handle. JASS: SetSoundPitch.
// JASS: SetSoundPitch, SetSoundPitchBJ
func (s Sound) SetPitch(p float64) {
	if !s.valid("Sound.SetPitch") {
		return
	}
	if p <= 0 {
		p = 0.001
	}
	if p > 4 {
		p = 4
	}
	s.g.emitAudio(AudioEvent{Kind: AudioSetPitch, Cue: s.id, Pitch: p})
}

func (s Sound) valid(verb string) bool {
	if !s.Valid() {
		if s.g != nil {
			s.g.reportInvalid(verb)
		}
		return false
	}
	return true
}
