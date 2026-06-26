package sim

import (
	"crypto/sha256"
	"encoding/binary"
	"testing"
)

const (
	katSeed  = 0xC0FFEE_5EED
	katN     = 256
	katTicks = 10_000
	katEvery = 100
)

func traceBytes(tr []uint64) []byte {
	b := make([]byte, 0, len(tr)*8)
	for _, h := range tr {
		b = binary.LittleEndian.AppendUint64(b, h)
	}
	return b
}

func runKAT() []uint64 {
	return RunHashTrace(katSeed, katN, ScriptedCommands(katSeed, 300), katTicks, katEvery)
}

// The permanent reproducibility fixture (determinism.md §3): 10,000
// ticks, fixed seed, scripted command stream, hash every 100 ticks.
// 10 in-process runs must be bit-identical; the printed sha256 is the
// value to compare across -race, -gcflags="all=-N -l", and the CI
// matrix (#115/#116).
func TestDeterminism10kTickTrace(t *testing.T) {
	ref := runKAT()
	if len(ref) != katTicks/katEvery {
		t.Fatalf("trace has %d entries, want %d", len(ref), katTicks/katEvery)
	}
	t.Logf("trace head: [0]=0x%016X [1]=0x%016X [2]=0x%016X", ref[0], ref[1], ref[2])
	t.Logf("trace tail: [97]=0x%016X [98]=0x%016X [99]=0x%016X", ref[97], ref[98], ref[99])
	t.Logf("trace sha256: %x", sha256.Sum256(traceBytes(ref)))

	const runs = 10
	for r := 1; r < runs; r++ {
		got := runKAT()
		if idx := FirstDivergentEntry(ref, got); idx != -1 {
			t.Fatalf("run %d diverged at entry %d (tick %d): 0x%016X != 0x%016X",
				r, idx, (idx+1)*katEvery, got[idx], ref[idx])
		}
	}
	t.Logf("%d in-process runs bit-identical (100 entries each)", runs)
}

// Edge: a different seed must diverge at entry 0 — the seed defines
// the entire run.
func TestSeedChangeDivergesAtEntryZero(t *testing.T) {
	a := runKAT()
	b := RunHashTrace(katSeed+1, katN, ScriptedCommands(katSeed+1, 300), katTicks, katEvery)
	idx := FirstDivergentEntry(a, b)
	t.Logf("seed 0x%X vs 0x%X: first divergent entry = %d (a[0]=0x%016X b[0]=0x%016X)",
		uint64(katSeed), uint64(katSeed+1), idx, a[0], b[0])
	if idx != 0 {
		t.Fatalf("seed change should diverge at entry 0, got %d", idx)
	}
}

// Edge: window localization — perturb one command in the (5000,5100]
// tick window; traces must be identical through entry 49 and diverge
// exactly at entry 50.
func TestCommandPerturbationLocalizedToWindow(t *testing.T) {
	cmds := ScriptedCommands(katSeed, 300)
	perturbed := make([]Command, len(cmds))
	copy(perturbed, cmds)
	pi := -1
	for i, c := range perturbed {
		if c.Tick > 5000 && c.Tick <= 5100 {
			pi = i
			break
		}
	}
	if pi == -1 {
		t.Fatal("no command in (5000,5100] — adjust stream length")
	}
	t.Logf("perturbing command %d at tick %d: %+v -> damage A=%d B=1999",
		pi, perturbed[pi].Tick, perturbed[pi], perturbed[pi].A)
	perturbed[pi].Kind = cmdDamage
	perturbed[pi].B = 1999

	a := RunHashTrace(katSeed, katN, cmds, katTicks, katEvery)
	b := RunHashTrace(katSeed, katN, perturbed, katTicks, katEvery)
	idx := FirstDivergentEntry(a, b)
	t.Logf("first divergent entry = %d (entry 49 covers ticks 4901-5000, entry 50 covers 5001-5100)", idx)
	t.Logf("entry 49: a=0x%016X b=0x%016X (must match)", a[49], b[49])
	t.Logf("entry 50: a=0x%016X b=0x%016X (must differ)", a[50], b[50])
	if idx != 50 {
		t.Fatalf("perturbation at tick %d should first show at entry 50, got %d", perturbed[pi].Tick, idx)
	}
}

// Sub-hash bisect on the full workload: two worlds, one gets an extra
// hp nudge — the divergence report must name "units" first.
func TestSubHashNamesDivergentSystem(t *testing.T) {
	cmds := ScriptedCommands(katSeed, 50)
	wa := NewDetWorld(katSeed, katN, cmds)
	wb := NewDetWorld(katSeed, katN, cmds)
	for i := 0; i < 500; i++ {
		wa.Step()
		wb.Step()
	}
	wb.ents[7].hp += 1 // one raw fixed-point LSB
	ha := *wa.Hash()
	haCopy := ha
	haCopy.Subs = append([]uint64{}, ha.Subs...)
	hb := wb.Hash()
	name, diverged := wa.reg.FirstDivergence(&haCopy, hb)
	t.Logf("subs A=%016X B=%016X -> diverged=%v system=%q", haCopy.Subs, hb.Subs, diverged, name)
	if !diverged || name != "units" {
		t.Fatalf("want divergence at \"units\", got (%q, %v)", name, diverged)
	}
}
