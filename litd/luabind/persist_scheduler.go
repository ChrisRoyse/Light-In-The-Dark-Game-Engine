package luabind

// Shared-pool scheduler persistence (#440). The per-thread persister
// (persist_thread.go) serializes each suspended coroutine into its OWN value
// graph, so an object (a closed upvalue cell, or a table) reachable from two
// scheduler coroutines is interned once PER graph and reconstructed as two
// independent objects — a live mutation seen through both pre-save becomes two
// diverging copies post-restore, a silent Game.StateHash break (R-FSV-2).
//
// This layer fixes that for the scheduler's coroutine table: ALL alive
// coroutines serialize against ONE shared intern pool (schedBlob.Graph), with
// each coroutine keeping only its execution shape (threadMeta) and the index
// range of its roots into the shared graph. A table/cell shared across
// coroutines now encodes once and every reference rebinds to the one object.
//
// Open upvalues are thread-local (they alias one coroutine's live registers), so
// each closure records its owner thread (sfunc.Owner) and rebinds its open
// upvalues to that thread on restore. A closure with an open upvalue that is
// reachable from a NON-owner coroutine (a live-frame closure passed across
// coroutines — pathological) is refused loudly at encode time by
// LitdUpvalueViews, never silently mis-bound.
//
// Out of scope here: sharing between a coroutine and an OnEvent handler. Handlers
// serialize in a SEPARATE save section restored before the sim (load-order
// constraint, savegame.go), so they cannot share this pool; that residual case
// stays guarded by DetectCrossThreadSharing.

import (
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// threadMeta is a parked coroutine's execution shape — everything ThreadImage
// carries except the value graph, which in the shared-pool format lives in
// schedBlob.Graph. StackNil marks register slots that were Go-nil.
type threadMeta struct {
	Dead         bool         `json:"dead"`
	Wrapped      bool         `json:"wrapped"`
	InstrLimited bool         `json:"instrLimited"`
	InstrLeft    int64        `json:"instrLeft"`
	MemLimited   bool         `json:"memLimited"`
	MemLeft      int64        `json:"memLeft"`
	NumRegisters int          `json:"nreg"`
	Frames       []FrameImage `json:"frames"`
	StackNil     []bool       `json:"stackNil,omitempty"`
}

// schedThread is one alive scheduler slot's persisted form: its execution shape
// plus the start index of its roots (NumRegisters registers followed by one
// function per frame) into the shared graph's Roots.
type schedThread struct {
	Meta      threadMeta `json:"meta"`
	RootStart int        `json:"rootStart"`
}

// schedBlob is the whole scheduler's suspended state: one shared value graph for
// all alive coroutines, plus a per-alive-slot execution shape + root range.
type schedBlob struct {
	Threads []schedThread `json:"threads"`
	Graph   valuesBlob    `json:"graph"`
}

// threadSnapshot extracts a parked coroutine's serializable value roots
// (registers followed by each frame's function, the same root layout
// SaveThread uses) and its execution meta, WITHOUT encoding them — so a shared
// encoder can intern objects across several coroutines. Fails loudly on a
// Go-function frame, exactly as SaveThread would.
func threadSnapshot(th *lua.LState) (vals []lua.LValue, meta threadMeta, err error) {
	v := th.LitdSnapshot()
	meta = threadMeta{
		Dead: v.Dead, Wrapped: v.Wrapped,
		InstrLimited: v.InstrLimited, InstrLeft: v.InstrLeft,
		MemLimited: v.MemLimited, MemLeft: v.MemLeft,
		NumRegisters: len(v.Stack),
	}
	for i, f := range v.Frames {
		if f.Fn == nil || f.Fn.IsG || f.Fn.Proto == nil {
			return nil, threadMeta{}, fmt.Errorf("luabind: frame %d is a Go-function frame — cannot persist", i)
		}
		meta.Frames = append(meta.Frames, FrameImage{
			Idx: f.Idx, Pc: f.Pc, Base: f.Base, LocalBase: f.LocalBase,
			ReturnBase: f.ReturnBase, NArgs: f.NArgs, NRet: f.NRet, TailCall: f.TailCall,
		})
	}
	vals = make([]lua.LValue, 0, len(v.Stack)+len(v.Frames))
	meta.StackNil = make([]bool, len(v.Stack))
	anyNil := false
	for i, sv := range v.Stack {
		if sv == nil {
			vals = append(vals, lua.LNil)
			meta.StackNil[i] = true
			anyNil = true
			continue
		}
		vals = append(vals, sv)
	}
	if !anyNil {
		meta.StackNil = nil
	}
	for _, f := range v.Frames {
		vals = append(vals, f.Fn)
	}
	return vals, meta, nil
}

// serializeScheduler encodes every alive coroutine into ONE shared value graph,
// interning tables / closed cells / closures / userdata across all of them so a
// cross-coroutine shared object round-trips as a single object (#440). Each
// coroutine is encoded under its own owner (so open upvalues classify against
// the right register file) and records its root range. A cross-thread
// open-upvalue closure reached by a non-owner coroutine is refused loudly by the
// encoder (LitdUpvalueViews), never silently mis-bound.
func serializeScheduler(reg *ChunkRegistry, alive []*lua.LState, handles HandleMarshaler) (schedBlob, error) {
	e := &vEncoder{
		ids:       map[*lua.LTable]int{},
		fnIDs:     map[*lua.LFunction]int{},
		cellIDs:   map[*lua.Upvalue]int{},
		threadIDs: map[*lua.LState]int{},
		udIDs:     map[*lua.LUserData]int{},
		reg:       reg,
		handles:   handles,
	}
	var sb schedBlob
	var roots []sval
	for idx, th := range alive {
		vals, meta, err := threadSnapshot(th)
		if err != nil {
			return schedBlob{}, fmt.Errorf("luabind: coroutine %d: %w", idx, err)
		}
		e.owner = th
		e.curThread = idx
		start := len(roots)
		for i, v := range vals {
			r, err := e.encode(v)
			if err != nil {
				return schedBlob{}, fmt.Errorf("luabind: coroutine %d value %d: %w", idx, i, err)
			}
			roots = append(roots, r)
		}
		sb.Threads = append(sb.Threads, schedThread{Meta: meta, RootStart: start})
	}
	sb.Graph = valuesBlob{
		Roots: roots, Tables: e.tables, Funcs: e.funcs,
		Cells: e.cells, Threads: e.threads, UserData: e.uds,
	}
	return sb, nil
}

// loadScheduler reconstructs all alive coroutines from a shared-pool schedBlob.
// It decodes the shared graph ONCE, slices each coroutine's roots, rebuilds each
// resumable LState, then wires every closure's upvalues — open upvalues to their
// owner thread, closed cells to the shared interned cells — so cross-coroutine
// shared objects come back as one object. Returns the threads and their top-frame
// functions, indexed as schedBlob.Threads (i.e. per alive slot, in order).
func loadScheduler(reg *ChunkRegistry, parent *lua.LState, sb *schedBlob, handles HandleMarshaler) ([]*lua.LState, []*lua.LFunction, error) {
	d, err := newGraphDecoder(parent, reg, &sb.Graph, handles)
	if err != nil {
		return nil, nil, err
	}
	roots, err := d.roots()
	if err != nil {
		return nil, nil, err
	}
	threads := make([]*lua.LState, len(sb.Threads))
	topFns := make([]*lua.LFunction, len(sb.Threads))
	for k := range sb.Threads {
		st := &sb.Threads[k]
		nReg, nFrames, start := st.Meta.NumRegisters, len(st.Meta.Frames), st.RootStart
		if nReg < 0 || start < 0 || start+nReg+nFrames > len(roots) {
			return nil, nil, fmt.Errorf("luabind: coroutine %d root range [%d,%d) out of %d shared roots", k, start, start+nReg+nFrames, len(roots))
		}
		v := &lua.LitdThreadView{
			Dead: st.Meta.Dead, Wrapped: st.Meta.Wrapped,
			InstrLimited: st.Meta.InstrLimited, InstrLeft: st.Meta.InstrLeft,
			MemLimited: st.Meta.MemLimited, MemLeft: st.Meta.MemLeft,
		}
		v.Stack = make([]lua.LValue, nReg)
		for i := 0; i < nReg; i++ {
			if i < len(st.Meta.StackNil) && st.Meta.StackNil[i] {
				v.Stack[i] = nil // restore Go-nil register
				continue
			}
			v.Stack[i] = roots[start+i]
		}
		var topFn *lua.LFunction
		for fi, fim := range st.Meta.Frames {
			fn, ok := roots[start+nReg+fi].(*lua.LFunction)
			if !ok {
				return nil, nil, fmt.Errorf("luabind: coroutine %d frame %d function root is %s, not a function", k, fi, roots[start+nReg+fi].Type())
			}
			topFn = fn
			v.Frames = append(v.Frames, lua.LitdFrameView{
				Fn: fn, Idx: fim.Idx, Pc: fim.Pc, Base: fim.Base, LocalBase: fim.LocalBase,
				ReturnBase: fim.ReturnBase, NArgs: fim.NArgs, NRet: fim.NRet, TailCall: fim.TailCall,
			})
		}
		threads[k] = parent.LitdRestoreThread(v)
		topFns[k] = topFn
	}
	// All coroutine register files now exist; wire every closure's upvalues —
	// open upvalues into their OWNER coroutine, closed cells to the shared pool.
	if err := d.wireUpvaluesMulti(threads); err != nil {
		return nil, nil, err
	}
	return threads, topFns, nil
}

// wireUpvaluesMulti is wireUpvalues for the shared scheduler pool: a closure's
// OPEN upvalues bind to its owner thread (sfunc.Owner), not a single thread.
// Closed cells are populated once and shared across every closure that closed
// over them (#437), so a cell shared across coroutines is one object.
func (d *graphDecoder) wireUpvaluesMulti(threads []*lua.LState) error {
	for k := range d.blob.Cells {
		val, err := d.decode(d.blob.Cells[k])
		if err != nil {
			return fmt.Errorf("luabind: upvalue cell %d: %w", k, err)
		}
		d.cellPool[k].SetValue(val)
	}
	for i, sf := range d.blob.Funcs {
		fn := d.fnPool[i]
		for j, up := range sf.Upvals {
			if up.Open {
				if sf.Owner < 0 || sf.Owner >= len(threads) {
					return fmt.Errorf("luabind: closure %d open upvalue %d: owner thread %d out of range (%d threads)", i, j, sf.Owner, len(threads))
				}
				threads[sf.Owner].LitdBindOpenUpvalue(fn, j, up.Index)
				continue
			}
			if up.Cell < 0 || up.Cell >= len(d.cellPool) {
				return fmt.Errorf("luabind: closure %d upvalue %d: cell ref %d out of range (%d cells)", i, j, up.Cell, len(d.cellPool))
			}
			fn.LitdSetUpvalueCell(j, d.cellPool[up.Cell])
		}
	}
	return nil
}
