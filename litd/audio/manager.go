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
	groupVol   [numGroups]float64 // World / UI / Music / Ambience master groups (#231/#314)
	culled     int                // world voices dropped by distance cull (observability)
	dropped    int                // voices dropped by admission (partition full, lost eviction; #230)
	table      *SoundTable        // per-asset domain/priority classifier (#428); nil → channel inference
	alloc      *Allocator         // 32-voice budget + admission control (#230)
	seq        int64              // monotonic event sequence → retrigger ordering (no wall clock)
}

// NewManager builds a manager over backend. A nil backend (no device present, or
// device-open failed) falls back to the null backend — full accounting, no sound
// — so the absence of a device is a clean degrade, never a crash (fail-safe).
func NewManager(backend Backend) *Manager {
	if backend == nil {
		backend = nullBackend{}
	}
	m := &Manager{backend: backend, alloc: NewAllocator(FalloffRadius)}
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

// SetSoundTable installs the per-asset sound classification table (#428). Once
// set, a cue present in the table is classified by its data entry (domain →
// flat/3D + volume group), taking precedence over the mixer channel; a cue absent
// from the table falls back to channel inference (the documented interim, valid
// during the #313 content rollout). Pass nil to revert to pure channel inference.
func (m *Manager) SetSoundTable(t *SoundTable) { m.table = t }

// classify resolves a cue's playback domain, volume group, and admission priority:
// the data table (authoritative, #428) when the cue is classified, else channel
// inference. An unclassified cue has no per-asset priority, so it defaults to the
// lowest (PrioAmbient) — it can never evict a classified sound, only lose to one.
func (m *Manager) classify(cue uint32, ch api.SoundChannel) (Domain, VolumeGroup, Priority) {
	if m.table != nil {
		if e, ok := m.table.LookupByID(cue); ok {
			return e.Domain, GroupForDomain(e.Domain), e.Priority
		}
	}
	return DomainOf(ch), GroupOf(ch), PrioAmbient
}

// partitionFor maps a voice to its fixed source pool: music/ambience streams to
// the stream partition, UI-domain sounds to the UI partition, everything else to
// world. The partition split is what keeps a 500-unit battle from starving UI
// feedback (#230): a full world pool cannot evict a UI voice.
func partitionFor(dom Domain, ch api.SoundChannel) Partition {
	switch ch {
	case api.ChannelMusic, api.ChannelAmbient:
		return PartitionStream
	}
	if dom == DomainUI {
		return PartitionUI
	}
	return PartitionWorld
}

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

// resolve computes a voice's (gain, pan, culled) from its RESOLVED domain/group
// (classify already chose table-or-channel), folding in the channel master and the
// domain's volume-group master. A UI-domain sound is flat regardless of any
// position it was played at — the crux of #428: classification, not coincidence of
// channel or position, decides flatness.
func (m *Manager) resolve(dom Domain, group VolumeGroup, ch api.SoundChannel, pos Vec3, hasPos bool, vol float64) Resolved {
	effVol := vol * m.channelMaster(ch)
	gmaster := m.groupMaster(group)
	if dom == DomainUI || !hasPos {
		return ResolveFlat(effVol, gmaster)
	}
	return ResolveWorld(pos, m.listener, effVol, gmaster)
}

// SetListener binds the audio listener to pos (the camera focus point), updates the
// device, and re-resolves every active positional voice so a camera move re-pans
// (and re-attenuates) live world sounds.
func (m *Manager) SetListener(pos Vec3) {
	m.listener = pos
	m.backend.SetListener(pos)
	if m.alloc != nil {
		m.alloc.SetListener(pos) // keep the admission cull/closer-wins reference in sync
	}
	for i := range m.voices {
		if m.voices[i].HasPos {
			m.reresolve(i)
		}
	}
}

// SetGroupVolume sets a domain master group (World / UI / Music / Ambience) and
// re-resolves every active voice in that group. This is the settings-menu volume
// control (#311).
func (m *Manager) SetGroupVolume(g VolumeGroup, vol float64) {
	if int(g) < 0 || int(g) >= numGroups {
		return
	}
	m.groupVol[g] = clamp(vol, 0, 1)
	for i := range m.voices {
		if m.voices[i].Group == g { // resolved group (table or channel), not re-inferred
			m.reresolve(i)
		}
	}
}

// reresolve recomputes gain/pan for an active voice from its persisted requested
// volume and the current listener/channel/group state, and re-issues it.
func (m *Manager) reresolve(i int) {
	v := &m.voices[i]
	r := m.resolve(v.Domain, v.Group, api.SoundChannel(v.Channel), v.Pos, v.HasPos, m.voiceVol[i])
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
		ev.Channel = api.ChannelMusic
		if ev.Volume == 0 {
			ev.Volume = 1
		}
		m.add(ev, false, Vec3{})
	case api.AudioStop:
		m.stopCue(ev.Cue)
	case api.AudioStopMusic:
		m.stopChannel(api.ChannelMusic)
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

// add classifies, runs the #230 admission chain (distance cull → duplicate
// coalescing → priority eviction → drop), and on admission resolves and appends
// the voice. The Allocator is the budget authority; this mixer mirrors only the
// admitted voices, linked one-to-one by slot index.
func (m *Manager) add(ev api.AudioEvent, hasPos bool, pos Vec3) {
	ch := api.SoundChannel(ev.Channel)
	dom, group, pri := m.classify(ev.Cue, ch)

	slot := -1
	if m.alloc != nil {
		d := m.alloc.Admit(VoiceRequest{
			Cue:       ev.Cue,
			Asset:     ev.Cue, // cue identifies the asset for coalescing (no per-instance variants yet)
			Partition: partitionFor(dom, ch),
			Priority:  pri,
			Pos:       pos,
			HasPos:    hasPos,
			Volume:    ev.Volume,
			TimeMs:    m.seq,
		})
		m.seq++
		switch d.Outcome {
		case CulledDistance:
			m.culled++
			return
		case Dropped:
			m.dropped++
			return
		case Coalesced:
			// Merge into the existing same-asset voice: bump its gain (already capped
			// by the allocator), never restart it.
			if i := m.voiceAtSlot(d.Slot); i >= 0 {
				m.voices[i].Gain = clamp(m.voices[i].Gain+d.GainBump, 0, 1)
				m.backend.Play(m.voices[i])
			}
			return
		case Stolen:
			// The allocator already reassigned slot d.Victim to this request; evict the
			// old voice occupying it before we append the new one at the same slot.
			m.evictVoiceAtSlot(d.Victim)
		}
		slot = d.Slot
	}

	r := m.resolve(dom, group, ch, pos, hasPos, ev.Volume)
	if r.Culled {
		// Defensive: the allocator already culls at the same radius, so an admitted
		// voice should never resolve as culled. Free the slot to keep the invariant.
		m.culled++
		if slot >= 0 {
			m.alloc.Release(slot)
		}
		return // beyond max audible — never plays (R-AUD-1.3)
	}
	pitch := ev.Pitch
	if pitch == 0 {
		pitch = 1.0
	}
	v := Voice{
		Cue:     ev.Cue,
		Channel: uint8(ev.Channel),
		Domain:  dom,
		Group:   group,
		Gain:    r.Gain,
		Pan:     r.Pan,
		Pitch:   pitch,
		Pos:     pos,
		HasPos:  hasPos,
		Slot:    slot,
	}
	m.voices = append(m.voices, v)
	m.voiceVol = append(m.voiceVol, ev.Volume)
	m.backend.Play(v)
}

// voiceAtSlot returns the index of the active voice occupying allocator slot, or
// -1. The slot↔voice link is 1:1, so the first match is the only match.
func (m *Manager) voiceAtSlot(slot int) int {
	for i := range m.voices {
		if m.voices[i].Slot == slot {
			return i
		}
	}
	return -1
}

// evictVoiceAtSlot silences and removes the mixer voice occupying slot. The
// allocator slot itself is NOT released here — the caller (a steal) has already
// reassigned it to the incoming request.
func (m *Manager) evictVoiceAtSlot(slot int) {
	i := m.voiceAtSlot(slot)
	if i < 0 {
		return
	}
	m.backend.Stop(m.voices[i].Cue)
	m.removeVoice(i)
}

// removeVoice swap-deletes the voice at index i (order-independent; the dump
// sorts nothing, and determinism holds because admission order is deterministic).
func (m *Manager) removeVoice(i int) {
	last := len(m.voices) - 1
	m.voices[i] = m.voices[last]
	m.voiceVol[i] = m.voiceVol[last]
	m.voices = m.voices[:last]
	m.voiceVol = m.voiceVol[:last]
}

// stopCue removes (and silences) every active voice for cue, freeing each one's
// allocator slot so the budget reflects the stop.
func (m *Manager) stopCue(cue uint32) {
	m.backend.Stop(cue)
	dst := 0
	for i := range m.voices {
		if m.voices[i].Cue == cue {
			if m.alloc != nil && m.voices[i].Slot >= 0 {
				m.alloc.Release(m.voices[i].Slot)
			}
			continue
		}
		m.voices[dst] = m.voices[i]
		m.voiceVol[dst] = m.voiceVol[i]
		dst++
	}
	m.voices = m.voices[:dst]
	m.voiceVol = m.voiceVol[:dst]
}

// stopChannel removes (and silences) every active voice on ch. StopMusic uses
// this because its public API intentionally does not expose the current cue id.
func (m *Manager) stopChannel(ch api.SoundChannel) {
	dst := 0
	for i := range m.voices {
		if api.SoundChannel(m.voices[i].Channel) == ch {
			m.backend.Stop(m.voices[i].Cue)
			if m.alloc != nil && m.voices[i].Slot >= 0 {
				m.alloc.Release(m.voices[i].Slot)
			}
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
