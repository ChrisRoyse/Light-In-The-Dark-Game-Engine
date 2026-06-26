package obs

// Opt-in local crash-report capture (#185, D-2026-06-11-22 "no third-party
// SDKs"): when the process panics, a Reporter writes a self-contained
// crash-<timestamp>.txt to the user data dir and then lets the process die
// nonzero. Nothing leaves the machine — the user attaches the file to a report
// by hand. The crash path performs zero work (and zero allocation) until a
// panic actually unwinds into Reporter.Recover, so R-GC-1 on the hot path is
// untouched.

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"strings"
	"time"
)

// CrashDirName is the per-user subdirectory crash dumps are written under,
// inside os.UserConfigDir().
const CrashDirName = "litd"

// crashStackBudget bounds the all-goroutine stack capture. 1 MiB holds a very
// large fleet of goroutines; it is allocated only when a crash occurs.
const crashStackBudget = 1 << 20

// BuildStamp identifies the running binary in a crash report. It is sourced
// from Go's native VCS stamping (runtime/debug) so a report maps back to a
// commit without any release-process scaffolding; the release-versioning work
// (#184) can later enrich Version, but the hash is available today.
type BuildStamp struct {
	Hash     string // vcs.revision, or "unknown" for an unstamped build
	Modified bool   // vcs.modified — dirty working tree at build time
	Version  string // module version, or "(devel)"
	Go       string // runtime.Version()
}

// ReadBuildStamp extracts the build identity from the embedded build info.
// Missing fields fall back to "unknown" rather than erroring — a crash report
// with a partial stamp is still strictly better than none.
func ReadBuildStamp() BuildStamp {
	s := BuildStamp{Hash: "unknown", Version: "unknown", Go: runtime.Version()}
	bi, ok := debug.ReadBuildInfo()
	if !ok {
		return s
	}
	if bi.Main.Version != "" {
		s.Version = bi.Main.Version
	}
	for _, kv := range bi.Settings {
		switch kv.Key {
		case "vcs.revision":
			if kv.Value != "" {
				s.Hash = kv.Value
			}
		case "vcs.modified":
			s.Modified = kv.Value == "true"
		}
	}
	return s
}

// Reporter captures a crash to a local file. The zero value is not usable;
// construct one with NewReporter. The clock/exit/stderr seams are exported as
// unexported fields with test setters so the induced-crash path can be
// verified without terminating the test process.
type Reporter struct {
	// Dir is the directory crash dumps are written to. Empty means "resolve
	// the default user data dir lazily at crash time".
	Dir string
	// Enabled gates file capture. When false, a crash still prints its stack
	// to Stderr and exits nonzero — it just leaves no file (the opt-out).
	Enabled bool
	// Stamp identifies the binary in the report.
	Stamp BuildStamp
	// Tick, when non-nil, is read at crash time for the current sim tick.
	Tick func() uint32
	// Log, when non-nil, supplies the trailing ring of log lines.
	Log *Logger
	// ExitCode is the process exit status used after a crash (must be
	// nonzero — a crash is never a success).
	ExitCode int

	// seams (overridden in tests):
	now    func() time.Time
	exit   func(int)
	stderr io.Writer
}

// NewReporter builds a Reporter with production seams (real clock, os.Exit,
// os.Stderr) and a nonzero default exit code. dir may be empty to defer to the
// default user data dir.
func NewReporter(dir string, enabled bool) *Reporter {
	return &Reporter{
		Dir:      dir,
		Enabled:  enabled,
		Stamp:    ReadBuildStamp(),
		ExitCode: 2,
		now:      time.Now,
		exit:     os.Exit,
		stderr:   os.Stderr,
	}
}

// DefaultCrashDir is the standard per-user crash directory
// (os.UserConfigDir()/litd). It is created on demand by the writer.
func DefaultCrashDir() (string, error) {
	base, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, CrashDirName), nil
}

// Recover is the deferred crash hook: `defer reporter.Recover()` at the top of
// a client-shell goroutine. It is a no-op when no panic is in flight; on a
// panic it captures the report, writes it (if enabled), and exits nonzero so
// the crash is never masked.
func (r *Reporter) Recover() {
	rec := recover()
	if rec == nil {
		return
	}
	r.Handle(rec)
}

// Handle processes a recovered panic value: capture all goroutine stacks,
// write the dump (when enabled and writable), and exit nonzero. It is exported
// so tests can drive it directly with an injected exit seam.
func (r *Reporter) Handle(recovered any) {
	// Capture every goroutine's stack — the panicking goroutine plus any
	// others holding state at the moment of the crash.
	buf := make([]byte, crashStackBudget)
	n := runtime.Stack(buf, true)
	stacks := buf[:n]

	if r.Enabled {
		path, err := r.write(recovered, stacks)
		if err != nil {
			// Fail-closed to stderr: the data dir was unwritable, but the
			// crash must still surface and the process must still die. No
			// hang, no swallow.
			fmt.Fprintf(r.stderr, "litd: crash report could not be written (%v); panic: %v\n%s",
				err, recovered, stacks)
		} else {
			fmt.Fprintf(r.stderr, "litd: crash report written to %s; panic: %v\n", path, recovered)
		}
	} else {
		// Capture opted out: stack to stderr only, no file.
		fmt.Fprintf(r.stderr, "litd: panic: %v\n%s", recovered, stacks)
	}

	code := r.ExitCode
	if code == 0 {
		code = 2 // never report a crash as success
	}
	r.exit(code)
}

// write composes and writes the dump to a uniquely named file, returning its
// path. The filename is timestamped to the second; if that name already exists
// (two crashes within one second), a numeric suffix is appended so no report
// is ever overwritten.
func (r *Reporter) write(recovered any, stacks []byte) (string, error) {
	dir := r.Dir
	if dir == "" {
		d, err := DefaultCrashDir()
		if err != nil {
			return "", err
		}
		dir = d
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}

	ts := r.now().Format("20060102-150405")
	var path string
	var f *os.File
	for i := 0; ; i++ {
		name := "crash-" + ts + ".txt"
		if i > 0 {
			name = fmt.Sprintf("crash-%s-%d.txt", ts, i)
		}
		candidate := filepath.Join(dir, name)
		// O_EXCL: the open itself is the uniqueness check, so two crashes
		// racing on the same timestamp can never clobber one another.
		file, err := os.OpenFile(candidate, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
		if err == nil {
			f, path = file, candidate
			break
		}
		if !os.IsExist(err) {
			return "", err
		}
	}
	defer f.Close()

	if err := r.render(f, recovered, stacks); err != nil {
		return "", err
	}
	return path, nil
}

// render writes the human-readable dump body. Order: identity header, then the
// trailing log ring, then the goroutine stacks — most-summarized first so a
// reader sees build/tick/panic before scrolling into stacks.
func (r *Reporter) render(w io.Writer, recovered any, stacks []byte) error {
	var b strings.Builder
	b.WriteString("LitD crash report\n")
	fmt.Fprintf(&b, "time:    %s\n", r.now().Format(time.RFC3339Nano))
	fmt.Fprintf(&b, "build:   %s (modified=%t)\n", r.Stamp.Hash, r.Stamp.Modified)
	fmt.Fprintf(&b, "version: %s\n", r.Stamp.Version)
	fmt.Fprintf(&b, "go:      %s\n", r.Stamp.Go)
	fmt.Fprintf(&b, "os/arch: %s/%s\n", runtime.GOOS, runtime.GOARCH)
	tick := "n/a"
	if r.Tick != nil {
		tick = fmt.Sprintf("%d", r.Tick())
	}
	fmt.Fprintf(&b, "sim tick: %s\n", tick)
	fmt.Fprintf(&b, "panic:   %v\n", recovered)

	b.WriteString("\n--- last log lines ---\n")
	if _, err := io.WriteString(w, b.String()); err != nil {
		return err
	}
	if r.Log != nil {
		if err := r.Log.Dump(w); err != nil {
			return err
		}
	} else {
		if _, err := io.WriteString(w, "(no logger attached)\n"); err != nil {
			return err
		}
	}

	if _, err := io.WriteString(w, "\n--- goroutine stacks ---\n"); err != nil {
		return err
	}
	if _, err := w.Write(stacks); err != nil {
		return err
	}
	return nil
}
