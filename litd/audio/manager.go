package audio

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// Tunables for the presentation mix. These are device-independent and shared by
// every backend so the null path computes identical numbers to a real device.
const (
	// PanWidth is the lateral world distance mapped to full stereo pan (±1).
	PanWidth = 900.0
	// FalloffRadius is the world distance at which a positional sound becomes
	// inaudible. It is the canonical "audible" radius shared with the domain model
	// (MaxAudibleDistance) and the voice allocator's distance cull.
	FalloffRadius = 1200.0
	// MaxVoices is the device source-pool size reported in the dump (see the #230
	// allocator for budget/admission; this mixer only tracks active voices).
	MaxVoices = 32
	// numChannels covers api.ChannelEffects..api.ChannelVoice.
	numChannels = 5
)

// Manager is the presentation audio mixer. It owns the listener position, the
// active-voice table, the per-channel and per-group master volumes, resolves each
// voice's final gain/pan through the domain model (domains.go), and dispatches to
// the Backend. Pure Go, never touches the sim. Not safe for concurrent use — drive
// it from the single render thread.
type Manager struct {
	backend    Backend
	listener   Vec3
	voices     []Voice
	voiceVol   []float64 // originally-requested [0,1] event volume, parallel to voices
	channelVol [numChannels]float64
	groupVol   [numGroups]float64 // World / UI / Music master groups (#231 §8)
	culled     int                // world voices dropped by distance cull (observability)
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
		m.channelVol[i] = 1.0
	}
	for i := range m.groupVol {
		m.groupVol[i] = 1.0
	}
	return m
}

// Backend reports the active backend name ("null" / "openal") for the dump.
func (m *Manager) Backend() string { return m.backend.Name() }

// channelMaster returns the fine-grained master for channel ch (1.0 if out of range).
func (m *Manager) channelMaster(ch api.SoundChannel) float64 {
	if int(ch) < 0 || int(ch) >= numChannels {
		return 1.0
	}
	return m.channelVol[ch]
}

// groupMaster returns the master volume of a domain volume group.
func (m *Manager) groupMaster(g VolumeGroup) float64 {
	if int(g) < 0 || int(g) >= numGroups {
		return 1.0
	}
	return m.groupVol[g]
}

// resolve computes a voice's (gain, pan, culled) for the given channel/position via
// the domain model, folding in the channel master and the domain's volume group.
func (m *Manager) resolve(ch api.SoundChannel, pos Vec3, hasPos bool, vol float64) Resolved {
	effVol := vol * m.channelMaster(ch)
	group := m.groupMaster(GroupOf(ch))
	// UI sounds are flat regardless of any position; world sounds without a position
	// (e.g. a global cue) are also flat; positioned world sounds get the 3D model.
	if DomainOf(ch) == DomainUI || !hasPos {
		return ResolveFlat(effVol, group)
	}
	return ResolveWorld(pos, m.listener, effVol, group)
}

// SetListener binds the audio listener to pos (the camera focus point), updates the
// device, and re-resolves every active positional voice so a camera move re-pans
// (and re-attenuates) live world sounds.
func (m *Manager) SetListener(pos Vec3) {
	m.listener = pos
	m.backend.SetListener(pos)
	for i := range m.voices {
		if m.voices[i].HasPos {
			m.reresolve(i)
		}
	}
}

// SetGroupVolume sets a domain master group (World / UI / Music) and re-resolves
// every active voice in that group. This is the settings-menu volume control (#311).
func (m *Manager) SetGroupVolume(g VolumeGroup, vol float64) {
	if int(g) < 0 || int(g) >= numGroups {
		return
	}
	m.groupVol[g] = clamp(vol, 0, 1)
	for i := range m.voices {
		if GroupOf(api.SoundChannel(m.voices[i].Channel)) == g {
			m.reresolve(i)
		}
	}
}

// reresolve recomputes gain/pan for an active voice from its persisted requested
// volume and the current listener/channel/group state, and re-issues it.
func (m *Manager) reresolve(i int) {
	v := &m.voices[i]
	r := m.resolve(api.SoundChannel(v.Channel), v.Pos, v.HasPos, m.voiceVol[i])
	if r.Culled {
		v.Gain = 0 // moved out of audible range; silence (left in table, see note)
	} else {
		v.Gain, v.Pan = r.Gain, r.Pan
	}
	m.backend.Play(*v)
}

// Handle consumes one resolved presentation event (installed via Game.OnAudio).
// Audio is sim-inert, so it returns nothing.
func (m *Manager) Handle(ev api.AudioEvent) {
	ch := api.SoundChannel(ev.Channel)
	switch ev.Kind {
	case api.AudioPlay, api.AudioPlayOn:
		m.add(ev, false, Vec3{})
	case api.AudioPlayAt:
		m.add(ev, true, Vec3{ev.Pos.X, ev.Pos.Y, ev.Z})
	case api.AudioPlayMusic:
		m.add(ev, false, Vec3{})
	case api.AudioStop, api.AudioStopMusic:
		m.stopCue(ev.Cue)
	case api.AudioSetVolume:
		m.retune(ev.Cue, ev.Volume)
	case api.AudioSetPitch:
		m.repitch(ev.Cue, ev.Pitch)
	case api.AudioSetMusicVolume:
		m.setChannel(api.ChannelMusic, ev.Volume)
	case api.AudioSetChannelVolume:
		m.setChannel(ch, ev.Volume)
	}
}

// add resolves and appends a voice, or drops it if a positional world sound is
// distance-culled.
func (m *Manager) add(ev api.AudioEvent, hasPos bool, pos Vec3) {
	ch := api.SoundChannel(ev.Channel)
	r := m.resolve(ch, pos, hasPos, ev.Volume)
	if r.Culled {
		m.culled++
		return // beyond max audible — never plays (R-AUD-1.3)
	}
	pitch := ev.Pitch
	if pitch == 0 {
		pitch = 1.0
	}
	v := Voice{
		Cue:     ev.Cue,
		Channel: uint8(ev.Channel),
		Gain:    r.Gain,
		Pan:     r.Pan,
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

// retune updates the requested volume of active voices for cue and re-resolves.
func (m *Manager) retune(cue uint32, vol float64) {
	for i := range m.voices {
		if m.voices[i].Cue == cue {
			m.voiceVol[i] = vol
			m.reresolve(i)
		}
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

// setChannel sets a fine-grained channel master and re-resolves voices on it.
func (m *Manager) setChannel(ch api.SoundChannel, vol float64) {
	if int(ch) < 0 || int(ch) >= numChannels {
		return
	}
	m.channelVol[ch] = clamp(vol, 0, 1)
	for i := range m.voices {
		if api.SoundChannel(m.voices[i].Channel) == ch {
			m.reresolve(i)
		}
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
