//go:build openal

package audio

import (
	"fmt"
	"io"
	"os"
	"strings"
	"unsafe"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio/oggmeta"
	g3naudio "github.com/g3n/engine/audio"
	"github.com/g3n/engine/audio/al"
)

type openALStream struct {
	active           bool
	kind             StreamKind
	slot             int
	source           uint32
	buffers          []uint32
	file             *g3naudio.AudioFile
	filename         string
	loop             bool
	format           uint32
	channels         int
	sampleRate       int
	scratch          []byte
	nextBuffer       int
	totalBytesQueued int64
	refills          int
	underruns        int
	eof              bool
	lastReadBytes    int
}

func (b *openALBackend) SetStreamUnderrunReporter(r StreamUnderrunReporter) {
	b.streamReporter = r
}

func (b *openALBackend) StartStream(kind StreamKind, filename string, loop bool, gain float64) (StreamDeviceInfo, error) {
	idx, slot, err := openALStreamIndex(kind)
	if err != nil {
		return StreamDeviceInfo{}, err
	}
	if len(b.sources) <= slot {
		return StreamDeviceInfo{}, fmt.Errorf("audio: OpenAL source slot %d unavailable for %s stream", slot, kind)
	}
	if len(b.streamBuffers[idx]) != StreamRingChunks {
		return StreamDeviceInfo{}, fmt.Errorf("audio: OpenAL %s stream has %d buffers, want %d", kind, len(b.streamBuffers[idx]), StreamRingChunks)
	}
	if err := validateOpenALStreamFile(filename); err != nil {
		return b.streamInfoAt(idx), err
	}

	af, err := g3naudio.NewAudioFile(filename)
	if err != nil {
		return b.streamInfoAt(idx), fmt.Errorf("audio: open %s stream %s: %w", kind, filename, err)
	}
	af.SetLooping(loop)
	ainfo := af.Info()
	if ainfo.Channels != 2 || ainfo.SampleRate <= 0 || ainfo.Format <= 0 {
		af.Close()
		return b.streamInfoAt(idx), fmt.Errorf("audio: %s stream %s decoder layout channels=%d sampleRate=%d format=%d", kind, filename, ainfo.Channels, ainfo.SampleRate, ainfo.Format)
	}

	if _, err := b.stopStreamAt(idx); err != nil {
		_ = af.Close()
		return b.streamInfoAt(idx), err
	}
	source := b.sources[slot]
	st := &b.streams[idx]
	*st = openALStream{
		active:     true,
		kind:       kind,
		slot:       slot,
		source:     source,
		buffers:    b.streamBuffers[idx],
		file:       af,
		filename:   filename,
		loop:       loop,
		format:     uint32(ainfo.Format),
		channels:   ainfo.Channels,
		sampleRate: ainfo.SampleRate,
		scratch:    make([]byte, StreamChunkBytes),
	}

	al.SourceStop(source)
	if err := b.unqueueStreamBuffers(source); err != nil {
		_ = af.Close()
		*st = openALStream{}
		return b.streamInfoAt(idx), err
	}
	al.Sourcei(source, al.Buffer, 0)
	al.Sourcef(source, al.Gain, float32(clamp(gain, 0, 1)))
	al.Sourcef(source, al.Pitch, 1)
	al.Sourcef(source, al.RolloffFactor, 0)
	al.Source3f(source, al.Position, 0, 0, 0)

	queued, err := b.fillStreamRing(st, len(st.buffers))
	if err != nil {
		_ = af.Close()
		*st = openALStream{}
		return b.streamInfoAt(idx), err
	}
	if queued == 0 {
		_ = af.Close()
		*st = openALStream{}
		return b.streamInfoAt(idx), fmt.Errorf("audio: %s stream %s decoded no PCM bytes", kind, filename)
	}
	al.SourcePlay(source)
	if err := al.GetError(); err != nil {
		_ = af.Close()
		*st = openALStream{}
		return b.streamInfoAt(idx), fmt.Errorf("audio: start %s stream %s: %w", kind, filename, err)
	}
	return b.streamInfoAt(idx), nil
}

func (b *openALBackend) UpdateStream(kind StreamKind) (StreamDeviceInfo, error) {
	idx, _, err := openALStreamIndex(kind)
	if err != nil {
		return StreamDeviceInfo{}, err
	}
	st := &b.streams[idx]
	if !st.active {
		return b.streamInfoAt(idx), nil
	}

	processed := al.GetSourcei(st.source, al.BuffersProcessed)
	if processed > 0 {
		al.SourceUnqueueBuffers(st.source, uint32(processed), nil)
		if err := al.GetError(); err != nil {
			return b.streamInfoAt(idx), fmt.Errorf("audio: unqueue %s stream buffers: %w", kind, err)
		}
		for i := 0; i < int(processed); i++ {
			buf := st.buffers[st.nextBuffer%len(st.buffers)]
			st.nextBuffer = (st.nextBuffer + 1) % len(st.buffers)
			ok, err := b.queueStreamBuffer(st, buf)
			if err != nil {
				return b.streamInfoAt(idx), err
			}
			if !ok {
				break
			}
		}
	}

	queued := al.GetSourcei(st.source, al.BuffersQueued)
	state := al.GetSourcei(st.source, al.SourceState)
	if queued == 0 && !st.eof {
		if err := b.recoverStreamUnderrun(st, 0); err != nil {
			return b.streamInfoAt(idx), err
		}
		queued = al.GetSourcei(st.source, al.BuffersQueued)
		state = al.GetSourcei(st.source, al.SourceState)
	}
	if queued > 0 && state != int32(al.Playing) && !st.eof {
		if err := b.recoverStreamUnderrun(st, int(queued)*StreamChunkBytes); err != nil {
			return b.streamInfoAt(idx), err
		}
	}
	if st.eof && al.GetSourcei(st.source, al.BuffersQueued) == 0 {
		st.active = false
		if st.file != nil {
			if err := st.file.Close(); err != nil {
				return b.streamInfoAt(idx), fmt.Errorf("audio: close finished %s stream: %w", kind, err)
			}
			st.file = nil
		}
	}
	return b.streamInfoAt(idx), nil
}

func (b *openALBackend) StopStream(kind StreamKind) (StreamDeviceInfo, error) {
	idx, _, err := openALStreamIndex(kind)
	if err != nil {
		return StreamDeviceInfo{}, err
	}
	return b.stopStreamAt(idx)
}

func (b *openALBackend) StreamInfo(kind StreamKind) (StreamDeviceInfo, bool) {
	idx, _, err := openALStreamIndex(kind)
	if err != nil {
		return StreamDeviceInfo{}, false
	}
	return b.streamInfoAt(idx), true
}

func (b *openALBackend) stopStreamAt(idx int) (StreamDeviceInfo, error) {
	if idx < 0 || idx >= len(b.streams) {
		return StreamDeviceInfo{}, fmt.Errorf("audio: invalid OpenAL stream index %d", idx)
	}
	info := b.streamInfoAt(idx)
	st := &b.streams[idx]
	source := info.SourceID
	if source != 0 {
		al.SourceStop(source)
		if err := b.unqueueStreamBuffers(source); err != nil {
			return info, err
		}
		al.Sourcei(source, al.Buffer, 0)
		if err := al.GetError(); err != nil {
			return info, fmt.Errorf("audio: stop %s stream: %w", info.Kind, err)
		}
	}
	if st.file != nil {
		if err := st.file.Close(); err != nil {
			return info, fmt.Errorf("audio: close %s stream %s: %w", st.kind, st.filename, err)
		}
	}
	*st = openALStream{}
	return b.streamInfoAt(idx), nil
}

func (b *openALBackend) fillStreamRing(st *openALStream, limit int) (int, error) {
	queued := 0
	for i := 0; i < limit; i++ {
		buf := st.buffers[st.nextBuffer%len(st.buffers)]
		st.nextBuffer = (st.nextBuffer + 1) % len(st.buffers)
		ok, err := b.queueStreamBuffer(st, buf)
		if err != nil {
			return queued, err
		}
		if !ok {
			return queued, nil
		}
		queued++
	}
	return queued, nil
}

func (b *openALBackend) queueStreamBuffer(st *openALStream, buffer uint32) (bool, error) {
	if st.file == nil {
		st.eof = true
		return false, nil
	}
	n, rerr := st.file.Read(unsafe.Pointer(&st.scratch[0]), len(st.scratch))
	st.lastReadBytes = n
	if n > 0 {
		al.BufferData(buffer, st.format, unsafe.Pointer(&st.scratch[0]), uint32(n), uint32(st.sampleRate))
		if err := al.GetError(); err != nil {
			return false, fmt.Errorf("audio: buffer %s stream chunk: %w", st.kind, err)
		}
		al.SourceQueueBuffers(st.source, buffer)
		if err := al.GetError(); err != nil {
			return false, fmt.Errorf("audio: queue %s stream chunk: %w", st.kind, err)
		}
		st.totalBytesQueued += int64(n)
		st.refills++
		if rerr == io.EOF {
			st.eof = true
		}
		return true, nil
	}
	if rerr == io.EOF {
		st.eof = true
		return false, nil
	}
	if rerr != nil {
		return false, fmt.Errorf("audio: decode %s stream %s: %w", st.kind, st.filename, rerr)
	}
	return false, fmt.Errorf("audio: decode %s stream %s returned 0 bytes without EOF", st.kind, st.filename)
}

func (b *openALBackend) recoverStreamUnderrun(st *openALStream, observedFill int) error {
	st.underruns++
	if b.streamReporter != nil {
		b.streamReporter.ReportUnderrun(st.kind, observedFill)
	}
	if al.GetSourcei(st.source, al.BuffersQueued) == 0 && !st.eof {
		if _, err := b.fillStreamRing(st, len(st.buffers)); err != nil {
			return err
		}
	}
	if al.GetSourcei(st.source, al.BuffersQueued) > 0 {
		al.SourcePlay(st.source)
		if err := al.GetError(); err != nil {
			return fmt.Errorf("audio: restart %s stream after underrun: %w", st.kind, err)
		}
	}
	return nil
}

func (b *openALBackend) unqueueStreamBuffers(source uint32) error {
	queued := al.GetSourcei(source, al.BuffersQueued)
	if queued <= 0 {
		return nil
	}
	al.SourceUnqueueBuffers(source, uint32(queued), nil)
	if err := al.GetError(); err != nil {
		return fmt.Errorf("audio: unqueue OpenAL stream buffers: %w", err)
	}
	return nil
}

func (b *openALBackend) streamInfoAt(idx int) StreamDeviceInfo {
	kind, slot := openALStreamIdentity(idx)
	source := uint32(0)
	if slot >= 0 && slot < len(b.sources) {
		source = b.sources[slot]
	}
	buffers := append([]uint32(nil), b.streamBuffers[idx]...)
	st := &b.streams[idx]
	if st.source != 0 {
		source = st.source
	}
	info := StreamDeviceInfo{
		Kind:             kind,
		Slot:             slot,
		SourceID:         source,
		BufferIDs:        buffers,
		ChunkBytes:       StreamChunkBytes,
		RingChunks:       StreamRingChunks,
		BufferBytes:      StreamChunkBytes * StreamRingChunks,
		Active:           st.active,
		Filename:         st.filename,
		Loop:             st.loop,
		Channels:         st.channels,
		SampleRate:       st.sampleRate,
		TotalBytesQueued: st.totalBytesQueued,
		Refills:          st.refills,
		Underruns:        st.underruns,
		EOF:              st.eof,
		LastReadBytes:    st.lastReadBytes,
	}
	if source != 0 {
		info.Queued = al.GetSourcei(source, al.BuffersQueued)
		info.Processed = al.GetSourcei(source, al.BuffersProcessed)
		info.SourceState = al.GetSourcei(source, al.SourceState)
		info.Playing = info.SourceState == int32(al.Playing)
	}
	return info
}

func validateOpenALStreamFile(filename string) error {
	if openALCategoryOfFilename(filename) != oggmeta.CatMusic {
		return fmt.Errorf("audio: stream file %s must live under a music directory", filename)
	}
	f, err := os.Open(filename)
	if err != nil {
		return fmt.Errorf("audio: read stream %s: %w", filename, err)
	}
	info, perr := oggmeta.ParseReader(f)
	cerr := f.Close()
	if perr != nil {
		return fmt.Errorf("audio: parse stream %s: %w", filename, perr)
	}
	if cerr != nil {
		return fmt.Errorf("audio: close stream %s: %w", filename, cerr)
	}
	findings, resident := oggmeta.CheckLayout(info, oggmeta.CatMusic)
	if len(findings) > 0 {
		parts := make([]string, 0, len(findings))
		for _, f := range findings {
			parts = append(parts, f.Rule+": "+f.Msg)
		}
		return fmt.Errorf("audio: stream layout %s: %s", filename, strings.Join(parts, "; "))
	}
	if resident {
		return fmt.Errorf("audio: stream %s was classified resident; expected streamed music", filename)
	}
	return nil
}

func openALStreamIndex(kind StreamKind) (idx, slot int, err error) {
	switch kind {
	case StreamMusic:
		return 0, MusicStreamSlot, nil
	case StreamAmbience:
		return 1, AmbienceStreamSlot, nil
	default:
		return -1, -1, fmt.Errorf("audio: unsupported stream kind %q", kind)
	}
}

func openALStreamIdentity(idx int) (StreamKind, int) {
	switch idx {
	case 0:
		return StreamMusic, MusicStreamSlot
	case 1:
		return StreamAmbience, AmbienceStreamSlot
	default:
		return StreamKind(""), -1
	}
}
