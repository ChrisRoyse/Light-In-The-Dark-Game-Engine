package luabind

// Regression net for the cross-process thread persister (#264 step 4). The
// authoritative verification is the manual FSV harness (cmd/dbg/fsv264x, run
// and inspected by hand): these tests lock the round-trip + the two fail-closed
// edges so the behavior cannot silently regress.

import (
	"encoding/json"
	"strings"
	"testing"

	lua "github.com/yuin/gopher-lua"
)

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

	img, err := SaveThread(regHot, co)
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
	if len(img.Frames) != 1 || img.Frames[0].Pc != 6 || img.Frames[0].ProtoPath != "" {
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
	th, topFn, err := LoadThread(regCold, rtCold, &img2)
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
	img, err := SaveThread(regHot, co)
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
	th, topFn, err := LoadThread(regCold, rtCold, &img2)
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
	img, err := SaveThread(regHot, co)
	if err != nil {
		t.Fatalf("SaveThread: %v", err)
	}
	regWrong := NewChunkRegistry()
	defer regWrong.Close()
	if _, err := regWrong.Register("world", "return 1\n"); err != nil {
		t.Fatalf("register: %v", err)
	}
	_, _, err = LoadThread(regWrong, lua.NewState(), img)
	if err == nil {
		t.Fatal("expected a content-hash-mismatch error, got nil")
	}
	if !strings.Contains(err.Error(), "chunk-hash mismatch") {
		t.Fatalf("error did not name the mismatch: %v", err)
	}
	t.Logf("FSV mismatch fail-closed: %v", err)
}

// TestSaveThreadRejectsNonDataRegister — a function/closure sitting in a
// register cannot be serialized in the step-4 data graph; SaveThread must
// reject it loudly rather than emit a save that cannot be restored.
func TestSaveThreadRejectsNonDataRegister(t *testing.T) {
	reg := NewChunkRegistry()
	defer reg.Close()
	co, _ := runToYield(t, reg, `
		local f = function() return 1 end
		coroutine.yield()
		return f()
	`)
	_, err := SaveThread(reg, co)
	if err == nil {
		t.Fatal("expected SaveThread to reject a closure register, got nil")
	}
	if !strings.Contains(err.Error(), "type function") {
		t.Fatalf("error did not name the function value: %v", err)
	}
	t.Logf("FSV non-data reject: %v", err)
}
