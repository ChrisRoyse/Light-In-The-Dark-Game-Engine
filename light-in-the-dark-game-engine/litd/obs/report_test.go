package obs

// #250 debug-report bundle FSV. SoT = the generated zip on disk: open it, list
// members, and read each member's bytes back. The clock seam makes filenames
// and bundle bytes deterministic.

import (
	"archive/zip"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func reportClock(ts string) func() time.Time {
	t, err := time.Parse(time.RFC3339Nano, ts)
	if err != nil {
		panic(err)
	}
	return func() time.Time { return t }
}

// readZip returns member name -> bytes.
func readZip(t *testing.T, path string) map[string][]byte {
	t.Helper()
	zr, err := zip.OpenReader(path)
	if err != nil {
		t.Fatalf("open zip: %v", err)
	}
	defer zr.Close()
	out := map[string][]byte{}
	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			t.Fatalf("open member %s: %v", f.Name, err)
		}
		b, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			t.Fatalf("read member %s: %v", f.Name, err)
		}
		out[f.Name] = b
	}
	return out
}

func zips(t *testing.T, dir string) []string {
	t.Helper()
	m, _ := filepath.Glob(filepath.Join(dir, "litd-report-*.zip"))
	return m
}

// TestReportBundleContainsEverySectionFSV — happy path: every supplied section
// lands in the zip with its exact bytes, the replay is stored verbatim, and the
// README carries the build hash.
func TestReportBundleContainsEverySectionFSV(t *testing.T) {
	dir := t.TempDir()
	log := New(64)
	msg := log.Register("evt {0}")
	log.Log(1, 0, Info, ChSimTick, msg, 7, 0, 0, 0)
	counters := NewDefaultCounters()
	counters.Sample(1, 1)

	replay := []byte("REPLAY-BYTES-\x00\x01\x02-verbatim")
	in := ReportInputs{
		Stamp:         BuildStamp{Hash: "abcdef0123456789", Modified: false, Version: "v1.2.3", Go: "go-test"},
		WorldHash:     "worldhash123",
		Log:           log,
		Counters:      counters,
		StateDumpJSON: []byte(`{"tick":42,"units":3}`),
		StateHash:     "hash: cafef00d\nsub: tick 1234\n",
		Replay:        replay,
	}

	path, warnings, err := WriteReport(dir, in, reportClock("2026-03-04T05:06:07Z"))
	if err != nil {
		t.Fatalf("WriteReport: %v", err)
	}
	t.Logf("FSV bundle: %s warnings=%v", filepath.Base(path), warnings)
	if len(warnings) != 0 {
		t.Fatalf("full inputs should produce no warnings, got %v", warnings)
	}
	if want := "litd-report-abcdef01-20260304-050607.zip"; filepath.Base(path) != want {
		t.Fatalf("bundle name = %s, want %s", filepath.Base(path), want)
	}

	members := readZip(t, path)
	// SoT: replay stored byte-for-byte.
	if got := members["replay.litdreplay"]; string(got) != string(replay) {
		t.Fatalf("replay not verbatim: got %q want %q", got, replay)
	}
	// SoT: state dump verbatim.
	if string(members["state.json"]) != `{"tick":42,"units":3}` {
		t.Fatalf("state.json wrong: %q", members["state.json"])
	}
	// statehash present.
	if !strings.Contains(string(members["statehash.txt"]), "cafef00d") {
		t.Fatalf("statehash.txt missing: %q", members["statehash.txt"])
	}
	// log dump present.
	if !strings.Contains(string(members["log.txt"]), "evt 7") {
		t.Fatalf("log.txt missing entry: %q", members["log.txt"])
	}
	// counters present.
	if !strings.Contains(string(members["counters.txt"]), "counter history") {
		t.Fatalf("counters.txt missing: %q", members["counters.txt"])
	}
	// README carries build identity + world hash.
	readme := string(members["README.txt"])
	for _, want := range []string{"abcdef0123456789", "v1.2.3", "worldhash123"} {
		if !strings.Contains(readme, want) {
			t.Fatalf("README missing %q:\n%s", want, readme)
		}
	}
	t.Logf("FSV members: %v", keys(members))
}

// TestReportEmptyReplayIsLoudNotCrash — F10 at tick 0: an absent replay is
// recorded as a loud warning + a MISSING note member; the bundle still writes.
func TestReportEmptyReplayIsLoudNotCrash(t *testing.T) {
	dir := t.TempDir()
	in := ReportInputs{
		Stamp:         BuildStamp{Hash: "deadbeef", Version: "v0", Go: "go-test"},
		StateDumpJSON: []byte("{}"),
		StateHash:     "hash: 0\n",
		// Replay deliberately nil.
	}
	path, warnings, err := WriteReport(dir, in, reportClock("2026-03-04T05:06:07Z"))
	if err != nil {
		t.Fatalf("empty-replay bundle must not error: %v", err)
	}
	t.Logf("FSV empty-replay warnings: %v", warnings)
	foundReplayWarn := false
	for _, w := range warnings {
		if strings.HasPrefix(w, "replay:") {
			foundReplayWarn = true
		}
	}
	if !foundReplayWarn {
		t.Fatalf("missing loud replay warning: %v", warnings)
	}
	members := readZip(t, path)
	if _, ok := members["replay.litdreplay"]; ok {
		t.Fatalf("empty replay must not write a replay member")
	}
	if _, ok := members["replay-MISSING.txt"]; !ok {
		t.Fatalf("expected replay-MISSING.txt note member; got %v", keys(members))
	}
	if !strings.Contains(string(members["warnings.txt"]), "replay") {
		t.Fatalf("warnings.txt should record the empty replay: %q", members["warnings.txt"])
	}
}

// TestReportUnwritableDirRemovesPartial — a write failure removes the partial
// artifact and returns a loud error (no truncated bundle left behind).
func TestReportUnwritableDirRemovesPartial(t *testing.T) {
	base := t.TempDir()
	blocker := filepath.Join(base, "blocker")
	if err := os.WriteFile(blocker, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(blocker, "sub") // parent is a file => MkdirAll fails
	in := ReportInputs{Stamp: BuildStamp{Hash: "h", Version: "v", Go: "g"}, Replay: []byte("r")}

	path, _, err := WriteReport(dir, in, reportClock("2026-03-04T05:06:07Z"))
	if err == nil {
		t.Fatalf("expected error writing to unwritable dir, got path %q", path)
	}
	t.Logf("FSV unwritable error: %v", err)
	// No partial artifact anywhere under base.
	if got := zips(t, base); len(got) != 0 {
		t.Fatalf("partial bundle left behind: %v", got)
	}
}

// TestReportTwoBundlesDistinct — two reports in the same clock-second produce
// two distinct files (O_EXCL uniqueness loop), neither overwritten.
func TestReportTwoBundlesDistinct(t *testing.T) {
	dir := t.TempDir()
	clk := reportClock("2026-03-04T05:06:07Z")
	in := ReportInputs{Stamp: BuildStamp{Hash: "abcdef01", Version: "v", Go: "g"}, Replay: []byte("r")}

	p1, _, err := WriteReport(dir, in, clk)
	if err != nil {
		t.Fatal(err)
	}
	p2, _, err := WriteReport(dir, in, clk)
	if err != nil {
		t.Fatal(err)
	}
	if p1 == p2 {
		t.Fatalf("two bundles must have distinct paths, both %s", p1)
	}
	got := zips(t, dir)
	t.Logf("FSV two-bundle files: %v", got)
	if len(got) != 2 {
		t.Fatalf("want 2 distinct bundles, got %d: %v", len(got), got)
	}
}

// TestProbeSysInfoNonEmpty — the headless probe fills the runtime-available
// fields rather than leaving them blank.
func TestProbeSysInfoNonEmpty(t *testing.T) {
	s := ProbeSysInfo()
	t.Logf("FSV sysinfo: os=%s arch=%s cpus=%d memSys=%d locale=%q", s.OS, s.Arch, s.NumCPU, s.GoMemSys, s.Locale)
	if s.OS == "" || s.Arch == "" || s.NumCPU < 1 {
		t.Fatalf("sysinfo probe incomplete: %+v", s)
	}
}

func keys(m map[string][]byte) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}
