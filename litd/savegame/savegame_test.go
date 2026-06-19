package savegame

// #204 FSV (D-9 mid-game save/load). SoT = Game.StateHash() (R-FSV-2): a match
// saved through the container at an arbitrary tick, loaded into a fresh runtime,
// and run to completion must reach a state hash bit-identical to the unbroken run.
// Plus fail-closed container checks: corrupt / version-mismatched / wrong-world /
// truncated saves are refused loudly with NO partial application.

import (
	"bytes"
	"encoding/binary"
	"hash/crc32"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

const fp = uint64(0x5A5E6A3E)

// worldScript creates its own unit (so LoadState restores it) and runs a coroutine
// that parks on a PolledWait, then sets a marker life — a suspended scheduler
// record the save must round-trip. Mirrors the proven luabind round-trip scenario.
const worldScript = `Run(function()
	local u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=100, y=100}, 0)
	Unit_SetLife(u, 30)
	PolledWait(0.20)
	Unit_SetLife(u, 77)
end)`

func newGame(t *testing.T) (*api.Game, *lua.LState) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L
}

// runWorld registers worldScript in reg and runs it via the registered proto, so
// its coroutine's protos are resolvable for save (matches the world loader).
func runWorld(t *testing.T, L *lua.LState, reg *luabind.ChunkRegistry) {
	t.Helper()
	cid, err := reg.Register("world", worldScript)
	if err != nil {
		t.Fatalf("register chunk: %v", err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("ResolveProto: %v", err)
	}
	L.Push(L.NewFunctionFromProto(proto))
	if err := L.PCall(0, 0, nil); err != nil {
		t.Fatalf("run world chunk: %v", err)
	}
}

// unbroken runs the scenario straight through totalTicks and returns the hash.
func unbroken(t *testing.T, totalTicks int) uint64 {
	g, L := newGame(t)
	defer L.Close()
	reg := luabind.NewChunkRegistry()
	defer reg.Close()
	runWorld(t, L, reg)
	g.Advance(totalTicks)
	return g.StateHash()
}

// validSave runs to saveTick and returns a complete container.
func validSave(t *testing.T, saveTick int) []byte {
	g, L := newGame(t)
	defer L.Close()
	reg := luabind.NewChunkRegistry()
	defer reg.Close()
	runWorld(t, L, reg)
	g.Advance(saveTick)
	var buf bytes.Buffer
	if err := Write(&buf, g, L, reg, fp); err != nil {
		t.Fatalf("Write: %v", err)
	}
	return buf.Bytes()
}

func TestMidGameSaveLoadHashIdenticalFSV(t *testing.T) {
	const total = 8 // 0.20s wait = 4 ticks; run well past it
	ref := unbroken(t, total)
	if ref2 := unbroken(t, total); ref2 != ref {
		t.Fatalf("unbroken run not reproducible: %#x != %#x", ref2, ref)
	}

	// Save at tick 1 (mid-wait, coroutine parked), restore into a FRESH runtime,
	// run the remaining ticks, compare to the unbroken hash.
	container := validSave(t, 1)
	t.Logf("FSV #204 save@tick1: container=%d bytes", len(container))

	gB, LB := newGame(t)
	defer LB.Close()
	regB := luabind.NewChunkRegistry()
	defer regB.Close()
	if _, err := regB.Register("world", worldScript); err != nil { // re-register, do NOT run
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(container), gB, LB, regB, fp); err != nil {
		t.Fatalf("Load: %v", err)
	}
	if n := luabind.PendingScriptWaits(LB); n != 1 {
		t.Fatalf("post-restore want 1 parked coroutine, got %d", n)
	}
	gB.Advance(total - 1) // saved at tick 1
	got := gB.StateHash()
	if got != ref {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x — mid-game save/load not bit-identical", got, ref)
	}
	t.Logf("FSV #204 keystone: save@1 → fresh restore (1 parked coroutine) → resume → StateHash %#x == unbroken %#x", got, ref)
}

// save→load→save: the re-saved container's embedded state must reload to the same
// hash (idempotent persistence; edge 4).
func TestResaveStableFSV(t *testing.T) {
	c1 := validSave(t, 1)
	g, L := newGame(t)
	defer L.Close()
	reg := luabind.NewChunkRegistry()
	defer reg.Close()
	if _, err := reg.Register("world", worldScript); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(c1), g, L, reg, fp); err != nil {
		t.Fatalf("Load c1: %v", err)
	}
	var c2 bytes.Buffer
	if err := Write(&c2, g, L, reg, fp); err != nil {
		t.Fatalf("re-Write: %v", err)
	}
	// Load the re-saved container into yet another fresh runtime; advance both the
	// once- and twice-saved games to completion and compare.
	g2, L2 := newGame(t)
	defer L2.Close()
	reg2 := luabind.NewChunkRegistry()
	defer reg2.Close()
	if _, err := reg2.Register("world", worldScript); err != nil {
		t.Fatalf("re-register2: %v", err)
	}
	if err := Load(bytes.NewReader(c2.Bytes()), g2, L2, reg2, fp); err != nil {
		t.Fatalf("Load c2: %v", err)
	}
	g.Advance(7)
	g2.Advance(7)
	if g.StateHash() != g2.StateHash() {
		t.Fatalf("save→load→save diverged: %#x != %#x", g.StateHash(), g2.StateHash())
	}
	t.Logf("FSV #204 resave: save→load→save→load reaches identical final hash %#x", g.StateHash())
}

// scenario10k exercises the scheduler-suspension save path at scale: six coroutines
// each create + walk their own unit (in-Lua handles, so they marshal cleanly) over
// 600 hops spaced 10 ticks apart = 6,000 ticks of activity — so at the tick-5,000
// save point all six are PARKED mid-wait, and they complete before tick 10,000.
// (api.OnEvent Lua-closure handlers are intentionally NOT used here — they do not
// survive save/load; see the filed discovery. The reserved scheduler waits do.)
const scenario10k = `for i=1,6 do
	Run(function()
		local u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=i*10, y=0}, 0)
		for k=1,600 do
			local p = Unit_Position(u)
			Unit_SetPosition(u, {x = p.x + 1, y = p.y})
			PolledWait(0.5)
		end
	end)
end`

func runScenario10k(t *testing.T) (*api.Game, *lua.LState, *luabind.ChunkRegistry) {
	t.Helper()
	g, L := newGame(t)
	reg := luabind.NewChunkRegistry()
	cid, err := reg.Register("world10k", scenario10k)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("ResolveProto: %v", err)
	}
	L.Push(L.NewFunctionFromProto(proto))
	if err := L.PCall(0, 0, nil); err != nil {
		t.Fatalf("run scenario: %v", err)
	}
	return g, L, reg
}

// #430 / #271 edge: a mid-run save/restore at scale (tick 5,000 of 10,000) through
// the real savegame container must reach a final hash bit-identical to the unbroken
// 10k run. This is the determinism thesis on the actual save path, not a toy. It
// uses its own self-consistent comparison (not #271's committed golden, which the
// existing single-platform test guards), so the two tests stay independent.
func TestMidGameSaveLoad10kFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick save/restore scenario is slow; run without -short")
	}
	// Unbroken reference, run twice to prove reproducibility.
	gR, LR, regR := runScenario10k(t)
	gR.Advance(10000)
	refHash := gR.StateHash()
	LR.Close()
	regR.Close()
	gR2, LR2, regR2 := runScenario10k(t)
	gR2.Advance(10000)
	if h2 := gR2.StateHash(); h2 != refHash {
		t.Fatalf("unbroken 10k not reproducible: %#x != %#x", h2, refHash)
	}
	LR2.Close()
	regR2.Close()

	// Save at tick 5,000 through the container, restore into a fresh runtime, run
	// the remaining 5,000.
	gA, LA, regA := runScenario10k(t)
	gA.Advance(5000)
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@5000: %v", err)
	}
	parked := luabind.PendingScriptWaits(LA)
	LA.Close()
	regA.Close()
	t.Logf("FSV #430 save@5000: container=%d bytes, %d coroutines parked", buf.Len(), parked)

	gB, LB := newGame(t)
	defer LB.Close()
	regB := luabind.NewChunkRegistry()
	defer regB.Close()
	if _, err := regB.Register("world10k", scenario10k); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(buf.Bytes()), gB, LB, regB, fp); err != nil {
		t.Fatalf("Load@5000: %v", err)
	}
	if n := luabind.PendingScriptWaits(LB); n != parked {
		t.Fatalf("post-restore parked=%d, want %d (wake schedule did not round-trip)", n, parked)
	}
	gB.Advance(5000)
	got := gB.StateHash()
	if got != refHash {
		t.Fatalf("HASH MISMATCH at 10k: restored %#x != unbroken %#x — mid-run save/restore not bit-identical", got, refHash)
	}
	t.Logf("FSV #430 keystone: save@5000 (%d coroutines parked) → fresh restore → run to 10000 → StateHash %#x == unbroken %#x",
		parked, got, refHash)
}

func withFreshCRC(body []byte) []byte {
	out := make([]byte, len(body)+4)
	copy(out, body)
	binary.LittleEndian.PutUint32(out[len(body):], crc32.ChecksumIEEE(body))
	return out
}

// Fail-closed container edges: every malformation is refused with no partial load.
func TestCorruptSaveRefusedFSV(t *testing.T) {
	good := validSave(t, 1)

	tryLoad := func(name string, data []byte, wantMsg string) {
		g, L := newGame(t)
		defer L.Close()
		reg := luabind.NewChunkRegistry()
		defer reg.Close()
		reg.Register("world", worldScript)
		hashBefore := g.StateHash()
		err := Load(bytes.NewReader(data), g, L, reg, fp)
		if err == nil {
			t.Fatalf("%s: must be refused, got nil error", name)
		}
		// No partial application: a fresh game's hash is unchanged by a refused load.
		if g.StateHash() != hashBefore {
			t.Fatalf("%s: refused load still mutated the game (partial load!)", name)
		}
		t.Logf("FSV #204 refuse %-18s → %v", name, err)
		_ = wantMsg
	}

	// (a) single-byte corruption mid-payload → CRC mismatch.
	corrupt := append([]byte(nil), good...)
	corrupt[20] ^= 0xFF
	tryLoad("byte-flip", corrupt, "CRC")

	// (b) truncated tail → too short / CRC.
	tryLoad("truncated", good[:len(good)-10], "truncated")

	// (c) version mismatch (re-CRC'd so it passes integrity, fails the version gate).
	body := append([]byte(nil), good[:len(good)-4]...)
	body[8] = FormatVersion + 9
	tryLoad("bad-version", withFreshCRC(body), "version")

	// (d) bad magic (re-CRC'd).
	body2 := append([]byte(nil), good[:len(good)-4]...)
	body2[0] = 'X'
	tryLoad("bad-magic", withFreshCRC(body2), "magic")

	// (e) wrong world fingerprint (valid file, mismatched world).
	g, L := newGame(t)
	defer L.Close()
	reg := luabind.NewChunkRegistry()
	defer reg.Close()
	reg.Register("world", worldScript)
	if err := Load(bytes.NewReader(good), g, L, reg, fp+1); err == nil {
		t.Fatal("wrong-fingerprint load must be refused")
	} else {
		t.Logf("FSV #204 refuse wrong-fingerprint → %v", err)
	}
}
