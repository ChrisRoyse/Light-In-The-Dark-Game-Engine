package luabind

// Regression net for the cross-process thread persister (#264 step 4). The
// authoritative verification is the manual FSV harness (cmd/dbg/fsv264x, run
// and inspected by hand): these tests lock the round-trip + the two fail-closed
// edges so the behavior cannot silently regress.

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// intHandleMarshaler is a synthetic HandleMarshaler standing in for the binding
// layer: it round-trips a userdata whose Value is an int token. (The real
// api.RefOf/Game.Resolve codec is verified in litd/api/handle_marshal_test.go;
// here we exercise the persister's userdata path with the marshaler injected.)
type intHandleMarshaler struct{}

func (intHandleMarshaler) MarshalUserData(ud *lua.LUserData) ([]byte, error) {
	n, ok := ud.Value.(int)
	if !ok {
		return nil, fmt.Errorf("userdata value is %T, not a handle token", ud.Value)
	}
	return json.Marshal(n)
}

func (intHandleMarshaler) UnmarshalUserData(data []byte) (*lua.LUserData, error) {
	var n int
	if err := json.Unmarshal(data, &n); err != nil {
		return nil, err
	}
	return &lua.LUserData{Value: n}, nil
}

const persistSrc = `
	local x = 111
	local y = 222
	coroutine.yield(x + y)
	return x * y
`

// runToYield registers src in reg and resumes a fresh coroutine to its first
// yield, returning the suspended coroutine and its runtime.
func runToYield(t *testing.T, reg *ChunkRegistry, src string) (*lua.LState, *lua.LState) {
	t.Helper()
	cid, err := reg.Register("world", src)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	rt := lua.NewState()
	co, _ := rt.NewThread()
	st, rerr, _ := rt.Resume(co, rt.NewFunctionFromProto(proto))
	if st != lua.ResumeYield || rerr != nil {
		t.Fatalf("expected yield, got state=%v err=%v", st, rerr)
	}
	return co, rt
}

// TestSaveLoadThreadColdRoundTrip — save a suspended coroutine, marshal to
// JSON, then reload on a COLD registry + fresh runtime and resume. SoT = the
// artifact bytes (no bytecode; data captured) and the cold-resume value.
func TestSaveLoadThreadColdRoundTrip(t *testing.T) {
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, persistSrc)

	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, err := json.Marshal(img)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	// SoT: the artifact references content, never embeds bytecode.
	if strings.Contains(string(blob), "OP_") {
		t.Fatalf("save artifact contains bytecode: %s", blob)
	}
	if len(img.Frames) != 1 || img.Frames[0].Pc != 6 {
		t.Fatalf("unexpected frame image: %+v", img.Frames)
	}
	// SoT: the locals 111/222 are captured in the register slots.
	if !strings.Contains(string(blob), "111") || !strings.Contains(string(blob), "222") {
		t.Fatalf("save artifact missing register data: %s", blob)
	}

	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", persistSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, nil)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}
	st, rerr, vals := rtCold.Resume(th, topFn)
	if st != lua.ResumeOK || rerr != nil {
		t.Fatalf("cold resume failed: state=%v err=%v", st, rerr)
	}
	if len(vals) != 1 || vals[0] != lua.LNumber(24642) {
		t.Fatalf("cold resume = %v, want [24642] (111*222)", vals)
	}
	t.Logf("FSV cold round-trip: artifact bytecode-free, cold resume = %v", vals)
}

// TestSaveLoadThreadPreservesRegisterAliasing — two registers aliasing one
// table must round-trip as a single shared object, not two copies. SoT = a
// mutation through one alias being visible through the other after a cold
// reload (42, not 0).
func TestSaveLoadThreadPreservesRegisterAliasing(t *testing.T) {
	const aliasSrc = `
		local a = { n = 0 }
		local b = a
		coroutine.yield()
		a.n = 42
		return b.n
	`
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, aliasSrc)
	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)

	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", aliasSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, nil)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}
	_, _, vals := rtCold.Resume(th, topFn)
	if len(vals) != 1 || vals[0] != lua.LNumber(42) {
		t.Fatalf("alias not preserved: got %v, want [42] (mutation via one alias unseen by the other => duplicated tables)", vals)
	}
	t.Logf("FSV register aliasing: cold resume = %v (shared table preserved)", vals)
}

// TestLoadThreadContentMismatch — loading against a registry whose content
// hashes differently must fail loudly (fail-closed), not silently mis-resume.
func TestLoadThreadContentMismatch(t *testing.T) {
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, persistSrc)
	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	regWrong := NewChunkRegistry()
	defer regWrong.Close()
	if _, err := regWrong.Register("world", "return 1\n"); err != nil {
		t.Fatalf("register: %v", err)
	}
	_, _, err = LoadThread(regWrong, lua.NewState(), img, nil)
	if err == nil {
		t.Fatal("expected a content-hash-mismatch error, got nil")
	}
	if !strings.Contains(err.Error(), "chunk-hash mismatch") {
		t.Fatalf("error did not name the mismatch: %v", err)
	}
	t.Logf("FSV mismatch fail-closed: %v", err)
}

// TestSaveLoadThreadClosureSharedUpvalue — two closures in registers sharing
// one OPEN upvalue over a local must round-trip as two closures sharing one
// cell that aliases the restored register. SoT = the cold-resumed value, which
// is 2 only if a post-resume write through one closure is seen by the other.
func TestSaveLoadThreadClosureSharedUpvalue(t *testing.T) {
	const closureSrc = `
		local x = 0
		local inc = function() x = x + 1 end
		local get = function() return x end
		inc()                 -- x = 1
		coroutine.yield()
		inc()                 -- x = 2 if inc/get share x; 1 if duplicated
		return get()
	`
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, closureSrc)
	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)
	if strings.Contains(string(blob), "OP_") {
		t.Fatalf("closure artifact embeds bytecode: %s", blob)
	}

	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", closureSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, nil)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}
	_, _, vals := rtCold.Resume(th, topFn)
	if len(vals) != 1 || vals[0] != lua.LNumber(2) {
		t.Fatalf("shared open upvalue not preserved: got %v, want [2] (1 => duplicated closures)", vals)
	}
	t.Logf("FSV closure shared upvalue: cold resume = %v", vals)
}

// TestSaveLoadThreadNestedCoroutine — a parent coroutine holding a SUSPENDED
// child coroutine (with its own captured state) must round-trip: the child is
// serialized recursively as a nested ThreadImage and reconstructed resumable.
// SoT = the parent's cold-resumed value, which depends on resuming the child.
func TestSaveLoadThreadNestedCoroutine(t *testing.T) {
	const nestedSrc = `
		local child = coroutine.create(function()
			local s = 100
			coroutine.yield()
			return s + 11          -- 111
		end)
		coroutine.resume(child)    -- child runs to its yield (s=100 live)
		coroutine.yield()          -- parent yields holding the suspended child
		local ok, v = coroutine.resume(child)
		return v
	`
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, nestedSrc)
	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)
	if !strings.Contains(string(blob), `"threads"`) {
		t.Fatalf("artifact lacks the nested thread image: %s", blob)
	}

	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", nestedSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, nil)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}
	st, rerr, vals := rtCold.Resume(th, topFn)
	if st != lua.ResumeOK || rerr != nil {
		t.Fatalf("cold resume failed: state=%v err=%v", st, rerr)
	}
	if len(vals) != 1 || vals[0] != lua.LNumber(111) {
		t.Fatalf("nested coroutine not preserved: got %v, want [111] (child 100+11)", vals)
	}
	t.Logf("FSV nested coroutine: cold resume = %v", vals)
}

// TestSaveThreadRejectsUserdataRegister — userdata (a host object) in a register
// is unpersistable WITHOUT a HandleMarshaler: saving with a nil marshaler must
// fail loudly (fail-closed), never silently drop the handle. (The positive path
// — a marshaler rebinding the userdata — is TestSaveLoadThreadUserDataRebind.)
func TestSaveThreadRejectsUserdataRegister(t *testing.T) {
	reg := NewChunkRegistry()
	defer reg.Close()
	cid, err := reg.Register("world", `
		local u = injected_userdata
		coroutine.yield()
		return type(u)
	`)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	rt := lua.NewState()
	rt.SetGlobal("injected_userdata", rt.NewUserData()) // a host object in a local
	co, _ := rt.NewThread()
	if st, _, _ := rt.Resume(co, rt.NewFunctionFromProto(proto)); st != lua.ResumeYield {
		t.Fatalf("expected yield, got %v", st)
	}
	_, err = SaveThread(reg, co, nil)
	if err == nil {
		t.Fatal("expected SaveThread to reject a userdata register, got nil")
	}
	if !strings.Contains(err.Error(), "userdata") {
		t.Fatalf("error did not name the userdata value: %v", err)
	}
	t.Logf("FSV userdata reject: %v", err)
}

// TestSaveLoadThreadFrameWithUpvalues — yielding INSIDE a closure that has
// upvalues makes the suspended frame's function carry upvalues. The frame
// function is serialized through the shared graph and its upvalues are wired on
// restore, so the closure still reads its captured local after a cold reload.
// SoT = the cold-resumed value (the captured x).
func TestSaveLoadThreadFrameWithUpvalues(t *testing.T) {
	const frameUpSrc = `
		local x = 5
		local f = function() coroutine.yield(); return x * 2 end
		return f()             -- yields inside f; f's frame holds the upvalue x
	`
	regHot := NewChunkRegistry()
	defer regHot.Close()
	co, _ := runToYield(t, regHot, frameUpSrc)
	img, err := SaveThread(regHot, co, nil)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)

	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", frameUpSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, nil)
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}
	st, rerr, vals := rtCold.Resume(th, topFn)
	if st != lua.ResumeOK || rerr != nil {
		t.Fatalf("cold resume failed: state=%v err=%v", st, rerr)
	}
	if len(vals) != 1 || vals[0] != lua.LNumber(10) {
		t.Fatalf("frame closure upvalue not preserved: got %v, want [10] (x=5 *2)", vals)
	}
	t.Logf("FSV frame closure w/ upvalue: cold resume = %v", vals)
}

// TestSaveLoadThreadUserDataRebind — a suspended coroutine holding a userdata
// (host handle) in registers, saved WITH a HandleMarshaler, must (1) emit the
// handle's opaque token into the artifact (no Go pointer), (2) on cold reload
// rebuild the userdata through the marshaler, and (3) preserve identity: a
// userdata aliased across two registers round-trips as ONE *LUserData, not two.
// SoT = the artifact's userdata token pool + the reloaded thread's register
// file (read via LitdSnapshot), plus the thread still resuming to completion.
func TestSaveLoadThreadUserDataRebind(t *testing.T) {
	const udSrc = `
		local h = injected_handle    -- a userdata (host handle), token 7
		local also = h               -- alias: same userdata in a 2nd register
		coroutine.yield()
		return 1
	`
	regHot := NewChunkRegistry()
	defer regHot.Close()
	cid, err := regHot.Register("world", udSrc)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	proto, err := regHot.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	rt := lua.NewState()
	ud := rt.NewUserData()
	ud.Value = 7 // synthetic handle token the marshaler round-trips
	rt.SetGlobal("injected_handle", ud)
	co, _ := rt.NewThread()
	if st, rerr, _ := rt.Resume(co, rt.NewFunctionFromProto(proto)); st != lua.ResumeYield || rerr != nil {
		t.Fatalf("expected yield, got state=%v err=%v", st, rerr)
	}

	// Save WITH the marshaler. SoT #1: the artifact carries the token, not a ptr.
	img, err := SaveThread(regHot, co, intHandleMarshaler{})
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	blob, _ := json.Marshal(img)
	t.Logf("FSV userdata artifact: %s", blob)
	if !strings.Contains(string(blob), `"userdata":[7]`) {
		t.Fatalf("artifact missing userdata token pool [7]: %s", blob)
	}
	if strings.Contains(string(blob), "0x") {
		t.Fatalf("artifact leaked a Go pointer: %s", blob)
	}

	// Edge (fail-closed): the SAME thread saved with no marshaler must error loud.
	if _, err := SaveThread(regHot, co, nil); err == nil || !strings.Contains(err.Error(), "HandleMarshaler") {
		t.Fatalf("nil marshaler must fail loudly naming HandleMarshaler, got %v", err)
	}

	// Cold reload with a fresh marshaler instance.
	var img2 ThreadImage
	if err := json.Unmarshal(blob, &img2); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	regCold := NewChunkRegistry()
	defer regCold.Close()
	if _, err := regCold.Register("world", udSrc); err != nil {
		t.Fatalf("cold register: %v", err)
	}
	rtCold := lua.NewState()
	th, topFn, err := LoadThread(regCold, rtCold, &img2, intHandleMarshaler{})
	if err != nil {
		t.Fatalf("LoadThread: %v", err)
	}

	// SoT #2: read the reloaded register file. Every token-7 userdata slot must
	// be the SAME pointer (identity interned), Value rebound to 7.
	v := th.LitdSnapshot()
	var seven []*lua.LUserData
	for _, sv := range v.Stack {
		if u, ok := sv.(*lua.LUserData); ok && u.Value == 7 {
			seven = append(seven, u)
		}
	}
	if len(seven) < 2 {
		t.Fatalf("expected >=2 register slots holding the rebound userdata (h, also), got %d", len(seven))
	}
	for i, u := range seven {
		if u != seven[0] {
			t.Fatalf("aliased userdata not interned: slot %d %p != %p", i, u, seven[0])
		}
	}
	t.Logf("FSV userdata rebind: %d token-7 slots, all shared=%v, Value=%v", len(seven), seven[1] == seven[0], seven[0].Value)

	// SoT #3: the rebound thread still resumes to completion.
	st, rerr, vals := rtCold.Resume(th, topFn)
	if st != lua.ResumeOK || rerr != nil {
		t.Fatalf("cold resume after rebind failed: state=%v err=%v", st, rerr)
	}
	if len(vals) != 1 || vals[0] != lua.LNumber(1) {
		t.Fatalf("resume = %v, want [1]", vals)
	}
	t.Logf("FSV: thread resumable after userdata rebind, returned %v", vals)
}
