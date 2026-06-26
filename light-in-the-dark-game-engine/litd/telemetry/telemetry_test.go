package telemetry

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"sort"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
)

// #254 FSV: the opt-in telemetry client, exercised against a REAL httptest ingest
// endpoint (the issue requires a real endpoint, no mocks). SoT = the bytes the
// server actually received, the client preview bytes, the WARN sink, and the
// crash-sig file. Edges: default-off makes zero network calls; an unreachable
// endpoint WARNs once with no retry; a crash sig persists and is taken exactly
// once.

// ingest is a real endpoint that records every received body.
type ingest struct {
	srv  *httptest.Server
	body []byte
	hits int
}

func newIngest(t *testing.T) *ingest {
	t.Helper()
	ig := &ingest{}
	ig.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		b, _ := io.ReadAll(r.Body)
		ig.body = b
		ig.hits++
		w.WriteHeader(http.StatusNoContent)
	}))
	t.Cleanup(ig.srv.Close)
	return ig
}

func samplePayload() Payload {
	return Payload{Build: "abc123", Preset: "high", GPU: "Synthetic GL 1.0",
		TickMSP50: 1.5, TickMSP95: 3.0, TickMSP99: 4.0,
		FrameMSP50: 8.0, FrameMSP95: 12.0, FrameMSP99: 16.0}
}

// Default-off: a disabled client makes NO network call.
func TestTelemetryDefaultOffZeroNetworkFSV(t *testing.T) {
	ig := newIngest(t)
	c := Client{Endpoint: ig.srv.URL} // Enabled defaults to false
	t.Logf("BEFORE: enabled=%v server hits=%d", c.Enabled, ig.hits)
	if err := c.Send(samplePayload()); err != nil {
		t.Fatalf("disabled Send should be a no-op, got %v", err)
	}
	t.Logf("AFTER: server hits=%d", ig.hits)
	if ig.hits != 0 {
		t.Fatalf("default-off made %d network call(s); want 0", ig.hits)
	}
}

// Enabled: the server receives EXACTLY the preview bytes (byte-for-byte).
func TestTelemetryServerMatchesPreviewFSV(t *testing.T) {
	ig := newIngest(t)
	c := Client{Enabled: true, Endpoint: ig.srv.URL}
	p := samplePayload()
	preview, err := c.Preview(p)
	if err != nil {
		t.Fatalf("Preview: %v", err)
	}
	t.Logf("BEFORE: preview=%s", preview)
	if err := c.Send(p); err != nil {
		t.Fatalf("Send: %v", err)
	}
	t.Logf("AFTER: server received=%s (hits=%d)", ig.body, ig.hits)
	if ig.hits != 1 {
		t.Fatalf("server hits=%d, want 1", ig.hits)
	}
	if string(ig.body) != string(preview) {
		t.Fatalf("server bytes != preview:\n server:  %s\n preview: %s", ig.body, preview)
	}
}

// No-PII: the serialized payload carries only the closed allow-list of keys.
func TestTelemetryNoPIIFieldsFSV(t *testing.T) {
	b, err := samplePayload().Marshal()
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatal(err)
	}
	allowed := map[string]bool{
		"build": true, "preset": true, "gpu": true,
		"tick_ms_p50": true, "tick_ms_p95": true, "tick_ms_p99": true,
		"frame_ms_p50": true, "frame_ms_p95": true, "frame_ms_p99": true,
		"crash_sig": true,
	}
	got := make([]string, 0, len(m))
	for k := range m {
		got = append(got, k)
		if !allowed[k] {
			t.Fatalf("payload carries non-allow-listed key %q (possible PII/gameplay leak)", k)
		}
	}
	sort.Strings(got)
	t.Logf("FSV no-PII: payload keys = %v (all allow-listed, no username/path/world)", got)
}

// Unreachable endpoint → WARN once, no retry storm.
func TestTelemetryUnreachableWarnsOnceFSV(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	url := srv.URL
	srv.Close() // now refused

	var warns []string
	c := Client{Enabled: true, Endpoint: url, HTTP: &http.Client{Timeout: 2 * time.Second},
		Warn: func(s string) { warns = append(warns, s) }}

	// Three sends against the dead endpoint.
	for i := 0; i < 3; i++ {
		if err := c.Send(samplePayload()); err == nil {
			t.Fatalf("send %d to dead endpoint should error", i)
		}
	}
	t.Logf("AFTER: 3 sends to a dead endpoint → %d WARN(s): %v", len(warns), warns)
	if len(warns) != 1 {
		t.Fatalf("warned %d times across 3 failed sends; want exactly 1 (no retry storm)", len(warns))
	}
}

// Crash sig persists and is taken exactly once (reported next launch, then gone).
func TestTelemetryCrashSigNextLaunchFSV(t *testing.T) {
	path := t.TempDir() + "/pending-crash"
	sig := CrashSignature("goroutine 1 [running]:\nmain.boom()", "abc123")
	t.Logf("BEFORE: sig=%s", sig)
	if err := WriteCrashSig(path, sig); err != nil {
		t.Fatalf("WriteCrashSig: %v", err)
	}

	// Next launch takes it...
	got, ok := TakeCrashSig(path)
	if !ok || got != sig {
		t.Fatalf("TakeCrashSig = (%q,%v), want (%q,true)", got, ok, sig)
	}
	// ...and it is gone (never reported twice).
	if _, ok2 := TakeCrashSig(path); ok2 {
		t.Fatal("crash sig taken twice — must be consumed on first take")
	}
	t.Logf("AFTER: took sig once (%q), second take empty", got)

	// The taken sig rides in the next payload.
	p := Payload{Build: "abc123", CrashSig: got}
	b, _ := p.Marshal()
	if !contains(string(b), got) {
		t.Fatalf("crash sig not in payload: %s", b)
	}
}

// BuildPayload computes percentiles from real counter history (X+X=Y synthetic
// samples with known ranks).
func TestTelemetryBuildPayloadPercentilesFSV(t *testing.T) {
	c := obs.NewDefaultCounters()
	std := obs.RegisterStandardCounters(c)
	// 10 samples: sim tick 1..10 ms, render frame 10..100 ms (ns values).
	for i := 1; i <= 10; i++ {
		c.Set(std.SimTickNS, int64(i)*1_000_000)
		c.Set(std.RenderFrameNS, int64(i)*10_000_000)
		c.Sample(uint32(i), uint32(i))
	}
	p := BuildPayload(c, std, Meta{Build: "abc123", Preset: "low", GPU: "g"})
	t.Logf("AFTER: tick p50/p95/p99 = %.1f/%.1f/%.1f ms; frame = %.1f/%.1f/%.1f ms",
		p.TickMSP50, p.TickMSP95, p.TickMSP99, p.FrameMSP50, p.FrameMSP95, p.FrameMSP99)
	// nearest-rank over 10 samples: p50→5th, p95/p99→10th.
	if p.TickMSP50 != 5 || p.TickMSP95 != 10 || p.TickMSP99 != 10 {
		t.Fatalf("tick percentiles = %.1f/%.1f/%.1f, want 5/10/10", p.TickMSP50, p.TickMSP95, p.TickMSP99)
	}
	if p.FrameMSP50 != 50 || p.FrameMSP95 != 100 {
		t.Fatalf("frame percentiles = %.1f/%.1f, want 50/100", p.FrameMSP50, p.FrameMSP95)
	}
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
