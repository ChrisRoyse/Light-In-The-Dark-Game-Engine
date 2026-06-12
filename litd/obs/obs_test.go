package obs

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"unsafe"
)

func TestEntryIs64Bytes(t *testing.T) {
	if got := unsafe.Sizeof(Entry{}); got != entryBytes {
		t.Fatalf("Entry is %d bytes, spec requires %d", got, entryBytes)
	}
	t.Logf("Entry size = %d bytes (R-OBS-1)", unsafe.Sizeof(Entry{}))
}

func TestChannelNamesComplete(t *testing.T) {
	want := []string{"sim.tick", "sim.path", "sim.combat", "sim.sched",
		"render", "asset", "lua", "ai", "net", "audio", "ui"}
	if int(NumChannels) != len(want) {
		t.Fatalf("NumChannels=%d want %d", NumChannels, len(want))
	}
	for i, w := range want {
		if Channel(i).String() != w {
			t.Fatalf("channel %d = %q want %q", i, Channel(i), w)
		}
	}
}

func TestZeroAllocLogHotPath(t *testing.T) {
	l := New(RingCap)
	mid := l.Register("path expanded {0} nodes in {1} us")
	sink := filepath.Join(t.TempDir(), "errors.bin")
	if err := l.AttachSink(sink, Warn); err != nil {
		t.Fatal(err)
	}
	tick := uint32(0)
	if n := testing.AllocsPerRun(10000, func() {
		tick++
		l.Log(tick, tick*3, Info, ChSimPath, mid, int64(tick), 17, 0, 0)
		l.Log(tick, tick*3, Error, ChSimCombat, mid, 1, 2, 3, 4) // hits the disk sink
	}); n != 0 {
		t.Fatalf("Log allocates %v/op; R-OBS-1 requires 0 (incl. sink path)", n)
	}
	t.Log("AllocsPerRun = 0 for Log (INFO ring-only and ERROR ring+disk)")
	if err := l.CloseSink(); err != nil {
		t.Fatal(err)
	}
}

// Edge: ring wraparound — entry 65,537 evicts the oldest.
func TestRingWraparound(t *testing.T) {
	l := New(RingCap)
	mid := l.Register("seq {0}")
	for i := 0; i < RingCap; i++ {
		l.Log(uint32(i), 0, Info, ChSimTick, mid, int64(i), 0, 0, 0)
	}
	snap := l.Snapshot(nil)
	t.Logf("before wrap: len=%d head=%q tail=%q", len(snap),
		l.FormatEntry(&snap[0]), l.FormatEntry(&snap[len(snap)-1]))
	if snap[0].Args[0] != 0 || snap[len(snap)-1].Args[0] != RingCap-1 {
		t.Fatalf("pre-wrap ring wrong: head seq %d tail seq %d", snap[0].Args[0], snap[len(snap)-1].Args[0])
	}

	l.Log(RingCap, 0, Info, ChSimTick, mid, RingCap, 0, 0, 0) // entry 65,537
	snap = l.Snapshot(snap)
	t.Logf("after wrap:  len=%d head=%q tail=%q", len(snap),
		l.FormatEntry(&snap[0]), l.FormatEntry(&snap[len(snap)-1]))
	if len(snap) != RingCap {
		t.Fatalf("ring grew past capacity: %d", len(snap))
	}
	if snap[0].Args[0] != 1 {
		t.Fatalf("oldest entry not evicted: head seq %d, want 1", snap[0].Args[0])
	}
	if snap[len(snap)-1].Args[0] != RingCap {
		t.Fatalf("newest entry missing: tail seq %d, want %d", snap[len(snap)-1].Args[0], RingCap)
	}
	if l.Total() != RingCap+1 {
		t.Fatalf("Total=%d want %d", l.Total(), RingCap+1)
	}
}

// Edge: concurrent sim + render writers — run under -race.
func TestConcurrentSimRenderWriters(t *testing.T) {
	l := New(1024)
	mid := l.Register("worker {0} item {1}")
	var wg sync.WaitGroup
	for w := 0; w < 2; w++ { // goroutine 0 = sim, 1 = render
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			ch := ChSimTick
			if id == 1 {
				ch = ChRender
			}
			for i := 0; i < 5000; i++ {
				l.Log(uint32(i), uint32(i), Debug, ch, mid, int64(id), int64(i), 0, 0)
			}
		}(w)
	}
	wg.Wait()
	if l.Total() != 10000 {
		t.Fatalf("lost entries: total %d want 10000", l.Total())
	}
	t.Logf("2 writers × 5000 entries: total=%d, ring holds %d (race detector must be clean)", l.Total(), l.Len())
}

// Edge: ERROR with all 4 args used vs 0 args used.
func TestFormattingArgCounts(t *testing.T) {
	l := New(64)
	m4 := l.Register("desync unit={0} tick={1} got={2} want={3}")
	m0 := l.Register("asset cache flushed")
	l.Log(42, 7, Error, ChSimCombat, m4, 1001, 8400, -3, 12)
	l.Log(43, 8, Error, ChAsset, m0, 0, 0, 0, 0)
	snap := l.Snapshot(nil)
	got4 := l.FormatEntry(&snap[0])
	got0 := l.FormatEntry(&snap[1])
	t.Logf("4-arg: %s", got4)
	t.Logf("0-arg: %s", got0)
	if !strings.Contains(got4, "desync unit=1001 tick=8400 got=-3 want=12") {
		t.Fatalf("4-arg formatting wrong: %s", got4)
	}
	if !strings.Contains(got0, "asset cache flushed") || strings.Contains(got0, "{0}") {
		t.Fatalf("0-arg formatting wrong: %s", got0)
	}
}

// 100k mixed-level entries -> dump -> read the file back; plus the
// release disk sink containing exactly the ERROR/WARN subset.
func TestHundredKDumpAndSink(t *testing.T) {
	dir := t.TempDir()
	l := New(RingCap)
	mTick := l.Register("tick {0} took {1} us")
	mErr := l.Register("pathfind failed for unit {0} at ({1},{2}) code {3}")
	sinkPath := filepath.Join(dir, "errwarn.bin")
	if err := l.AttachSink(sinkPath, Warn); err != nil {
		t.Fatal(err)
	}
	errCount := 0
	for i := 0; i < 100_000; i++ {
		lvl := Level(i % 5)
		if lvl <= Warn {
			errCount++
			l.Log(uint32(i), uint32(i), lvl, ChSimPath, mErr, int64(i), int64(i%512), int64(i%512+1), 7)
		} else {
			l.Log(uint32(i), uint32(i), lvl, ChSimTick, mTick, int64(i), int64(i%900), 0, 0)
		}
	}
	if err := l.CloseSink(); err != nil {
		t.Fatal(err)
	}

	dumpPath := filepath.Join(dir, "ring.dump")
	if err := l.DumpFile(dumpPath); err != nil {
		t.Fatal(err)
	}
	dump, err := os.ReadFile(dumpPath)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimRight(string(dump), "\n"), "\n")
	t.Logf("ring dump: %d lines; first 3:", len(lines))
	for _, ln := range lines[:3] {
		t.Logf("  %s", ln)
	}
	t.Logf("  ... last: %s", lines[len(lines)-1])
	if len(lines) != RingCap+1 { // header + capacity entries
		t.Fatalf("dump has %d lines, want %d", len(lines), RingCap+1)
	}
	if !strings.Contains(lines[len(lines)-1], "tick 99999 took") {
		t.Fatalf("last dump line wrong: %s", lines[len(lines)-1])
	}

	raw, err := os.ReadFile(sinkPath)
	if err != nil {
		t.Fatal(err)
	}
	if len(raw) != errCount*entryBytes {
		t.Fatalf("sink %d bytes, want %d (%d ERROR/WARN × %d)", len(raw), errCount*entryBytes, errCount, entryBytes)
	}
	var text bytes.Buffer
	if err := l.DecodeSink(bytes.NewReader(raw), &text); err != nil {
		t.Fatal(err)
	}
	sinkLines := strings.Split(strings.TrimRight(text.String(), "\n"), "\n")
	t.Logf("disk sink: %d bytes = %d records; first: %s", len(raw), len(sinkLines), sinkLines[0])
	if len(sinkLines) != errCount {
		t.Fatalf("decoded %d sink records, want %d", len(sinkLines), errCount)
	}
	for _, ln := range sinkLines[:50] {
		if !strings.Contains(ln, "ERROR") && !strings.Contains(ln, "WARN") {
			t.Fatalf("non-ERROR/WARN entry leaked into release sink: %s", ln)
		}
	}
}

// Per-channel level config: a channel set to Warn drops Info.
func TestChannelLevelFilter(t *testing.T) {
	l := New(64)
	mid := l.Register("noise {0}")
	l.SetChannelLevel(ChAudio, Warn)
	l.Log(1, 1, Info, ChAudio, mid, 1, 0, 0, 0) // dropped
	l.Log(1, 1, Warn, ChAudio, mid, 2, 0, 0, 0) // kept
	l.Log(1, 1, Info, ChUI, mid, 3, 0, 0, 0)    // other channel unaffected
	if l.Total() != 2 {
		t.Fatalf("filter wrong: total %d want 2", l.Total())
	}
	t.Log("ChAudio@Warn drops INFO, keeps WARN; ChUI unaffected")
}

func BenchmarkLog(b *testing.B) {
	l := New(RingCap)
	mid := l.Register("tick {0} took {1} us")
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		l.Log(uint32(i), uint32(i), Info, ChSimTick, mid, int64(i), 42, 0, 0)
	}
}
