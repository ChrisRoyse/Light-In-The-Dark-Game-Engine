// Package monitor is the hub+relay production-monitoring core (#188): an ops
// metrics registry (counters/gauges with a text exposition serializer — the body
// a /metrics endpoint returns) and a clock-injected alert evaluator (service
// down > N, error-rate, debounced so a sustained condition pages exactly once).
//
// This is the logic half. The HTTP /metrics endpoints, the alert delivery
// (email/webhook), the dashboard, and the deployed hub+relay are gated on the
// hosting bring-up (deploy/hub/) and stay deferred on #188; this package is what
// they wrap, verifiable headlessly with synthetic probes and an injected clock.
// Metrics are counts/gauges only — no per-player tracking, and the relay's
// counters never inspect turn payloads (the issue's privacy constraint is met by
// construction: there is no payload input here).
package monitor

import (
	"fmt"
	"sort"
	"strings"
	"time"
)

// Metrics is a registry of named monotonic counters and instantaneous gauges.
// Not safe for concurrent use; a service updates it from its own loop.
type Metrics struct {
	counters map[string]int64
	gauges   map[string]int64
}

// NewMetrics builds an empty registry.
func NewMetrics() *Metrics {
	return &Metrics{counters: map[string]int64{}, gauges: map[string]int64{}}
}

// Inc adds 1 to a counter. Add adds n (n must be >= 0; counters never decrease).
func (m *Metrics) Inc(name string) { m.Add(name, 1) }

// Add increases a counter by n. A negative n panics — counters are monotonic.
func (m *Metrics) Add(name string, n int64) {
	if n < 0 {
		panic(fmt.Sprintf("monitor: counter %q decreased by %d (counters are monotonic)", name, n))
	}
	m.counters[name] += n
}

// SetGauge sets a gauge to v (gauges may rise or fall — e.g. active sessions).
func (m *Metrics) SetGauge(name string, v int64) { m.gauges[name] = v }

// Counter / Gauge read current values (0 if unset).
func (m *Metrics) Counter(name string) int64 { return m.counters[name] }
func (m *Metrics) Gauge(name string) int64   { return m.gauges[name] }

// Serialize renders the registry in a stable Prometheus-style text exposition
// (sorted by name) — deterministic so a scrape diff is meaningful.
func (m *Metrics) Serialize() string {
	var b strings.Builder
	writeSorted(&b, m.counters, "counter")
	writeSorted(&b, m.gauges, "gauge")
	return b.String()
}

func writeSorted(b *strings.Builder, kv map[string]int64, kind string) {
	names := make([]string, 0, len(kv))
	for n := range kv {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		fmt.Fprintf(b, "# TYPE %s %s\n%s %d\n", n, kind, n, kv[n])
	}
}

// AlertKind enumerates the alert transitions the evaluator emits.
type AlertKind uint8

const (
	AlertServiceDown AlertKind = iota
	AlertRecovered
	AlertHighErrorRate
	AlertErrorRateCleared
)

func (k AlertKind) String() string {
	switch k {
	case AlertServiceDown:
		return "service-down"
	case AlertRecovered:
		return "recovered"
	case AlertHighErrorRate:
		return "high-error-rate"
	case AlertErrorRateCleared:
		return "error-rate-cleared"
	default:
		return "unknown"
	}
}

// Alert is one transition the operator is paged about.
type Alert struct {
	Kind    AlertKind
	Service string
	Detail  string
}

// Probe is one health observation of a service.
type Probe struct {
	Service   string
	OK        bool    // did the uptime check pass (hub index GET / relay handshake)
	ErrorRate float64 // recent error ratio [0,1]
}

// Rules configure the alert thresholds.
type Rules struct {
	DownFor      time.Duration // sustained-down duration before paging (#188: 2 min)
	ErrorRateMax float64       // error ratio above which the error-rate alert fires
}

type svcState struct {
	firstFailAt time.Duration
	failing     bool
	downAlerted bool
	errAlerted  bool
}

// Monitor applies the alert rules to a stream of probes under an injected clock.
// Not safe for concurrent use.
type Monitor struct {
	rules Rules
	state map[string]*svcState
}

// NewMonitor builds an evaluator with the given rules.
func NewMonitor(rules Rules) *Monitor {
	return &Monitor{rules: rules, state: map[string]*svcState{}}
}

// Observe feeds one probe at time now and returns the alert transitions it
// triggers (usually none). A sustained-down service pages exactly once (debounced
// until it recovers); recovery and error-rate crossings likewise fire once per
// transition — so a flapping dashboard never spams the operator.
func (mo *Monitor) Observe(p Probe, now time.Duration) []Alert {
	s := mo.state[p.Service]
	if s == nil {
		s = &svcState{}
		mo.state[p.Service] = s
	}
	var alerts []Alert

	if !p.OK {
		if !s.failing {
			s.failing = true
			s.firstFailAt = now
		}
		if !s.downAlerted && now-s.firstFailAt >= mo.rules.DownFor {
			s.downAlerted = true
			alerts = append(alerts, Alert{AlertServiceDown, p.Service,
				fmt.Sprintf("down for %s (threshold %s)", now-s.firstFailAt, mo.rules.DownFor)})
		}
	} else {
		if s.downAlerted {
			alerts = append(alerts, Alert{AlertRecovered, p.Service, "uptime check passing again"})
		}
		s.failing = false
		s.downAlerted = false
		s.firstFailAt = 0
	}

	// Error-rate rule is independent of up/down.
	if p.ErrorRate > mo.rules.ErrorRateMax {
		if !s.errAlerted {
			s.errAlerted = true
			alerts = append(alerts, Alert{AlertHighErrorRate, p.Service,
				fmt.Sprintf("error rate %.2f > %.2f", p.ErrorRate, mo.rules.ErrorRateMax)})
		}
	} else if s.errAlerted {
		s.errAlerted = false
		alerts = append(alerts, Alert{AlertErrorRateCleared, p.Service,
			fmt.Sprintf("error rate %.2f back under %.2f", p.ErrorRate, mo.rules.ErrorRateMax)})
	}

	return alerts
}
