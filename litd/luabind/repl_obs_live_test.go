package luabind

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
)

// #399 FSV: LiveObs wires the REAL observability subsystem (*obs.Logger,
// *obs.Counters) to the REPL's ReplObs surface. repl_test.go proves the eval engine
// with a fake; this proves the *production* bridge against real obs holding KNOWN
// synthetic state — both directly and end-to-end through EvalLine. SoT = the bytes
// the adapter returns / the level mutation in the real logger, not a return code.
// X+X=Y: test.alpha=42 in obs.Counters must read back as 42 through obs.counters()
// in Lua.
func TestLiveObsAdapterFSV(t *testing.T) {
	// --- real obs subsystem, known synthetic state ---
	ctr := obs.NewCounters(8, 4)
	alpha := ctr.Register("test.alpha", "count", obs.CounterGauge)
	beta := ctr.Register("test.beta", "ms", obs.CounterGauge)
	ctr.Set(alpha, 42)
	ctr.Set(beta, 7)

	log := obs.New(64)
	mid := log.Register("synthetic marker val={0}")
	log.Log(100, 5, obs.Info, obs.ChLua, mid, 1234, 0, 0, 0) // recorded: Info ≤ default Trace

	live := &LiveObs{Log: log, Counts: ctr}

	// --- Counters(): values + units match the real counters ---
	got := map[string]int64{}
	units := map[string]string{}
	for _, r := range live.Counters() {
		got[r.Name] = r.Value
		units[r.Name] = r.Unit
	}
	t.Logf("FSV Counters(): alpha=%d beta=%d unit(beta)=%q", got["test.alpha"], got["test.beta"], units["test.beta"])
	if got["test.alpha"] != 42 || got["test.beta"] != 7 || units["test.beta"] != "ms" {
		t.Fatalf("Counters() wrong: values=%v units=%v", got, units)
	}

	// --- DumpLog(): the logged entry renders with its arg substituted ---
	dump := live.DumpLog()
	t.Logf("FSV DumpLog():\n%s", dump)
	if !strings.Contains(dump, "synthetic marker val=1234") {
		t.Fatalf("DumpLog() missing the synthetic entry rendered with args:\n%s", dump)
	}

	// --- LogLevel()/SetLogLevel() against the real logger ---
	if lvl := live.LogLevel(); lvl != int(obs.Trace) {
		t.Fatalf("LogLevel() = %d, want %d (Trace default)", lvl, int(obs.Trace))
	}
	if !live.SetLogLevel(int(obs.Info)) {
		t.Fatal("SetLogLevel(Info) returned false")
	}
	if live.LogLevel() != int(obs.Info) {
		t.Fatalf("after SetLogLevel(Info), LogLevel()=%d", live.LogLevel())
	}
	for ch := 0; ch < int(obs.NumChannels); ch++ { // SoT: every real channel updated
		if log.ChannelLevel(obs.Channel(ch)) != obs.Info {
			t.Fatalf("channel %d level = %v, want Info", ch, log.ChannelLevel(obs.Channel(ch)))
		}
	}

	// --- edge: out-of-range level is rejected, no mutation (fail-closed) ---
	if live.SetLogLevel(99) || live.SetLogLevel(-1) {
		t.Fatal("out-of-range SetLogLevel must return false")
	}
	if live.LogLevel() != int(obs.Info) {
		t.Fatalf("rejected SetLogLevel mutated state: %d", live.LogLevel())
	}

	// --- edge: nil-backed adapter (headless game, observability off) is inert ---
	empty := &LiveObs{}
	if empty.DumpLog() != "" || empty.Counters() != nil || empty.LogLevel() != 0 || empty.SetLogLevel(2) {
		t.Fatal("nil-backed LiveObs must be inert and fail-closed")
	}

	// --- end-to-end through the Lua REPL with the live adapter installed ---
	L := newReplState(live)
	defer L.Close()
	if r := EvalLine(L, "local c=obs.counters(); for i=1,#c do if c[i].name=='test.alpha' then return c[i].value end end"); !r.Ok || r.Output != "42" {
		t.Fatalf("e2e obs.counters() alpha -> %+v, want 42", r)
	}
	if r := EvalLine(L, "return obs.loglevel()"); !r.Ok || r.Output != "2" {
		t.Fatalf("e2e obs.loglevel() -> %+v, want 2 (Info)", r)
	}
	if r := EvalLine(L, "return obs.loglevel(0)"); !r.Ok || r.Output != "true" {
		t.Fatalf("e2e obs.loglevel(0) -> %+v, want true", r)
	}
	if log.ChannelLevel(obs.ChLua) != obs.Error { // SoT: the e2e set reached the real logger
		t.Fatalf("e2e loglevel(0) did not reach the real logger: ChLua=%v", log.ChannelLevel(obs.ChLua))
	}
	t.Log("FSV #399: LiveObs bridges real obs.Logger/Counters to the REPL — direct + through EvalLine, fail-closed on out-of-range and nil")
}
