package luabind

// End-to-end FSV of the in-engine archive load path (#205 "the game loads its
// own map through the archive"; #209 exit 6). A First Flame world (the real
// committed map + the map-driven beacon script) is packed into a .litdworld,
// opened + verified by worldarchive, and then RUN entirely from the archive's
// fs.FS: mapdata.Load reads the map from the archive, RegisterMap exposes it,
// LoadWorldFS compiles + runs the world's Lua from the archive. SoT = the
// beacon state the world publishes to Storage after advancing the sim, plus the
// map fingerprint (archive-served == directory-served).

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	worldarchive "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// packWorldArchive stages the real firstflame map under data/maps/firstflame/
// and the beacon world's Lua under scripts/, then writes a deterministic
// .litdworld (manifest mirrors the worldpack format). Returns the archive path.
func packWorldArchive(t *testing.T, out string) {
	t.Helper()
	stage := t.TempDir()
	// Real committed map (repo root is two levels up from litd/luabind).
	mapSrc := filepath.Join("..", "..", "data", "maps", "firstflame")
	mapDst := filepath.Join(stage, "data", "maps", "firstflame")
	if err := os.MkdirAll(mapDst, 0o755); err != nil {
		t.Fatal(err)
	}
	ents, err := os.ReadDir(mapSrc)
	if err != nil {
		t.Fatalf("read map: %v", err)
	}
	for _, e := range ents {
		b, err := os.ReadFile(filepath.Join(mapSrc, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		os.WriteFile(filepath.Join(mapDst, e.Name()), b, 0o644)
	}
	// The proven map-driven beacon world becomes the archive's entry script.
	beacon, err := os.ReadFile(filepath.Join("..", "..", "worlds", "firstflame-beacon", "main.lua"))
	if err != nil {
		t.Fatalf("read beacon world: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(stage, "scripts"), 0o755); err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(stage, "scripts", "main.lua"), beacon, 0o644)

	// Collect + hash, sorted by rel.
	type ent struct {
		rel, hash string
		size      int64
		body      []byte
	}
	var es []ent
	filepath.WalkDir(stage, func(p string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		b, _ := os.ReadFile(p)
		rel, _ := filepath.Rel(stage, p)
		sum := sha256.Sum256(b)
		es = append(es, ent{filepath.ToSlash(rel), hex.EncodeToString(sum[:]), int64(len(b)), b})
		return nil
	})
	sort.Slice(es, func(i, j int) bool { return es[i].rel < es[j].rel })
	agg := sha256.New()
	for _, e := range es {
		agg.Write([]byte(e.hash + "\n"))
	}
	var man strings.Builder
	man.WriteString("litdworld-version: 1\n")
	man.WriteString("engine-range: >=0.1.0 <0.2.0\n")
	man.WriteString("author: Light in the Dark\ntitle: First Flame\ndescription: beacon duel\n")
	man.WriteString("aggregate-sha256: " + hex.EncodeToString(agg.Sum(nil)) + "\n")
	man.WriteString("files: " + itoa(len(es)) + "\n")
	for _, e := range es {
		man.WriteString(e.hash + " " + itoa(int(e.size)) + " " + e.rel + "\n")
	}
	f, err := os.Create(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	mw, _ := zw.Create(".litdworld-manifest")
	mw.Write([]byte(man.String()))
	for _, e := range es {
		w, _ := zw.Create(e.rel)
		w.Write(e.body)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestRunWorldFromArchiveFSV(t *testing.T) {
	arcPath := filepath.Join(t.TempDir(), "firstflame.litdworld")
	packWorldArchive(t, arcPath)

	// Open + verify the archive (fail-closed verification happens here).
	arc, err := worldarchive.Open(arcPath, "0.1.5") // engine in-range
	if err != nil {
		t.Fatalf("worldarchive.Open: %v", err)
	}
	defer arc.Close()

	// Map read FROM the archive; fingerprint must equal the directory read.
	arcMap, err := mapdata.Load(arc.FS(), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("map load from archive: %v", err)
	}
	dirMap, err := mapdata.Load(os.DirFS(filepath.Join("..", "..")), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("map load from dir: %v", err)
	}
	if arcMap.Fingerprint != dirMap.Fingerprint {
		t.Fatalf("archive map fingerprint %#x != directory %#x", arcMap.Fingerprint, dirMap.Fingerprint)
	}

	// Central beacon world coords from the archive-served map.
	var central mapdata.Beacon
	for _, b := range arcMap.Beacons() {
		if b.ID == 1 {
			central = b
		}
	}
	beaconWorld := api.Vec2{X: float64(central.X*32 + 16), Y: float64(central.Y*32 + 16)}

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
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, arcMap) // map exposed to Lua came from the archive

	reg := NewChunkRegistry()
	defer reg.Close()

	if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), beaconWorld, api.Deg(0)).Valid() {
		t.Fatal("capturing unit invalid")
	}

	// Run the world's Lua FROM the archive: scripts/ subtree, main.lua entry.
	scriptsFS, err := fs.Sub(arc.FS(), "scripts")
	if err != nil {
		t.Fatalf("fs.Sub scripts: %v", err)
	}
	if _, err := LoadWorldFS(L, reg, scriptsFS, "firstflame.litdworld@scripts"); err != nil {
		t.Fatalf("LoadWorldFS from archive: %v", err)
	}

	// SoT: the beacon the world published reads the archive-served map coords.
	bx, _ := g.Storage().GetInt("beacon1", "x")
	by, _ := g.Storage().GetInt("beacon1", "y")
	if bx != int(beaconWorld.X) || by != int(beaconWorld.Y) {
		t.Fatalf("world beacon (%d,%d) != archive map central (%v,%v)", bx, by, beaconWorld.X, beaconWorld.Y)
	}

	// Advance past the capture threshold; SoT = beacon lit for player 1 + vision.
	g.Advance(70)
	lit, _ := g.Storage().GetInt("beacon1", "state")
	owner, _ := g.Storage().GetInt("beacon1", "owner")
	if lit != 1 || owner != 1 {
		t.Fatalf("beacon not captured running from archive: lit=%d owner=%d", lit, owner)
	}
	if fsAt := g.FogStateAt(g.Player(1), beaconWorld); fsAt != api.FogVisible {
		t.Fatalf("captured beacon not revealed: FogStateAt=%d", int(fsAt))
	}
	t.Logf("FSV #205+#209: world RAN entirely from the verified archive — map fp %#x (==dir), central beacon (%v,%v) captured lit=1 owner=1, vision stamped",
		arcMap.Fingerprint, beaconWorld.X, beaconWorld.Y)
}
