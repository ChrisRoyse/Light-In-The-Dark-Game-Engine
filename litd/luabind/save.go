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
// (#413) adds a per-slot waitKind (EventKind a coroutine is parked on, 0 for a
// timer wait) after the flags byte; a v1 blob is rejected loudly on load.
const scriptSaveMagic = "LITDLUA\x02"

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
		if e.alive {
			// A live slot at a tick boundary is a parked coroutine — serialize it.
			img, err := SaveThread(reg, e.co, handles)
			if err != nil {
				return fmt.Errorf("luabind: SaveScripts: slot %d: %w", i, err)
			}
			blob, err := json.Marshal(img)
			if err != nil {
				return fmt.Errorf("luabind: SaveScripts: marshal slot %d: %w", i, err)
			}
			bw.u32(uint32(len(blob)))
			bw.writeRaw(blob)
		}
	}
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
			blobLen := br.u32()
			blob := br.readRaw(int(blobLen))
			if br.err != nil {
				return fmt.Errorf("luabind: LoadScripts: slot %d body: %w", i, br.err)
			}
			var img ThreadImage
			if err := json.Unmarshal(blob, &img); err != nil {
				return fmt.Errorf("luabind: LoadScripts: slot %d unmarshal: %w", i, err)
			}
			th, topFn, err := LoadThread(reg, L, &img, handles)
			if err != nil {
				return fmt.Errorf("luabind: LoadScripts: slot %d restore: %w", i, err)
			}
			e.co = th
			e.fn = topFn
			if e.waiting {
				pending++
			}
		}
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
