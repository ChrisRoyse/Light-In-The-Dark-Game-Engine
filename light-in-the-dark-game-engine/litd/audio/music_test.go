package audio

import (
	"math/rand"
	"os"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func musicOgg(seconds int) []byte {
	return vorbisOgg(2, 44100, int64(44100*seconds))
}

func musicFS() fstest.MapFS {
	return fstest.MapFS{
		"audio/music/a.ogg":        {Data: musicOgg(5)},
		"audio/music/b.ogg":        {Data: musicOgg(5)},
		"audio/music/c.ogg":        {Data: musicOgg(5)},
		"audio/music/ambience.ogg": {Data: musicOgg(3)},
	}
}

func eventLog(s StreamSnapshot, event string) (StreamLog, bool) {
	for _, l := range s.Logs {
		if l.Event == event {
			return l, true
		}
	}
	return StreamLog{}, false
}

func TestLoadMusicTableRealDataFSV(t *testing.T) {
	table, err := LoadMusicTable(os.DirFS("../../data"), "music/firstflame.toml")
	if err != nil {
		t.Fatalf("real music table rejected: %v", err)
	}
	sel, ok := table.Lookup("firstflame", "vigil")
	if !ok {
		t.Fatal("firstflame/vigil playlist missing")
	}
	if sel.Ambience != "audio/music/gloam_ambience.ogg" || !sel.AmbienceLoop || sel.CrossfadeMs != 1500 || !sel.Shuffle {
		t.Fatalf("firstflame/vigil metadata mismatch: %+v", sel)
	}
	want := []string{"audio/music/vigil_match.ogg", "audio/music/menu_theme.ogg"}
	if strings.Join(sel.Tracks, ",") != strings.Join(want, ",") {
		t.Fatalf("firstflame/vigil tracks got %v want %v", sel.Tracks, want)
	}
	t.Logf("FSV #314 data SoT: maps=%v firstflame/vigil ambience=%s crossfade=%d tracks=%v",
		table.Maps(), sel.Ambience, sel.CrossfadeMs, sel.Tracks)
}

func TestStreamAssetIndexRealMusicAssetsFSV(t *testing.T) {
	if _, err := os.Stat("../../assets/audio/music/gloam_ambience.ogg"); err != nil {
		t.Skip("real ignored music assets are not present in this checkout")
	}
	table, err := LoadMusicTable(os.DirFS("../../data"), "music/firstflame.toml")
	if err != nil {
		t.Fatalf("real music table rejected: %v", err)
	}
	vigil, ok := table.Lookup("firstflame", "vigil")
	if !ok {
		t.Fatal("firstflame/vigil missing")
	}
	unbound, ok := table.Lookup("firstflame", "unbound")
	if !ok {
		t.Fatal("firstflame/unbound missing")
	}
	paths := []string{vigil.Ambience}
	paths = append(paths, vigil.Tracks...)
	paths = append(paths, unbound.Tracks...)
	idx := LoadStreamAssetIndex(os.DirFS("../../assets"), paths)
	if issues := idx.Issues(); len(issues) != 0 {
		t.Fatalf("real music stream assets rejected: %+v", issues)
	}
	got := idx.Assets()
	if len(got) != 4 {
		t.Fatalf("real music asset index got %d assets %v, want 4 unique tracks", len(got), got)
	}
	for _, p := range got {
		a, _ := idx.Lookup(p)
		if a.StreamChunkBytes != StreamChunkBytes || a.StreamRingChunks != StreamRingChunks || a.StreamBufferBytes != StreamChunkBytes*StreamRingChunks {
			t.Fatalf("real music asset %s has wrong stream ring: %+v", p, a)
		}
	}
	t.Logf("FSV #314 real stream index: assets=%v ringBytes=%d issues=%v", got, StreamChunkBytes*StreamRingChunks, idx.Issues())
}

func TestStreamControllerRealAssetsTenMinuteFSV(t *testing.T) {
	if _, err := os.Stat("../../assets/audio/music/gloam_ambience.ogg"); err != nil {
		t.Skip("real ignored music assets are not present in this checkout")
	}
	table, err := LoadMusicTable(os.DirFS("../../data"), "music/firstflame.toml")
	if err != nil {
		t.Fatalf("real music table rejected: %v", err)
	}
	sel, ok := table.Lookup("firstflame", "vigil")
	if !ok {
		t.Fatal("firstflame/vigil missing")
	}
	paths := append([]string{sel.Ambience}, sel.Tracks...)
	idx := LoadStreamAssetIndex(os.DirFS("../../assets"), paths)
	if issues := idx.Issues(); len(issues) != 0 {
		t.Fatalf("real music stream assets rejected: %+v", issues)
	}
	c := NewStreamController(sel, idx, rand.New(rand.NewSource(314)))
	c.Start()
	for i := 0; i < 600; i++ {
		c.Advance(1000)
	}
	snap := c.Dump()
	loops, fades, skips, underruns := 0, 0, 0, 0
	for _, l := range snap.Logs {
		switch l.Event {
		case "loop":
			loops++
		case "crossfade-start":
			fades++
			if l.FadeMs != int64(sel.CrossfadeMs) {
				t.Fatalf("fade duration log=%d want table crossfade=%d in %+v", l.FadeMs, sel.CrossfadeMs, l)
			}
		case "skip":
			skips++
		case "underrun-recover":
			underruns++
		}
	}
	if snap.NowMs != 600_000 || snap.ActiveStreams != 2 || loops == 0 || fades == 0 || skips != 0 || underruns != 0 {
		t.Fatalf("10min stream state wrong: now=%d active=%d loops=%d fades=%d skips=%d underruns=%d snap=%+v",
			snap.NowMs, snap.ActiveStreams, loops, fades, skips, underruns, snap)
	}
	t.Logf("FSV #314 10min real run: now=%d active=%d loops=%d fades=%d skips=%d underruns=%d final=%+v",
		snap.NowMs, snap.ActiveStreams, loops, fades, skips, underruns, snap.Streams)
}

func TestMusicTableRejectsEdgesFSV(t *testing.T) {
	load := func(body string) error {
		_, err := LoadMusicTable(fstest.MapFS{"music/test.toml": {Data: []byte(body)}}, "music/test.toml")
		return err
	}
	validHead := `
[[map]]
id = "x"
ambience = "audio/music/ambience.ogg"
ambience-loop = true
crossfade-ms = 1000
shuffle = false

  [[map.playlist]]
  faction = "vigil"
  tracks = ["audio/music/a.ogg"]
`
	cases := []struct {
		name string
		body string
		want string
	}{
		{
			name: "unknown-field",
			body: validHead + "\nextra = 1\n",
			want: "unknown field",
		},
		{
			name: "missing-loop",
			body: strings.Replace(validHead, "ambience-loop = true\n", "", 1),
			want: "must declare ambience-loop",
		},
		{
			name: "unsafe-path",
			body: strings.Replace(validHead, "audio/music/a.ogg", "../a.ogg", 1),
			want: "relative asset path",
		},
		{
			name: "crossfade-max-plus-one",
			body: strings.Replace(validHead, "crossfade-ms = 1000", "crossfade-ms = 30001", 1),
			want: "outside [0,30000]",
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			err := load(c.body)
			if err == nil || !strings.Contains(err.Error(), c.want) {
				t.Fatalf("edge %s got err=%v want containing %q", c.name, err, c.want)
			}
			t.Logf("FSV #314 table edge %s: %v", c.name, err)
		})
	}
}

func TestStreamControllerCrossfadeAndAmbienceLoopFSV(t *testing.T) {
	sel := MusicSelection{
		MapID:        "synthetic",
		Faction:      "vigil",
		Ambience:     "audio/music/ambience.ogg",
		AmbienceLoop: true,
		CrossfadeMs:  1000,
		Tracks:       []string{"audio/music/a.ogg", "audio/music/b.ogg"},
	}
	paths := append([]string{sel.Ambience}, sel.Tracks...)
	idx := LoadStreamAssetIndex(musicFS(), paths)
	if issues := idx.Issues(); len(issues) != 0 {
		t.Fatalf("fixture stream metadata rejected: %+v", issues)
	}
	c := NewStreamController(sel, idx, rand.New(rand.NewSource(1)))
	c.Start()
	before := c.Dump()
	if before.ActiveStreams != 2 || len(before.Streams) != 2 {
		t.Fatalf("start must activate music+ambience streams, got %+v", before)
	}

	c.Advance(3000) // ambience duration is exactly 3000 ms
	afterLoop := c.Dump()
	loop, ok := eventLog(afterLoop, "loop")
	if !ok || loop.Kind != StreamAmbience || loop.BufferFillBytes != StreamChunkBytes*StreamRingChunks || loop.PositionMs != 0 {
		t.Fatalf("ambience loop log missing/incorrect: %+v snapshot=%+v", loop, afterLoop)
	}

	c.Advance(1000) // music reaches 5000-1000 => crossfade starts at 4000
	afterFadeStart := c.Dump()
	start, ok := eventLog(afterFadeStart, "crossfade-start")
	if !ok || start.FadeMs != 1000 || start.FadeStartMs != 4000 || start.FadeEndMs != 5000 || start.NextTrack != "audio/music/b.ogg" {
		t.Fatalf("crossfade-start log wrong: %+v snapshot=%+v", start, afterFadeStart)
	}
	if !afterFadeStart.Streams[0].Crossfade || afterFadeStart.Streams[0].NextTrack != "audio/music/b.ogg" {
		t.Fatalf("music stream should be crossfading in one slot: %+v", afterFadeStart.Streams[0])
	}

	c.Advance(1000)
	afterFadeEnd := c.Dump()
	end, ok := eventLog(afterFadeEnd, "crossfade-end")
	if !ok || end.NextTrack != "audio/music/b.ogg" {
		t.Fatalf("crossfade-end log wrong: %+v snapshot=%+v", end, afterFadeEnd)
	}
	if afterFadeEnd.Streams[0].Track != "audio/music/b.ogg" || afterFadeEnd.Streams[0].PositionMs != 1000 || afterFadeEnd.Streams[0].Slot != MusicStreamSlot {
		t.Fatalf("music stream did not switch to b at +1000ms in the same slot: %+v", afterFadeEnd.Streams[0])
	}
	t.Logf("FSV #314 crossfade/loop: before=%+v loop=%+v fadeStart=%+v fadeEnd=%+v finalMusic=%+v",
		before.Streams, loop, start, end, afterFadeEnd.Streams[0])
}

func TestStreamControllerSkipsCorruptTrackAndRecoversUnderrunFSV(t *testing.T) {
	fsys := musicFS()
	fsys["audio/music/bad.ogg"] = &fstest.MapFile{Data: []byte("not an ogg")}
	sel := MusicSelection{
		MapID:        "synthetic",
		Faction:      "unbound",
		Ambience:     "audio/music/ambience.ogg",
		AmbienceLoop: true,
		CrossfadeMs:  1000,
		Tracks:       []string{"audio/music/bad.ogg", "audio/music/a.ogg"},
	}
	idx := LoadStreamAssetIndex(fsys, []string{sel.Ambience, "audio/music/bad.ogg", "audio/music/a.ogg"})
	if issues := idx.Issues(); len(issues) != 1 || issues[0].Rule != "AUD-PARSE" {
		t.Fatalf("bad Ogg should yield one AUD-PARSE issue, got %+v", issues)
	}
	c := NewStreamController(sel, idx, rand.New(rand.NewSource(1)))
	c.Start()
	started := c.Dump()
	skip, ok := eventLog(started, "skip")
	if !ok || skip.Track != "audio/music/bad.ogg" || skip.Rule != "AUD-PARSE" {
		t.Fatalf("corrupt track skip log wrong: %+v snapshot=%+v", skip, started)
	}
	if started.Streams[0].Track != "audio/music/a.ogg" {
		t.Fatalf("controller should skip corrupt track and play next valid track: %+v", started.Streams[0])
	}

	c.ReportUnderrun(StreamMusic, 0)
	recovered := c.Dump()
	underrun, ok := eventLog(recovered, "underrun-recover")
	if !ok || underrun.BufferBeforeBytes != 0 || underrun.BufferFillBytes != StreamChunkBytes*StreamRingChunks {
		t.Fatalf("underrun recovery log wrong: %+v snapshot=%+v", underrun, recovered)
	}
	t.Logf("FSV #314 corrupt/underrun: skip=%+v recovery=%+v musicState=%+v", skip, underrun, recovered.Streams[0])
}

func TestStreamShuffleUsesNonSimRNGFSV(t *testing.T) {
	wOff := sim.NewWorld(sim.Caps{})
	wOn := sim.NewWorld(sim.Caps{})
	beforeOff := wOff.RNGCursor()
	beforeOn := wOn.RNGCursor()

	sel := MusicSelection{
		MapID:        "synthetic",
		Faction:      "vigil",
		Ambience:     "audio/music/ambience.ogg",
		AmbienceLoop: true,
		CrossfadeMs:  1000,
		Shuffle:      true,
		Tracks:       []string{"audio/music/a.ogg", "audio/music/b.ogg", "audio/music/c.ogg"},
	}
	idx := LoadStreamAssetIndex(musicFS(), append([]string{sel.Ambience}, sel.Tracks...))
	c := NewStreamController(sel, idx, rand.New(rand.NewSource(99)))
	c.Start()
	afterOff := wOff.RNGCursor()
	afterOn := wOn.RNGCursor()
	if beforeOff != afterOff || beforeOn != afterOn || afterOff != afterOn {
		t.Fatalf("stream shuffle touched sim PRNG: off %v→%v on %v→%v", beforeOff, afterOff, beforeOn, afterOn)
	}
	snap := c.Dump()
	if snap.RNGDraws != len(sel.Tracks)-1 {
		t.Fatalf("Fisher-Yates draw count = %d, want %d", snap.RNGDraws, len(sel.Tracks)-1)
	}
	t.Logf("FSV #314 PRNG isolation: sim cursor off=%+v→%+v on=%+v→%+v; presentation RNG draws=%d order=%s",
		beforeOff, afterOff, beforeOn, afterOn, snap.RNGDraws, snap.Logs[0].Detail)
}
