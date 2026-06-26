// Package telemetry is the opt-in anonymous perf/crash telemetry client
// (#254; R-OBS-4; observability-and-debugging.md §4). It is OFF by default, sends
// a single one-line JSON document with NO PII and no gameplay/world data, and
// lets the config show the exact bytes before enabling. The ingest endpoint is
// own-site (deployment is gated on hosting); this package is the client + the
// wire contract.
//
// Payload fields are a closed allow-list — percentile tick/frame milliseconds,
// GPU string, graphics preset, build hash, and an optional crash signature
// (stack hash + build hash). There is deliberately no field for a username,
// path, world, or any gameplay datum.
package telemetry

import (
	"encoding/json"
	"math"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
)

// Payload is the exact document sent to the ingest endpoint. Every field is
// anonymous by construction.
type Payload struct {
	Build      string  `json:"build"`  // build hash (also part of the crash sig)
	Preset     string  `json:"preset"` // graphics preset name
	GPU        string  `json:"gpu"`    // GL renderer string
	TickMSP50  float64 `json:"tick_ms_p50"`
	TickMSP95  float64 `json:"tick_ms_p95"`
	TickMSP99  float64 `json:"tick_ms_p99"`
	FrameMSP50 float64 `json:"frame_ms_p50"`
	FrameMSP95 float64 `json:"frame_ms_p95"`
	FrameMSP99 float64 `json:"frame_ms_p99"`
	CrashSig   string  `json:"crash_sig,omitempty"` // stack hash + build hash; absent when no crash
}

// Marshal returns the canonical one-line JSON bytes. This is THE payload — both
// the config preview and the network send use it, so the preview is byte-for-byte
// what the server receives.
func (p Payload) Marshal() ([]byte, error) {
	return json.Marshal(p)
}

// Meta is the non-counter context stamped on a payload.
type Meta struct {
	Build    string
	Preset   string
	GPU      string
	CrashSig string // empty when this launch had no prior-crash record
}

// BuildPayload assembles a Payload from the live counter history: it reads the
// sim-tick and render-frame duration samples and computes their p50/p95/p99 in
// milliseconds. No gameplay data is touched.
func BuildPayload(c *obs.Counters, std obs.StandardCounters, m Meta) Payload {
	tick := historyMS(c, std.SimTickNS)
	frame := historyMS(c, std.RenderFrameNS)
	return Payload{
		Build: m.Build, Preset: m.Preset, GPU: m.GPU, CrashSig: m.CrashSig,
		TickMSP50: pct(tick, 50), TickMSP95: pct(tick, 95), TickMSP99: pct(tick, 99),
		FrameMSP50: pct(frame, 50), FrameMSP95: pct(frame, 95), FrameMSP99: pct(frame, 99),
	}
}

// historyMS collects a counter's history samples as nanoseconds.
func historyMS(c *obs.Counters, id obs.CounterID) []int64 {
	n := c.Len()
	out := make([]int64, 0, n)
	for i := 0; i < n; i++ {
		if v, ok := c.HistoryValue(i, id); ok {
			out = append(out, v)
		}
	}
	return out
}

// pct returns the q-th percentile (nearest-rank) of ns values, converted to ms.
func pct(ns []int64, q float64) float64 {
	if len(ns) == 0 {
		return 0
	}
	s := append([]int64(nil), ns...)
	sort.Slice(s, func(i, j int) bool { return s[i] < s[j] })
	rank := int(math.Ceil(q/100*float64(len(s)))) - 1
	if rank < 0 {
		rank = 0
	}
	if rank >= len(s) {
		rank = len(s) - 1
	}
	return float64(s[rank]) / 1e6
}
