package render

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// #541 static tier (protects the dominant #537 fix, patches/engine/0012): the
// per-panel GraphicMaterial copy that escaped to the heap once per panel per frame
// must stay eliminated. The allocation is a compile-time property, so the Go escape
// analyzer is the source of truth — no GL context needed.
//
// We run `go build -gcflags='<g3n renderer>=-m'` and assert the signature
// `moved to heap: mat` (the removed loop-local panel copy) never reappears. That
// variable name is unique to the deleted construct — the constructor has no `mat`,
// and renderGraphicMaterial's `mat` does not escape — so its presence is a precise
// regression signal.
//
// Cache discipline: the Go build cache is content-hash keyed and REPLAYS or
// suppresses cached -m diagnostics, so a warm cache can emit nothing and a naive
// "string absent" check would false-pass (a fail-open the gate must avoid). We
// force a COLD cache via a throwaway GOCACHE so the renderer is genuinely
// recompiled and -m is guaranteed to print. The test is non-vacuous: it first
// asserts the build succeeded and that escape diagnostics for renderer.go actually
// appeared, so an empty/failed run fails RED rather than trivially passing.
//
// Heavy (cold-cache recompile of the renderer + deps) — skipped under -short; the
// FULL preflight gate runs it.
func TestRendererPanelMaterialNoEscapeGateFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping cold-cache escape-analysis gate in -short")
	}

	const pkg = "github.com/g3n/engine/renderer"
	coldCache := t.TempDir()

	cmd := exec.Command("go", "build", "-gcflags="+pkg+"=-m", pkg)
	cmd.Env = append(os.Environ(), "GOCACHE="+coldCache)
	out, err := cmd.CombinedOutput()
	text := string(out)
	if err != nil {
		t.Fatalf("escape-analysis build of %s failed (gate must run on a buildable tree): %v\n%s", pkg, err, text)
	}

	// Non-vacuity: the -m run must actually have analyzed renderer.go, or an empty
	// output would make the absence check below trivially (and falsely) pass.
	if !strings.Contains(text, "renderer.go") || !strings.Contains(text, "escapes to heap") {
		t.Fatalf("escape analysis produced no renderer.go diagnostics — gate is vacuous "+
			"(cold cache may not have triggered -m). Output:\n%s", clip(text))
	}

	// The regression signal: the per-panel copy escaping to the heap.
	const regressed = "moved to heap: mat"
	if strings.Contains(text, regressed) {
		var hits []string
		for _, line := range strings.Split(text, "\n") {
			if strings.Contains(line, regressed) {
				hits = append(hits, strings.TrimSpace(line))
			}
		}
		t.Fatalf("REGRESSION (#537/#541): %q reappeared in renderer escape analysis — the per-panel\n"+
			"GraphicMaterial copy is allocating on the heap again (patch 0012 lost?). Sites:\n  %s",
			regressed, strings.Join(hits, "\n  "))
	}
	t.Logf("FSV #541: cold-cache escape analysis of %s shows NO %q — panel alloc stays eliminated.", pkg, regressed)

	// Belt-and-suspenders: the tracked patch that encodes the fix must still exist
	// (a fresh checkout restores it via restore-repoes.sh). If it vanishes, the
	// gate above only protects the current working tree, not a clean clone.
	root := moduleRoot(t)
	patch := filepath.Join(root, "patches", "engine", "0012-renderer-panel-material-no-escape.patch")
	if _, statErr := os.Stat(patch); statErr != nil {
		t.Fatalf("tracked fix patch missing (%v) — restore-repoes.sh would not reapply the #537 panel fix", statErr)
	}
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
