package sim

// #203 tests: state-dump JSON (R-FSV-2) + structured event log
// (R-FSV-3). SoT = the emitted bytes themselves.

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// dumpWorld: two opposed armed units that will fight, plus movement.
func dumpWorld(t *testing.T) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	w.BindDamageMatrix([][]int32{{1000}})
	a := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, fixed.One)
	b := atkUnit(t, w, 1, fixed.Vec2{X: 1050 * fixed.One, Y: 1000 * fixed.One}, 0)
	arm(t, w, a, 0, 0)
	return w, a, b
}

// Edge: emitting a dump never mutates state — full-state hash before
// and after is bit-identical.
func TestDumpReadOnly(t *testing.T) {
	w, _, _ := dumpWorld(t)
	for i := 0; i < 50; i++ {
		w.Step()
	}
	reg := NewHashRegistry()
	var before, after statehash.Snapshot
	w.HashState(reg, &before)
	var buf bytes.Buffer
	if err := w.DumpState(&buf); err != nil {
		t.Fatal(err)
	}
	w.HashState(reg, &after)
	t.Logf("hash before=%016x after=%016x dumpBytes=%d", before.Top, after.Top, buf.Len())
	if before.Top != after.Top {
		t.Fatal("DumpState mutated the world")
	}
}

// Edge: dump at tick 0 vs tick N — tick and hash differ, schema
// identical; the dump's embedded hash equals an independent
// HashState of the same state.
func TestDumpSchemaTickAndHash(t *testing.T) {
	w, a, _ := dumpWorld(t)
	parse := func() (map[string]any, []byte) {
		var buf bytes.Buffer
		if err := w.DumpState(&buf); err != nil {
			t.Fatal(err)
		}
		var m map[string]any
		if err := json.Unmarshal(buf.Bytes(), &m); err != nil {
			t.Fatal(err)
		}
		return m, buf.Bytes()
	}
	d0, _ := parse()
	for i := 0; i < 100; i++ {
		w.Step()
	}
	dN, raw := parse()
	if d0["tick"].(float64) != 0 || dN["tick"].(float64) != 100 {
		t.Fatalf("ticks: %v, %v", d0["tick"], dN["tick"])
	}
	if d0["hash"] == dN["hash"] {
		t.Fatal("hash did not move across 100 ticks")
	}
	for _, k := range []string{"tick", "hash", "subs", "prngState", "entities", "buffs", "unitCount"} {
		if _, ok0 := d0[k]; !ok0 {
			t.Errorf("tick-0 dump missing %q", k)
		}
		if _, okN := dN[k]; !okN {
			t.Errorf("tick-N dump missing %q", k)
		}
	}
	// embedded hash equals an independent recompute
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	if want := dN["hash"].(string); want != hex16(snap.Top) {
		t.Fatalf("embedded hash %s != recomputed %s", want, hex16(snap.Top))
	}
	// hand-picked unit present with raw+decimal position
	found := false
	for _, e := range dN["entities"].([]any) {
		em := e.(map[string]any)
		if uint32(em["id"].(float64)) == uint32(a) {
			found = true
			px := em["posX"].(map[string]any)
			if int64(px["raw"].(float64)) == 0 {
				t.Error("raw position missing")
			}
			t.Logf("unit %d: posX raw=%v dec=%v life=%v", uint32(a), px["raw"], px["dec"], em["life"])
		}
	}
	if !found {
		t.Fatal("hand-picked unit absent from dump")
	}
	t.Logf("tick0 hash=%v tickN hash=%v (%d bytes)", d0["hash"], dN["hash"], len(raw))
}

func hex16(v uint64) string {
	const d = "0123456789abcdef"
	out := make([]byte, 16)
	for i := 15; i >= 0; i-- {
		out[i] = d[v&0xf]
		v >>= 4
	}
	return string(out)
}

// Edge: the same run twice produces byte-identical event logs.
func TestEventLogDeterministic(t *testing.T) {
	run := func() []byte {
		var buf bytes.Buffer
		w, a, b := dumpWorld(t)
		w.AttachEventLog(&buf)
		w.Combats.Target[w.Combats.Row(a)] = b // fight: damage + death events
		for i := 0; i < 400; i++ {
			w.Step()
		}
		if err := w.EventLogErr(); err != nil {
			t.Fatal(err)
		}
		return buf.Bytes()
	}
	l1, l2 := run(), run()
	if len(l1) == 0 {
		t.Fatal("degenerate: no events logged")
	}
	if !bytes.Equal(l1, l2) {
		t.Fatal("twin runs produced different event logs")
	}
	t.Logf("byte-identical logs, %d bytes, first line: %s", len(l1), bytes.SplitN(l1, []byte("\n"), 2)[0])
}

// Edge: a unit dying at tick N logs its death at N and is absent
// from a dump taken after N.
func TestEventLogDeathThenDumpAbsence(t *testing.T) {
	var log bytes.Buffer
	w, _, b := dumpWorld(t)
	w.AttachEventLog(&log)
	w.OnScriptPhase = func(tick uint32) {
		if tick == 30 {
			w.QueueDamage(DamagePacket{Source: 0, Target: b, Amount: 1000 * fixed.One})
		}
	}
	for i := 0; i < 35; i++ {
		w.Step()
	}
	if !bytes.Contains(log.Bytes(), []byte(`"name":"unit-death"`)) {
		t.Fatalf("no death record in log:\n%s", log.String())
	}
	var buf bytes.Buffer
	if err := w.DumpState(&buf); err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(buf.Bytes(), &m); err != nil {
		t.Fatal(err)
	}
	for _, e := range m["entities"].([]any) {
		if uint32(e.(map[string]any)["id"].(float64)) == uint32(b) {
			t.Fatal("dead unit still present in post-death dump")
		}
	}
	for _, line := range bytes.Split(log.Bytes(), []byte("\n")) {
		if bytes.Contains(line, []byte("unit-death")) {
			t.Logf("death record: %s", line)
		}
	}
	t.Logf("dump at tick %v has %d entities (victim gone)", m["tick"], len(m["entities"].([]any)))
}

// R-GC-3: with no log attached, the dispatch path does zero extra
// work — a stepping world with events stays allocation-free.
func TestEventLogDisabledZeroAlloc(t *testing.T) {
	w, _, _ := dumpWorld(t)
	w.Step()
	avg := testing.AllocsPerRun(100, func() { w.Step() })
	if avg != 0 {
		t.Fatalf("allocs/tick with log disabled = %v, want 0", avg)
	}
	t.Logf("allocs = %v", avg)
}

// #335: the decimal renderer is pure integer math — exact cases
// including the rounding boundary, negatives, and MinInt64.
func TestFixedDecString(t *testing.T) {
	cases := []struct {
		raw  int64
		want string
	}{
		{0, "0.000000"},
		{1 << 32, "1.000000"},
		{-(1 << 32), "-1.000000"},
		{(3 << 32) | (1 << 31), "3.500000"},          // exactly .5
		{1, "0.000000"},                              // 2^-32 rounds down
		{0xFFFFFFFF, "1.000000"},                     // just under 1 rounds up, carries
		{-((10 << 32) | (1 << 30)), "-10.250000"},    // negative fraction
		{-9223372036854775808, "-2147483648.000000"}, // MinInt64
	}
	for _, c := range cases {
		got := fixedDecString(c.raw)
		t.Logf("raw=%d -> %s (want %s)", c.raw, got, c.want)
		if got != c.want {
			t.Errorf("fixedDecString(%d) = %q, want %q", c.raw, got, c.want)
		}
	}
}
