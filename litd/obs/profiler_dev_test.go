//go:build litddev

package obs

import (
	"io"
	"net/http"
	"strings"
	"testing"
	"time"
)

// TestProfilerServesValidProfilesFSV is the dev-build FSV: a started profiler
// serves the pprof index and emits a real (gzip-compressed protobuf) profile.
// SoT = the actual HTTP response bytes.
func TestProfilerServesValidProfilesFSV(t *testing.T) {
	if !DevProfilerAvailable {
		t.Fatal("DevProfilerAvailable must be true under -tags litddev")
	}
	p, err := StartProfiler("127.0.0.1:0")
	if err != nil {
		t.Fatalf("StartProfiler: %v", err)
	}
	defer p.Close()

	base := "http://" + p.Addr()
	client := &http.Client{Timeout: 10 * time.Second}

	// Index page lists the available profiles.
	resp, err := client.Get(base + "/debug/pprof/")
	if err != nil {
		t.Fatalf("GET index: %v", err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 {
		t.Fatalf("index status = %d, want 200", resp.StatusCode)
	}
	if !strings.Contains(string(body), "/debug/pprof/") || !strings.Contains(string(body), "heap") {
		t.Fatalf("index body missing profile listing:\n%s", string(body)[:min(400, len(body))])
	}

	// A heap profile must be a gzip-compressed pprof protobuf (magic 0x1f 0x8b).
	resp, err = client.Get(base + "/debug/pprof/heap")
	if err != nil {
		t.Fatalf("GET heap: %v", err)
	}
	prof, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	if resp.StatusCode != 200 {
		t.Fatalf("heap status = %d, want 200", resp.StatusCode)
	}
	if len(prof) < 2 || prof[0] != 0x1f || prof[1] != 0x8b {
		t.Fatalf("heap profile is not gzip pprof: len=%d head=%x", len(prof), prof[:min(8, len(prof))])
	}
	t.Logf("FSV: index 200 (%d bytes), heap profile %d bytes gzip-pprof", len(body), len(prof))

	// CPU profile (spec FSV) — short, skipped under -short.
	if !testing.Short() {
		resp, err = client.Get(base + "/debug/pprof/profile?seconds=1")
		if err != nil {
			t.Fatalf("GET cpu profile: %v", err)
		}
		cpu, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		if resp.StatusCode != 200 || len(cpu) < 2 || cpu[0] != 0x1f || cpu[1] != 0x8b {
			t.Fatalf("cpu profile invalid: status=%d len=%d", resp.StatusCode, len(cpu))
		}
		t.Logf("FSV: 1s CPU profile %d bytes gzip-pprof", len(cpu))
	}
}

// TestProfilerRefusesNonLoopback proves the localhost-only constraint: a
// non-loopback bind address is refused before any listener opens.
func TestProfilerRefusesNonLoopback(t *testing.T) {
	if _, err := StartProfiler("8.8.8.8:0"); err != ErrProfilerNotLoopback {
		t.Fatalf("StartProfiler(non-loopback) err = %v, want ErrProfilerNotLoopback", err)
	}
	// A bare port (":0") is rewritten to 127.0.0.1 and must succeed.
	p, err := StartProfiler(":0")
	if err != nil {
		t.Fatalf("StartProfiler(:0): %v", err)
	}
	defer p.Close()
	if !strings.HasPrefix(p.Addr(), "127.0.0.1:") {
		t.Fatalf("bare-port bind addr = %q, want 127.0.0.1:<port>", p.Addr())
	}
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
