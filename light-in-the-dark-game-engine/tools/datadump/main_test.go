package main

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

// TestLoaderDumpConversionsFSV is the datadump contract test: the real strict
// loader, fed the committed data/ tables, converts authored seconds/units-per-
// second rows into the exact ticks/fixed-point integers the dump surfaces.
// Known input (human.toml) -> known output. footman: 270 units/s * 0.05 s/tick
// = 13.5 units/tick = 13.5 << 32 raw; cooldown 1.35 s / 0.05 = 27 ticks.
func TestLoaderDumpConversionsFSV(t *testing.T) {
	tables, err := data.Load(os.DirFS("../../data"))
	if err != nil {
		t.Fatalf("load data: %v", err)
	}
	var footman *data.Unit
	for i := range tables.Units {
		if tables.Units[i].ID == "footman" {
			footman = &tables.Units[i]
			break
		}
	}
	if footman == nil {
		t.Fatal("footman row not found in data/units")
	}
	if footman.Life != 420 || footman.Armor != 2 {
		t.Fatalf("footman base stats wrong: life=%d armor=%d; want 420/2", footman.Life, footman.Armor)
	}
	const wantMove = int64(135) << 32 / 10 // 13.5 << 32
	if int64(footman.MoveSpeedPerTick) != wantMove {
		t.Fatalf("footman move-speed conversion wrong: got %d, want %d (270 u/s -> 13.5 u/tick)",
			int64(footman.MoveSpeedPerTick), wantMove)
	}
	if len(footman.Attacks) == 0 {
		t.Fatal("footman has no attack")
	}
	if got := footman.Attacks[0].CooldownTicks; got != 27 {
		t.Fatalf("footman cooldown conversion wrong: got %d ticks, want 27 (1.35s / 50ms)", got)
	}
	if tables.Fingerprint == 0 {
		t.Fatal("content fingerprint is zero")
	}
	t.Logf("FSV: footman life=420 armor=2 move=13.5u/tick cooldown=27t; fingerprint=0x%016x", tables.Fingerprint)
}

// TestLoaderFailsClosedOnUnknownField proves the dump's validation leg: the
// loader rejects an unrecognized key, naming it — the #119 "move_sped" edge.
func TestLoaderFailsClosedOnUnknownField(t *testing.T) {
	dir := t.TempDir()
	if err := copyTree("../../data", dir); err != nil {
		t.Fatalf("copy data: %v", err)
	}
	path := dir + "/units/human.toml"
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	// Inject an unknown key under the first [[unit]] row.
	bad := string(body) + "\n[[unit]]\nid = \"glitch\"\nlife = 1\nmove_sped = 1\n"
	if err := os.WriteFile(path, []byte(bad), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := data.Load(os.DirFS(dir)); err == nil {
		t.Fatal("loader must fail closed on unknown field move_sped")
	}
}

func copyTree(src, dst string) error {
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}
	for _, e := range entries {
		s := src + "/" + e.Name()
		d := dst + "/" + e.Name()
		if e.IsDir() {
			if err := copyTree(s, d); err != nil {
				return err
			}
			continue
		}
		b, err := os.ReadFile(s)
		if err != nil {
			return err
		}
		if err := os.WriteFile(d, b, 0o644); err != nil {
			return err
		}
	}
	return nil
}
