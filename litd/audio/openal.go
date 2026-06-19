//go:build openal

// Package audio's OpenAL device backend. Built ONLY with `-tags openal` so the
// core package (and CI) stays pure-Go and device-free; the desktop renderdemo
// builds with the tag to get real sound. Device-open failure degrades to the null
// backend via NewManager(nil) — absence of a device is never fatal.
//
// Scope (#227): device/context lifecycle, listener positioning, and a source pool
// to which the Manager's pre-resolved gain/pan/pitch are applied. Distance/channel
// attenuation is already folded into Voice.Gain by the Manager, so OpenAL's own
// rolloff is disabled (RolloffFactor 0) to avoid double-attenuation. Binding cue →
// decoded PCM buffer (actual audible playback) is #228; until then a source is
// configured but not started. On-hardware FSV is tracked separately (this repo's
// device-dependent paths cannot be verified in the headless CI/WSL environment).
package audio

import (
	"fmt"

	"github.com/g3n/engine/audio/al"
)

// openALBackend drives a real OpenAL device. One pooled source per active voice
// slot (MaxVoices); the Manager owns the mix, this only places sources.
type openALBackend struct {
	dev     *al.Device
	ctx     *al.Context
	sources []uint32          // the source pool
	byCue   map[uint32]uint32 // cue -> assigned source (latest)
	next    int               // round-robin cursor into sources
}

// OpenDevice opens the default OpenAL device and builds a backend. Returns an
// error (so the caller can fall back to null) when no device is available.
func OpenDevice() (Backend, error) {
	dev, err := al.OpenDevice("")
	if err != nil || dev == nil {
		return nil, fmt.Errorf("audio: OpenAL device unavailable: %w", err)
	}
	ctx, err := al.CreateContext(dev, nil)
	if err != nil {
		al.CloseDevice(dev)
		return nil, fmt.Errorf("audio: OpenAL context: %w", err)
	}
	if err := al.MakeContextCurrent(ctx); err != nil {
		al.DestroyContext(ctx)
		al.CloseDevice(dev)
		return nil, fmt.Errorf("audio: OpenAL make-current: %w", err)
	}
	b := &openALBackend{
		dev:     dev,
		ctx:     ctx,
		sources: make([]uint32, MaxVoices),
		byCue:   make(map[uint32]uint32, MaxVoices),
	}
	al.GenSources(b.sources)
	return b, nil
}

func (b *openALBackend) Name() string { return "openal" }

// assign returns the source for cue, round-robin allocating one on first use.
func (b *openALBackend) assign(cue uint32) uint32 {
	if s, ok := b.byCue[cue]; ok {
		return s
	}
	s := b.sources[b.next%len(b.sources)]
	b.next++
	b.byCue[cue] = s
	return s
}

func (b *openALBackend) Play(v Voice) {
	s := b.assign(v.Cue)
	al.Sourcef(s, al.Gain, float32(v.Gain))
	al.Sourcef(s, al.Pitch, float32(v.Pitch))
	al.Sourcef(s, al.RolloffFactor, 0) // Manager already applied distance attenuation
	al.Source3f(s, al.Position, float32(v.Pos.X), float32(v.Pos.Y), float32(v.Pos.Z))
	// Buffer binding (cue -> decoded PCM) and SourcePlay are #228; configured here,
	// not started, so this never emits an unbuffered (silent) source error.
}

func (b *openALBackend) Stop(cue uint32) {
	if s, ok := b.byCue[cue]; ok {
		al.SourceStop(s)
		delete(b.byCue, cue)
	}
}

func (b *openALBackend) SetListener(pos Vec3) {
	al.Listener3f(al.Position, float32(pos.X), float32(pos.Y), float32(pos.Z))
}

func (b *openALBackend) Close() error {
	if len(b.sources) > 0 {
		al.DeleteSources(b.sources)
	}
	al.MakeContextCurrent(nil)
	al.DestroyContext(b.ctx)
	if err := al.CloseDevice(b.dev); err != nil {
		return fmt.Errorf("audio: OpenAL close: %w", err)
	}
	return nil
}
