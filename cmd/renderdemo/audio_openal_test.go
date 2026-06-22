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

func TestBuildAudioInitDumpOpenALFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	dump, err := buildAudioInitDump("openal")
	if err != nil {
		t.Fatalf("OpenAL audio init dump failed: %v", err)
	}
	if !dump.OK {
		t.Fatalf("OpenAL audio init dump not OK: %+v", dump.Errors)
	}
	if dump.Backend != "openal" || dump.BackendSources != litaudio.MaxVoices || dump.Snapshot.BackendSources != litaudio.MaxVoices {
		t.Fatalf("OpenAL backend/source SoT mismatch: %+v", dump)
	}
	if dump.AccountingMaxVoices != litaudio.MaxVoices || dump.Snapshot.MaxVoices != litaudio.MaxVoices || dump.Snapshot.VoiceCount != 3 {
		t.Fatalf("OpenAL accounting counts wrong: %+v", dump.Snapshot)
	}
	if !dump.ListenerMatchesFocus || dump.ListenerMatchesEye || dump.Listener != dump.CameraFocus {
		t.Fatalf("OpenAL listener should bind camera focus, not eye: focus=%+v eye=%+v listener=%+v", dump.CameraFocus, dump.CameraEye, dump.Listener)
	}
	if !dump.AccountingMatchesNull || dump.NullAccountingHash != dump.BackendAccountingHash {
		t.Fatalf("OpenAL/null accounting diverged: null=%s backend=%s", dump.NullAccountingHash, dump.BackendAccountingHash)
	}
	if !dump.PanSignFlipped || len(dump.PanTrace) != 2 || dump.PanTrace[0].Pan <= 0 || dump.PanTrace[1].Pan >= 0 {
		t.Fatalf("pan trace did not flip sign: %+v", dump.PanTrace)
	}
	if !dump.SimHash.Equal || dump.SimHash.AudioCalls != 2 {
		t.Fatalf("audio-on/off hash pair wrong: %+v", dump.SimHash)
	}
	t.Logf("FSV #227 renderdemo audio-init OpenAL null-driver backend=%s sources=%d listener=%+v focus=%+v pan=%+.3f→%+.3f accountingHash=%s simHash=%s",
		dump.Backend, dump.BackendSources, dump.Listener, dump.CameraFocus, dump.PanTrace[0].Pan, dump.PanTrace[1].Pan, dump.BackendAccountingHash, dump.SimHash.AudioOn)
}
