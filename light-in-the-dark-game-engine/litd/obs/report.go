package obs

// Debug-report bundle (#250, R-OBS-3, observability-and-debugging.md §3): one
// keypress (F10) or -report-on-exit writes a single self-contained zip a field
// reporter can attach to a bug. Because the engine is deterministic (G5), the
// bundled replay reproduces the bug bit-identically — re-running it headless
// and diffing per-system sub-hashes pinpoints the failing system.
//
// The writer takes its sim-specific sections as plain byte/section values plus
// the in-package Logger/Counters, so litd/obs takes NO dependency on litd/sim
// or the replay package — the client shell wires the producers in.

import (
	"archive/zip"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// SysInfo is the startup-probe section. GPU/Driver are render-supplied (empty
// in a headless probe); the rest come from the Go runtime + environment.
type SysInfo struct {
	OS, Arch string
	NumCPU   int
	GoMemSys uint64 // runtime.MemStats.Sys — process memory reserved from the OS
	Locale   string
	GPU      string // render-supplied; "" when unknown
	Driver   string // render-supplied; "" when unknown
}

// ProbeSysInfo gathers the headless-available system facts. The render shell
// fills GPU/Driver before calling WriteReport.
func ProbeSysInfo() SysInfo {
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	locale := os.Getenv("LANG")
	if locale == "" {
		locale = os.Getenv("LC_ALL")
	}
	return SysInfo{
		OS:       runtime.GOOS,
		Arch:     runtime.GOARCH,
		NumCPU:   runtime.NumCPU(),
		GoMemSys: ms.Sys,
		Locale:   locale,
	}
}

// ReportInputs are the bundle sections. Every section is optional; a missing
// one is recorded loudly in warnings.txt and the README rather than silently
// dropped (fsv.md: no silent gaps).
type ReportInputs struct {
	Stamp         BuildStamp // build identity (default: ReadBuildStamp)
	WorldHash     string     // loaded world-archive hash, or ""
	Log           *Logger    // ring-buffer log
	Counters      *Counters  // perf counter history
	StateDumpJSON []byte     // R-FSV-2 full state dump
	StateHash     string     // formatted StateHash + per-system sub-hashes
	Replay        []byte     // .litdreplay command stream from match start
	Sys           *SysInfo   // system probe; nil => ProbeSysInfo()
}

// reportEpoch is the fixed zip-entry modtime: bundle bytes depend only on the
// inputs (and the injected clock for the name), never on filesystem time, so
// two bundles of the same inputs are byte-identical and diffable.
var reportEpoch = time.Date(1980, 1, 1, 0, 0, 0, 0, time.UTC)

// WriteReport composes the bundle into dir and returns its path plus any loud
// warnings (missing/empty sections). It is fail-closed: on any write error the
// partial zip is removed and the error returned — never a truncated artifact.
func WriteReport(dir string, in ReportInputs, now func() time.Time) (path string, warnings []string, err error) {
	if now == nil {
		now = time.Now
	}
	stamp := in.Stamp
	if stamp.Hash == "" {
		stamp = ReadBuildStamp()
	}
	hash8 := stamp.Hash
	if len(hash8) > 8 {
		hash8 = hash8[:8]
	}
	if hash8 == "" {
		hash8 = "unknown"
	}

	if err = os.MkdirAll(dir, 0o755); err != nil {
		return "", nil, fmt.Errorf("report: create dir: %w", err)
	}

	// Unique, non-overwriting name (two F10s in one second still both land).
	ts := now().Format("20060102-150405")
	var f *os.File
	for i := 0; ; i++ {
		name := fmt.Sprintf("litd-report-%s-%s.zip", hash8, ts)
		if i > 0 {
			name = fmt.Sprintf("litd-report-%s-%s-%d.zip", hash8, ts, i)
		}
		candidate := filepath.Join(dir, name)
		file, oerr := os.OpenFile(candidate, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
		if oerr == nil {
			f, path = file, candidate
			break
		}
		if !os.IsExist(oerr) {
			return "", nil, fmt.Errorf("report: open: %w", oerr)
		}
	}

	warnings, err = writeBundle(f, stamp, hash8, in, now)
	cerr := f.Close()
	if err == nil {
		err = cerr
	}
	if err != nil {
		// Fail-closed: remove the partial artifact so nobody ships a truncated
		// bundle that silently lacks sections.
		_ = os.Remove(path)
		return "", warnings, fmt.Errorf("report: write bundle (partial removed): %w", err)
	}
	return path, warnings, nil
}

// writeBundle streams every member into the zip, collecting loud warnings for
// absent/empty sections. A returned error means the caller removes the file.
func writeBundle(f *os.File, stamp BuildStamp, hash8 string, in ReportInputs, now func() time.Time) ([]string, error) {
	zw := zip.NewWriter(f)
	var warnings []string

	add := func(name string, write func(w stringWriter) error) error {
		hdr := &zip.FileHeader{Name: name, Method: zip.Deflate, Modified: reportEpoch}
		w, err := zw.CreateHeader(hdr)
		if err != nil {
			return err
		}
		return write(w)
	}
	note := func(section, why string) string {
		w := fmt.Sprintf("%s: %s", section, why)
		warnings = append(warnings, w)
		return w
	}

	sys := in.Sys
	if sys == nil {
		s := ProbeSysInfo()
		sys = &s
	}

	// sysinfo.txt
	if err := add("sysinfo.txt", func(w stringWriter) error {
		_, e := fmt.Fprintf(w, "os/arch: %s/%s\nnumcpu:  %d\ngo-mem-sys: %d bytes\nlocale:  %s\ngpu:     %s\ndriver:  %s\n",
			sys.OS, sys.Arch, sys.NumCPU, sys.GoMemSys, emptyDash(sys.Locale), emptyDash(sys.GPU), emptyDash(sys.Driver))
		return e
	}); err != nil {
		return warnings, err
	}

	// log.txt
	if in.Log != nil {
		if err := add("log.txt", func(w stringWriter) error { return in.Log.Dump(w) }); err != nil {
			return warnings, err
		}
	} else {
		note("log", "no logger attached")
	}

	// state.json
	if len(in.StateDumpJSON) > 0 {
		if err := add("state.json", func(w stringWriter) error { _, e := w.Write(in.StateDumpJSON); return e }); err != nil {
			return warnings, err
		}
	} else {
		note("state", "no state dump provided")
	}

	// statehash.txt
	if in.StateHash != "" {
		if err := add("statehash.txt", func(w stringWriter) error { _, e := w.Write([]byte(in.StateHash)); return e }); err != nil {
			return warnings, err
		}
	} else {
		note("statehash", "no state hash provided")
	}

	// replay.litdreplay — the repro feedback loop. An empty replay (e.g. F10 at
	// tick 0) is loud, not a crash: the bundle still writes, with a note member.
	if len(in.Replay) > 0 {
		if err := add("replay.litdreplay", func(w stringWriter) error { _, e := w.Write(in.Replay); return e }); err != nil {
			return warnings, err
		}
	} else {
		why := note("replay", "EMPTY — bundle cannot reproduce the bug (captured before any command, e.g. F10 at tick 0)")
		if err := add("replay-MISSING.txt", func(w stringWriter) error { _, e := w.Write([]byte(why + "\n")); return e }); err != nil {
			return warnings, err
		}
	}

	// counters.txt
	if in.Counters != nil {
		if err := add("counters.txt", func(w stringWriter) error { return in.Counters.ExportBench(w) }); err != nil {
			return warnings, err
		}
	} else {
		note("counters", "no counters attached")
	}

	// README.txt last so it can summarize what made it in.
	if err := add("README.txt", func(w stringWriter) error {
		var b strings.Builder
		b.WriteString("LitD debug report (R-OBS-3)\n")
		fmt.Fprintf(&b, "generated: %s\n", now().Format(time.RFC3339Nano))
		fmt.Fprintf(&b, "build:     %s (modified=%t)\n", stamp.Hash, stamp.Modified)
		fmt.Fprintf(&b, "version:   %s\n", stamp.Version)
		fmt.Fprintf(&b, "go:        %s\n", stamp.Go)
		fmt.Fprintf(&b, "world-hash: %s\n", emptyDash(in.WorldHash))
		b.WriteString("\ncontents:\n")
		for _, m := range []string{"sysinfo.txt", "log.txt", "state.json", "statehash.txt", "replay.litdreplay", "counters.txt"} {
			b.WriteString("  - " + m + "\n")
		}
		if len(warnings) > 0 {
			b.WriteString("\nWARNINGS (missing/empty sections):\n")
			for _, w := range warnings {
				b.WriteString("  ! " + w + "\n")
			}
		}
		b.WriteString("\nrepro: extract replay.litdreplay and run `headless -verify replay.litdreplay`;\n")
		b.WriteString("the final StateHash must equal statehash.txt. Sub-hash divergence pinpoints the failing system.\n")
		_, e := w.Write([]byte(b.String()))
		return e
	}); err != nil {
		return warnings, err
	}

	if len(warnings) > 0 {
		if err := add("warnings.txt", func(w stringWriter) error {
			_, e := w.Write([]byte(strings.Join(warnings, "\n") + "\n"))
			return e
		}); err != nil {
			return warnings, err
		}
	}

	return warnings, zw.Close()
}

// stringWriter is the minimal write surface the members need (zip entries and
// the Logger/Counters dumpers all satisfy io.Writer).
type stringWriter interface {
	Write([]byte) (int, error)
}

func emptyDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}
