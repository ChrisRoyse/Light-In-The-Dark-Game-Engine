package luabind

// Cross-process thread persistence (#264) — the layer that turns the fork's
// in-process snapshot/restore primitives (LitdSnapshot / LitdRestoreThread)
// into a save artifact that survives process exit and a cold restart.
//
// A suspended coroutine serializes as one shared value graph (the register file
// PLUS every call frame's executing function) followed by per-frame execution
// cursors. Functions are encoded as a (chunk-id, proto-path) reference mapped
// through the ChunkRegistry by pointer identity — NEVER embedded bytecode — so
// the save carries no code and a content-hash mismatch on load is a loud "world
// content changed" error, not silent corruption. Because registers and frame
// functions share one interned graph, a table/closure aliased across registers,
// frames, and upvalues round-trips as a single object; open upvalues rebind to
// the restored register file (and shared cells coincide).
//
// LoadThread reconstructs against a COLD registry (a fresh process re-registers
// the world's chunks, which content-address to the same ids) and rebuilds a
// resumable LState via LitdRestoreThread.
//
// Persists: the data subset (nil/bool/number/string/table, cycles, shared
// identity), Lua closures (open+closed upvalues), and nested coroutines.
// Fails closed: a Go-function frame/value, and userdata (a host object — handle
// rebind is gated on the binding-layer handle store, #267).

import (
	"encoding/json"
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// FrameImage is one serialized call frame's execution cursor and register
// window. The frame's executing function is serialized in the shared graph (as
// a trailing root, one per frame in order) so its upvalues share identity with
// the registers; FrameImage itself carries no proto reference.
type FrameImage struct {
	Idx        int `json:"idx"`
	Pc         int `json:"pc"`
	Base       int `json:"base"`
	LocalBase  int `json:"localBase"`
	ReturnBase int `json:"returnBase"`
	NArgs      int `json:"nargs"`
	NRet       int `json:"nret"`
	TailCall   int `json:"tailcall"`
}

// ThreadImage is the serializable form of a suspended LState. Stack is one
// shared value graph whose roots are the register slots (NumRegisters of them)
// followed by each frame's executing function, in frame order — so a value
// aliased across registers, frames, and upvalues round-trips as one object.
// StackNil marks register slots that were Go-nil (uninitialized) so they
// restore as Go-nil rather than LNil.
type ThreadImage struct {
	Dead         bool            `json:"dead"`
	Wrapped      bool            `json:"wrapped"`
	InstrLimited bool            `json:"instrLimited"`
	InstrLeft    int64           `json:"instrLeft"`
	MemLimited   bool            `json:"memLimited"`
	MemLeft      int64           `json:"memLeft"`
	NumRegisters int             `json:"nreg"`
	Frames       []FrameImage    `json:"frames"`
	Stack        json.RawMessage `json:"stack"`
	StackNil     []bool          `json:"stackNil,omitempty"`
}

// SaveThread serializes a suspended coroutine th into a ThreadImage. Registers
// and frame functions are encoded as one shared graph (so their upvalues share
// identity with the registers); frame metadata records the execution cursors.
// Fails loudly on a Go-function frame or any non-serializable value rather than
// producing a save that cannot be restored.
func SaveThread(reg *ChunkRegistry, th *lua.LState) (*ThreadImage, error) {
	v := th.LitdSnapshot()
	img := &ThreadImage{
		Dead:         v.Dead,
		Wrapped:      v.Wrapped,
		InstrLimited: v.InstrLimited,
		InstrLeft:    v.InstrLeft,
		MemLimited:   v.MemLimited,
		MemLeft:      v.MemLeft,
		NumRegisters: len(v.Stack),
	}
	for i, f := range v.Frames {
		if f.Fn == nil || f.Fn.IsG || f.Fn.Proto == nil {
			return nil, fmt.Errorf("luabind: frame %d is a Go-function frame — cannot persist", i)
		}
		img.Frames = append(img.Frames, FrameImage{
			Idx: f.Idx, Pc: f.Pc, Base: f.Base, LocalBase: f.LocalBase,
			ReturnBase: f.ReturnBase, NArgs: f.NArgs, NRet: f.NRet, TailCall: f.TailCall,
		})
	}
	// One shared graph: register slots (Go-nil sent as LNil + recorded in
	// StackNil so they restore as Go-nil) followed by each frame's function.
	vals := make([]lua.LValue, 0, len(v.Stack)+len(v.Frames))
	img.StackNil = make([]bool, len(v.Stack))
	anyNil := false
	for i, sv := range v.Stack {
		if sv == nil {
			vals = append(vals, lua.LNil)
			img.StackNil[i] = true
			anyNil = true
			continue
		}
		vals = append(vals, sv)
	}
	if !anyNil {
		img.StackNil = nil
	}
	for _, f := range v.Frames {
		vals = append(vals, f.Fn)
	}
	blob, err := serializeRegisters(reg, th, vals)
	if err != nil {
		return nil, fmt.Errorf("luabind: register stack: %w", err)
	}
	img.Stack = blob
	return img, nil
}

// LoadThread reconstructs a resumable coroutine from img against reg (typically
// a cold registry in a fresh process). It returns the thread and its top-frame
// function; resume it with parent.Resume(thread, topFn, args...). It fails
// loudly if any frame's chunk-id is unknown (content changed since save) or a
// register value cannot be deserialized.
func LoadThread(reg *ChunkRegistry, parent *lua.LState, img *ThreadImage) (*lua.LState, *lua.LFunction, error) {
	v := &lua.LitdThreadView{
		Dead:         img.Dead,
		Wrapped:      img.Wrapped,
		InstrLimited: img.InstrLimited,
		InstrLeft:    img.InstrLeft,
		MemLimited:   img.MemLimited,
		MemLeft:      img.MemLeft,
	}
	// Decode the shared graph: roots are NumRegisters register slots followed by
	// one function per frame, in frame order.
	var blob valuesBlob
	if err := json.Unmarshal(img.Stack, &blob); err != nil {
		return nil, nil, fmt.Errorf("luabind: register stack: malformed graph: %w", err)
	}
	d, err := newGraphDecoder(parent, reg, &blob)
	if err != nil {
		return nil, nil, err
	}
	roots, err := d.roots()
	if err != nil {
		return nil, nil, err
	}
	nReg := img.NumRegisters
	if nReg < 0 || nReg+len(img.Frames) != len(roots) {
		return nil, nil, fmt.Errorf("luabind: graph has %d roots, expected %d registers + %d frame functions", len(roots), nReg, len(img.Frames))
	}

	v.Stack = make([]lua.LValue, nReg)
	for i := 0; i < nReg; i++ {
		if i < len(img.StackNil) && img.StackNil[i] {
			v.Stack[i] = nil // restore Go-nil register
			continue
		}
		v.Stack[i] = roots[i]
	}

	var topFn *lua.LFunction
	for i, fi := range img.Frames {
		fn, ok := roots[nReg+i].(*lua.LFunction)
		if !ok {
			return nil, nil, fmt.Errorf("luabind: frame %d function root is %s, not a function", i, roots[nReg+i].Type())
		}
		topFn = fn
		v.Frames = append(v.Frames, lua.LitdFrameView{
			Fn: fn, Idx: fi.Idx, Pc: fi.Pc, Base: fi.Base, LocalBase: fi.LocalBase,
			ReturnBase: fi.ReturnBase, NArgs: fi.NArgs, NRet: fi.NRet, TailCall: fi.TailCall,
		})
	}

	th := parent.LitdRestoreThread(v)
	// Wire closure upvalues AFTER restore (covers both register closures and the
	// frame functions, which all live in the graph's func pool), so open
	// upvalues bind into th's live register file and shared cells coincide.
	if err := d.wireUpvalues(th); err != nil {
		return nil, nil, err
	}
	return th, topFn, nil
}
