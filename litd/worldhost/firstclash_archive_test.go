package worldhost_test

// #648/#664 FSV: firstclash round-trips through a production .litdworld archive
// and runs bit-identical to its source directory. This is the deliverable that
// the caged-require relaxation (#664) unblocks — before it, packing firstclash
// produced an archive that failed `assetcheck archive` / LoadArchive with
// "entry fails sandbox lint" because main.lua dispatches per-race scripts via
// require("melee/"..race). require is now permitted in archives precisely because
// the runtime resolver is caged to the world's own (hash-verified) chunks; the
// escape-proof is luabind/require_escape_test.go.
//
// SoT = the sim StateHash after a fixed advance. The archive and the directory
// differ ONLY in their byte source (verified zip vs on-disk); identical inputs ⇒
// identical hash. A mismatch means the archive path diverged from the directory
// path (a packaging or mount bug), not a lint question.

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func TestFirstclashArchiveRoundTripFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("packs an archive + runs two headless worlds; full gate only")
	}

	// Stage firstclash into the archive layout: world + map data under data/, Lua
	// (incl. match.toml, read from the script root) under scripts/. firstclash is
	// mapless, so only the world's own data/ is staged.
	stage := t.TempDir()
	mustMkdir(t, filepath.Join(stage, "scripts", "melee"))
	mustMkdir(t, filepath.Join(stage, "data"))
	copyTree(t, filepath.Join(firstclashDir, "data"), filepath.Join(stage, "data"))
	copyFile(t, filepath.Join(firstclashDir, "main.lua"), filepath.Join(stage, "scripts", "main.lua"))
	copyFile(t, filepath.Join(firstclashDir, "match.toml"), filepath.Join(stage, "scripts", "match.toml"))
	copyTree(t, filepath.Join(firstclashDir, "melee"), filepath.Join(stage, "scripts", "melee"))

	// Pack a deterministic archive (the same producer pack-world.sh / worldpack use).
	out := filepath.Join(t.TempDir(), "firstclash.litdworld")
	if err := worldpack.Pack(stage, out, ">=0.1.0 <0.2.0",
		worldpack.Hosting{Author: "LITD", Title: "First Clash", Description: "AI melee validation"},
		nil); err != nil {
		t.Fatalf("worldpack.Pack: %v", err)
	}
	if fi, err := os.Stat(out); err != nil || fi.Size() == 0 {
		t.Fatalf("packed archive missing/empty: %v", err)
	}

	const seed, budget, ticks = 1337, 5_000_000, 200

	// Directory load — the reference run.
	hd, err := worldhost.Load(firstclashDir, seed, budget)
	if err != nil {
		t.Fatalf("dir load: %v", err)
	}
	hd.Game.Advance(ticks)
	dirHash := hd.Game.StateHash()
	hd.Close()

	// Archive load — previously failed the sandbox lint on require (#664). It must
	// now load AND reproduce the directory hash bit-for-bit.
	ha, err := worldhost.LoadArchive(out, "0.1.0", seed, budget)
	if err != nil {
		t.Fatalf("archive load (require must be permitted in archives, #664): %v", err)
	}
	ha.Game.Advance(ticks)
	arcHash := ha.Game.StateHash()
	ha.Close()

	t.Logf("FSV #648/#664: dir=%#x archive=%#x after %d ticks", dirHash, arcHash, ticks)
	if dirHash == 0 {
		t.Fatal("dir hash zero — world did not initialize")
	}
	if arcHash != dirHash {
		t.Fatalf("archive hash %#x != dir hash %#x — archive path diverged from directory path", arcHash, dirHash)
	}
}

func mustMkdir(t *testing.T, p string) {
	t.Helper()
	if err := os.MkdirAll(p, 0o755); err != nil {
		t.Fatal(err)
	}
}

func copyFile(t *testing.T, src, dst string) {
	t.Helper()
	b, err := os.ReadFile(src)
	if err != nil {
		t.Fatalf("read %s: %v", src, err)
	}
	if err := os.WriteFile(dst, b, 0o644); err != nil {
		t.Fatalf("write %s: %v", dst, err)
	}
}

func copyTree(t *testing.T, src, dst string) {
	t.Helper()
	if err := os.CopyFS(dst, os.DirFS(src)); err != nil {
		t.Fatalf("copy tree %s -> %s: %v", src, dst, err)
	}
}
