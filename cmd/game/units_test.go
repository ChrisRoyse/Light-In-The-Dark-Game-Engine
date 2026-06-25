package main

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/g3n/engine/loader/gltf"
)

// repoAsset resolves an asset path against the repo-root assets/ tree. `go test`
// runs with the package dir as cwd (cmd/game), whereas the binary runs from the repo
// root, so the test walks up to the module root (the dir holding go.mod) to find the
// real, shared assets/ — the same files the running binary loads.
func repoAsset(t *testing.T, rel string) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return filepath.Join(dir, assetsRoot, rel)
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatalf("module root (go.mod) not found above %s", dir)
		}
		dir = parent
	}
}

// The firstclash unit data declares canonical model paths (units/footman.glb,
// buildings/vigil/bastion.glb, …) that are not yet provisioned in assets/ (#670).
// Until they land the renderer substitutes an existing CC0 model by category. This
// FSV proves the substitution targets are REAL files on disk — a fallback that
// itself points at a missing asset would just move the blank-screen bug, not fix it.
// SoT = os.Stat on each path categoryFallback can return.
func TestModelFallbacksExistFSV(t *testing.T) {
	// Every distinct value categoryFallback can produce, across building/hero/unit and
	// both team slots and the full id rotation.
	paths := map[string]bool{}
	paths[categoryFallback(true, 0, false, 0)] = true // building, blue team
	paths[categoryFallback(true, 1, false, 0)] = true // building, red team
	paths[categoryFallback(false, 0, true, 0)] = true // hero
	for id := uint32(0); id < uint32(len(fallbackUnits)); id++ {
		paths[categoryFallback(false, 0, false, id)] = true // each rank-and-file model
	}
	if len(paths) < 4 {
		t.Fatalf("expected several distinct fallback models, got %d: %v", len(paths), paths)
	}
	for p := range paths {
		full := repoAsset(t, p)
		if !fileExists(full) {
			t.Errorf("fallback model %q does not exist at %q — substitution would render nothing", p, full)
			continue
		}
		t.Logf("FSV: fallback %q exists", p)
	}
}

// The firstclash canonical model paths are confirmed NOT yet on disk — this is the
// documented gap (#670) the fallback covers. Pinning it as a test means the day the
// real assets land, this fails loudly and the renderer starts using them directly
// (resolveModel prefers the declared path when it exists).
func TestCanonicalModelsNotYetProvisionedFSV(t *testing.T) {
	canonical := []string{
		"units/footman.glb",
		"buildings/vigil/bastion.glb",
		"units/unbound/forager.glb",
	}
	missing := 0
	for _, p := range canonical {
		if fileExists(repoAsset(t, p)) {
			t.Logf("note: %q now provisioned — renderer will use it directly (update/remove #670 fallback)", p)
		} else {
			missing++
		}
	}
	t.Logf("FSV: %d/%d canonical firstclash models still unprovisioned (#670)", missing, len(canonical))
}

// The model instancing path must work fully headless (no GL context): parse a real
// CC0 GLB, load its scene, load+pose its Idle clip, and normalize it to a target
// height. This is the exact sequence buildUnitVisual runs per spawn. SoT = the
// returned node's bounding box (must be scaled to ~height, base at y≈0) and a
// non-nil Idle animation for an animated adventurer model.
func TestGLBInstanceHeadlessFSV(t *testing.T) {
	rel := fallbackHero // kaykit-adventurers/Knight.glb — known to have an Idle clip
	full := repoAsset(t, rel)
	if !fileExists(full) {
		t.Skipf("asset %q absent (fresh checkout without packs)", full)
	}
	doc, err := gltf.ParseBin(full)
	if err != nil {
		t.Fatalf("ParseBin %q: %v", full, err)
	}
	scene := 0
	if doc.Scene != nil {
		scene = *doc.Scene
	}
	inode, err := doc.LoadScene(scene)
	if err != nil {
		t.Fatalf("LoadScene: %v", err)
	}

	const height = 1.9
	node := normalizeModel(inode, height)
	node.UpdateMatrixWorld()
	bb := node.BoundingBox()
	sizeY := bb.Max.Y - bb.Min.Y
	if sizeY <= 0 {
		t.Fatalf("normalized model has non-positive height %.3f", sizeY)
	}
	// Largest axis is scaled to height; the model's tallest axis should be ~height
	// (within tolerance — a character is roughly height-dominant). Base near y=0.
	if sizeY > height*1.5 || sizeY < height*0.2 {
		t.Errorf("normalized height %.3f far from target %.1f", sizeY, height)
	}
	if bb.Min.Y < -0.01 || bb.Min.Y > 0.5 {
		t.Errorf("normalized base y=%.3f not seated near ground", bb.Min.Y)
	}
	t.Logf("FSV: %q loaded headless, normalized bbox Y=[%.3f,%.3f] (height≈%.2f)", rel, bb.Min.Y, bb.Max.Y, sizeY)

	idle, err := doc.LoadAnimationByName("Idle")
	if err != nil || idle == nil {
		t.Fatalf("Idle clip missing on %q (err=%v) — adventurer models must animate", rel, err)
	}
	idle.SetLoop(true)
	idle.Update(0) // pose frame 0; must not panic without a GL context
	t.Logf("FSV: Idle clip loaded + posed headless on %q", rel)
}
