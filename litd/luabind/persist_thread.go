package luabind

// Cross-process thread persistence (#264 step 4) — the layer that turns the
// fork's in-process snapshot/restore primitives (LitdSnapshot /
// LitdRestoreThread) into a save artifact that survives process exit and a
// cold restart. A suspended coroutine is serialized as:
//
//   - each call frame as a (chunk-id, proto-path) reference plus its execution
//     cursor (Pc) and register window — the frame's *FunctionProto is mapped
//     through the step-1 ChunkRegistry by pointer identity, NEVER embedded, so
//     the save never carries bytecode and a content-hash mismatch on load is a
//     loud "world content changed" error rather than silent corruption;
//   - each register slot through the step-2 value-graph serializer.
//
// LoadThread reconstructs against a COLD registry (a fresh process re-registers
// the world's chunks, which content-address to the same ids) and rebuilds a
// resumable LState via LitdRestoreThread.
//
// Scope (step 4): pure-Lua coroutines whose register slots hold the data subset
// (nil/bool/number/string/table). A Go-function frame, or a function/closure/
// userdata value sitting in a register, fails loudly — the shared-upvalue
// graph, nested coroutines and userdata→handle rebind are step 5. Per-slot
// value serialization does not yet preserve table identity SHARED across
// distinct registers (also step 5); single-register graphs are exact.

import (
	"encoding/json"
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// FrameImage is one serialized call frame: a proto reference (never bytecode)
// plus the execution cursor and register window.
type FrameImage struct {
	ChunkID    string `json:"chunk"`
	ProtoPath  string `json:"proto"`
	Idx        int    `json:"idx"`
	Pc         int    `json:"pc"`
	Base       int    `json:"base"`
	LocalBase  int    `json:"localBase"`
	ReturnBase int    `json:"returnBase"`
	NArgs      int    `json:"nargs"`
	NRet       int    `json:"nret"`
	TailCall   int    `json:"tailcall"`
}

// slotImage is one register slot. A Go-nil slot (uninitialized register) is
// distinct from an LNil value and is preserved as such.
type slotImage struct {
	Nil bool            `json:"nil,omitempty"`
	Val json.RawMessage `json:"val,omitempty"`
}

// ThreadImage is the serializable form of a suspended LState.
type ThreadImage struct {
	Dead         bool         `json:"dead"`
	Wrapped      bool         `json:"wrapped"`
	InstrLimited bool         `json:"instrLimited"`
	InstrLeft    int64        `json:"instrLeft"`
	MemLimited   bool         `json:"memLimited"`
	MemLeft      int64        `json:"memLeft"`
	Frames       []FrameImage `json:"frames"`
	Stack        []slotImage  `json:"stack"`
}

// SaveThread serializes a suspended coroutine th into a ThreadImage, mapping
// every frame's prototype through reg. It fails loudly on a Go-function frame
// or a non-serializable register value rather than producing a save that
// cannot be restored.
func SaveThread(reg *ChunkRegistry, th *lua.LState) (*ThreadImage, error) {
	v := th.LitdSnapshot()
	img := &ThreadImage{
		Dead:         v.Dead,
		Wrapped:      v.Wrapped,
		InstrLimited: v.InstrLimited,
		InstrLeft:    v.InstrLeft,
		MemLimited:   v.MemLimited,
		MemLeft:      v.MemLeft,
	}
	for i, f := range v.Frames {
		if f.Fn == nil || f.Fn.IsG || f.Fn.Proto == nil {
			return nil, fmt.Errorf("luabind: frame %d is a Go-function frame — cannot persist (step 5)", i)
		}
		cid, path, err := reg.PathOf(f.Fn.Proto)
		if err != nil {
			return nil, fmt.Errorf("luabind: frame %d: %w", i, err)
		}
		img.Frames = append(img.Frames, FrameImage{
			ChunkID: cid, ProtoPath: path,
			Idx: f.Idx, Pc: f.Pc, Base: f.Base, LocalBase: f.LocalBase,
			ReturnBase: f.ReturnBase, NArgs: f.NArgs, NRet: f.NRet, TailCall: f.TailCall,
		})
	}
	img.Stack = make([]slotImage, len(v.Stack))
	for i, sv := range v.Stack {
		if sv == nil {
			img.Stack[i] = slotImage{Nil: true}
			continue
		}
		b, err := SerializeValue(sv)
		if err != nil {
			return nil, fmt.Errorf("luabind: register slot %d: %w", i, err)
		}
		img.Stack[i] = slotImage{Val: b}
	}
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
	var topFn *lua.LFunction
	for i, fi := range img.Frames {
		proto, err := reg.ResolveProto(fi.ChunkID, fi.ProtoPath)
		if err != nil {
			return nil, nil, fmt.Errorf("luabind: frame %d: %w", i, err)
		}
		fn := parent.NewFunctionFromProto(proto)
		topFn = fn
		v.Frames = append(v.Frames, lua.LitdFrameView{
			Fn: fn, Idx: fi.Idx, Pc: fi.Pc, Base: fi.Base, LocalBase: fi.LocalBase,
			ReturnBase: fi.ReturnBase, NArgs: fi.NArgs, NRet: fi.NRet, TailCall: fi.TailCall,
		})
	}
	v.Stack = make([]lua.LValue, len(img.Stack))
	for i, si := range img.Stack {
		if si.Nil {
			v.Stack[i] = nil
			continue
		}
		val, err := DeserializeValue(parent, si.Val)
		if err != nil {
			return nil, nil, fmt.Errorf("luabind: register slot %d: %w", i, err)
		}
		v.Stack[i] = val
	}
	return parent.LitdRestoreThread(v), topFn, nil
}
