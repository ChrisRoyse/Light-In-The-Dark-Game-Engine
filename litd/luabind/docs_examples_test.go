package luabind

// FSV for #484: the public Trigger/ECA + combat-override modder docs are not
// prose-only — every runnable example in them is EXECUTED here against the real
// binding surface, every documented API symbol is checked to exist, and the
// generated event-coverage doc is checked against the E1 manifest. A doc that
// drifts from the engine fails this test loudly (no pseudo-code, no stale claim).
//
// Contract embedded in the markdown:
//   ```lua            → executed in a fresh sandbox; must run without error.
//   ```lua-norun      → symbol-linted only (illustrative: needs data-table wiring
//                       the runner does not stand up, e.g. a data-bound ability).
//   <!-- fsv <cat>.<key> == <int> [@<ticks>] -->   immediately before a ```lua
//                       block: after running it (and advancing <ticks>, default 0)
//                       Storage(cat,key) must equal <int> — the documented outcome.
// Every API-looking token (Foo_Bar( ) in a lua/lua-norun block must be a global
// registered by Register — the doc-symbol lint.

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

const docsDir = "../../docs/api"

// docsSandbox builds the standard game every runnable doc example runs against:
// hfoo (1000 life), a burn DoT buff, normal/holy attack types vs one armor type
// (normal 100%, holy 200%), and players 1 & 2 set as mutual enemies. Examples
// create their own units and write observed outcomes into Storage("doc", ...).
func docsSandbox(t *testing.T) (*api.Game, *lua.LState) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: 9})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 1000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "burn", DurationTicks: 60, Stacking: data.StackRefresh, MaxStacks: 1},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	if err := g.DefineDamageTypes([]string{"normal", "holy"}, []string{"unarmored"}); err != nil {
		t.Fatalf("DefineDamageTypes: %v", err)
	}
	if err := g.DefineCombat([][]int{{1000}, {2000}}); err != nil {
		t.Fatalf("DefineCombat: %v", err)
	}
	p1, p2 := g.Player(1), g.Player(2)
	p1.SetAlliance(p2, 0)
	p2.SetAlliance(p1, 0)
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L
}

// registeredGlobals returns the set of global names a fresh sandbox exposes.
func registeredGlobals(t *testing.T) map[string]bool {
	t.Helper()
	_, L := docsSandbox(t)
	defer L.Close()
	if err := L.DoString(`__names = {}; for k, v in pairs(_G) do __names[#__names+1] = k end`); err != nil {
		t.Fatalf("collect globals: %v", err)
	}
	tbl, ok := L.GetGlobal("__names").(*lua.LTable)
	if !ok {
		t.Fatal("globals table missing")
	}
	set := map[string]bool{}
	tbl.ForEach(func(_, v lua.LValue) { set[v.String()] = true })
	return set
}

type docBlock struct {
	lua      string
	run      bool
	line     int
	wantCat  string
	wantKey  string
	wantVal  int
	wantTick int
	hasWant  bool
}

var (
	fenceRe  = regexp.MustCompile("^```lua(-norun)?\\s*$")
	directRe = regexp.MustCompile(`<!--\s*fsv\s+(\w+)\.(\w+)\s*==\s*(-?\d+)\s*(?:@(\d+))?\s*-->`)
	symRe    = regexp.MustCompile(`\b([A-Z][A-Za-z0-9]*_[A-Za-z0-9]+)\s*\(`)
)

// parseDocBlocks extracts the fenced lua blocks (and the fsv directive that may
// precede them) from a markdown file.
func parseDocBlocks(t *testing.T, path string) []docBlock {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	lines := strings.Split(string(raw), "\n")
	var blocks []docBlock
	var pending *docBlock
	for i := 0; i < len(lines); i++ {
		if m := directRe.FindStringSubmatch(lines[i]); m != nil {
			val, _ := strconv.Atoi(m[3])
			tick, _ := strconv.Atoi(m[4])
			pending = &docBlock{hasWant: true, wantCat: m[1], wantKey: m[2], wantVal: val, wantTick: tick}
			continue
		}
		if m := fenceRe.FindStringSubmatch(lines[i]); m != nil {
			b := docBlock{run: m[1] == "", line: i + 1}
			if pending != nil {
				b.hasWant, b.wantCat, b.wantKey, b.wantVal, b.wantTick = pending.hasWant, pending.wantCat, pending.wantKey, pending.wantVal, pending.wantTick
			}
			var body []string
			for i++; i < len(lines) && !strings.HasPrefix(lines[i], "```"); i++ {
				body = append(body, lines[i])
			}
			b.lua = strings.Join(body, "\n")
			blocks = append(blocks, b)
			pending = nil
			continue
		}
		// A non-blank, non-directive line between a directive and its block voids
		// the directive (keeps the contract tight).
		if pending != nil && strings.TrimSpace(lines[i]) != "" {
			pending = nil
		}
	}
	return blocks
}

func TestDocExamplesRunAndLintFSV(t *testing.T) {
	globals := registeredGlobals(t)
	docs := []string{"triggers.md", "combat-overrides.md", "event-coverage.md"}
	totalRun, totalWant := 0, 0
	for _, doc := range docs {
		path := filepath.Join(docsDir, doc)
		blocks := parseDocBlocks(t, path)
		for _, b := range blocks {
			// Doc-symbol lint: every API-looking call must resolve to a real global.
			for _, m := range symRe.FindAllStringSubmatch(b.lua, -1) {
				sym := m[1]
				if !globals[sym] {
					t.Errorf("%s:%d doc-symbol %q is not a registered API global", doc, b.line, sym)
				}
			}
			if !b.run {
				continue
			}
			totalRun++
			g, L := docsSandbox(t)
			if err := L.DoString(b.lua); err != nil {
				L.Close()
				t.Errorf("%s:%d example failed to run: %v", doc, b.line, err)
				continue
			}
			if b.hasWant {
				totalWant++
				g.Advance(b.wantTick)
				got, _ := g.Storage().GetInt(b.wantCat, b.wantKey)
				if got != b.wantVal {
					t.Errorf("%s:%d outcome %s.%s = %d, doc says %d",
						doc, b.line, b.wantCat, b.wantKey, got, b.wantVal)
				} else {
					t.Logf("%s:%d ✓ %s.%s == %d (after %d ticks)", doc, b.line, b.wantCat, b.wantKey, got, b.wantTick)
				}
			}
			L.Close()
		}
	}
	t.Logf("#484 docs runner: %d runnable examples executed, %d outcome-verified", totalRun, totalWant)
	if totalRun == 0 {
		t.Fatal("no runnable examples found — docs missing or contract broken")
	}
}

// TestEventCoverageDocMatchesManifestFSV — docs/api/event-coverage.md is generated
// from docs/api/event-coverage.json (#466 / E1). This regenerates the expected
// body and asserts the committed doc matches: a drift in either the manifest or
// the doc fails loudly (the "coverage count == manifest count" edge).
func TestEventCoverageDocMatchesManifestFSV(t *testing.T) {
	want := renderEventCoverage(t)
	got, err := os.ReadFile(filepath.Join(docsDir, "event-coverage.md"))
	if err != nil {
		t.Fatalf("read event-coverage.md: %v", err)
	}
	if string(got) != want {
		t.Fatalf("event-coverage.md is stale — regenerate from event-coverage.json (run TestEventCoverageDocMatchesManifestFSV with -update is not wired; see renderEventCoverage). First diff:\n%s",
			firstDiff(string(got), want))
	}
	t.Logf("#484 event-coverage.md matches manifest")
}

type ecManifest struct {
	Total      int            `json:"total"`
	Mapped     int            `json:"mapped"`
	Tombstoned int            `json:"tombstoned"`
	Families   map[string]int `json:"families"`
	Events     []struct {
		Name   string `json:"name"`
		Family string `json:"family"`
		Raw    int    `json:"raw"`
		Status string `json:"status"`
		Reason string `json:"reason"`
	} `json:"events"`
}

func loadECManifest(t *testing.T) ecManifest {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(docsDir, "event-coverage.json"))
	if err != nil {
		t.Fatalf("read event-coverage.json: %v", err)
	}
	var m ecManifest
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("parse event-coverage.json: %v", err)
	}
	return m
}

// renderEventCoverage builds the canonical event-coverage.md body from the
// manifest. Deterministic (events are already sorted in the manifest).
func renderEventCoverage(t *testing.T) string {
	m := loadECManifest(t)
	var b strings.Builder
	b.WriteString("# Event coverage\n\n")
	b.WriteString("<!-- GENERATED from docs/api/event-coverage.json by jassgen -eventcov (#466).\n")
	b.WriteString("     Do not edit by hand; TestEventCoverageDocMatchesManifestFSV (#484) enforces parity. -->\n\n")
	b.WriteString(fmt.Sprintf("Every WC3 `EVENT_*` constant is accounted for: **%d total**, **%d mapped** to a LitD event kind, **%d tombstoned** (out of deterministic-sim scope, with a reason). Source of truth: `repoes/war3-types/scripts/common.j`.\n\n",
		m.Total, m.Mapped, m.Tombstoned))
	b.WriteString("## By family\n\n")
	b.WriteString("| Family | Count |\n|---|---|\n")
	for _, fam := range sortedKeys(m.Families) {
		b.WriteString(fmt.Sprintf("| %s | %d |\n", fam, m.Families[fam]))
	}
	b.WriteString("\n## Every event\n\n")
	b.WriteString("| EVENT_ | Family | Status | Notes |\n|---|---|---|---|\n")
	for _, e := range m.Events {
		note := e.Reason
		if e.Status == "mapped" && note == "" {
			note = "mapped to a LitD event kind"
		}
		b.WriteString(fmt.Sprintf("| `%s` | %s | %s | %s |\n", e.Name, e.Family, e.Status, note))
	}
	return b.String()
}

func sortedKeys(m map[string]int) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	// small, stable insertion sort (no map-iteration order dependence)
	for i := 1; i < len(ks); i++ {
		for j := i; j > 0 && ks[j] < ks[j-1]; j-- {
			ks[j], ks[j-1] = ks[j-1], ks[j]
		}
	}
	return ks
}

func firstDiff(a, b string) string {
	la, lb := strings.Split(a, "\n"), strings.Split(b, "\n")
	for i := 0; i < len(la) || i < len(lb); i++ {
		var x, y string
		if i < len(la) {
			x = la[i]
		}
		if i < len(lb) {
			y = lb[i]
		}
		if x != y {
			return fmt.Sprintf("line %d:\n  committed: %q\n  expected:  %q", i+1, x, y)
		}
	}
	return "(no line diff; trailing bytes differ)"
}
