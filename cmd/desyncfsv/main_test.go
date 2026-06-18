package main

// Headless CI coverage for the #82 desync FSV harness: drives runHarness for the
// three required scenarios and independently re-parses the dump files to confirm
// the divergence is isolated to the named system. No mocks — real sim clients.

import (
	"encoding/json"
	"os"
	"testing"
)

type dumpSystem struct {
	Name string `json:"name"`
	Hash uint64 `json:"hash"`
}
type dumpFile struct {
	Turn    uint64       `json:"turn"`
	Client  uint8        `json:"client"`
	Top     uint64       `json:"top"`
	Systems []dumpSystem `json:"systems"`
}

func loadDump(t *testing.T, path string) dumpFile {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read dump %s: %v", path, err)
	}
	var d dumpFile
	if err := json.Unmarshal(b, &d); err != nil {
		t.Fatalf("parse dump %s: %v", path, err)
	}
	return d
}

// divergingSystems returns the sub-hash names that differ between two client
// dumps for the same turn.
func divergingSystems(a, b dumpFile) []string {
	bm := map[string]uint64{}
	for _, s := range b.Systems {
		bm[s.Name] = s.Hash
	}
	var diff []string
	for _, s := range a.Systems {
		if h, ok := bm[s.Name]; !ok || h != s.Hash {
			diff = append(diff, s.Name)
		}
	}
	return diff
}

func baseCfg(dir string) config {
	return config{clients: 2, K: 10, turnLen: 2, totalTicks: 600, seed: 7, dumpDir: dir}
}

func TestDesyncFSV_PRNGInjectionIsolated(t *testing.T) {
	cfg := baseCfg(t.TempDir())
	cfg.injectSystem = "prng"
	cfg.injectTick = 405 // mid-cadence → real detection lag
	res, err := runHarness(cfg)
	if err != nil {
		t.Fatalf("runHarness: %v", err)
	}
	if len(res.events) == 0 {
		t.Fatal("prng divergence never detected (timeout)")
	}
	first := res.events[0]
	delta := first.turn - res.injectTurn
	if delta > uint64(cfg.K) {
		t.Fatalf("detection lag %d turns exceeds K=%d", delta, cfg.K)
	}
	if first.system != "prng" {
		t.Fatalf("bisected to %q, want prng", first.system)
	}
	// Independently re-parse the dumps: only the prng sub-hash must differ.
	if len(first.dumps) != 2 {
		t.Fatalf("expected 2 client dumps, got %d", len(first.dumps))
	}
	d0 := loadDump(t, first.dumps[0])
	d1 := loadDump(t, first.dumps[1])
	diff := divergingSystems(d0, d1)
	if len(diff) != 1 || diff[0] != "prng" {
		t.Fatalf("prng injection diverged %v, want exactly [prng]", diff)
	}
	t.Logf("FSV prng: detected turn %d (Δ=%d ≤K=%d), bisected to prng; dumps isolate %v", first.turn, delta, cfg.K, diff)
}

func TestDesyncFSV_EntitiesInjectionBisected(t *testing.T) {
	cfg := baseCfg(t.TempDir())
	cfg.injectSystem = "entities"
	cfg.injectTick = 405
	res, err := runHarness(cfg)
	if err != nil {
		t.Fatalf("runHarness: %v", err)
	}
	if len(res.events) == 0 {
		t.Fatal("entities divergence never detected (timeout)")
	}
	first := res.events[0]
	if first.system != "entities" {
		t.Fatalf("bisected to %q, want entities (the first system an extra unit perturbs)", first.system)
	}
	d0 := loadDump(t, first.dumps[0])
	d1 := loadDump(t, first.dumps[1])
	diff := divergingSystems(d0, d1)
	if len(diff) == 0 || diff[0] != "entities" {
		t.Fatalf("entities injection: first diverging system %v, want entities first", diff)
	}
	t.Logf("FSV entities: detected turn %d, bisected to entities; dump diverging set %v", first.turn, diff)
}

func TestDesyncFSV_ControlNoFalsePositive(t *testing.T) {
	cfg := baseCfg(t.TempDir()) // injectSystem "" = control
	res, err := runHarness(cfg)
	if err != nil {
		t.Fatalf("runHarness: %v", err)
	}
	if len(res.events) != 0 {
		t.Fatalf("FALSE POSITIVE: %d desync events on an undisturbed match", len(res.events))
	}
	if res.comparisons != res.hashRounds || res.comparisons == 0 {
		t.Fatalf("control: comparisons=%d hashRounds=%d, want equal and >0", res.comparisons, res.hashRounds)
	}
	t.Logf("FSV control: %d comparisons, 0 desync events (no false positive)", res.comparisons)
}

func TestDesyncFSV_TransformsInjectionBisected(t *testing.T) {
	cfg := baseCfg(t.TempDir())
	cfg.injectSystem = "transforms" // a divergent move order on one client
	cfg.injectTick = 405
	res, err := runHarness(cfg)
	if err != nil {
		t.Fatalf("runHarness: %v", err)
	}
	if len(res.events) == 0 {
		t.Fatal("transforms divergence never detected (timeout)")
	}
	first := res.events[0]
	delta := first.turn - res.injectTurn
	if delta > uint64(cfg.K) {
		t.Fatalf("detection lag %d turns exceeds K=%d", delta, cfg.K)
	}
	if first.system != "transforms" {
		t.Fatalf("bisected to %q, want transforms (a move order moves the unit's position)", first.system)
	}
	d0 := loadDump(t, first.dumps[0])
	d1 := loadDump(t, first.dumps[1])
	diff := divergingSystems(d0, d1)
	if len(diff) == 0 || diff[0] != "transforms" {
		t.Fatalf("transforms injection: first diverging system %v, want transforms first", diff)
	}
	t.Logf("FSV transforms: detected turn %d (Δ=%d ≤K=%d), bisected to transforms; dump diverging set %v", first.turn, delta, cfg.K, diff)
}

func TestDesyncFSV_UnsupportedSystemRefused(t *testing.T) {
	cfg := baseCfg(t.TempDir())
	cfg.injectSystem = "combat" // not yet wired (needs opposing units)
	if _, err := runHarness(cfg); err == nil {
		t.Fatal("combat injection accepted; not wired yet")
	}
	t.Log("FSV: unsupported injection system refused")
}
