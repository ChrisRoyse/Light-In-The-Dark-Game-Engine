package monitor

import (
	"strings"
	"testing"
	"time"
)

// #188 FSV: the monitoring core. SoT = the serialized metrics text and the alert
// transitions Observe returns under an injected clock. No real time, no network.

func TestMetricsSerializeFSV(t *testing.T) {
	m := NewMetrics()
	// Relay counters/gauges (counts only — no payload inspection).
	m.SetGauge("relay_active_sessions", 0)
	t.Logf("BEFORE: turns=%d sessions=%d", m.Counter("relay_turns_forwarded"), m.Gauge("relay_active_sessions"))

	m.Add("relay_turns_forwarded", 3)
	m.Inc("relay_turns_forwarded")
	m.Inc("relay_drops")
	m.SetGauge("relay_active_sessions", 2)
	t.Logf("AFTER:  turns=%d sessions=%d drops=%d", m.Counter("relay_turns_forwarded"), m.Gauge("relay_active_sessions"), m.Counter("relay_drops"))

	if m.Counter("relay_turns_forwarded") != 4 {
		t.Fatalf("turns_forwarded = %d, want 4", m.Counter("relay_turns_forwarded"))
	}
	out := m.Serialize()
	t.Logf("exposition:\n%s", out)
	for _, want := range []string{
		"# TYPE relay_turns_forwarded counter\nrelay_turns_forwarded 4",
		"# TYPE relay_drops counter\nrelay_drops 1",
		"# TYPE relay_active_sessions gauge\nrelay_active_sessions 2",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("exposition missing %q in:\n%s", want, out)
		}
	}
	// Deterministic + sorted: counters before gauges, names ascending.
	if m.Serialize() != out {
		t.Fatal("Serialize not deterministic")
	}
	if strings.Index(out, "relay_drops") > strings.Index(out, "relay_turns_forwarded") {
		t.Fatal("counters not name-sorted")
	}
}

func TestMonitorCounterMonotonicPanicFSV(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("Add of a negative delta must panic (counters monotonic)")
		}
	}()
	NewMetrics().Add("x", -1)
}

func TestMonitorServiceDownAlertFSV(t *testing.T) {
	mo := NewMonitor(Rules{DownFor: 2 * time.Minute, ErrorRateMax: 0.10})
	min := time.Minute

	// Up → nothing.
	if a := mo.Observe(Probe{Service: "relay", OK: true}, 0); len(a) != 0 {
		t.Fatalf("healthy probe alerted: %v", a)
	}
	// Goes down at t=0; not yet past threshold at t=0 and t=1min.
	if a := mo.Observe(Probe{Service: "relay", OK: false}, 0); len(a) != 0 {
		t.Fatalf("down<2min alerted at t=0: %v", a)
	}
	if a := mo.Observe(Probe{Service: "relay", OK: false}, 1*min); len(a) != 0 {
		t.Fatalf("down<2min alerted at t=1m: %v", a)
	}
	// At t=2min → ServiceDown fires once.
	a := mo.Observe(Probe{Service: "relay", OK: false}, 2*min)
	if len(a) != 1 || a[0].Kind != AlertServiceDown {
		t.Fatalf("at 2min want one service-down, got %v", a)
	}
	t.Logf("FSV down: paged at 2min — %s: %s", a[0].Kind, a[0].Detail)
	// Still down at t=3min → debounced, no re-page.
	if a := mo.Observe(Probe{Service: "relay", OK: false}, 3*min); len(a) != 0 {
		t.Fatalf("re-paged while still down (no debounce): %v", a)
	}
	// Recovers → Recovered fires once.
	a = mo.Observe(Probe{Service: "relay", OK: true}, 4*min)
	if len(a) != 1 || a[0].Kind != AlertRecovered {
		t.Fatalf("recovery want one recovered, got %v", a)
	}
	t.Logf("FSV recover: %s at 4min", a[0].Kind)
	// New failure window must page again (state reset).
	mo.Observe(Probe{Service: "relay", OK: false}, 5*min)
	a = mo.Observe(Probe{Service: "relay", OK: false}, 7*min)
	if len(a) != 1 || a[0].Kind != AlertServiceDown {
		t.Fatalf("second outage want service-down, got %v", a)
	}
}

func TestMonitorErrorRateAlertFSV(t *testing.T) {
	mo := NewMonitor(Rules{DownFor: 2 * time.Minute, ErrorRateMax: 0.10})

	// Under threshold while up → quiet.
	if a := mo.Observe(Probe{Service: "hub", OK: true, ErrorRate: 0.05}, 0); len(a) != 0 {
		t.Fatalf("low error rate alerted: %v", a)
	}
	// 5xx spike crosses threshold → fires once.
	a := mo.Observe(Probe{Service: "hub", OK: true, ErrorRate: 0.40}, time.Minute)
	if len(a) != 1 || a[0].Kind != AlertHighErrorRate {
		t.Fatalf("want high-error-rate, got %v", a)
	}
	t.Logf("FSV errrate: %s — %s", a[0].Kind, a[0].Detail)
	// Still high → debounced.
	if a := mo.Observe(Probe{Service: "hub", OK: true, ErrorRate: 0.50}, 2*time.Minute); len(a) != 0 {
		t.Fatalf("re-alerted while still high: %v", a)
	}
	// Back under → cleared once.
	a = mo.Observe(Probe{Service: "hub", OK: true, ErrorRate: 0.02}, 3*time.Minute)
	if len(a) != 1 || a[0].Kind != AlertErrorRateCleared {
		t.Fatalf("want error-rate-cleared, got %v", a)
	}
	t.Logf("FSV errclear: %s", a[0].Kind)
}
