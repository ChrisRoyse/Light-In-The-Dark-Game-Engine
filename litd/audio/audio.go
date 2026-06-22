// Package audio is the presentation-side audio manager (#227, sound-and-music.md;
// PRD §9.1). It consumes the sim's resolved api.AudioEvent stream (installed via
// Game.OnAudio) and drives an audio device through a thin Backend.
//
// Determinism boundary (R-AUD-1): audio is presentation-only and never feeds back
// into the sim. ALL voice bookkeeping — listener tracking, distance attenuation,
// stereo pan, per-channel mix, the active-voice table — lives in Manager, which is
// pure Go and device-independent. The Backend is only the final device sink. So a
// "null" backend (no device) runs the EXACT SAME accounting code path as a real
// device; headless CI therefore exercises the real logic, and a scripted scenario
// produces a byte-identical audio state dump whether or not a device exists. The
// real OpenAL backend lives behind the `openal` build tag (openal.go) so the core
// package — and its tests — build and run with no cgo / no device.
package audio

import "math"

// Vec3 is a world position in audio space: the sim's planar (X,Y) plus height Z.
// Audio uses its own value type rather than importing a render/math vector so the
// package carries no rendering dependency.
type Vec3 struct {
	X, Y, Z float64
}

// sub returns v-o.
func (v Vec3) sub(o Vec3) Vec3 { return Vec3{v.X - o.X, v.Y - o.Y, v.Z - o.Z} }

// length returns the Euclidean magnitude of v.
func (v Vec3) length() float64 {
	return math.Sqrt(v.X*v.X + v.Y*v.Y + v.Z*v.Z)
}

// Voice is one resolved, currently-active sound on the device. Its Gain and Pan
// are the FINAL computed values (channel mix × distance attenuation × stereo
// position) — the Backend plays them verbatim. This struct is the unit of audio
// state the FSV dump inspects.
type Voice struct {
	Cue     uint32      `json:"cue"`     // Sound cue hash (api.AudioEvent.Cue)
	Channel uint8       `json:"channel"` // mix group (api.SoundChannel)
	Domain  Domain      `json:"domain"`  // resolved playback domain (table-classified or channel-inferred; #428)
	Group   VolumeGroup `json:"group"`   // resolved master volume group (matches Domain for SFX)
	Gain    float64     `json:"gain"`    // final playback gain in [0,1]
	Pan     float64     `json:"pan"`     // stereo pan in [-1,1]; <0 left, >0 right, 0 center
	Pitch   float64     `json:"pitch"`   // pitch multiplier (1.0 = unshifted)
	Pos     Vec3        `json:"pos"`     // world position (zero for non-positional)
	HasPos  bool        `json:"hasPos"`  // true for 3D positional voices
	Slot    int         `json:"slot"`    // allocator source slot (#230); -1 if admission disabled
}

// Backend is the device sink. The Manager hands it fully-resolved Voices and
// listener updates; the Backend's only job is to make (or not make) sound. It
// holds NO mix/accounting state — that is the Manager's, so every Backend shares
// one accounting code path.
type Backend interface {
	// Name identifies the backend in the state dump ("null", "openal").
	Name() string
	// SourceCount reports the number of concrete device sources owned by this
	// backend. The null backend reports 0; OpenAL reports the preallocated pool.
	SourceCount() int
	// Play emits a resolved voice on the device. The null backend no-ops.
	Play(v Voice)
	// Stop silences any device voices for cue.
	Stop(cue uint32)
	// SetListener moves the device listener to pos.
	SetListener(pos Vec3)
	// Close releases device resources.
	Close() error
}

// CueBufferInfo is the OpenAL cue-buffer state exposed for FSV. It is only
// populated by a real buffer-owning backend; null/headless backends do not
// implement CueBufferBackend.
type CueBufferInfo struct {
	Cue         uint32 `json:"cue"`
	BufferID    uint32 `json:"bufferId"`
	Bytes       int    `json:"bytes"`
	Channels    int    `json:"channels"`
	SampleRate  int    `json:"sampleRate"`
	SourceID    uint32 `json:"sourceId,omitempty"`
	SourceState int32  `json:"sourceState,omitempty"`
	Playing     bool   `json:"playing"`
}

// CueBufferBackend is the optional resident cue-buffer capability provided by
// the OpenAL backend. It is kept outside Backend so headless/null tests preserve
// their exact no-device behavior.
type CueBufferBackend interface {
	LoadCueBuffer(cue uint32, filename string) error
	CueBufferInfo(cue uint32) (CueBufferInfo, bool)
}

// StreamDeviceInfo is the OpenAL stream-ring state exposed for FSV. It is the
// device-side counterpart to StreamController's pure-Go stream snapshot.
type StreamDeviceInfo struct {
	Kind             StreamKind `json:"kind"`
	Slot             int        `json:"slot"`
	SourceID         uint32     `json:"sourceId,omitempty"`
	BufferIDs        []uint32   `json:"bufferIds,omitempty"`
	ChunkBytes       int        `json:"chunkBytes"`
	RingChunks       int        `json:"ringChunks"`
	BufferBytes      int        `json:"bufferBytes"`
	Queued           int32      `json:"queued"`
	Processed        int32      `json:"processed"`
	SourceState      int32      `json:"sourceState"`
	Playing          bool       `json:"playing"`
	Active           bool       `json:"active"`
	Filename         string     `json:"filename,omitempty"`
	Loop             bool       `json:"loop"`
	Channels         int        `json:"channels,omitempty"`
	SampleRate       int        `json:"sampleRate,omitempty"`
	TotalBytesQueued int64      `json:"totalBytesQueued"`
	Refills          int        `json:"refills"`
	Underruns        int        `json:"underruns"`
	EOF              bool       `json:"eof"`
	LastReadBytes    int        `json:"lastReadBytes"`
}

// StreamUnderrunReporter is implemented by StreamController. A streaming backend
// calls it after observing an empty device queue and before refilling the ring.
type StreamUnderrunReporter interface {
	ReportUnderrun(kind StreamKind, observedFill int)
}

// StreamBackend is the optional queued-buffer stream capability provided by the
// OpenAL backend for the two fixed stream slots: music and ambience.
type StreamBackend interface {
	StartStream(kind StreamKind, filename string, loop bool, gain float64) (StreamDeviceInfo, error)
	UpdateStream(kind StreamKind) (StreamDeviceInfo, error)
	StopStream(kind StreamKind) (StreamDeviceInfo, error)
	StreamInfo(kind StreamKind) (StreamDeviceInfo, bool)
	SetStreamUnderrunReporter(StreamUnderrunReporter)
}

// nullBackend is the no-op device: it makes no sound but is otherwise a fully
// valid sink. The Manager does all accounting regardless, so the null backend is
// the deterministic, headless-testable path that mirrors a real device exactly.
type nullBackend struct{}

func (nullBackend) Name() string     { return "null" }
func (nullBackend) SourceCount() int { return 0 }
func (nullBackend) Play(Voice)       {}
func (nullBackend) Stop(uint32)      {}
func (nullBackend) SetListener(Vec3) {}
func (nullBackend) Close() error     { return nil }
