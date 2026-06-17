//go:build !litddev

package obs

import "testing"

// TestProfilerCompiledOutInRelease is the release-build FSV: the profiler is
// absent — StartProfiler fails loudly and DevProfilerAvailable is false. The
// binary-symbol absence (no net/http/pprof) is verified separately via
// `go tool nm` (see the commit's FSV log).
func TestProfilerCompiledOutInRelease(t *testing.T) {
	if DevProfilerAvailable {
		t.Fatal("DevProfilerAvailable must be false without -tags litddev")
	}
	p, err := StartProfiler("127.0.0.1:0")
	if err != ErrProfilerUnavailable {
		t.Fatalf("StartProfiler err = %v, want ErrProfilerUnavailable", err)
	}
	if p != nil {
		t.Fatalf("StartProfiler returned non-nil Profiler in release build: %v", p)
	}
}
