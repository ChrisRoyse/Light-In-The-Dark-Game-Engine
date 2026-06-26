package audio

// Snapshot is the audio manager's full observable state — the Source of Truth for
// FSV and for the renderdemo `-dump` artifact (#227 acceptance §9.1). Because all
// accounting is device-independent, this snapshot is identical on the null and
// OpenAL backends for the same event sequence (only Backend differs), which is
// exactly the "audio-on == audio-off" determinism guarantee.
type Snapshot struct {
	Backend        string    `json:"backend"`        // "null" | "openal"
	BackendSources int       `json:"backendSources"` // concrete device source count (0 on null, 32 on OpenAL)
	Listener       Vec3      `json:"listener"`       // current listener position
	VoiceCount     int       `json:"voiceCount"`     // active voices
	MaxVoices      int       `json:"maxVoices"`      // manager accounting budget
	Culled         int       `json:"culled"`         // world voices dropped by distance cull
	Dropped        int       `json:"dropped"`        // voices dropped by admission (full partition, lost eviction; #230)
	Voices         []Voice   `json:"voices"`         // active voices with final gain/pan
	ChannelVol     []float64 `json:"channelVol"`     // per-channel master volumes
	GroupVol       []float64 `json:"groupVol"`       // World / UI / Music / Ambience master groups
}

// Dump returns a deep copy of the current audio state. Callers may serialize or
// inspect it freely without aliasing manager internals.
func (m *Manager) Dump() Snapshot {
	voices := make([]Voice, len(m.voices))
	copy(voices, m.voices)
	chans := make([]float64, len(m.channelVol))
	copy(chans, m.channelVol[:])
	groups := make([]float64, len(m.groupVol))
	copy(groups, m.groupVol[:])
	return Snapshot{
		Backend:        m.backend.Name(),
		BackendSources: m.backend.SourceCount(),
		Listener:       m.listener,
		VoiceCount:     len(m.voices),
		MaxVoices:      MaxVoices,
		Culled:         m.culled,
		Dropped:        m.dropped,
		Voices:         voices,
		ChannelVol:     chans,
		GroupVol:       groups,
	}
}
