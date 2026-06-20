package luabind

// save.go is the Lua half of mid-game save/load (#270): it serializes the
// suspended-coroutine table that backs Run/PolledWait, to be written alongside
// the sim save (api.Game.SaveState, whose scheduler blob holds the matching
// descriptive (slot,gen) wake records). On load it rebuilds the table to the
// EXACT shape it had at save time — every slot's generation and alive/waiting
// flag, and the free-list order — not merely the parked coroutines.
//
// Why the exact shape matters (determinism, R-FSV-2): the sim state hash
// includes each pending wake record's value-typed State{slot,gen}. A coroutine
// spawned AFTER a restore draws its slot from this table's free list; if the
// table were rebuilt to a different shape, that slot (hence the new record's
// State, hence the hash) would diverge from the unbroken run. Reconstructing the
// table verbatim keeps a restored run bit-identical to the uninterrupted one.
//
// Fail-closed (§2.4, #270): a coroutine whose state cannot be serialized (an
// unpersistable userdata reachable from its stack) is a loud error from
// SaveThread — never a silent drop. A bad magic / truncated blob / unknown chunk
// on load is a loud refusal, never a partial restore.

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"

	lua "github.com/yuin/gopher-lua"
)

// scriptSaveMagic tags the Lua save section and pins its format version. v2
// (#413) added a per-slot waitKind (EventKind a coroutine is parked on, 0 for a
// timer wait) after the flags byte. v3 (#437) interned closed upvalue cells in
// the value-graph blob so cells shared across closures round-trip. v4 (#440)
// replaces the per-coroutine value graphs with ONE shared graph for all alive
// coroutines (schedBlob), so a table/cell shared across coroutines round-trips as
// a single object instead of diverging copies. v5 (#435) folds the world's data
// globals into that same shared graph, so a top-level global (counter/config)
// survives a save/load — and a global whose value is also captured by a
// coroutine round-trips as one object. v6 (#446) folds the OnEvent handler
// closures into that same shared graph too, so a mutable object shared between a
// coroutine and a handler round-trips as one object (the event section now
// carries only the handler kinds/order; the closures live here). An older blob is
// rejected loudly on load.
const scriptSaveMagic = "LITDLUA\x06"

const (
	flagAlive   = 1 << 0
	flagWaiting = 1 << 1
)

// SaveScripts writes the suspended-coroutine table on L to w. Pair it with
// api.Game.SaveState at the same tick boundary. Deterministic: slots are written
// in ascending order. reg resolves coroutine function protos by chunk id (no
// bytecode is embedded). A coroutine with unpersistable state is a loud error.
func SaveScripts(L *lua.LState, reg *ChunkRegistry, w io.Writer) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: SaveScripts: no scheduler bound to this LState")
	}
	handles := GameHandles{G: s.g}

	// Collect the alive coroutines in slot order and serialize them against ONE
	// shared intern pool (#440), so an object shared across coroutines round-trips
	// as a single object. schedBlob.Threads aligns 1:1 with the alive slots, in
	// order.
	alive := make([]*lua.LState, 0, len(s.threads))
	for i := range s.threads {
		if s.threads[i].alive {
			alive = append(alive, s.threads[i].co)
		}
	}
	// Handler closures join the same pool (#446), in registration order, so a
	// mutable object shared between a coroutine and a handler interns once. The
	// matching kinds/order are written separately by SaveEventHandlers.
	hfns := make([]lua.LValue, len(s.eventHandlers))
	for i, h := range s.eventHandlers {
		hfns[i] = h.fn
	}
	sb, err := serializeScheduler(reg, L, alive, s.worldGlobals(), hfns, handles)
	if err != nil {
		return fmt.Errorf("luabind: SaveScripts: %w", err)
	}
	blob, err := json.Marshal(sb)
	if err != nil {
		return fmt.Errorf("luabind: SaveScripts: marshal shared graph: %w", err)
	}

	bw := &errWriter{w: w}
	bw.writeRaw([]byte(scriptSaveMagic))
	bw.u32(uint32(len(s.threads)))
	for i := range s.threads {
		e := &s.threads[i]
		var flags uint8
		if e.alive {
			flags |= flagAlive
		}
		if e.waiting {
			flags |= flagWaiting
		}
		bw.u32(e.gen)
		bw.u8(flags)
		bw.u16(e.waitKind) // #413: EventKind this slot is parked on (0 = timer/none)
	}
	bw.u32(uint32(len(blob)))
	bw.writeRaw(blob)
	bw.u32(uint32(len(s.threadFree)))
	for _, slot := range s.threadFree {
		bw.u32(slot)
	}
	return bw.err
}

// LoadScripts rebuilds L's coroutine table from r (written by SaveScripts),
// resolving each parked coroutine against reg (which must already hold the
// world's chunks). After this the sim scheduler's restored wake records resolve
// to live coroutines by (slot,gen). The scheduler continuation must already be
// registered (Register does this) so the sim save's records reload. A nil
// scheduler, bad magic, truncated data, or unresolvable coroutine is a loud
// error and the table is left cleared (no partial restore).
func LoadScripts(L *lua.LState, reg *ChunkRegistry, r io.Reader) error {
	s := getScheduler(L)
	if s == nil {
		return fmt.Errorf("luabind: LoadScripts: no scheduler bound to this LState")
	}
	handles := GameHandles{G: s.g}
	br := &errReader{r: r}

	magic := br.readRaw(len(scriptSaveMagic))
	if br.err != nil {
		return fmt.Errorf("luabind: LoadScripts: %w", br.err)
	}
	if string(magic) != scriptSaveMagic {
		return fmt.Errorf("luabind: LoadScripts: bad magic %q", magic)
	}

	// Clear the live table; rebuild it exactly from the blob.
	s.threads = nil
	s.threadFree = nil
	s.pending = 0

	n := br.u32()
	if br.err != nil {
		return fmt.Errorf("luabind: LoadScripts: %w", br.err)
	}
	threads := make([]scriptThread, n)
	pending := 0
	// Per-slot headers first (the alive slots, in order, map 1:1 to the shared
	// blob's threads).
	var aliveSlots []uint32
	for i := uint32(0); i < n; i++ {
		gen := br.u32()
		flags := br.u8()
		waitKind := br.u16() // #413
		if br.err != nil {
			return fmt.Errorf("luabind: LoadScripts: slot %d header: %w", i, br.err)
		}
		e := &threads[i]
		e.gen = gen
		e.alive = flags&flagAlive != 0
		e.waiting = flags&flagWaiting != 0
		e.waitKind = waitKind
		if e.alive {
			aliveSlots = append(aliveSlots, i)
			if e.waiting {
				pending++
			}
		}
	}

	// One shared value graph for all alive coroutines (#440).
	blobLen := br.u32()
	blob := br.readRaw(int(blobLen))
	if br.err != nil {
		return fmt.Errorf("luabind: LoadScripts: shared graph: %w", br.err)
	}
	var sb schedBlob
	if err := json.Unmarshal(blob, &sb); err != nil {
		return fmt.Errorf("luabind: LoadScripts: shared graph unmarshal: %w", err)
	}
	if len(sb.Threads) != len(aliveSlots) {
		return fmt.Errorf("luabind: LoadScripts: shared graph has %d coroutines but %d slots are alive", len(sb.Threads), len(aliveSlots))
	}
	restored, topFns, globals, handlerFns, err := loadScheduler(reg, L, &sb, handles)
	if err != nil {
		return fmt.Errorf("luabind: LoadScripts: restore: %w", err)
	}
	for k, slot := range aliveSlots {
		threads[slot].co = restored[k]
		threads[slot].fn = topFns[k]
	}
	// Restore the world's data globals into the global table (#435). The world
	// chunk is re-registered but never re-run, so without this a top-level
	// `counter = 0` would read nil after load; the persister already unified any
	// global shared with a coroutine to a single object.
	for _, gv := range globals {
		L.SetGlobal(gv.Key, gv.Val)
	}
	// Bind the restored OnEvent handler closures (#446) back to the subscriptions
	// RestoreEventHandlers pre-registered (by slot index) before the sim restore.
	// The handler set is now part of the SAME shared pool, so a cell/table shared
	// between a handler and a coroutine is one object. Count must match the kinds
	// the event section restored, or the save is internally inconsistent.
	if len(handlerFns) != len(s.eventHandlers) {
		return fmt.Errorf("luabind: LoadScripts: %d handler closures but %d handler subscriptions registered", len(handlerFns), len(s.eventHandlers))
	}
	for i, hf := range handlerFns {
		fn, ok := hf.(*lua.LFunction)
		if !ok {
			return fmt.Errorf("luabind: LoadScripts: handler %d decoded to %s, not a function", i, hf.Type())
		}
		s.eventHandlers[i].fn = fn
	}

	nFree := br.u32()
	if br.err != nil {
		return fmt.Errorf("luabind: LoadScripts: free list: %w", br.err)
	}
	free := make([]uint32, nFree)
	for i := uint32(0); i < nFree; i++ {
		free[i] = br.u32()
	}
	if br.err != nil {
		return fmt.Errorf("luabind: LoadScripts: free list: %w", br.err)
	}

	s.threads = threads
	s.threadFree = free
	s.pending = pending
	// #413: an event-parked coroutine (waitKind != 0) needs no re-subscription here.
	// The dispatcher handler is registered at VM setup (RegisterScriptEventDispatcher,
	// before this LoadState), and the kind→handler subscription is serialized sim
	// state that LoadState already restored — so a post-load event fires straight
	// through to dispatchEvent, which finds the restored waiter by (slot, waitKind).
	return nil
}

// errWriter / errReader are minimal sticky-error binary helpers (little-endian),
// so the save/load bodies read top-to-bottom without an error check per field.

type errWriter struct {
	w   io.Writer
	err error
}

func (b *errWriter) writeRaw(p []byte) {
	if b.err != nil {
		return
	}
	_, b.err = b.w.Write(p)
}

func (b *errWriter) u8(v uint8) { b.writeRaw([]byte{v}) }

func (b *errWriter) u16(v uint16) {
	var s [2]byte
	binary.LittleEndian.PutUint16(s[:], v)
	b.writeRaw(s[:])
}

func (b *errWriter) u32(v uint32) {
	var s [4]byte
	binary.LittleEndian.PutUint32(s[:], v)
	b.writeRaw(s[:])
}

type errReader struct {
	r   io.Reader
	err error
}

func (b *errReader) readRaw(n int) []byte {
	if b.err != nil {
		return nil
	}
	p := make([]byte, n)
	if _, b.err = io.ReadFull(b.r, p); b.err != nil {
		return nil
	}
	return p
}

func (b *errReader) u8() uint8 {
	p := b.readRaw(1)
	if b.err != nil {
		return 0
	}
	return p[0]
}

func (b *errReader) u16() uint16 {
	p := b.readRaw(2)
	if b.err != nil {
		return 0
	}
	return binary.LittleEndian.Uint16(p)
}

func (b *errReader) u32() uint32 {
	p := b.readRaw(4)
	if b.err != nil {
		return 0
	}
	return binary.LittleEndian.Uint32(p)
}
