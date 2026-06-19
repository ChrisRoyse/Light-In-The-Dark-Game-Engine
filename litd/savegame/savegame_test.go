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

// runChunk registers src as a named chunk and runs it through the registered
// proto (so coroutine/handler protos resolve for save), returning the registry.
func runChunk(t *testing.T, L *lua.LState, name, src string) *luabind.ChunkRegistry {
	t.Helper()
	reg := luabind.NewChunkRegistry()
	cid, err := reg.Register(name, src)
	if err != nil {
		t.Fatalf("register %s: %v", name, err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("ResolveProto: %v", err)
	}
	L.Push(L.NewFunctionFromProto(proto))
	if err := L.PCall(0, 0, nil); err != nil {
		t.Fatalf("run chunk %s: %v", name, err)
	}
	return reg
}

// liveUnits returns (count, X-of-lone-unit) for the sole-survivor assertions
// below; X is NaN-safe only in that we never call it with count != 1.
func liveUnits(g *api.Game) (int, float64) {
	us := g.AllUnits(nil)
	if len(us) == 1 {
		return 1, us[0].Position().X
	}
	return len(us), 0
}

// #433: an OnEvent handler registered by a world must survive save/load. Before
// the fix, LoadState failed closed ("subscription for kind 1 references
// unregistered HandlerID") because the kind→HandlerID subscription serialized but
// the handler was never re-registered.
//
// SoT = the SIM (Game.StateHash + the live-unit set), NOT a Lua global: mid-game
// save deliberately does not re-run the world chunk, so Lua DATA globals set at
// top level are not persisted (separate limitation, filed). The handler is made
// self-contained — on a unit death it CREATES a marker unit at x=20, referencing
// only re-bound global FUNCTIONS, no captured data. This proves three things:
//   (a) the container loads (no dangling-HandlerID refusal),
//   (b) the restored handler actually FIRES — the coroutine kills the original
//       unit (x=10) at tick 1,000 (after the save), the death event drives the
//       handler, and the sole survivor is the handler's NEW unit at x=20 (X+X=Y:
//       had the handler not fired, zero units would remain),
//   (c) the final Game.StateHash equals the unbroken run, bit-for-bit.
func TestEventHandlerSurvivesSaveLoadFSV(t *testing.T) {
	// kind 1 = unit death. The handler creates a marker unit at x=20 (distinct
	// from the original at x=10). The coroutine creates the original, waits 50s
	// (1,000 ticks), then kills it — so a save at tick 500 parks the coroutine
	// mid-wait and the death fires only in the restored run (tick 1,000).
	const src = `OnEvent(1, function()
	Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=20, y=0}, 0)
end)
Run(function()
	local u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=10, y=0}, 0)
	PolledWait(50.0)
	Unit_Kill(u)
end)`
	const saveTick, total = 500, 1200 // kill at 1000; save before, finish after

	// Unbroken reference (run twice for reproducibility) + final unit SoT.
	gR, LR := newGame(t)
	regR := runChunk(t, LR, "evt", src)
	gR.Advance(total)
	refHash := gR.StateHash()
	refN, refX := liveUnits(gR)
	LR.Close()
	regR.Close()
	if refN != 1 || refX < 19 || refX > 21 {
		t.Fatalf("unbroken final: want 1 unit at x≈20 (handler's marker), got %d units x=%.1f", refN, refX)
	}
	gR2, LR2 := newGame(t)
	regR2 := runChunk(t, LR2, "evt", src)
	gR2.Advance(total)
	if h := gR2.StateHash(); h != refHash {
		t.Fatalf("unbroken not reproducible: %#x != %#x", h, refHash)
	}
	LR2.Close()
	regR2.Close()

	// Save mid-wait, BEFORE the kill: exactly the original unit (x=10) is alive.
	gA, LA := newGame(t)
	regA := runChunk(t, LA, "evt", src)
	gA.Advance(saveTick)
	if n, x := liveUnits(gA); n != 1 || x < 9 || x > 11 {
		t.Fatalf("pre-save: want 1 unit at x≈10 (original, not yet killed), got %d units x=%.1f", n, x)
	}
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d: %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	// Restore into a fresh runtime — the registry must hold the world chunk, as
	// the world loader would.
	gB, LB := newGame(t)
	defer LB.Close()
	regB := luabind.NewChunkRegistry()
	defer regB.Close()
	if _, err := regB.Register("evt", src); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(buf.Bytes()), gB, LB, regB, fp); err != nil {
		t.Fatalf("Load@%d: %v (the #433 regression — handler subscription not restored)", saveTick, err)
	}
	// SoT right after restore: still the original unit at x≈10, coroutine parked.
	if n, x := liveUnits(gB); n != 1 || x < 9 || x > 11 {
		t.Fatalf("post-restore pre-advance: want 1 unit at x≈10, got %d units x=%.1f", n, x)
	}

	// Drive past the kill (tick 1000): the coroutine kills the original; the
	// RESTORED handler must fire and leave its marker as the sole survivor.
	gB.Advance(total - saveTick)
	gotN, gotX := liveUnits(gB)
	if gotN != 1 {
		t.Fatalf("restored final: %d units — restored OnEvent handler did NOT fire (expected its marker as sole survivor)", gotN)
	}
	if gotX < 19 || gotX > 21 {
		t.Fatalf("restored survivor at x=%.1f, want x≈20 (handler's marker) — wrong unit survived", gotX)
	}
	if gotHash := gB.StateHash(); gotHash != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x", gotHash, refHash)
	}
	t.Logf("FSV #433: OnEvent handler restored & fired — original (x=10) killed post-restore, handler's marker (x=%.1f) is sole survivor; StateHash %#x == unbroken", gotX, refHash)
}

// #433 fail-closed edge (§2.4): an OnEvent handler that captures an upvalue
// cannot yet be persisted (a proto reference alone can't rebind the captured
// cell). SaveEventHandlers must REFUSE loudly, not silently drop the handler.
func TestUpvalueHandlerRejectedFSV(t *testing.T) {
	// `n` is a local of the chunk → an UPVALUE of the handler closure.
	const src = `local n = 0
OnEvent(1, function() n = n + 1 end)`
	g, L := newGame(t)
	defer L.Close()
	reg := runChunk(t, L, "upval", src)
	defer reg.Close()
	g.Advance(2)

	var buf bytes.Buffer
	err := Write(&buf, g, L, reg, fp)
	if err == nil {
		t.Fatal("save of an upvalue-capturing handler must be refused, got nil error")
	}
	if !bytes.Contains([]byte(err.Error()), []byte("upvalue")) {
		t.Fatalf("expected an upvalue-rejection error, got: %v", err)
	}
	if buf.Len() != 0 {
		t.Fatalf("refused save still wrote %d bytes — must write nothing", buf.Len())
	}
	t.Logf("FSV #433 fail-closed: upvalue-capturing handler refused → %v", err)
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
