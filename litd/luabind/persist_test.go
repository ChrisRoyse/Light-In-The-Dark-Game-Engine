package luabind_test

// #264 persister, step 1: chunk registry + proto addressing FSV. SoT = the
// proto pointers the registry holds, reached two ways (by path, and by reverse
// lookup) and asserted identical; plus the loud failures on unknown/modified
// chunks. No fork internals touched here — this layer is the content-addressed
// naming the LState-graph serializer will resolve against.

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// a chunk with nested function literals at two depths.
const nestedChunk = `
local function outer()
	local function inner()
		return 1
	end
	return inner
end
local h = function() return 2 end
return outer, h
`

// walkProtos returns every proto reachable from top in deterministic
// depth-first order (top first, then each child subtree).
func walkProtos(top *lua.FunctionProto) []*lua.FunctionProto {
	out := []*lua.FunctionProto{top}
	for _, c := range top.FunctionPrototypes {
		out = append(out, walkProtos(c)...)
	}
	return out
}

// TestPersistChunkProtoRoundTrip — every proto in a registered chunk resolves
// by path and reverse-resolves to the SAME pointer. This is the addressing
// invariant the serializer relies on: name a function, get it back exactly.
func TestPersistChunkProtoRoundTrip(t *testing.T) {
	r := luabind.NewChunkRegistry()
	defer r.Close()

	id, err := r.Register("nested.lua", nestedChunk)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	top, err := r.ResolveProto(id, "")
	if err != nil {
		t.Fatalf("resolve top: %v", err)
	}
	protos := walkProtos(top)
	t.Logf("FSV chunk %s.. has %d protos (registry ProtoCount=%d)", id[:8], len(protos), r.ProtoCount(id))
	if r.ProtoCount(id) != len(protos) {
		t.Fatalf("ProtoCount=%d but walk found %d", r.ProtoCount(id), len(protos))
	}
	if len(protos) < 3 {
		t.Fatalf("expected >=3 protos (top + outer + inner + h), got %d", len(protos))
	}

	for i, p := range protos {
		cid, path, err := r.PathOf(p)
		if err != nil {
			t.Fatalf("PathOf proto %d: %v", i, err)
		}
		if cid != id {
			t.Fatalf("proto %d reverse-resolved to chunk %s, want %s", i, cid, id)
		}
		back, err := r.ResolveProto(cid, path)
		if err != nil {
			t.Fatalf("resolve (%s,%q): %v", cid, path, err)
		}
		if back != p {
			t.Fatalf("round-trip proto %d: path %q resolved to a DIFFERENT pointer", i, path)
		}
		t.Logf("FSV proto %d: path=%-6q source=%q -> identity OK", i, path, p.SourceName)
	}
}

// TestPersistChunkIDContentAddressed — the chunk-id is the source hash: stable
// for identical source, different for any edit. This is the property that makes
// a stale-chunk restore fail closed.
func TestPersistChunkIDContentAddressed(t *testing.T) {
	a := luabind.ChunkID(nestedChunk)
	b := luabind.ChunkID(nestedChunk)
	modified := strings.Replace(nestedChunk, "return 1", "return 99", 1)
	c := luabind.ChunkID(modified)
	t.Logf("FSV chunk-id: same=%v (a==b), changed=%v (a!=c)\n a=%s\n c=%s", a == b, a != c, a, c)
	if a != b {
		t.Fatalf("ChunkID not stable for identical source")
	}
	if a == c {
		t.Fatalf("ChunkID collision: edited source produced the same id")
	}
}

// TestPersistResolveUnknownChunkFailsLoud — edge: resolving a chunk-id absent
// from the registry (the modified-world case) is a loud chunk-hash mismatch,
// never a silent nil.
func TestPersistResolveUnknownChunkFailsLoud(t *testing.T) {
	r := luabind.NewChunkRegistry()
	defer r.Close()
	_, _ = r.Register("orig.lua", nestedChunk)

	// id of a chunk that was never registered (simulates an edited world script).
	staleID := luabind.ChunkID(strings.Replace(nestedChunk, "return 2", "return 222", 1))
	_, err := r.ResolveProto(staleID, "")
	t.Logf("FSV unknown chunk-id -> err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "world content changed") {
		t.Fatalf("resolving an unknown chunk-id must fail loudly with a mismatch, got: %v", err)
	}
}

// TestPersistResolvePathOutOfRange — edge: a path that over-indexes the nested
// protos is a loud error, not a panic or a wrong proto.
func TestPersistResolvePathOutOfRange(t *testing.T) {
	r := luabind.NewChunkRegistry()
	defer r.Close()
	id, _ := r.Register("nested.lua", nestedChunk)

	_, err := r.ResolveProto(id, "99")
	t.Logf("FSV out-of-range path -> err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "out of range") {
		t.Fatalf("over-indexed proto-path must fail loudly, got: %v", err)
	}
	_, err = r.ResolveProto(id, "0.99")
	t.Logf("FSV out-of-range nested path -> err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "out of range") {
		t.Fatalf("over-indexed nested proto-path must fail loudly, got: %v", err)
	}
}

// TestPersistPathOfForeignProtoFailsLoud — edge: a proto compiled outside the
// registry has no path; PathOf must refuse it rather than fabricate a name (a
// silent name would corrupt the save).
func TestPersistPathOfForeignProtoFailsLoud(t *testing.T) {
	r := luabind.NewChunkRegistry()
	defer r.Close()
	_, _ = r.Register("nested.lua", nestedChunk)

	// Compile the same source independently — different proto pointers.
	foreign := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer foreign.Close()
	fn, err := foreign.Load(strings.NewReader(nestedChunk), "foreign.lua")
	if err != nil {
		t.Fatalf("foreign compile: %v", err)
	}
	_, _, err = r.PathOf(fn.Proto)
	t.Logf("FSV foreign proto -> err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "no registered chunk") {
		t.Fatalf("PathOf on a foreign proto must fail loudly, got: %v", err)
	}
}

// TestPersistEmptyChunk — edge: a chunk with no function literals has exactly
// one proto (the top-level chunk body) and SourceName preserved.
func TestPersistEmptyChunk(t *testing.T) {
	r := luabind.NewChunkRegistry()
	defer r.Close()
	id, err := r.Register("flat.lua", `return 1 + 1`)
	if err != nil {
		t.Fatalf("register: %v", err)
	}
	top, err := r.ResolveProto(id, "")
	if err != nil {
		t.Fatalf("resolve: %v", err)
	}
	t.Logf("FSV flat chunk: ProtoCount=%d SourceName=%q nested=%d", r.ProtoCount(id), top.SourceName, len(top.FunctionPrototypes))
	if r.ProtoCount(id) != 1 {
		t.Fatalf("flat chunk ProtoCount = %d, want 1", r.ProtoCount(id))
	}
	if top.SourceName != "flat.lua" {
		t.Fatalf("SourceName = %q, want flat.lua", top.SourceName)
	}
}
