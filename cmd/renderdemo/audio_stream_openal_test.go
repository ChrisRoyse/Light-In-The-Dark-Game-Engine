//go:build openal

package main

import (
	"math/rand"
	"os"
	"path/filepath"
	"strings"
	"testing"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/g3n/engine/audio/al"
)

type streamUnderrunCapture struct {
	events []streamUnderrunEvent
}

type streamUnderrunEvent struct {
	kind litaudio.StreamKind
	fill int
}

func (r *streamUnderrunCapture) ReportUnderrun(kind litaudio.StreamKind, observedFill int) {
	r.events = append(r.events, streamUnderrunEvent{kind: kind, fill: observedFill})
}

func TestOpenALStreamBackendQueuesMusicAndAmbienceFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	streams, ok := backend.(litaudio.StreamBackend)
	if !ok {
		t.Fatal("OpenAL backend does not expose stream backend")
	}

	dir := t.TempDir()
	music := writeAudioFixture(t, dir, "music/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))
	ambience := writeAudioFixture(t, dir, "music/ambience.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))

	before, ok := streams.StreamInfo(litaudio.StreamMusic)
	if !ok || before.Active || before.Queued != 0 || before.SourceID == 0 || len(before.BufferIDs) != litaudio.StreamRingChunks {
		t.Fatalf("BEFORE music stream state wrong: ok=%v info=%+v", ok, before)
	}

	mi, err := streams.StartStream(litaudio.StreamMusic, music, true, 0.75)
	if err != nil {
		t.Fatalf("StartStream music: %v", err)
	}
	ai, err := streams.StartStream(litaudio.StreamAmbience, ambience, true, 0.5)
	if err != nil {
		t.Fatalf("StartStream ambience: %v", err)
	}
	if mi.Slot != litaudio.MusicStreamSlot || ai.Slot != litaudio.AmbienceStreamSlot || mi.SourceID == ai.SourceID {
		t.Fatalf("stream slots/source IDs wrong: music=%+v ambience=%+v", mi, ai)
	}
	for _, info := range []litaudio.StreamDeviceInfo{mi, ai} {
		if !info.Active || !info.Playing || info.Queued <= 0 || info.Queued > int32(litaudio.StreamRingChunks) {
			t.Fatalf("AFTER start stream state wrong: %+v", info)
		}
		if info.ChunkBytes != litaudio.StreamChunkBytes || info.BufferBytes != litaudio.StreamChunkBytes*litaudio.StreamRingChunks {
			t.Fatalf("stream ring sizing wrong: %+v", info)
		}
		if info.LastReadBytes <= 0 || info.LastReadBytes > litaudio.StreamChunkBytes || info.TotalBytesQueued <= 0 {
			t.Fatalf("stream read accounting wrong: %+v", info)
		}
	}

	updated, err := streams.UpdateStream(litaudio.StreamMusic)
	if err != nil {
		t.Fatalf("UpdateStream music: %v", err)
	}
	if !updated.Active || updated.SourceID != mi.SourceID || updated.Queued <= 0 {
		t.Fatalf("AFTER update stream state wrong: before=%+v after=%+v", mi, updated)
	}

	stopped, err := streams.StopStream(litaudio.StreamMusic)
	if err != nil {
		t.Fatalf("StopStream music: %v", err)
	}
	if stopped.Active || stopped.Queued != 0 || stopped.Playing {
		t.Fatalf("AFTER stop stream state wrong: %+v", stopped)
	}
	t.Logf("FSV #509 stream happy: before=%+v musicStart=%+v ambienceStart=%+v musicUpdate=%+v musicStop=%+v",
		before, mi, ai, updated, stopped)
}

func TestOpenALStreamBackendRejectsEdgesFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	streams := backend.(litaudio.StreamBackend)

	dir := t.TempDir()
	stereoMusic := writeAudioFixture(t, dir, "music/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))
	monoMusic := writeAudioFixture(t, dir, "music/mono.ogg", mustDecodeB64(t, renderdemoMonoVorbis1sB64))
	stereoSFX := writeAudioFixture(t, dir, "sfx/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))

	cases := []struct {
		name string
		kind litaudio.StreamKind
		path string
		want string
	}{
		{name: "invalid-kind", kind: litaudio.StreamKind("voice"), path: stereoMusic, want: "unsupported stream kind"},
		{name: "missing-file", kind: litaudio.StreamMusic, path: filepath.Join(dir, "music", "missing.ogg"), want: "read stream"},
		{name: "mono-music", kind: litaudio.StreamMusic, path: monoMusic, want: "music must be stereo"},
		{name: "non-music-dir", kind: litaudio.StreamMusic, path: stereoSFX, want: "must live under a music directory"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			before, _ := streams.StreamInfo(litaudio.StreamMusic)
			got, err := streams.StartStream(c.kind, c.path, true, 1)
			after, _ := streams.StreamInfo(litaudio.StreamMusic)
			if err == nil || !strings.Contains(err.Error(), c.want) {
				t.Fatalf("edge %s got info=%+v err=%v, want err containing %q", c.name, got, err, c.want)
			}
			if after.Active || after.Queued != 0 {
				t.Fatalf("edge %s mutated stream state: before=%+v after=%+v", c.name, before, after)
			}
			t.Logf("FSV #509 stream edge %s: before=%+v err=%v after=%+v", c.name, before, err, after)
		})
	}
}

func TestOpenALStreamBackendRecoversUnderrunFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	streams := backend.(litaudio.StreamBackend)
	reporter := &streamUnderrunCapture{}
	streams.SetStreamUnderrunReporter(reporter)

	dir := t.TempDir()
	music := writeAudioFixture(t, dir, "music/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))
	started, err := streams.StartStream(litaudio.StreamMusic, music, true, 1)
	if err != nil {
		t.Fatalf("StartStream music: %v", err)
	}

	al.SourceStop(started.SourceID)
	queued := al.GetSourcei(started.SourceID, al.BuffersQueued)
	if queued > 0 {
		al.SourceUnqueueBuffers(started.SourceID, uint32(queued), nil)
		if err := al.GetError(); err != nil {
			t.Fatalf("force-drain OpenAL stream queue: %v", err)
		}
	}
	drained, _ := streams.StreamInfo(litaudio.StreamMusic)
	if drained.Queued != 0 {
		t.Fatalf("forced underrun setup failed: started=%+v drained=%+v", started, drained)
	}

	recovered, err := streams.UpdateStream(litaudio.StreamMusic)
	if err != nil {
		t.Fatalf("UpdateStream recovery: %v", err)
	}
	if !recovered.Active || recovered.Queued <= 0 || recovered.Underruns != 1 || len(reporter.events) != 1 {
		t.Fatalf("underrun recovery state wrong: drained=%+v recovered=%+v reporter=%+v", drained, recovered, reporter.events)
	}
	if reporter.events[0].kind != litaudio.StreamMusic || reporter.events[0].fill != 0 {
		t.Fatalf("underrun reporter wrong: %+v", reporter.events[0])
	}
	t.Logf("FSV #509 underrun: started=%+v drained=%+v recovered=%+v reporter=%+v",
		started, drained, recovered, reporter.events)
}

func TestOpenALStreamBackendRealMusicAssetsFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	defer chdirRepoRoot(t)()
	music := filepath.Join("assets", "audio", "music", "vigil_match.ogg")
	ambience := filepath.Join("assets", "audio", "music", "gloam_ambience.ogg")
	if _, err := os.Stat(music); err != nil {
		t.Skip("real ignored music assets are not present in this checkout")
	}
	if _, err := os.Stat(ambience); err != nil {
		t.Skip("real ignored ambience asset is not present in this checkout")
	}

	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	streams := backend.(litaudio.StreamBackend)
	beforeMusic, _ := streams.StreamInfo(litaudio.StreamMusic)
	beforeAmbience, _ := streams.StreamInfo(litaudio.StreamAmbience)

	musicStarted, err := streams.StartStream(litaudio.StreamMusic, music, true, 0.8)
	if err != nil {
		t.Fatalf("real music StartStream: %v", err)
	}
	ambienceStarted, err := streams.StartStream(litaudio.StreamAmbience, ambience, true, 0.6)
	if err != nil {
		t.Fatalf("real ambience StartStream: %v", err)
	}
	if musicStarted.Queued != int32(litaudio.StreamRingChunks) || ambienceStarted.Queued != int32(litaudio.StreamRingChunks) {
		t.Fatalf("real assets did not fill both stream rings: music=%+v ambience=%+v", musicStarted, ambienceStarted)
	}
	if musicStarted.TotalBytesQueued != int64(litaudio.StreamChunkBytes*litaudio.StreamRingChunks) ||
		ambienceStarted.TotalBytesQueued != int64(litaudio.StreamChunkBytes*litaudio.StreamRingChunks) {
		t.Fatalf("real assets queued unexpected byte counts: music=%+v ambience=%+v", musicStarted, ambienceStarted)
	}
	afterMusic, err := streams.UpdateStream(litaudio.StreamMusic)
	if err != nil {
		t.Fatalf("real music UpdateStream: %v", err)
	}
	afterAmbience, err := streams.UpdateStream(litaudio.StreamAmbience)
	if err != nil {
		t.Fatalf("real ambience UpdateStream: %v", err)
	}
	t.Logf("FSV #509 real assets: beforeMusic=%+v beforeAmbience=%+v musicStart=%+v ambienceStart=%+v musicAfter=%+v ambienceAfter=%+v",
		beforeMusic, beforeAmbience, musicStarted, ambienceStarted, afterMusic, afterAmbience)
}

func TestOpenALStreamBackendPlaysControllerSelectionFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	defer chdirRepoRoot(t)()
	if _, err := os.Stat(filepath.Join("assets", "audio", "music", "gloam_ambience.ogg")); err != nil {
		t.Skip("real ignored music assets are not present in this checkout")
	}
	table, err := litaudio.LoadMusicTable(os.DirFS("data"), "music/firstflame.toml")
	if err != nil {
		t.Fatalf("LoadMusicTable: %v", err)
	}
	sel, ok := table.Lookup("firstflame", "vigil")
	if !ok {
		t.Fatal("firstflame/vigil selection missing")
	}
	paths := append([]string{sel.Ambience}, sel.Tracks...)
	idx := litaudio.LoadStreamAssetIndex(os.DirFS("assets"), paths)
	if issues := idx.Issues(); len(issues) != 0 {
		t.Fatalf("stream asset index issues: %+v", issues)
	}
	controller := litaudio.NewStreamController(sel, idx, rand.New(rand.NewSource(509)))
	controller.Start()
	snap := controller.Dump()
	if snap.ActiveStreams != 2 {
		t.Fatalf("controller should start music+ambience streams: %+v", snap)
	}

	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	streams := backend.(litaudio.StreamBackend)
	streams.SetStreamUnderrunReporter(controller)

	started := make(map[litaudio.StreamKind]litaudio.StreamDeviceInfo)
	for _, st := range snap.Streams {
		if !st.Active {
			continue
		}
		info, err := streams.StartStream(st.Kind, filepath.Join("assets", filepath.FromSlash(st.Track)), st.Loop, 1)
		if err != nil {
			t.Fatalf("StartStream from controller state %+v: %v", st, err)
		}
		started[st.Kind] = info
	}
	if len(started) != 2 || started[litaudio.StreamMusic].Queued != int32(litaudio.StreamRingChunks) ||
		started[litaudio.StreamAmbience].Queued != int32(litaudio.StreamRingChunks) {
		t.Fatalf("backend did not bind controller streams: snap=%+v started=%+v", snap.Streams, started)
	}

	music := started[litaudio.StreamMusic]
	al.SourceStop(music.SourceID)
	queued := al.GetSourcei(music.SourceID, al.BuffersQueued)
	if queued > 0 {
		al.SourceUnqueueBuffers(music.SourceID, uint32(queued), nil)
		if err := al.GetError(); err != nil {
			t.Fatalf("force-drain controller music stream queue: %v", err)
		}
	}
	beforeRecovery := controller.Dump()
	recovered, err := streams.UpdateStream(litaudio.StreamMusic)
	if err != nil {
		t.Fatalf("UpdateStream controller music recovery: %v", err)
	}
	afterRecovery := controller.Dump()
	if recovered.Underruns != 1 || recovered.Queued != int32(litaudio.StreamRingChunks) {
		t.Fatalf("backend underrun recovery wrong: recovered=%+v", recovered)
	}
	if !streamLogSince(afterRecovery.Logs, len(beforeRecovery.Logs), "underrun-recover", litaudio.StreamMusic) {
		t.Fatalf("controller did not log backend underrun recovery: before=%+v after=%+v", beforeRecovery.Logs, afterRecovery.Logs)
	}
	t.Logf("FSV #509 controller->OpenAL: controllerBefore=%+v started=%+v recovered=%+v controllerAfter=%+v",
		snap.Streams, started, recovered, afterRecovery.Streams)
}

func TestOpenALResidentCuePoolDoesNotUseStreamSourcesFSV(t *testing.T) {
	t.Setenv("ALSOFT_DRIVERS", "null")
	backend, err := litaudio.OpenDevice()
	if err != nil {
		t.Fatalf("OpenAL null-driver device unavailable: %v", err)
	}
	defer backend.Close()
	buffers := backend.(litaudio.CueBufferBackend)
	streams := backend.(litaudio.StreamBackend)

	dir := t.TempDir()
	sfx := writeAudioFixture(t, dir, "sfx/hit.ogg", mustDecodeB64(t, renderdemoMonoVorbis1sB64))
	musicInfo, _ := streams.StreamInfo(litaudio.StreamMusic)
	ambienceInfo, _ := streams.StreamInfo(litaudio.StreamAmbience)
	streamSources := map[uint32]bool{musicInfo.SourceID: true, ambienceInfo.SourceID: true}

	for i := 0; i < litaudio.MusicStreamSlot+4; i++ {
		if i >= litaudio.MusicStreamSlot {
			backend.Stop(uint32(9000 + i - litaudio.MusicStreamSlot))
		}
		cue := uint32(9000 + i)
		if err := buffers.LoadCueBuffer(cue, sfx); err != nil {
			t.Fatalf("LoadCueBuffer %d: %v", cue, err)
		}
		backend.Play(litaudio.Voice{Cue: cue, Gain: 1, Pitch: 1, Slot: i % litaudio.MusicStreamSlot})
		info, ok := buffers.CueBufferInfo(cue)
		if !ok || info.SourceID == 0 || streamSources[info.SourceID] {
			t.Fatalf("resident cue used stream source: cue=%d slot=%d info=%+v streamSources=%v", cue, i%litaudio.MusicStreamSlot, info, streamSources)
		}
	}
	t.Logf("FSV #509 source reservation: resident cue pool avoided musicSource=%d ambienceSource=%d after %d resident plays",
		musicInfo.SourceID, ambienceInfo.SourceID, litaudio.MusicStreamSlot+4)
}

func writeAudioFixture(t *testing.T, dir, rel string, data []byte) string {
	t.Helper()
	p := filepath.Join(dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, data, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(p); err != nil {
		t.Fatalf("fixture write did not land at %s: %v", p, err)
	}
	return p
}

func streamLogSince(logs []litaudio.StreamLog, start int, event string, kind litaudio.StreamKind) bool {
	for i := start; i < len(logs); i++ {
		if logs[i].Event == event && logs[i].Kind == kind {
			return true
		}
	}
	return false
}
