package worldhost_test

// #204 (D-9) mid-game save/load determinism — at the WHOLE-STACK level. The sim
// proves save→load→resume hash-identity on a raw sim.World (litd/sim/save_test.go);
// this proves it through the full worldhost→api→Lua stack on REAL loaded worlds,
// the path the in-game F5/F9 quicksave actually uses. SoT = Game.StateHash()
// (R-FSV-2): saving at tick N, loading into a freshly-loaded world, and resuming M
// ticks must land on the SAME hash as the unbroken run at tick N+M — including a
// world whose Game_Every timers leave the cooperative scheduler suspended across
// the save. X+X=Y: same world, same seed, save+resume vs straight-through → equal.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// fingerprint is the world-identity tag SaveState stamps and LoadState requires;
// the value is arbitrary for the test as long as save and load agree.
const saveFP uint64 = 0x4C49544435415645

func TestWorldSaveResumeHashIdenticalFSV(t *testing.T) {
	// dev-sandbox carries unit/ability/buff state; firstflame-slice runs combat +
	// the firebolt Game_Every timer, exercising scheduler suspension across a save.
	for _, world := range []string{"../../worlds/dev-sandbox", "../../worlds/firstflame-slice"} {
		t.Run(world, func(t *testing.T) {
			const saveAt, resume = 10, 20

			// Unbroken reference run: step to saveAt, save, keep going to saveAt+resume.
			ref, err := worldhost.Load(world, 7, 50_000_000)
			if err != nil {
				t.Fatalf("load ref: %v", err)
			}
			defer ref.Close()
			ref.Game.Advance(saveAt)
			var buf bytes.Buffer
			if err := ref.Game.SaveState(&buf, saveFP); err != nil {
				t.Fatalf("SaveState: %v", err)
			}
			saved := append([]byte(nil), buf.Bytes()...)
			hashAtSave := ref.Game.StateHash()
			ref.Game.Advance(resume)
			unbrokenHash := ref.Game.StateHash()
			t.Logf("FSV %s: hash@%d=%016x  unbroken@%d=%016x  (save=%d bytes)",
				world, saveAt, hashAtSave, saveAt+resume, unbrokenHash, len(saved))

			// Restored run: fresh load (re-runs setup), LoadState the snapshot, resume.
			restored, err := worldhost.Load(world, 7, 50_000_000)
			if err != nil {
				t.Fatalf("load restored: %v", err)
			}
			defer restored.Close()
			if err := restored.Game.LoadState(bytes.NewReader(saved), saveFP); err != nil {
				t.Fatalf("LoadState: %v", err)
			}
			if got := restored.Game.StateHash(); got != hashAtSave {
				t.Fatalf("loaded hash %016x != hash at save %016x", got, hashAtSave)
			}
			restored.Game.Advance(resume)
			resumedHash := restored.Game.StateHash()
			t.Logf("FSV %s: saved@%d -> resume %d = %016x", world, saveAt, resume, resumedHash)

			// X+X=Y: the resumed run is bit-identical to the unbroken run.
			if resumedHash != unbrokenHash {
				t.Fatalf("resume-after-load hash %016x != unbroken %016x", resumedHash, unbrokenHash)
			}
		})
	}
}

func TestWorldLoadFailsClosedFSV(t *testing.T) {
	// Save a real snapshot, then prove LoadState refuses corruption and a wrong
	// fingerprint loudly (no partial load) — the D-9 "load refuses corrupt/version-
	// mismatched saves" guard, at the api stack.
	src, err := worldhost.Load("../../worlds/dev-sandbox", 7, 50_000_000)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	defer src.Close()
	src.Game.Advance(8)
	var buf bytes.Buffer
	if err := src.Game.SaveState(&buf, saveFP); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	good := buf.Bytes()

	// Edge 1 — wrong fingerprint: refused.
	dst1, _ := worldhost.Load("../../worlds/dev-sandbox", 7, 50_000_000)
	defer dst1.Close()
	if err := dst1.Game.LoadState(bytes.NewReader(good), saveFP^0xDEAD); err == nil {
		t.Fatal("LoadState accepted a wrong-fingerprint save (must fail closed)")
	} else {
		t.Logf("FSV wrong-fingerprint refused: %v", err)
	}

	// Edge 2 — truncated/corrupt bytes: refused.
	dst2, _ := worldhost.Load("../../worlds/dev-sandbox", 7, 50_000_000)
	defer dst2.Close()
	corrupt := append([]byte(nil), good[:len(good)/2]...)
	if err := dst2.Game.LoadState(bytes.NewReader(corrupt), saveFP); err == nil {
		t.Fatal("LoadState accepted a truncated save (must fail closed)")
	} else {
		t.Logf("FSV truncated-save refused: %v", err)
	}

	// Edge 3 — empty input: refused.
	dst3, _ := worldhost.Load("../../worlds/dev-sandbox", 7, 50_000_000)
	defer dst3.Close()
	if err := dst3.Game.LoadState(bytes.NewReader(nil), saveFP); err == nil {
		t.Fatal("LoadState accepted empty input (must fail closed)")
	} else {
		t.Logf("FSV empty-input refused: %v", err)
	}
}
