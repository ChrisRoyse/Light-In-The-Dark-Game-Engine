package worldarchive

// #424 FSV: worldarchive.Open must enforce the PER-CATEGORY triangle budget on
// embedded .glb entries at load — not just the category-independent absolute
// ceiling (#411). The category travels in the v2 manifest row. SoT = the Open
// error string (refusal) and, on success, the parsed Manifest.Files[...].Category.
//
// packDirV2 is an INDEPENDENT v2 archive writer (deliberately NOT worldpack, so a
// writer bug cannot mask a load-time verification bug — same discipline as
// packDir in worldarchive_test.go). It hashes whatever is staged, so every
// archive it produces is hash-valid: only the catalog/budget can reject content.

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// triGLB builds a synthetic core-profile GLB whose mesh declares exactly `tris`
// triangles: one indexed TRIANGLES primitive over an index accessor of count
// 3*tris (trianglesForMode(4, 3*tris) == tris). No BIN chunk is needed — the
// catalog counts triangles from the accessor's declared count.
func triGLB(t *testing.T, tris int) []byte {
	return glbBytes(t, map[string]any{
		"asset":     map[string]any{"version": "2.0"},
		"meshes":    []any{map[string]any{"primitives": []any{map[string]any{"indices": 0}}}},
		"accessors": []any{map[string]any{"count": 3 * tris}},
	})
}

// packDirV2 writes a deterministic v2 archive of srcDir, stamping each row's
// category from cats (rel → category); a rel absent from cats gets "-". Independent
// of worldpack.
func packDirV2(t *testing.T, srcDir, out, engineRange string, cats map[string]string) {
	t.Helper()
	type ent struct {
		rel, hash, cat string
		size           int64
		body           []byte
	}
	var ents []ent
	err := filepath.WalkDir(srcDir, func(p string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		b, e := os.ReadFile(p)
		if e != nil {
			return e
		}
		rel, _ := filepath.Rel(srcDir, p)
		rel = filepath.ToSlash(rel)
		sum := sha256.Sum256(b)
		cat := cats[rel]
		if cat == "" {
			cat = "-"
		}
		ents = append(ents, ent{rel, hex.EncodeToString(sum[:]), cat, int64(len(b)), b})
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Slice(ents, func(i, j int) bool { return ents[i].rel < ents[j].rel })

	agg := sha256.New()
	for _, e := range ents {
		agg.Write([]byte(e.hash + "\n"))
	}

	var man strings.Builder
	man.WriteString("litdworld-version: 2\n")
	fmt.Fprintf(&man, "engine-range: %s\n", engineRange)
	man.WriteString("author: Light in the Dark\n")
	man.WriteString("title: First Flame\n")
	man.WriteString("description: ashen-veil duel\n")
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	fmt.Fprintf(&man, "files: %d\n", len(ents))
	for _, e := range ents {
		fmt.Fprintf(&man, "%s %d %s %s\n", e.hash, e.size, e.cat, e.rel)
	}

	f, err := os.Create(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	mw, _ := zw.Create(manifestName)
	mw.Write([]byte(man.String()))
	for _, e := range ents {
		w, _ := zw.Create(e.rel)
		w.Write(e.body)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestArchiveCategoryBudgetFSV(t *testing.T) {
	const glbRel = "assets/model.glb"
	model := triGLB(t, 2000) // 2,000 tris: over the 1,500 unit budget, under 4,000 building/absolute

	stageWith := func() string {
		stage := stageFirstFlame(t)
		if err := os.MkdirAll(filepath.Join(stage, "assets"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(stage, filepath.FromSlash(glbRel)), model, 0o644); err != nil {
			t.Fatal(err)
		}
		return stage
	}

	// --- Happy path / edge matrix. input → expected SoT (Open outcome). ---
	cases := []struct {
		name       string
		category   string
		version2   bool
		wantRefuse string // substring the Open error MUST contain; "" = must open
	}{
		{"unit-over-budget", "unit", true, "BUDGET-OVER"},      // 2000 > 1500 → refuse
		{"building-within-budget", "building", true, ""},       // 2000 <= 4000 → open
		{"other-unbounded", "other", true, ""},                 // unbounded → open
		{"unknown-category", "bogus", true, "BUDGET-CATEGORY"}, // fail-closed on unknown
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			stage := stageWith()
			arc := filepath.Join(t.TempDir(), tc.name+".litdworld")
			packDirV2(t, stage, arc, "*", map[string]string{glbRel: tc.category})

			a, err := Open(arc, "")
			if tc.wantRefuse != "" {
				if err == nil {
					a.Close()
					t.Fatalf("BEFORE: archive with a 2000-tri %q model; AFTER: Open succeeded — expected refusal containing %q", tc.category, tc.wantRefuse)
				}
				if !strings.Contains(err.Error(), "fails asset catalog") || !strings.Contains(err.Error(), tc.wantRefuse) {
					t.Fatalf("expected %q catalog refusal, got: %v", tc.wantRefuse, err)
				}
				t.Logf("FSV: 2000-tri %q model REFUSED at Open — %v", tc.category, err)
				return
			}
			if err != nil {
				t.Fatalf("BEFORE: 2000-tri %q model (within budget); AFTER: Open refused it: %v", tc.category, err)
			}
			defer a.Close()
			// SoT cross-check: the parsed manifest carries the category we stamped.
			got := a.Manifest.Files[glbRel].Category
			if got != tc.category {
				t.Fatalf("manifest category for %s = %q, want %q", glbRel, got, tc.category)
			}
			t.Logf("FSV: 2000-tri %q model OPENED; Manifest.Files[%q].Category=%q", tc.category, glbRel, got)
		})
	}

	// --- Back-compat edge: a v1 archive carries no category column. The same
	// 2000-tri model must still OPEN (only the absolute ceiling applies at load;
	// per-category enforcement requires a v2 category). Proves the version gate. ---
	t.Run("v1-backcompat-no-category", func(t *testing.T) {
		stage := stageWith()
		arc := filepath.Join(t.TempDir(), "v1.litdworld")
		packDir(t, stage, arc, "*", "") // v1 writer: 3-field rows, no category
		a, err := Open(arc, "")
		if err != nil {
			t.Fatalf("v1 archive with a 2000-tri model must open (no category to enforce), got: %v", err)
		}
		defer a.Close()
		if got := a.Manifest.Files[glbRel].Category; got != "" {
			t.Fatalf("v1 archive entry category = %q, want \"\" (no column)", got)
		}
		t.Logf("FSV back-compat: v1 archive (no category column) opens; category parsed as \"\"")
	})
}
