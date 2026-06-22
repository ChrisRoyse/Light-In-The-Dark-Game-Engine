//go:build openal

// Package audio's OpenAL device backend. Built ONLY with `-tags openal` so the
// core package (and CI) stays pure-Go and device-free; the desktop renderdemo
// builds with the tag to get real sound. Device-open failure degrades to the null
// backend via NewManager(nil) — absence of a device is never fatal.
//
// Scope (#227/#228): device/context lifecycle, listener positioning, pooled
// sources, and resident cue -> decoded PCM buffer binding. Distance/channel
// attenuation is already folded into Voice.Gain by the Manager, so OpenAL's own
// rolloff is disabled (RolloffFactor 0) to avoid double-attenuation. On-hardware
// FSV is tracked separately; the OpenAL null driver still verifies buffer/source
// state without requiring speakers.
package audio

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"unsafe"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio/oggmeta"
	"github.com/g3n/engine/audio/al"
)

type openALCueBuffer struct {
	id         uint32
	bytes      int
	channels   int
	sampleRate int
}

// openALBackend drives a real OpenAL device. One pooled source per active voice
// slot (MaxVoices); the Manager owns the mix, this only places sources.
type openALBackend struct {
	dev        *al.Device
	ctx        *al.Context
	sources    []uint32                   // the source pool
	byCue      map[uint32]uint32          // cue -> assigned source (latest)
	cueBuffers map[uint32]openALCueBuffer // cue -> resident OpenAL buffer
	next       int                        // round-robin cursor into sources
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
		dev:        dev,
		ctx:        ctx,
		sources:    make([]uint32, MaxVoices),
		byCue:      make(map[uint32]uint32, MaxVoices),
		cueBuffers: make(map[uint32]openALCueBuffer),
	}
	al.GenSources(b.sources)
	return b, nil
}

func (b *openALBackend) Name() string { return "openal" }

func (b *openALBackend) SourceCount() int { return len(b.sources) }

// assign returns the source for cue, round-robin allocating one on first use.
func (b *openALBackend) assign(cue uint32) (uint32, bool) {
	if s, ok := b.byCue[cue]; ok {
		return s, false
	}
	s := b.sources[b.next%len(b.sources)]
	b.next++
	b.byCue[cue] = s
	return s, true
}

// LoadCueBuffer decodes one resident Vorbis .ogg into a static OpenAL buffer for
// cue. Music is intentionally rejected here: music uses the streaming ring path.
func (b *openALBackend) LoadCueBuffer(cue uint32, filename string) error {
	if cue == 0 {
		return fmt.Errorf("audio: cannot load OpenAL buffer for zero cue")
	}
	body, err := os.ReadFile(filename)
	if err != nil {
		return fmt.Errorf("audio: cue %d read %s: %w", cue, filename, err)
	}
	info, err := oggmeta.Parse(body)
	if err != nil {
		return fmt.Errorf("audio: cue %d parse %s: %w", cue, filename, err)
	}
	cat := openALCategoryOfFilename(filename)
	findings, resident := oggmeta.CheckLayout(info, cat)
	if len(findings) > 0 {
		var parts []string
		for _, f := range findings {
			parts = append(parts, f.Rule+": "+f.Msg)
		}
		return fmt.Errorf("audio: cue %d layout %s: %s", cue, filename, strings.Join(parts, "; "))
	}
	if !resident {
		return fmt.Errorf("audio: cue %d %s is streamed music; resident cue buffers require world/ui/voice assets", cue, filename)
	}
	pcm, err := decodePCMFile(filename, info.DecodedBytes())
	if err != nil {
		return fmt.Errorf("audio: cue %d decode %s: %w", cue, filename, err)
	}
	if int64(len(pcm)) != info.DecodedBytes() {
		return fmt.Errorf("audio: cue %d decoded %d PCM bytes, metadata expected %d", cue, len(pcm), info.DecodedBytes())
	}
	if len(pcm) == 0 {
		return fmt.Errorf("audio: cue %d decoded no PCM bytes", cue)
	}
	format, err := openALPCMFormat(info.Channels)
	if err != nil {
		return fmt.Errorf("audio: cue %d: %w", cue, err)
	}
	bufs := al.GenBuffers(1)
	if len(bufs) != 1 || bufs[0] == 0 {
		return fmt.Errorf("audio: cue %d OpenAL buffer allocation failed", cue)
	}
	_ = al.GetError()
	al.BufferData(bufs[0], format, unsafe.Pointer(&pcm[0]), uint32(len(pcm)), uint32(info.SampleRate))
	if err := al.GetError(); err != nil {
		al.DeleteBuffers(bufs)
		return fmt.Errorf("audio: cue %d OpenAL buffer data: %w", cue, err)
	}
	if s, ok := b.byCue[cue]; ok {
		al.SourceStop(s)
		al.Sourcei(s, al.Buffer, 0)
		delete(b.byCue, cue)
	}
	if old, ok := b.cueBuffers[cue]; ok {
		al.DeleteBuffers([]uint32{old.id})
	}
	b.cueBuffers[cue] = openALCueBuffer{
		id:         bufs[0],
		bytes:      len(pcm),
		channels:   info.Channels,
		sampleRate: info.SampleRate,
	}
	return nil
}

func (b *openALBackend) CueBufferInfo(cue uint32) (CueBufferInfo, bool) {
	buf, ok := b.cueBuffers[cue]
	if !ok {
		return CueBufferInfo{}, false
	}
	info := CueBufferInfo{
		Cue:        cue,
		BufferID:   buf.id,
		Bytes:      buf.bytes,
		Channels:   buf.channels,
		SampleRate: buf.sampleRate,
	}
	if s, assigned := b.byCue[cue]; assigned {
		info.SourceID = s
		info.SourceState = al.GetSourcei(s, al.SourceState)
		info.Playing = info.SourceState == int32(al.Playing)
	}
	return info, true
}

func (b *openALBackend) Play(v Voice) {
	buf, ok := b.cueBuffers[v.Cue]
	if !ok {
		return
	}
	s, fresh := b.assign(v.Cue)
	al.Sourcef(s, al.Gain, float32(v.Gain))
	al.Sourcef(s, al.Pitch, float32(v.Pitch))
	al.Sourcef(s, al.RolloffFactor, 0) // Manager already applied distance attenuation
	al.Source3f(s, al.Position, float32(v.Pos.X), float32(v.Pos.Y), float32(v.Pos.Z))
	if fresh {
		al.Sourcei(s, al.Buffer, int32(buf.id))
		al.SourcePlay(s)
	}
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
	if len(b.cueBuffers) > 0 {
		bufs := make([]uint32, 0, len(b.cueBuffers))
		for _, buf := range b.cueBuffers {
			bufs = append(bufs, buf.id)
		}
		al.DeleteBuffers(bufs)
	}
	// G3N's MakeContextCurrent wrapper dereferences nil, so do not pass nil
	// here to clear the current context.
	al.DestroyContext(b.ctx)
	if err := al.CloseDevice(b.dev); err != nil {
		return fmt.Errorf("audio: OpenAL close: %w", err)
	}
	return nil
}

func openALPCMFormat(channels int) (uint32, error) {
	switch channels {
	case 1:
		return uint32(al.FormatMono16), nil
	case 2:
		return uint32(al.FormatStereo16), nil
	default:
		return 0, fmt.Errorf("unsupported OpenAL PCM channel count %d", channels)
	}
}

func openALCategoryOfFilename(filename string) oggmeta.Category {
	parts := strings.Split(strings.ToLower(filepath.ToSlash(filename)), "/")
	for i := len(parts) - 2; i >= 0; i-- {
		switch parts[i] {
		case "music":
			return oggmeta.CatMusic
		case "voice":
			return oggmeta.CatVoice
		case "ui":
			return oggmeta.CatUI
		case "sfx", "world":
			return oggmeta.CatWorldSFX
		}
	}
	return oggmeta.CategoryOf(filepath.ToSlash(filename))
}
