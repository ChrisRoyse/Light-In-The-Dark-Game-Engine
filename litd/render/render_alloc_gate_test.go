package render

import (
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// #541 static tier — protects the three per-frame allocations eliminated under
// #537 (patches/engine/0012-0014). Each allocation is a compile-time property, so
// the Go escape analyzer is the source of truth — no GL context needed.
//
// We run `go build -gcflags='<g3n renderer>=-m'` and assert the renderer's hot
// Render path is alloc-free for these three constructs:
//
//	0012  per-panel GraphicMaterial copy   signature: `moved to heap: mat`
//	0013  per-frame zLayers map realloc    `make(map[int][]gui.IPanel) escapes to heap` INSIDE Render
//	0014  per-frame cull frustum rebuild   `make([]math32.Plane, 6) escapes to heap`  INSIDE Render
//
// The panel check is a plain string-absence test: that variable name is unique to
// the removed loop-local copy (the constructor has no `mat`, renderGraphicMaterial's
// `mat` does not escape). The map and frustum allocations legitimately still occur
// ONCE in NewRenderer (the reused buffers are allocated there), so a string-absence
// test would false-fire; instead we anchor on Render's dynamically-computed line
// span and assert neither make() escapes from a line INSIDE Render. A regression
// (reverting 0013/0014) moves the make() back inside Render → the line falls in the
// span → RED. Robust to line shifts because the span is recomputed from source.
//
// Cache discipline: Go's build cache is content-hash keyed and replays/suppresses
// cached -m diagnostics, so a warm cache can emit nothing and a naive check would
// false-pass (fail-open). We force a COLD GOCACHE so the renderer is genuinely
// recompiled and -m is guaranteed to print, then assert non-vacuously that
// renderer.go escape diagnostics actually appeared before checking the signatures.
//
// Heavy (cold-cache recompile ~13s) — skipped under -short; FULL preflight runs it.
func TestRendererRenderNoEscapeGateFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping cold-cache escape-analysis gate in -short")
	}

	const pkg = "github.com/g3n/engine/renderer"
	root := moduleRoot(t)
	src := filepath.Join(root, "repoes", "engine", "renderer", "renderer.go")

	// --- run escape analysis under a cold cache ---
	cmd := exec.Command("go", "build", "-gcflags="+pkg+"=-m", pkg)
	cmd.Env = append(os.Environ(), "GOCACHE="+t.TempDir())
	out, err := cmd.CombinedOutput()
	text := string(out)
	if err != nil {
		t.Fatalf("escape-analysis build of %s failed (gate must run on a buildable tree): %v\n%s", pkg, err, clip(text))
	}
	// Non-vacuity: -m must actually have analyzed renderer.go.
	if !strings.Contains(text, "renderer.go") || !strings.Contains(text, "escapes to heap") {
		t.Fatalf("escape analysis produced no renderer.go diagnostics — gate is vacuous "+
			"(cold cache may not have triggered -m). Output:\n%s", clip(text))
	}

	// --- 0012: per-panel material copy must not escape at all ---
	const panelSig = "moved to heap: mat"
	if hits := grepLines(text, panelSig); len(hits) > 0 {
		t.Fatalf("REGRESSION (#537/#541, patch 0012): %q reappeared — the per-panel GraphicMaterial copy\n"+
			"is allocating on the heap again. Sites:\n  %s", panelSig, strings.Join(hits, "\n  "))
	}

	// --- 0013 / 0014: these make() escapes are allowed in NewRenderer but NOT inside Render ---
	renderStart, renderEnd := funcSpan(t, src, "func (r *Renderer) Render(")
	escRe := regexp.MustCompile(`renderer\.go:(\d+):\d+: (make\(map\[int\]\[\]gui\.IPanel\)|make\(\[\]math32\.Plane, 6\)) escapes to heap`)
	for _, m := range escRe.FindAllStringSubmatch(text, -1) {
		line, _ := strconv.Atoi(m[1])
		if line >= renderStart && line < renderEnd {
			patch := "0013"
			if strings.Contains(m[2], "Plane") {
				patch = "0014"
			}
			t.Fatalf("REGRESSION (#537/#541, patch %s): `%s` escapes to heap at renderer.go:%d, which is INSIDE\n"+
				"Render (lines %d-%d) — a per-frame allocation is back. It must be allocated once in NewRenderer\n"+
				"and reused, not rebuilt each frame.", patch, m[2], line, renderStart, renderEnd-1)
		}
	}
	t.Logf("FSV #541: Render (lines %d-%d) is free of the three #537 per-frame allocations "+
		"(panel copy, zLayers map, cull frustum). Allowed constructor-time allocs untouched.", renderStart, renderEnd-1)

	// Belt-and-suspenders: the tracked fix patches must still exist so a fresh
	// checkout's restore-repoes.sh reapplies them.
	for _, p := range []string{
		"0012-renderer-panel-material-no-escape.patch",
		"0013-renderer-reuse-zlayers-map.patch",
		"0014-renderer-reuse-cull-frustum.patch",
	} {
		if _, statErr := os.Stat(filepath.Join(root, "patches", "engine", p)); statErr != nil {
			t.Fatalf("tracked fix patch %s missing (%v) — restore-repoes.sh would not reapply this #537 fix", p, statErr)
		}
	}
}

// funcSpan returns [startLine, endLine) (1-based) of the function whose declaration
// line begins with declPrefix, where endLine is the next top-level `func ` (or EOF).
func funcSpan(t *testing.T, srcPath, declPrefix string) (int, int) {
	t.Helper()
	b, err := os.ReadFile(srcPath)
	if err != nil {
		t.Fatalf("read %s: %v", srcPath, err)
	}
	lines := strings.Split(string(b), "\n")
	start := -1
	for i, ln := range lines {
		if strings.HasPrefix(ln, declPrefix) {
			start = i + 1 // 1-based
			break
		}
	}
	if start < 0 {
		t.Fatalf("could not find %q in %s — renderer layout changed; update the gate", declPrefix, srcPath)
	}
	for i := start; i < len(lines); i++ { // first top-level func after start
		if strings.HasPrefix(lines[i], "func ") {
			return start, i + 1
		}
	}
	return start, len(lines) + 1
}

func grepLines(text, needle string) []string {
	var hits []string
	for _, line := range strings.Split(text, "\n") {
		if strings.Contains(line, needle) {
			hits = append(hits, strings.TrimSpace(line))
		}
	}
	return hits
}

func clip(s string) string {
	const max = 2000
	if len(s) > max {
		return s[:max] + "\n...(truncated)"
	}
	return s
}

// moduleRoot walks up from the test's CWD to the dir holding go.mod.
func moduleRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if _, statErr := os.Stat(filepath.Join(dir, "go.mod")); statErr == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("go.mod not found above test CWD")
		}
		dir = parent
	}
}
