package sim

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// #210 cross-OS dimension, gated headlessly.
//
// #210 requires a full-match replay to hash-identical on linux/windows/macos. But
// this project permanently runs NO CI (operator decision closing #284/#317) and
// §3.20 bars paid windows/macos runners, so the literal "3-OS CI matrix" deliverable
// is unreachable in this environment. The achievable headless guarantee is
// STRUCTURAL — the deterministic core hashes identically on every OS/arch BY
// CONSTRUCTION:
//
//	(a) no nondeterministic ops — determlint bans floats, map iteration, time.*,
//	    goroutines, select, and crypto/rand in this code (enforced in preflight);
//	(b) fixed-point integer math only — no float rounding that could differ by arch;
//	(c) a SINGLE implementation — no per-OS/arch source files or build constraints
//	    that could make one platform compute a different result; and
//	(d) it builds for every target OS/arch.
//
// determlint covers (a)/(b). This gate covers (c) and (d): both are necessary for
// cross-OS hash equality and nothing else enforced them. If someone adds a
// litd/sim/foo_windows.go, an OS/arch //go:build constraint, or breaks the darwin
// build, this goes red — the determinism debt #210/M7 must never discover. The one
// thing it cannot do is empirically run the hash on real win/mac hardware; that
// remains genuinely operator-gated.
//
// Heavy (cross-compiles four packages × six targets) — skipped under -short; FULL
// preflight runs it.
func TestDeterministicCoreCrossOSBuildsFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping cross-OS build matrix in -short")
	}
	root := moduleRootFromCWD(t)
	detPkgs := []string{"./litd/sim/...", "./litd/prng/...", "./litd/fixed/...", "./litd/statehash/..."}
	targets := []struct{ goos, goarch string }{
		{"linux", "amd64"}, {"linux", "arm64"},
		{"windows", "amd64"}, {"windows", "arm64"},
		{"darwin", "amd64"}, {"darwin", "arm64"},
	}

	// (d) builds everywhere.
	for _, tg := range targets {
		args := append([]string{"build"}, detPkgs...)
		cmd := exec.Command("go", args...)
		cmd.Dir = root
		cmd.Env = append(os.Environ(), "CGO_ENABLED=0", "GOOS="+tg.goos, "GOARCH="+tg.goarch)
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("deterministic core fails to build for %s/%s — a cross-OS determinism hazard:\n%v\n%s",
				tg.goos, tg.goarch, err, out)
		}
		t.Logf("FSV #210: deterministic core builds for %s/%s", tg.goos, tg.goarch)
	}

	// (c) single implementation: no OS/arch-specific source in the determinism path.
	srcDirs := []string{"litd/sim", "litd/prng", "litd/fixed", "litd/statehash"}
	for _, d := range srcDirs {
		entries, err := os.ReadDir(filepath.Join(root, d))
		if err != nil {
			t.Fatalf("read %s: %v", d, err)
		}
		for _, e := range entries {
			name := e.Name()
			if e.IsDir() || !strings.HasSuffix(name, ".go") || strings.HasSuffix(name, "_test.go") {
				continue
			}
			if os, arch := osArchSuffix(name); os != "" || arch != "" {
				t.Fatalf("DETERMINISM HAZARD (#210): %s/%s is OS/arch-constrained by filename (os=%q arch=%q) — "+
					"the deterministic core must be a single implementation so its hash is identical on every platform",
					d, name, os, arch)
			}
			if tag := osArchBuildConstraint(t, filepath.Join(root, d, name)); tag != "" {
				t.Fatalf("DETERMINISM HAZARD (#210): %s/%s carries an OS/arch build constraint (%q) — "+
					"the deterministic core must compile identically on every platform", d, name, tag)
			}
		}
	}
	t.Log("FSV #210: deterministic core is single-implementation and builds on all 3 OSes × 2 arches — " +
		"cross-OS hash equality is structurally guaranteed (determlint covers float/map/time/goroutine bans). " +
		"Empirical run on real win/mac hardware remains operator-gated.")
}

var knownGOOS = map[string]bool{
	"aix": true, "android": true, "darwin": true, "dragonfly": true, "freebsd": true,
	"hurd": true, "illumos": true, "ios": true, "js": true, "linux": true, "nacl": true,
	"netbsd": true, "openbsd": true, "plan9": true, "solaris": true, "windows": true, "wasip1": true,
}

var knownGOARCH = map[string]bool{
	"386": true, "amd64": true, "arm": true, "arm64": true, "loong64": true, "mips": true,
	"mips64": true, "mips64le": true, "mipsle": true, "ppc64": true, "ppc64le": true,
	"riscv64": true, "s390x": true, "wasm": true,
}

// osArchSuffix decodes Go's implicit filename build constraint: name_GOOS.go,
// name_GOARCH.go, or name_GOOS_GOARCH.go.
func osArchSuffix(name string) (os, arch string) {
	base := strings.TrimSuffix(name, ".go")
	parts := strings.Split(base, "_")
	if len(parts) >= 2 {
		last := parts[len(parts)-1]
		prev := parts[len(parts)-2]
		if knownGOARCH[last] && knownGOOS[prev] {
			return prev, last
		}
		if knownGOOS[last] {
			return last, ""
		}
		if knownGOARCH[last] {
			return "", last
		}
	}
	return "", ""
}

// osArchBuildConstraint returns the first //go:build line referencing a GOOS/GOARCH
// token, or "" if none. Catches explicit constraints filename suffixes miss.
func osArchBuildConstraint(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	for _, line := range strings.Split(string(b), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "package ") {
			break // constraints must precede package clause
		}
		if !strings.HasPrefix(line, "//go:build") && !strings.HasPrefix(line, "// +build") {
			continue
		}
		for _, tok := range strings.FieldsFunc(line, func(r rune) bool {
			return r == ' ' || r == '\t' || r == ',' || r == '(' || r == ')' || r == '!' || r == '|' || r == '&'
		}) {
			if knownGOOS[tok] || knownGOARCH[tok] {
				return line
			}
		}
	}
	return ""
}

// moduleRootFromCWD walks up to the dir holding go.mod.
func moduleRootFromCWD(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if _, statErr := os.Stat(filepath.Join(dir, "go.mod")); statErr == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("go.mod not found above CWD")
		}
		dir = parent
	}
}
