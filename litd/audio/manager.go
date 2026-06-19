package audio

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// Tunables for the presentation mix. These are device-independent and shared by
// every backend so the null path computes identical numbers to a real device.
const (
	// FalloffRadius is the world distance at which a positional sound's distance
	// gain reaches zero (linear rolloff from full at the listener).
	FalloffRadius = 1200.0
	// PanWidth is the lateral world distance mapped to full stereo pan (±1).
	PanWidth = 900.0
	// MaxVoices is the device source-pool size reported in the dump. Admission /
	// priority eviction when the pool is full is #230; this package only tracks.
	MaxVoices = 32
	// numChannels covers api.ChannelEffects..api.ChannelVoice.
	numChannels = 5
)

// Manager is the presentation audio mixer. It owns the listener position, the
// active-voice table, and the per-channel master volumes, computes each voice's
// final gain/pan, and dispatches to the Backend. It is pure Go and never touches
// the sim. Not safe for concurrent use — drive it from the single render thread.
type Manager struct {
	backend    Backend
	listener   Vec3
	voices     []Voice
	voiceVol   []float64 // originally-requested [0,1] event volume, parallel to voices
	channelVol [numChannels]float64
}

// NewManager builds a manager over backend. A nil backend (no device present, or
// device-open failed) falls back to the null backend — full accounting, no sound
// — so the absence of a device is a clean degrade, never a crash (fail-safe).
func NewManager(backend Backend) *Manager {
	if backend == nil {
		backend = nullBackend{}
	}
	m := &Manager{backend: backend}
	for i := range m.channelVol {
		m.channelVol[i] = 1.0 // channels start at unity gain
	}
	return m
}

// Backend reports the active backend name ("null" / "openal") for the dump.
func (m *Manager) Backend() string { return m.backend.Name() }

// SetListener binds the audio listener to pos (the camera focus point). It
// updates the device and recomputes every active positional voice's gain/pan, so
// a camera move immediately re-pans live sounds.
func (m *Manager) SetListener(pos Vec3) {
	m.listener = pos
	m.backend.SetListener(pos)
	for i := range m.voices {
		if m.voices[i].HasPos {
			// Recompute from the persisted requested volume (voiceVol) — never from
			// the previous Gain, which already folded in the old distance attenuation.
			m.voices[i].Gain, m.voices[i].Pan = m.spatial(m.voices[i].Pos, m.voices[i].Channel, m.voiceVol[i])
			m.backend.Play(m.voices[i]) // re-issue with updated gain/pan
		}
	}
}

// spatial computes (gain, pan) for a positional source at pos on channel ch with
// pre-spatial volume vol. Linear distance rolloff; lateral pan by X offset.
func (m *Manager) spatial(pos Vec3, ch uint8, vol float64) (gain, pan float64) {
	dist := pos.sub(m.listener).length()
	atten := 1.0 - dist/FalloffRadius
	atten = clamp(atten, 0, 1)
	gain = clamp(vol*m.channelMaster(ch)*atten, 0, 1)
	pan = clamp((pos.X-m.listener.X)/PanWidth, -1, 1)
	return gain, pan
}

// channelMaster returns the master volume for channel ch (1.0 if out of range).
func (m *Manager) channelMaster(ch uint8) float64 {
	if int(ch) < 0 || int(ch) >= numChannels {
		return 1.0
	}
	return m.channelVol[ch]
}

// Handle consumes one resolved presentation event. This is the function installed
// via Game.OnAudio. It mutates the voice table / channel mix and dispatches to the
// backend; it returns nothing because audio is sim-inert.
func (m *Manager) Handle(ev api.AudioEvent) {
	switch ev.Kind {
	case api.AudioPlay, api.AudioPlayOn:
		// Non-positional (UI / attached): centered, channel-scaled, no rolloff.
		m.addVoice(ev, false, Vec3{}, ev.Volume*m.channelMaster(uint8(ev.Channel)), 0)
	case api.AudioPlayAt:
		pos := Vec3{ev.Pos.X, ev.Pos.Y, ev.Z}
		gain, pan := m.spatial(pos, uint8(ev.Channel), ev.Volume)
		m.addVoice(ev, true, pos, gain, pan)
	case api.AudioPlayMusic:
		m.addVoice(ev, false, Vec3{}, ev.Volume*m.channelMaster(uint8(api.ChannelMusic)), 0)
	case api.AudioStop, api.AudioStopMusic:
		m.stopCue(ev.Cue)
	case api.AudioSetVolume:
		m.retune(ev.Cue, ev.Volume)
	case api.AudioSetPitch:
		m.repitch(ev.Cue, ev.Pitch)
	case api.AudioSetMusicVolume:
		m.setChannel(uint8(api.ChannelMusic), ev.Volume)
	case api.AudioSetChannelVolume:
		m.setChannel(uint8(ev.Channel), ev.Volume)
	}
}

// addVoice appends a resolved voice and emits it to the backend.
func (m *Manager) addVoice(ev api.AudioEvent, hasPos bool, pos Vec3, gain, pan float64) {
	pitch := ev.Pitch
	if pitch == 0 {
		pitch = 1.0
	}
	v := Voice{
		Cue:     ev.Cue,
		Channel: uint8(ev.Channel),
		Gain:    gain,
		Pan:     pan,
		Pitch:   pitch,
		Pos:     pos,
		HasPos:  hasPos,
	}
	m.voices = append(m.voices, v)
	m.voiceVol = append(m.voiceVol, ev.Volume)
	m.backend.Play(v)
}

// stopCue removes (and silences) every active voice for cue.
func (m *Manager) stopCue(cue uint32) {
	m.backend.Stop(cue)
	dst := 0
	for i := range m.voices {
		if m.voices[i].Cue == cue {
			continue
		}
		m.voices[dst] = m.voices[i]
		m.voiceVol[dst] = m.voiceVol[i]
		dst++
	}
	m.voices = m.voices[:dst]
	m.voiceVol = m.voiceVol[:dst]
}

// retune updates the requested volume of active voices for cue and re-resolves
// their gain (spatially for positional voices, channel-scaled otherwise).
func (m *Manager) retune(cue uint32, vol float64) {
	for i := range m.voices {
		if m.voices[i].Cue != cue {
			continue
		}
		m.voiceVol[i] = vol
		if m.voices[i].HasPos {
			m.voices[i].Gain, m.voices[i].Pan = m.spatial(m.voices[i].Pos, m.voices[i].Channel, vol)
		} else {
			m.voices[i].Gain = clamp(vol*m.channelMaster(m.voices[i].Channel), 0, 1)
		}
		m.backend.Play(m.voices[i])
	}
}

// repitch updates the pitch of active voices for cue.
func (m *Manager) repitch(cue uint32, pitch float64) {
	if pitch == 0 {
		pitch = 1.0
	}
	for i := range m.voices {
		if m.voices[i].Cue == cue {
			m.voices[i].Pitch = pitch
			m.backend.Play(m.voices[i])
		}
	}
}

// setChannel sets a channel master volume and re-resolves voices on it.
func (m *Manager) setChannel(ch uint8, vol float64) {
	if int(ch) < 0 || int(ch) >= numChannels {
		return
	}
	m.channelVol[ch] = clamp(vol, 0, 1)
	for i := range m.voices {
		if m.voices[i].Channel != ch {
			continue
		}
		if m.voices[i].HasPos {
			m.voices[i].Gain, m.voices[i].Pan = m.spatial(m.voices[i].Pos, ch, m.voiceVol[i])
		} else {
			m.voices[i].Gain = clamp(m.voiceVol[i]*m.channelVol[ch], 0, 1)
		}
		m.backend.Play(m.voices[i])
	}
}

// Close releases the backend.
func (m *Manager) Close() error { return m.backend.Close() }

// clamp constrains v to [lo,hi].
func clamp(v, lo, hi float64) float64 {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
