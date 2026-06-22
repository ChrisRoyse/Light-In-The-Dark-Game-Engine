//go:build openal

package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

func TestOpenALCueBufferBindingFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	buffers, ok := backend.(litaudio.CueBufferBackend)
	if !ok {
		t.Fatalf("OpenAL backend does not expose cue-buffer loading")
	}

	dir := t.TempDir()
	writeFixture := func(rel string, data []byte) string {
		t.Helper()
		p := filepath.Join(dir, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, data, 0o644); err != nil {
			t.Fatal(err)
		}
		return p
	}

	sfx := writeFixture("sfx/hit.ogg", mustDecodeB64(t, renderdemoMonoVorbis1sB64))
	music := writeFixture("music/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))
	cue := api.CueID("synthetic_hit")
	if _, ok := buffers.CueBufferInfo(cue); ok {
		t.Fatalf("BEFORE buffer state: cue %d already had a resident buffer", cue)
	}

	if err := buffers.LoadCueBuffer(cue, sfx); err != nil {
		t.Fatalf("LoadCueBuffer resident Vorbis: %v", err)
	}
	loaded, ok := buffers.CueBufferInfo(cue)
	if !ok || loaded.BufferID == 0 || loaded.Bytes != 88200 || loaded.Channels != 1 || loaded.SampleRate != 44100 {
		t.Fatalf("AFTER buffer state mismatch: ok=%v info=%+v", ok, loaded)
	}

	m := litaudio.NewManager(backend)
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: cue, Volume: 1, Channel: api.ChannelEffects,
		HasPos: true, Pos: api.Vec2{X: 25, Y: 0},
	})
	playing, ok := buffers.CueBufferInfo(cue)
	if !ok || playing.SourceID == 0 || !playing.Playing {
		t.Fatalf("AFTER play source state mismatch: ok=%v info=%+v", ok, playing)
	}

	if err := buffers.LoadCueBuffer(0, sfx); err == nil {
		t.Fatalf("zero cue must be rejected")
	}
	if err := buffers.LoadCueBuffer(api.CueID("theme"), music); err == nil || !strings.Contains(err.Error(), "streamed music") {
		t.Fatalf("music must stay on streaming path, got err=%v", err)
	}
	if err := buffers.LoadCueBuffer(api.CueID("missing"), filepath.Join(dir, "sfx", "missing.ogg")); err == nil {
		t.Fatalf("missing cue file must be rejected")
	}
	t.Logf("FSV #228 OpenAL cue buffer: before=absent after=buffer{id=%d bytes=%d ch=%d rate=%d} play={source=%d state=%d playing=%v}",
		loaded.BufferID, loaded.Bytes, loaded.Channels, loaded.SampleRate, playing.SourceID, playing.SourceState, playing.Playing)
}
