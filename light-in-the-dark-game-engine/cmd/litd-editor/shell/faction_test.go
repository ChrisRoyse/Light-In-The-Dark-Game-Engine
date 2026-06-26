package shell

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

func TestFactionCreatorEmitsTablesAndArchiveFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	dir := filepath.Join(root, "source")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	paths := factionOutputPaths("ember-vigil")
	for _, p := range paths {
		if _, err := os.Stat(filepath.Join(dir, filepath.FromSlash(p))); !os.IsNotExist(err) {
			t.Fatalf("expected %s absent before creator save, stat err=%v", p, err)
		}
	}
	before := app.Snapshot()
	draft := FactionCreatorDraft{
		ID:        "ember-vigil",
		Name:      "Ember Vigil",
		Culture:   "vigil",
		Traits:    []string{"beacon-stewards", "ember-raiders"},
		Grimoires: []string{"long-vigil"},
	}
	snap, err := app.SaveFactionDraft(draft)
	if err != nil {
		t.Fatalf("SaveFactionDraft: %v", err)
	}
	if !snap.Valid || snap.Preview.TownHall != "htow" || snap.Preview.Worker != "hpea" || len(snap.LastOutputs) != 3 {
		t.Fatalf("faction snapshot mismatch: %+v", snap)
	}
	if err := app.Save(); err != nil {
		t.Fatalf("save source form: %v", err)
	}
	meleeBody := mustRead(t, filepath.Join(dir, "data", "melee", "ember-vigil.toml"))
	metaBody := mustRead(t, filepath.Join(dir, "data", "factions", "ember-vigil.toml"))
	luaBody := mustRead(t, filepath.Join(dir, "scripts", "factions", "ember-vigil.lua"))
	parsed, err := melee.LoadFactionBytes(meleeBody)
	if err != nil {
		t.Fatalf("generated melee table does not load: %v\n%s", err, meleeBody)
	}
	t.Logf("FSV faction creator BEFORE paths absent dirty=%v AFTER snapshot=%+v", before.Dirty, snap)
	t.Logf("FSV generated melee table:\n%s", meleeBody)
	t.Logf("FSV generated metadata table:\n%s", metaBody)
	t.Logf("FSV generated Lua:\n%s", luaBody)
	if parsed.Name != "Ember Vigil" || parsed.TownHall != "htow" || parsed.Workers.Code != "hpea" || parsed.Workers.Count != 5 {
		t.Fatalf("generated melee table parsed wrong: %+v", parsed)
	}
	if !bytes.Contains(metaBody, []byte(`traits = ["beacon-stewards", "ember-raiders"]`)) ||
		!bytes.Contains(luaBody, []byte(`FactionDefinitions["ember-vigil"]`)) {
		t.Fatalf("metadata/Lua missing creator selections")
	}

	archivePath := filepath.Join(root, "faction-creator.litdworld")
	if err := app.SaveArchive(archivePath); err != nil {
		t.Fatalf("save archive: %v", err)
	}
	opened, err := worldarchive.Open(archivePath, "")
	if err != nil {
		t.Fatalf("open archive: %v", err)
	}
	defer opened.Close()
	for _, p := range paths {
		archived, err := fs.ReadFile(opened.FS(), p)
		if err != nil {
			t.Fatalf("read archived %s: %v", p, err)
		}
		source := mustRead(t, filepath.Join(dir, filepath.FromSlash(p)))
		if !bytes.Equal(archived, source) {
			t.Fatalf("archived %s differs from source", p)
		}
		entry, ok := opened.Manifest.Files[p]
		t.Logf("FSV archive output %s manifest=%+v sha256=%s", p, entry, sha256Hex(archived))
		if !ok {
			t.Fatalf("archive manifest missing %s", p)
		}
	}
}

func TestFactionCreatorValidationEdgesFSV(t *testing.T) {
	root := t.TempDir()
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		t.Fatal(err)
	}
	noGrimoire := FactionCreatorDraft{ID: "no-grimoire", Name: "No Grimoire", Culture: "vigil", Traits: []string{"beacon-stewards"}}
	beforeDirty := app.Snapshot().Dirty
	_, err := app.SaveFactionDraft(noGrimoire)
	afterNoGrimoire := app.Snapshot()
	t.Logf("FSV no-grimoire edge BEFORE dirty=%v AFTER dirty=%v err=%v", beforeDirty, afterNoGrimoire.Dirty, err)
	if err == nil || !strings.Contains(err.Error(), "grimoire") || afterNoGrimoire.Dirty {
		t.Fatalf("no-grimoire edge failed closed incorrectly: dirty=%v err=%v", afterNoGrimoire.Dirty, err)
	}

	conflict := FactionCreatorDraft{
		ID:        "trait-conflict",
		Name:      "Trait Conflict",
		Culture:   "vigil",
		Traits:    []string{"beacon-stewards", "gloam-touched"},
		Grimoires: []string{"long-vigil"},
	}
	_, err = app.SaveFactionDraft(conflict)
	afterConflict := app.Snapshot()
	t.Logf("FSV trait-conflict edge AFTER dirty=%v err=%v", afterConflict.Dirty, err)
	if err == nil || !strings.Contains(err.Error(), "conflicts") || afterConflict.Dirty {
		t.Fatalf("trait conflict edge failed closed incorrectly: dirty=%v err=%v", afterConflict.Dirty, err)
	}

	duplicate := FactionCreatorDraft{
		ID:        "duplicate-trait",
		Name:      "Duplicate Trait",
		Culture:   "unbound",
		Traits:    []string{"ember-raiders", "ember-raiders"},
		Grimoires: []string{"ember-road"},
	}
	_, err = app.SaveFactionDraft(duplicate)
	afterDuplicate := app.Snapshot()
	t.Logf("FSV duplicate-trait edge AFTER dirty=%v err=%v", afterDuplicate.Dirty, err)
	if err == nil || !strings.Contains(err.Error(), "duplicated") || afterDuplicate.Dirty {
		t.Fatalf("duplicate trait edge failed closed incorrectly: dirty=%v err=%v", afterDuplicate.Dirty, err)
	}
}

func TestFactionCreatorGeneratedFactionDeterministicSetupFSV(t *testing.T) {
	draft := FactionCreatorDraft{
		ID:        "replay-stable",
		Name:      "Replay Stable",
		Culture:   "unbound",
		Traits:    []string{"ember-raiders"},
		Grimoires: []string{"ember-road"},
	}
	buildA, err := buildFaction(draft)
	if err != nil {
		t.Fatal(err)
	}
	buildB, err := buildFaction(draft)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(buildA.files[0].body, buildB.files[0].body) {
		t.Fatal("same faction draft emitted different melee table bytes")
	}
	faction, err := melee.LoadFactionBytes(buildA.files[0].body)
	if err != nil {
		t.Fatalf("generated faction did not load: %v", err)
	}
	hashA, countA := generatedFactionSetupHash(t, faction)
	hashB, countB := generatedFactionSetupHash(t, faction)
	t.Logf("FSV generated faction deterministic setup: tableSHA=%s hashA=%#016x unitsA=%d hashB=%#016x unitsB=%d",
		sha256Hex(buildA.files[0].body), hashA, countA, hashB, countB)
	if hashA != hashB || countA != countB || countA != 7 {
		t.Fatalf("generated faction setup is not deterministic/selectable: hashA=%#x hashB=%#x countA=%d countB=%d", hashA, hashB, countA, countB)
	}
}

func generatedFactionSetupHash(t *testing.T, faction *melee.Faction) (uint64, int) {
	t.Helper()
	g, err := litd.NewGame(litd.GameOptions{MaxUnits: 64, Seed: 777})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "htow", Life: 1500, FoodProvided: 12, DepotMask: 1},
		{ID: "hpea", Life: 220},
		{ID: "ugol", Life: 1500, FoodProvided: 11, DepotMask: 1},
		{ID: "uaco", Life: 220},
		{ID: "ushd", Life: 340},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineEconomy(4); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	p := g.Player(1)
	p.SetStartLocation(litd.Vec2{X: 160, Y: 160})
	if err := melee.Standard(g, []melee.Setup{{Player: p, Faction: faction}}); err != nil {
		t.Fatalf("melee.Standard generated faction: %v", err)
	}
	return g.StateHash(), melee.PlayerUnitCount(g, p)
}

func factionOutputPaths(id string) []string {
	return []string{
		"data/factions/" + id + ".toml",
		"data/melee/" + id + ".toml",
		"scripts/factions/" + id + ".lua",
	}
}

func mustRead(t *testing.T, path string) []byte {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return body
}

func sha256Hex(body []byte) string {
	sum := sha256.Sum256(body)
	return hex.EncodeToString(sum[:])
}

func TestFactionCreatorCatalogStableFSV(t *testing.T) {
	cat := FactionCreatorCatalog()
	got := fmt.Sprintf("cultures=%d traits=%d grimoires=%d first=%s", len(cat.Cultures), len(cat.Traits), len(cat.Grimoires), cat.Cultures[0].ID)
	t.Logf("FSV faction catalog: %s", got)
	if got != "cultures=2 traits=4 grimoires=4 first=vigil" {
		t.Fatalf("catalog changed unexpectedly: %s", got)
	}
}
