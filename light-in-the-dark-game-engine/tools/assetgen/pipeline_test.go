package main

import (
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// #56 FSV: the pipeline skeleton runs end to end against the REAL assetcheck
// binary (no mock gate) and real files. SoT = the file tree (committed vs
// scratch), the MANIFEST entry, the gate findings, and the run log. Edges: a
// non-interactive curate refuses to commit; an accepted asset that fails the
// gate is blocked before assets/ is touched; a spec missing `category` is
// rejected at parse.

// buildAssetcheck compiles the real tools/assetcheck binary once for the gate.
func buildAssetcheck(t *testing.T) string {
	t.Helper()
	bin := filepath.Join(t.TempDir(), "assetcheck")
	cmd := exec.Command("go", "build", "-o", bin,
		"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("build assetcheck: %v\n%s", err, out)
	}
	return bin
}

func newAssetsDir(t *testing.T) string {
	t.Helper()
	d := t.TempDir()
	if err := os.WriteFile(filepath.Join(d, "MANIFEST"), []byte("# test ledger\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return d
}

func newPipeline(t *testing.T, assets, scratch, bin string, cur Curator, log io.Writer) Pipeline {
	return Pipeline{
		ScratchDir: scratch, AssetsDir: assets,
		Gen: StubGenerator{}, Cur: cur, Chk: ExecChecker{Bin: bin},
		Pack: "Light in the Dark (test)", Source: "assetgen", Retrieved: "2026-06-23",
		Log: log,
	}
}

func TestAssetgenPipelineFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("builds + execs the real assetcheck binary; -short skips")
	}
	bin := buildAssetcheck(t)

	// --- happy path: accept the first candidate, reject the second ---
	t.Run("AcceptCommits_RejectNeverTouchesAssets", func(t *testing.T) {
		assets, scratch := newAssetsDir(t), t.TempDir()
		spec := `
[[gen]]
category = "icon"
generator = "stub-0.1"
prompt = "a golden vigil sigil"
output = "icons/vigil.png"

[[gen]]
category = "icon"
generator = "stub-0.1"
prompt = "an ember unbound sigil"
output = "icons/unbound.png"
`
		items, err := ParseSpec(strings.NewReader(spec))
		if err != nil {
			t.Fatalf("ParseSpec: %v", err)
		}
		var log strings.Builder
		cur := NewInteractiveCurator(strings.NewReader("y\nSer Curator\nn\n"), io.Discard)
		rep, err := newPipeline(t, assets, scratch, bin, cur, &log).Run(items)
		if err != nil {
			t.Fatalf("Run: %v", err)
		}

		// accepted asset committed to assets/
		if _, err := os.Stat(filepath.Join(assets, "icons/vigil.png")); err != nil {
			t.Fatalf("accepted asset not committed: %v", err)
		}
		// rejected asset NEVER reaches assets/, but DID land in scratch
		if _, err := os.Stat(filepath.Join(assets, "icons/unbound.png")); !os.IsNotExist(err) {
			t.Fatalf("rejected asset leaked into assets/: %v", err)
		}
		if _, err := os.Stat(filepath.Join(scratch, "icons/unbound.png")); err != nil {
			t.Fatalf("rejected candidate should remain in scratch: %v", err)
		}
		// MANIFEST records the accept with generated provenance + curator sign-off
		mf, _ := os.ReadFile(filepath.Join(assets, "MANIFEST"))
		for _, want := range []string{"icons/vigil.png", `provenance = "generated"`, `curator = "Ser Curator"`, `category = "icon"`} {
			if !strings.Contains(string(mf), want) {
				t.Fatalf("MANIFEST missing %q:\n%s", want, mf)
			}
		}
		// tallies
		if len(rep.Committed) != 1 || rep.Committed[0] != "icons/vigil.png" {
			t.Fatalf("committed=%v, want [icons/vigil.png]", rep.Committed)
		}
		if rep.Accepted["icon"] != 1 || rep.Rejected["icon"] != 1 {
			t.Fatalf("tally accepted=%v rejected=%v, want 1/1", rep.Accepted, rep.Rejected)
		}
		// SoT: the committed assets/ dir passes a fresh full assetcheck run
		findings, err := ExecChecker{Bin: bin}.Check(assets)
		if err != nil {
			t.Fatalf("re-check committed assets: %v", err)
		}
		if len(findings) != 0 {
			t.Fatalf("committed assets not clean: %v", findings)
		}
		t.Logf("FSV happy: committed=%v; rejected stayed in scratch; final assets/ passes assetcheck clean", rep.Committed)
	})

	// --- edge 1: non-interactive curate refuses to commit (no bypass) ---
	t.Run("NonInteractiveCurate_RefusesCommit", func(t *testing.T) {
		assets, scratch := newAssetsDir(t), t.TempDir()
		items, _ := ParseSpec(strings.NewReader("[[gen]]\ncategory=\"icon\"\ngenerator=\"stub-0.1\"\nprompt=\"x\"\noutput=\"icons/x.png\"\n"))
		var log strings.Builder
		cur := NewInteractiveCurator(strings.NewReader(""), io.Discard) // EOF — non-interactive
		rep, err := newPipeline(t, assets, scratch, bin, cur, &log).Run(items)
		if err != nil {
			t.Fatalf("Run: %v", err)
		}
		if len(rep.Committed) != 0 {
			t.Fatalf("committed despite no curator decision: %v", rep.Committed)
		}
		if _, err := os.Stat(filepath.Join(assets, "icons/x.png")); !os.IsNotExist(err) {
			t.Fatalf("asset leaked into assets/ with no sign-off: %v", err)
		}
		if !strings.Contains(log.String(), "REFUSED") {
			t.Fatalf("expected a printed refusal, log:\n%s", log.String())
		}
		t.Logf("FSV edge1: non-interactive curate → no commit, refusal printed: %q", lineWith(log.String(), "REFUSED"))
	})

	// --- edge 2: accepted asset fails the gate → commit blocked ---
	t.Run("AcceptedButGateFails_Blocks", func(t *testing.T) {
		assets, scratch := newAssetsDir(t), t.TempDir()
		// A .glb output: the stub writes a non-glTF placeholder, so the real
		// assetcheck glTF-core gate must reject it.
		items, _ := ParseSpec(strings.NewReader("[[gen]]\ncategory=\"unit\"\ngenerator=\"stub-0.1\"\nprompt=\"a knight\"\noutput=\"units/knight.glb\"\n"))
		var log strings.Builder
		cur := NewInteractiveCurator(strings.NewReader("y\nSer Curator\n"), io.Discard)
		rep, err := newPipeline(t, assets, scratch, bin, cur, &log).Run(items)
		if err != nil {
			t.Fatalf("Run: %v", err)
		}
		if len(rep.Committed) != 0 {
			t.Fatalf("committed a gate-failing asset: %v", rep.Committed)
		}
		if len(rep.Blocked) != 1 || rep.Blocked[0].Output != "units/knight.glb" {
			t.Fatalf("expected 1 blocked (knight.glb), got %+v", rep.Blocked)
		}
		if _, err := os.Stat(filepath.Join(assets, "units/knight.glb")); !os.IsNotExist(err) {
			t.Fatalf("blocked asset leaked into assets/: %v", err)
		}
		joined := strings.Join(rep.Blocked[0].Findings, " ")
		if !strings.Contains(joined, "GLTF-CORE") {
			t.Fatalf("expected a GLTF-CORE finding, got: %v", rep.Blocked[0].Findings)
		}
		t.Logf("FSV edge2: accepted .glb placeholder BLOCKED before commit: %v", rep.Blocked[0].Findings)
	})

	// --- edge 3: spec missing `category` is rejected at parse ---
	t.Run("SpecMissingCategory_RejectedAtParse", func(t *testing.T) {
		_, err := ParseSpec(strings.NewReader("[[gen]]\ngenerator=\"stub-0.1\"\nprompt=\"x\"\noutput=\"icons/x.png\"\n"))
		if err == nil {
			t.Fatal("ParseSpec accepted a [[gen]] entry with no category")
		}
		if !strings.Contains(err.Error(), "category") {
			t.Fatalf("error should name the missing key: %v", err)
		}
		t.Logf("FSV edge3: spec without category refused at parse: %v", err)
	})
}

// lineWith returns the first line of s containing sub (for log assertions).
func lineWith(s, sub string) string {
	for _, ln := range strings.Split(s, "\n") {
		if strings.Contains(ln, sub) {
			return strings.TrimSpace(ln)
		}
	}
	return ""
}
