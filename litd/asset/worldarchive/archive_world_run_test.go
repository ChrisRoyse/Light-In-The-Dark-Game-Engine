package worldarchive_test

// #209/#205 keystone: the game LOADS AND RUNS its own world through the archive
// path with ZERO behavioral diff vs the directory. The existing archive tests
// prove the map's data FINGERPRINT matches a directory load, but none loads a
// REAL world's Lua + map through worldarchive.Open().FS() into a running sim and
// checks the SIM behaves identically. This packs the real First Flame (its map +
// the actual worlds/firstflame/main.lua beacon/victory logic), opens the verified
// archive, and drives the SAME scripted beacon-hold victory from BOTH the archive
// FS and the directory — asserting the 64-bit Game.StateHash is bit-identical.
// SoT = the recomputed state hash after 130 ticks from each load path + the
// victory the world publishes to Storage.

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	wa "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// repoRoot is three levels up from litd/asset/worldarchive.
const repoRoot = "../../.."

// packRealFirstFlame stages the committed First Flame map + the real world Lua
// into the archive layout (data/maps/firstflame/… + scripts/main.lua) and packs a
// deterministic .litdworld, mirroring scripts/pack-world.sh's producer format.
// Returns the archive path.
func packRealFirstFlame(t *testing.T) string {
	t.Helper()
	stage := t.TempDir()
	// Real map data.
	mapDst := filepath.Join(stage, "data", "maps", "firstflame")
	if err := os.MkdirAll(mapDst, 0o755); err != nil {
		t.Fatal(err)
	}
	mapSrc := filepath.Join(repoRoot, "data", "maps", "firstflame")
	ents, err := os.ReadDir(mapSrc)
	if err != nil {
		t.Fatalf("read map: %v", err)
	}
	for _, e := range ents {
		if e.IsDir() {
			continue
		}
		b, err := os.ReadFile(filepath.Join(mapSrc, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(mapDst, e.Name()), b, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	// Real world Lua under scripts/ (the in-engine mount layout).
	if err := os.MkdirAll(filepath.Join(stage, "scripts"), 0o755); err != nil {
		t.Fatal(err)
	}
	mainSrc, err := os.ReadFile(filepath.Join(repoRoot, "worlds", "firstflame", "main.lua"))
	if err != nil {
		t.Fatalf("read world main.lua: %v", err)
	}
	if err := os.WriteFile(filepath.Join(stage, "scripts", "main.lua"), mainSrc, 0o644); err != nil {
		t.Fatal(err)
	}

	// Pack (deterministic, rel-sorted; manifest format per pack-world.sh / worldpack).
	type ent struct {
		rel, hash string
		size      int64
		body      []byte
	}
	var es []ent
	err = filepath.WalkDir(stage, func(p string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		b, e := os.ReadFile(p)
		if e != nil {
			return e
		}
		rel, _ := filepath.Rel(stage, p)
		sum := sha256.Sum256(b)
		es = append(es, ent{filepath.ToSlash(rel), hex.EncodeToString(sum[:]), int64(len(b)), b})
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Slice(es, func(i, j int) bool { return es[i].rel < es[j].rel })
	agg := sha256.New()
	for _, e := range es {
		agg.Write([]byte(e.hash + "\n"))
	}
	var man strings.Builder
	man.WriteString("litdworld-version: 1\n")
	man.WriteString("engine-range: >=0.1.0 <0.2.0\n")
	man.WriteString("author: Light in the Dark\n")
	man.WriteString("title: First Flame\n")
	man.WriteString("description: ashen-veil duel\n")
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	fmt.Fprintf(&man, "files: %d\n", len(es))
	for _, e := range es {
		fmt.Fprintf(&man, "%s %d %s\n", e.hash, e.size, e.rel)
	}

	out := filepath.Join(t.TempDir(), "firstflame.litdworld")
	f, err := os.Create(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	mw, _ := zw.Create(".litdworld-manifest")
	if _, err := mw.Write([]byte(man.String())); err != nil {
		t.Fatal(err)
	}
	for _, e := range es {
		w, _ := zw.Create(e.rel)
		if _, err := w.Write(e.body); err != nil {
			t.Fatal(err)
		}
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	return out
}

// runFirstFlame loads the firstflame map + world from the given filesystems,
// drives the scripted both-beacon hold to victory (mirroring the canonical victory
// FSV), and returns (stateHash, decided, winnerSlot). mapFS/worldFS differ ONLY in
// their source (archive zip vs on-disk directory); identical bytes ⇒ identical hash.
func runFirstFlame(t *testing.T, mapFS fs.FS, worldFS fs.FS) (uint64, int, int) {
	t.Helper()
	m, err := mapdata.Load(mapFS, "data/maps/firstflame")
	if err != nil {
		t.Fatalf("mapdata.Load: %v", err)
	}
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	luabind.RegisterMap(L, m)
	reg := luabind.NewChunkRegistry()
	defer reg.Close()

	// P1 (slot 1) on both required beacons: id1 (128,128), id2 (88,88).
	for _, c := range [][2]int{{128, 128}, {88, 88}} {
		pos := api.Vec2{X: float64(c[0]*32 + 16), Y: float64(c[1]*32 + 16)}
		if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), pos, api.Deg(0)).Valid() {
			t.Fatalf("unit at %v invalid", c)
		}
	}
	if _, err := luabind.LoadWorldFS(L, reg, worldFS, "firstflame"); err != nil {
		t.Fatalf("LoadWorldFS: %v", err)
	}
	g.Advance(130)
	decided, _ := g.Storage().GetInt("match", "decided")
	winner, _ := g.Storage().GetInt("match", "winner")
	return g.StateHash(), decided, winner
}

func TestFirstFlameArchiveRunMatchesDirectoryFSV(t *testing.T) {
	// Directory load (the dev path).
	dirHash, dirDecided, dirWinner := runFirstFlame(t,
		os.DirFS(repoRoot),
		os.DirFS(filepath.Join(repoRoot, "worlds", "firstflame")))
	if dirDecided != 1 || dirWinner != 1 {
		t.Fatalf("directory run did not reach the expected victory: decided=%d winner=%d", dirDecided, dirWinner)
	}

	// Archive load (the shipped path): open the verified archive, mount its map FS
	// and its scripts/ subtree as the world FS.
	archivePath := packRealFirstFlame(t)
	ar, err := wa.Open(archivePath, "0.1.0")
	if err != nil {
		t.Fatalf("Open archive: %v", err)
	}
	defer ar.Close()
	// The manifest hosting metadata must be real (D-23).
	if ar.Manifest.Title == "" || ar.Manifest.Author == "" || ar.Manifest.Description == "" {
		t.Fatalf("archive manifest missing hosting metadata: %+v", ar.Manifest)
	}
	worldFS, err := fs.Sub(ar.FS(), "scripts")
	if err != nil {
		t.Fatalf("fs.Sub scripts: %v", err)
	}
	arcHash, arcDecided, arcWinner := runFirstFlame(t, ar.FS(), worldFS)

	// Behavioral parity: the archive-loaded sim is bit-identical to the directory.
	if arcHash != dirHash {
		t.Fatalf("archive run DIVERGED from directory: archive %016x != directory %016x", arcHash, dirHash)
	}
	if arcDecided != dirDecided || arcWinner != dirWinner {
		t.Fatalf("archive victory differs: decided=%d winner=%d, want %d/%d", arcDecided, arcWinner, dirDecided, dirWinner)
	}
	t.Logf("FSV #209/#205: First Flame ran through the archive FS bit-identical to directory — StateHash %016x, victory winner=slot%d (title=%q)",
		arcHash, arcWinner, ar.Manifest.Title)
}
