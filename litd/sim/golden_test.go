package sim

import (
	"bufio"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// -update regenerates the golden trace. REGENERATION IS A
// FORMAT-BREAK TRIPWIRE (determinism.md §2.4): after M3 a golden
// change is effectively a save/replay-format break. Never regenerate
// without a recorded justification in the PR description — the whole
// point of committing the trace is that the diff is visible in
// review.
var update = flag.Bool("update", false, "regenerate litd/sim/testdata/golden-10ktick.trace")

const goldenPath = "testdata/golden-10ktick.trace"

func goldenHeader() string {
	return fmt.Sprintf(`# golden 10k-tick hash trace — determinism.md §3, frozen per §2.4
# workload: NewDetWorld(seed=0x%X, n=%d, ScriptedCommands(seed, 300)); %d ticks; top hash every %d
# one entry per line: <entryIndex> <topHash hex>
# regenerate ONLY with: go test ./litd/sim -run Golden -args -update  (justification required in PR)
`, uint64(katSeed), katN, katTicks, katEvery)
}

func writeGolden(t *testing.T, trace []uint64) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
		t.Fatal(err)
	}
	var sb strings.Builder
	sb.WriteString(goldenHeader())
	for i, h := range trace {
		fmt.Fprintf(&sb, "%d %016X\n", i, h)
	}
	if err := os.WriteFile(goldenPath, []byte(sb.String()), 0o644); err != nil {
		t.Fatal(err)
	}
}

func readGolden(t *testing.T) []uint64 {
	t.Helper()
	f, err := os.Open(goldenPath)
	if err != nil {
		t.Fatalf("golden trace missing (%v) — generate once with -update and commit it", err)
	}
	defer f.Close()
	var trace []uint64
	sc := bufio.NewScanner(f)
	line := 0
	for sc.Scan() {
		line++
		text := strings.TrimSpace(sc.Text())
		if text == "" || strings.HasPrefix(text, "#") {
			continue
		}
		fields := strings.Fields(text)
		if len(fields) != 2 {
			t.Fatalf("golden line %d malformed: %q", line, text)
		}
		idx, err1 := strconv.Atoi(fields[0])
		h, err2 := strconv.ParseUint(fields[1], 16, 64)
		if err1 != nil || err2 != nil || idx != len(trace) {
			t.Fatalf("golden line %d malformed or out of order: %q", line, text)
		}
		trace = append(trace, h)
	}
	if err := sc.Err(); err != nil {
		t.Fatal(err)
	}
	return trace
}

// TestGolden10kTickTrace compares the live 10k-tick trace against the
// committed reference. This is the cross-arch tripwire: the same
// golden file must validate on every machine, OS, arch, and toolchain
// — any behavioral drift in fixed/prng/sched/statehash trips it
// everywhere, not just against the machine's own previous run.
func TestGolden10kTickTrace(t *testing.T) {
	live := runKAT()
	if *update {
		writeGolden(t, live)
		t.Logf("golden regenerated: %d entries -> %s (JUSTIFICATION REQUIRED IN PR)", len(live), goldenPath)
		return
	}
	want := readGolden(t)
	if idx := FirstDivergentEntry(want, live); idx != -1 {
		var got, exp uint64
		if idx < len(live) {
			got = live[idx]
		}
		if idx < len(want) {
			exp = want[idx]
		}
		t.Fatalf("golden divergence at entry %d (ticks %d-%d): got 0x%016X want 0x%016X\n"+
			"if this change is intentional it is a determinism-contract break: regenerate with -update and justify in the PR",
			idx, idx*katEvery+1, (idx+1)*katEvery, got, exp)
	}
	t.Logf("live trace matches committed golden: %d entries, golden[0]=0x%016X golden[99]=0x%016X",
		len(want), want[0], want[99])
}
