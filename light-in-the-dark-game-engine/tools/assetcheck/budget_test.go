package main

// #31 triangle-budget FSV. SoT = assetcheck findings over synthetic GLB
// fixtures whose triangle counts we know exactly (we build the GLB JSON from
// accessor counts, so the count is X+X=Y verifiable: an indices accessor of
// 3T elements is exactly T triangles). We run the real check() and the real
// waiver evaluator, and read back the findings/notes.

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// triDoc builds a glTF doc for one indexed triangle-list mesh with exactly
// `tris` triangles (indices accessor.count = 3*tris).
func triDoc(tris int) map[string]any {
	return map[string]any{
		"asset": map[string]any{"version": "2.0"},
		"accessors": []map[string]any{
			{"count": tris * 3}, // 0: indices  -> tris triangles
			{"count": tris},     // 1: POSITION (one vertex per triangle, value irrelevant)
		},
		"meshes": []map[string]any{
			{"primitives": []map[string]any{
				{"indices": 0, "attributes": map[string]any{"POSITION": 1}},
			}},
		},
	}
}

// budgetFixture is a MANIFEST+files fixture whose entries carry a category.
type budgetFixture struct {
	dir string
	man bytes.Buffer
}

func newBudgetFixture(t *testing.T) *budgetFixture {
	f := &budgetFixture{dir: t.TempDir()}
	f.man.WriteString("# budget test ledger\n")
	return f
}

func (f *budgetFixture) add(t *testing.T, rel string, content []byte, category string) {
	t.Helper()
	p := filepath.Join(f.dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, content, 0o644); err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(content)
	fmt.Fprintf(&f.man, "[[asset]]\npath = %q\npack = \"T\"\nsource = \"https://example.com\"\nlicense = \"CC0-1.0\"\nretrieved = \"2026-06-11\"\nsha256 = %q\n", rel, hex.EncodeToString(sum[:]))
	if category != "" {
		fmt.Fprintf(&f.man, "category = %q\n", category)
	}
}

func (f *budgetFixture) run(t *testing.T, ws waiverSet) ([]finding, []string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(f.dir, "MANIFEST"), f.man.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(f.dir)
	if err != nil {
		t.Fatal(err)
	}
	return check(f.dir, files, "", ws)
}

func onlyBudget(fs []finding) []finding {
	var out []finding
	for _, fd := range fs {
		if len(fd.Rule) >= 6 && fd.Rule[:6] == "BUDGET" {
			out = append(out, fd)
		}
	}
	return out
}

// TestTriangleCountFromGLB cross-checks the parser's triangle count against the
// known accessor counts we wrote — independent of the budget logic.
func TestTriangleCountFromGLB(t *testing.T) {
	cases := []struct {
		name string
		doc  map[string]any
		want int
	}{
		{"indexed-1500", triDoc(1500), 1500},
		{"non-indexed", map[string]any{
			"asset":     map[string]any{"version": "2.0"},
			"accessors": []map[string]any{{"count": 30}},
			"meshes": []map[string]any{{"primitives": []map[string]any{
				{"attributes": map[string]any{"POSITION": 0}}, // no indices -> 30/3 = 10
			}}},
		}, 10},
		{"triangle-strip", map[string]any{
			"asset":     map[string]any{"version": "2.0"},
			"accessors": []map[string]any{{"count": 12}},
			"meshes": []map[string]any{{"primitives": []map[string]any{
				{"mode": 5, "attributes": map[string]any{"POSITION": 0}}, // strip: 12-2 = 10
			}}},
		}, 10},
		{"points-mode-zero", map[string]any{
			"asset":     map[string]any{"version": "2.0"},
			"accessors": []map[string]any{{"count": 99}},
			"meshes": []map[string]any{{"primitives": []map[string]any{
				{"mode": 0, "attributes": map[string]any{"POSITION": 0}}, // POINTS: 0 triangles
			}}},
		}, 0},
	}
	for _, c := range cases {
		dir := t.TempDir()
		p := filepath.Join(dir, "m.glb")
		if err := os.WriteFile(p, buildGLB(t, c.doc), 0o644); err != nil {
			t.Fatal(err)
		}
		info, err := parseGLB(p)
		if err != nil {
			t.Fatalf("%s: parseGLB: %v", c.name, err)
		}
		t.Logf("FSV tri-count %s: got=%d want=%d", c.name, info.Triangles, c.want)
		if info.Triangles != c.want {
			t.Fatalf("%s: triangle count %d, want %d", c.name, info.Triangles, c.want)
		}
	}
}

// TestBudgetBoundaries — the four boundary fixtures from the issue: 1500 passes,
// 1501 fails; 4000 passes, 4001 fails. SoT = budget findings.
func TestBudgetBoundaries(t *testing.T) {
	type tc struct {
		rel     string
		cat     string
		tris    int
		wantBad bool
	}
	cases := []tc{
		{"units/exact.glb", "unit", 1500, false},
		{"units/over.glb", "unit", 1501, true},
		{"buildings/exact.glb", "building", 4000, false},
		{"buildings/over.glb", "building", 4001, true},
	}
	for _, c := range cases {
		f := newBudgetFixture(t)
		f.add(t, c.rel, buildGLB(t, triDoc(c.tris)), c.cat)
		raw, _ := f.run(t, newWaiverSet())
		got := onlyBudget(raw)
		t.Logf("FSV boundary %s (%s @ %d tris): findings=%v", c.rel, c.cat, c.tris, got)
		if c.wantBad {
			if len(got) != 1 || got[0].Rule != "BUDGET-OVER" || got[0].Path != c.rel {
				t.Fatalf("%s: want one BUDGET-OVER, got %v", c.rel, got)
			}
		} else if len(got) != 0 {
			t.Fatalf("%s: want pass at boundary, got %v", c.rel, got)
		}
	}
}

// TestBudgetUncategorizedAndUnknown — geometry without a category is a finding;
// an unknown category is a distinct finding. Neither is a silent pass.
func TestBudgetUncategorizedAndUnknown(t *testing.T) {
	f := newBudgetFixture(t)
	f.add(t, "units/nocat.glb", buildGLB(t, triDoc(10)), "")        // categorized = ""
	f.add(t, "units/bogus.glb", buildGLB(t, triDoc(10)), "untis")   // typo'd category
	f.add(t, "props/free.glb", buildGLB(t, triDoc(99999)), "other") // unbounded category passes
	raw, _ := f.run(t, newWaiverSet())
	got := onlyBudget(raw)
	t.Logf("FSV uncategorized/unknown: findings=%v", got)
	if len(got) != 2 {
		t.Fatalf("want 2 findings (uncategorized + unknown), got %v", got)
	}
	byPath := map[string]string{}
	for _, fd := range got {
		byPath[fd.Path] = fd.Rule
	}
	if byPath["units/nocat.glb"] != "BUDGET-UNCATEGORIZED" {
		t.Fatalf("nocat should be BUDGET-UNCATEGORIZED, got %v", got)
	}
	if byPath["units/bogus.glb"] != "BUDGET-CATEGORY" {
		t.Fatalf("bogus should be BUDGET-CATEGORY, got %v", got)
	}
}

// TestBudgetWaiverPassThenExpire — the headline waiver case: an over-budget
// unit passes WITH a named waiver while current<=expiry, and fails again once
// the current milestone is past expiry. BEFORE/AFTER printed for each.
func TestBudgetWaiverPassThenExpire(t *testing.T) {
	f := newBudgetFixture(t)
	f.add(t, "units/dragon.glb", buildGLB(t, triDoc(1501)), "unit")

	// valid: current M1, waiver until M3.
	wsValid := newWaiverSet()
	wsValid.current, wsValid.haveCurrent = "M1", true
	wsValid.byPath["units/dragon.glb"] = waiver{Path: "units/dragon.glb", Reason: "hero, decimation pending #999", Expiry: "M3", Line: 1}
	got, notes := f.run(t, wsValid)
	gb := onlyBudget(got)
	t.Logf("FSV waiver valid (current M1, expiry M3): budgetFindings=%v notes=%v", gb, notes)
	if len(gb) != 0 {
		t.Fatalf("valid waiver should pass, got findings %v", gb)
	}
	if len(notes) != 1 || !bytes.Contains([]byte(notes[0]), []byte("units/dragon.glb")) || !bytes.Contains([]byte(notes[0]), []byte("decimation pending")) {
		t.Fatalf("expected a WAIVED note naming the asset+reason, got %v", notes)
	}

	// expired: current M4, waiver only until M3.
	wsExpired := newWaiverSet()
	wsExpired.current, wsExpired.haveCurrent = "M4", true
	wsExpired.byPath["units/dragon.glb"] = waiver{Path: "units/dragon.glb", Reason: "hero, decimation pending #999", Expiry: "M3", Line: 1}
	got2, notes2 := f.run(t, wsExpired)
	gb2 := onlyBudget(got2)
	t.Logf("FSV waiver expired (current M4, expiry M3): budgetFindings=%v notes=%v", gb2, notes2)
	if len(gb2) != 1 || gb2[0].Rule != "BUDGET-OVER" || !bytes.Contains([]byte(gb2[0].Msg), []byte("expired")) {
		t.Fatalf("expired waiver should fail with BUDGET-OVER mentioning expiry, got %v", gb2)
	}
	if len(notes2) != 0 {
		t.Fatalf("expired waiver should emit no WAIVED note, got %v", notes2)
	}
}

// TestLoadWaiversRoundTrip parses a real waivers.toml fixture and verifies the
// parsed set drives the gate — proving the file format, not just the struct.
func TestLoadWaiversRoundTrip(t *testing.T) {
	dir := t.TempDir()
	wp := filepath.Join(dir, "waivers.toml")
	body := `# test waivers
current_milestone = "M2"

[[waiver]]
path = "units/dragon.glb"   # inline comment tolerated
reason = "art sign-off pending"
expiry = "M5"
`
	if err := os.WriteFile(wp, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	ws, err := loadWaivers(wp)
	if err != nil {
		t.Fatalf("loadWaivers: %v", err)
	}
	t.Logf("FSV parsed waivers: current=%q haveCurrent=%v entries=%v", ws.current, ws.haveCurrent, ws.byPath)
	if ws.current != "M2" || !ws.haveCurrent {
		t.Fatalf("current_milestone not parsed: %+v", ws)
	}
	w, ok := ws.byPath["units/dragon.glb"]
	if !ok || w.Reason != "art sign-off pending" || w.Expiry != "M5" {
		t.Fatalf("waiver entry not parsed: %+v", ws.byPath)
	}

	// malformed: missing expiry must fail closed.
	bad := filepath.Join(dir, "bad.toml")
	if err := os.WriteFile(bad, []byte("[[waiver]]\npath = \"x.glb\"\nreason = \"r\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := loadWaivers(bad); err == nil {
		t.Fatal("waiver missing expiry should be a parse error")
	}
}
