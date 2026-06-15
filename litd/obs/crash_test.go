package obs

// #185 crash-report FSV. SoT = the dump file on disk (and, for the opt-out /
// unwritable cases, the stderr bytes + the absence of a file). The exit and
// clock seams are injected so the induced-crash path is verified without
// terminating the test process.

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// fixedClock returns a deterministic clock for reproducible timestamps.
func fixedClock(ts string) func() time.Time {
	t, err := time.Parse(time.RFC3339Nano, ts)
	if err != nil {
		panic(err)
	}
	return func() time.Time { return t }
}

// testReporter builds a Reporter with injected seams and a captured exit code.
func testReporter(t *testing.T, dir string, enabled bool) (*Reporter, *bytes.Buffer, *int) {
	t.Helper()
	var stderr bytes.Buffer
	exitCode := -1
	r := &Reporter{
		Dir:      dir,
		Enabled:  enabled,
		Stamp:    BuildStamp{Hash: "deadbeefcafe", Modified: true, Version: "v9.9.9-test", Go: "go-test"},
		ExitCode: 2,
		now:      fixedClock("2026-01-02T15:04:05.123456789Z"),
		exit:     func(c int) { exitCode = c },
		stderr:   &stderr,
	}
	return r, &stderr, &exitCode
}

func onlyCrashFile(t *testing.T, dir string) string {
	t.Helper()
	m, _ := filepath.Glob(filepath.Join(dir, "crash-*.txt"))
	if len(m) != 1 {
		t.Fatalf("want exactly 1 crash file in %s, got %d: %v", dir, len(m), m)
	}
	return m[0]
}

// TestCrashReportWritesAllFieldsFSV — happy path: the dump file physically
// contains build hash, version, os/arch, sim tick, panic, log lines, and a
// goroutine-stack section, and the process is told to exit nonzero.
func TestCrashReportWritesAllFieldsFSV(t *testing.T) {
	dir := t.TempDir()
	r, stderr, exitCode := testReporter(t, dir, true)
	r.Tick = func() uint32 { return 100 }
	log := New(1024)
	msg := log.Register("synthetic tick {0}")
	for i := int64(1); i <= 3; i++ {
		log.Log(uint32(i), 0, Info, ChSimTick, msg, i, 0, 0, 0)
	}
	r.Log = log

	// SoT BEFORE: empty dir.
	if n := len(filesIn(t, dir)); n != 0 {
		t.Fatalf("dir not empty before: %d files", n)
	}

	r.Handle("synthetic boom at tick 100")

	// SoT AFTER: read the file back and assert every field.
	path := onlyCrashFile(t, dir)
	body := readFile(t, path)
	t.Logf("FSV dump file %s:\n%s", filepath.Base(path), body)

	for _, want := range []string{
		"LitD crash report",
		"build:   deadbeefcafe (modified=true)",
		"version: v9.9.9-test",
		"os/arch: ",
		"sim tick: 100",
		"panic:   synthetic boom at tick 100",
		"--- last log lines ---",
		"synthetic tick 1",
		"synthetic tick 3",
		"--- goroutine stacks ---",
		"goroutine ",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("dump missing %q", want)
		}
	}
	if *exitCode != 2 {
		t.Fatalf("exit code = %d, want 2 (crash must not be masked)", *exitCode)
	}
	if stderr.Len() == 0 || !strings.Contains(stderr.String(), path) {
		t.Errorf("stderr should name the written report; got %q", stderr.String())
	}
}

// TestCrashCaptureDisabledNoFile — opt-out: no file is written, the stack still
// goes to stderr, and the process still exits nonzero.
func TestCrashCaptureDisabledNoFile(t *testing.T) {
	dir := t.TempDir()
	r, stderr, exitCode := testReporter(t, dir, false)

	t.Logf("FSV BEFORE: %d files", len(filesIn(t, dir)))
	r.Handle("boom while disabled")
	after := filesIn(t, dir)
	t.Logf("FSV AFTER: %d files (want 0); stderr first line: %q", len(after), firstLine(stderr.String()))

	if len(after) != 0 {
		t.Fatalf("capture disabled but %d files written: %v", len(after), after)
	}
	if !strings.Contains(stderr.String(), "boom while disabled") || !strings.Contains(stderr.String(), "goroutine ") {
		t.Fatalf("disabled crash must print panic+stack to stderr; got %q", stderr.String())
	}
	if *exitCode != 2 {
		t.Fatalf("exit code = %d, want 2", *exitCode)
	}
}

// TestCrashUnwritableDirFallsBackToStderr — when the data dir cannot be
// created, the crash still surfaces on stderr and exits nonzero, no hang, no
// file.
func TestCrashUnwritableDirFallsBackToStderr(t *testing.T) {
	base := t.TempDir()
	// A regular file standing where a directory parent must be: MkdirAll fails.
	blocker := filepath.Join(base, "blocker")
	if err := os.WriteFile(blocker, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	dir := filepath.Join(blocker, "sub")
	r, stderr, exitCode := testReporter(t, dir, true)

	r.Handle("boom into unwritable dir")

	// The dir must not exist as a usable directory (stat errors with either
	// ENOENT or ENOTDIR since a regular file blocks the parent path).
	if fi, err := os.Stat(dir); err == nil && fi.IsDir() {
		t.Fatalf("unwritable dir should not have been created: %s exists", dir)
	}
	s := stderr.String()
	t.Logf("FSV stderr: %q", firstLine(s))
	if !strings.Contains(s, "could not be written") || !strings.Contains(s, "boom into unwritable dir") {
		t.Fatalf("unwritable crash must report the write failure + panic on stderr; got %q", s)
	}
	if *exitCode != 2 {
		t.Fatalf("exit code = %d, want 2", *exitCode)
	}
}

// TestCrashTwoReportsDistinctFiles — two crashes within the same clock-second
// produce two distinct files; the O_EXCL uniqueness loop never overwrites.
func TestCrashTwoReportsDistinctFiles(t *testing.T) {
	dir := t.TempDir()
	r, _, _ := testReporter(t, dir, true) // fixed clock => identical timestamps

	r.Handle("first crash")
	r.Handle("second crash")

	files := filesIn(t, dir)
	t.Logf("FSV two-crash files: %v", files)
	if len(files) != 2 {
		t.Fatalf("want 2 distinct crash files, got %d: %v", len(files), files)
	}
	// Both must exist on disk with distinct names and neither overwritten.
	a := readFile(t, filepath.Join(dir, files[0]))
	b := readFile(t, filepath.Join(dir, files[1]))
	if !(strings.Contains(a, "first crash") || strings.Contains(b, "first crash")) ||
		!(strings.Contains(a, "second crash") || strings.Contains(b, "second crash")) {
		t.Fatalf("both crash bodies must survive; got A=%q B=%q", firstLine(a), firstLine(b))
	}
}

// TestReporterRecoverCapturesPanicSite — the real deferred-recover path: a
// panic unwinds into Recover, which writes a dump whose stack names the panic
// site, and signals a nonzero exit.
func TestReporterRecoverCapturesPanicSite(t *testing.T) {
	dir := t.TempDir()
	r, _, exitCode := testReporter(t, dir, true)

	func() {
		defer r.Recover()
		panic("recovered-path boom")
	}()

	path := onlyCrashFile(t, dir)
	body := readFile(t, path)
	if !strings.Contains(body, "recovered-path boom") {
		t.Fatalf("dump missing panic value: %q", firstLine(body))
	}
	// The stack must name this test function (the panic site).
	if !strings.Contains(body, "TestReporterRecoverCapturesPanicSite") {
		t.Errorf("goroutine stack should name the panic site; dump:\n%s", body)
	}
	if *exitCode != 2 {
		t.Fatalf("exit code = %d, want 2", *exitCode)
	}
}

// TestReadBuildStampNeverEmpty — the stamp degrades to non-empty placeholders
// rather than erroring, so a report always has identity fields.
func TestReadBuildStampNeverEmpty(t *testing.T) {
	s := ReadBuildStamp()
	t.Logf("FSV build stamp: hash=%q modified=%v version=%q go=%q", s.Hash, s.Modified, s.Version, s.Go)
	if s.Hash == "" || s.Version == "" || s.Go == "" {
		t.Fatalf("build stamp has empty field: %+v", s)
	}
}

// --- small file helpers (SoT readers) ---

func filesIn(t *testing.T, dir string) []string {
	t.Helper()
	es, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var out []string
	for _, e := range es {
		out = append(out, e.Name())
	}
	return out
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}
