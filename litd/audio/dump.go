package audio

// Snapshot is the audio manager's full observable state — the Source of Truth for
// FSV and for the renderdemo `-dump` artifact (#227 acceptance §9.1). Because all
// accounting is device-independent, this snapshot is identical on the null and
// OpenAL backends for the same event sequence (only Backend differs), which is
// exactly the "audio-on == audio-off" determinism guarantee.
type Snapshot struct {
	Backend    string    `json:"backend"`    // "null" | "openal"
	Listener   Vec3      `json:"listener"`   // current listener position
	VoiceCount int       `json:"voiceCount"` // active voices
	MaxVoices  int       `json:"maxVoices"`  // device source-pool size
	Voices     []Voice   `json:"voices"`     // active voices with final gain/pan
	ChannelVol []float64 `json:"channelVol"` // per-channel master volumes
}

// Dump returns a deep copy of the current audio state. Callers may serialize or
// inspect it freely without aliasing manager internals.
func (m *Manager) Dump() Snapshot {
	voices := make([]Voice, len(m.voices))
	copy(voices, m.voices)
	chans := make([]float64, len(m.channelVol))
	copy(chans, m.channelVol[:])
	return Snapshot{
		Backend:    m.backend.Name(),
		Listener:   m.listener,
		VoiceCount: len(m.voices),
		MaxVoices:  MaxVoices,
		Voices:     voices,
		ChannelVol: chans,
	}
}
