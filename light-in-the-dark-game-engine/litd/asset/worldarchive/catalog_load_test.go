package worldarchive

// #411 FSV: worldarchive.Open must run the glTF catalog on embedded .glb entries
// at load (defense in depth, R-SEC-1 / §2.5) — a hash-valid hand-crafted archive
// carrying a Draco-compressed (G3N-undecodable) model must be REFUSED, the same
// rule assetcheck enforces in CI. SoT = the Open error. Reuses the packDir /
// stageFirstFlame harness from worldarchive_test.go (same package).

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func glbBytes(t *testing.T, doc map[string]any) []byte {
	t.Helper()
	j, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	for len(j)%4 != 0 {
		j = append(j, ' ')
	}
	var b bytes.Buffer
	w := func(v uint32) { binary.Write(&b, binary.LittleEndian, v) }
	w(0x46546C67) // glTF
	w(2)
	w(uint32(12 + 8 + len(j)))
	w(uint32(len(j)))
	w(0x4E4F534A) // JSON
	b.Write(j)
	return b.Bytes()
}

func TestArchiveGLBCatalogRefusedFSV(t *testing.T) {
	// Hash-valid archive whose only flaw is a Draco-compressed model. All manifest
	// hashes are correct (packDir hashes whatever we stage) — only the catalog can
	// catch it. This is the "an archive is not a validator bypass at load" gap.
	stage := stageFirstFlame(t)
	if err := os.MkdirAll(filepath.Join(stage, "assets"), 0o755); err != nil {
		t.Fatal(err)
	}
	draco := glbBytes(t, map[string]any{
		"asset":          map[string]any{"version": "2.0"},
		"extensionsUsed": []string{"KHR_draco_mesh_compression"},
	})
	if err := os.WriteFile(filepath.Join(stage, "assets", "unit.glb"), draco, 0o644); err != nil {
		t.Fatal(err)
	}
	arcPath := filepath.Join(t.TempDir(), "draco.litdworld")
	packDir(t, stage, arcPath, "*", "")

	if _, err := Open(arcPath, ""); err == nil {
		t.Fatal("archive with a Draco GLB opened — the load-time catalog must refuse it (#411)")
	} else if !strings.Contains(err.Error(), "fails asset catalog") || !strings.Contains(err.Error(), "GLTF-COMPRESS") {
		t.Fatalf("expected a GLTF-COMPRESS catalog refusal, got: %v", err)
	} else {
		t.Logf("FSV #411 defense-in-depth: hash-valid archive with a Draco GLB refused at Open — %v", err)
	}

	// Positive control: a clean core-profile GLB at the same path opens fine, so
	// the gate is not collateral damage to legitimate embedded models.
	clean := glbBytes(t, map[string]any{"asset": map[string]any{"version": "2.0"}})
	if err := os.WriteFile(filepath.Join(stage, "assets", "unit.glb"), clean, 0o644); err != nil {
		t.Fatal(err)
	}
	arc2 := filepath.Join(t.TempDir(), "clean.litdworld")
	packDir(t, stage, arc2, "*", "")
	a, err := Open(arc2, "")
	if err != nil {
		t.Fatalf("archive with a clean core-profile GLB must open, got: %v", err)
	}
	a.Close()
	t.Logf("FSV #411 control: archive with a clean core-profile GLB opens (catalog not collateral)")
}
