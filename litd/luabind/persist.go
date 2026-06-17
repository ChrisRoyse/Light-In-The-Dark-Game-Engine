package luabind

// Persistence of suspended Lua state (#264, LITD-PATCH 3 of 4; decisions.md
// D-2026-06-11-25 patch 3; execution-model.md §2.1 S-5). A saved game must be
// able to freeze a coroutine mid-execution and resume it bit-identically in a
// fresh process — the coroutine's call frames, value stack, registry, and
// upvalues are state like any other (execution-model.md §7).
//
// This file is the FOUNDATION layer: the chunk registry and proto addressing.
// The hard constraint from the issue is that function prototypes are NEVER
// serialized by pointer or by re-dumping bytecode (which could drift across
// builds). Instead every Lua function is identified by (chunk-id, proto-path):
//   - chunk-id  = SHA-256 of the chunk's source bytes (content-addressed, so a
//                 changed world script gets a different id);
//   - proto-path = the index path from the chunk's top-level proto down through
//                 nested FunctionPrototypes to the target proto.
// On load the host recompiles the world's chunks into a fresh registry and the
// saved (chunk-id, proto-path) pairs are re-resolved against it. If a chunk
// changed, its id changed, so the saved id is absent and restore fails LOUDLY
// ("world content changed") rather than resuming against drifted bytecode.
//
// The LState-graph serializer (call frames, registry slots, upvalue identity,
// userdata→handle rebinding) builds on this and lands in follow-up commits on
// this issue; it needs exported snapshot/restore hooks in the fork.

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"

	lua "github.com/yuin/gopher-lua"
)

// ChunkID returns the content-addressed id of a chunk's source: the hex
// SHA-256 of its bytes. Identical source → identical id on every build and
// arch; any edit changes the id (that is what makes a stale-chunk restore
// fail closed).
func ChunkID(source string) string {
	sum := sha256.Sum256([]byte(source))
	return hex.EncodeToString(sum[:])
}

// chunkRec is one compiled chunk: its top-level proto plus a reverse index from
// every reachable *FunctionProto to its proto-path, so the serializer can name
// any function it encounters.
type chunkRec struct {
	id     string
	name   string
	source string
	top    *lua.FunctionProto
	paths  map[*lua.FunctionProto]string // proto -> "0.2.1" (top-level proto -> "")
}

// ChunkRegistry holds the compiled world chunks addressable by chunk-id. It is
// rebuilt from source on every load; protos are never persisted, only resolved
// through it. Not safe for concurrent mutation (the sim is single-threaded).
type ChunkRegistry struct {
	compiler *lua.LState // throwaway state used only to compile; owns no runtime
	byID     map[string]*chunkRec
}

// NewChunkRegistry returns an empty registry. Callers must Close it.
func NewChunkRegistry() *ChunkRegistry {
	return &ChunkRegistry{
		compiler: lua.NewState(lua.Options{SkipOpenLibs: true}),
		byID:     map[string]*chunkRec{},
	}
}

// Close releases the registry's compiler state.
func (r *ChunkRegistry) Close() { r.compiler.Close() }

// Register compiles source under the given chunk name, indexes every proto in
// it, and returns the chunk-id. Re-registering identical source is idempotent
// (same id, no recompile). A compile error is returned loudly — a chunk that
// will not parse cannot back a save.
func (r *ChunkRegistry) Register(name, source string) (string, error) {
	id := ChunkID(source)
	if _, ok := r.byID[id]; ok {
		return id, nil
	}
	fn, err := r.compiler.Load(strings.NewReader(source), name)
	if err != nil {
		return "", fmt.Errorf("luabind: chunk %q failed to compile: %w", name, err)
	}
	rec := &chunkRec{
		id:     id,
		name:   name,
		source: source,
		top:    fn.Proto,
		paths:  map[*lua.FunctionProto]string{},
	}
	indexProtoPaths(fn.Proto, nil, rec.paths)
	r.byID[id] = rec
	return id, nil
}

// indexProtoPaths records, for proto and every nested prototype, the path key
// (dot-joined indices; "" for the top-level proto) into dst.
func indexProtoPaths(proto *lua.FunctionProto, path []int, dst map[*lua.FunctionProto]string) {
	dst[proto] = encodePath(path)
	for i, child := range proto.FunctionPrototypes {
		indexProtoPaths(child, append(path, i), dst)
	}
}

func encodePath(path []int) string {
	if len(path) == 0 {
		return ""
	}
	parts := make([]string, len(path))
	for i, n := range path {
		parts[i] = fmt.Sprint(n)
	}
	return strings.Join(parts, ".")
}

func decodePath(key string) ([]int, error) {
	if key == "" {
		return nil, nil
	}
	parts := strings.Split(key, ".")
	out := make([]int, len(parts))
	for i, p := range parts {
		var n int
		if _, err := fmt.Sscanf(p, "%d", &n); err != nil || n < 0 {
			return nil, fmt.Errorf("luabind: malformed proto-path %q", key)
		}
		out[i] = n
	}
	return out, nil
}

// ResolveProto returns the prototype named by (chunkID, protoPath). It fails
// loudly if the chunk-id is unknown (the canonical stale-content signal: the
// world script changed, so its id changed and the saved id is gone) or the
// path does not address a real nested proto.
func (r *ChunkRegistry) ResolveProto(chunkID, protoPath string) (*lua.FunctionProto, error) {
	rec, ok := r.byID[chunkID]
	if !ok {
		return nil, fmt.Errorf("luabind: unknown chunk-id %s — world content changed since save (chunk-hash mismatch)", chunkID)
	}
	path, err := decodePath(protoPath)
	if err != nil {
		return nil, err
	}
	proto := rec.top
	for depth, idx := range path {
		if idx < 0 || idx >= len(proto.FunctionPrototypes) {
			return nil, fmt.Errorf("luabind: proto-path %q out of range at depth %d (chunk %s has %d nested protos there)",
				protoPath, depth, chunkID, len(proto.FunctionPrototypes))
		}
		proto = proto.FunctionPrototypes[idx]
	}
	return proto, nil
}

// PathOf returns the (chunkID, protoPath) naming a prototype, found by pointer
// identity within the registered chunks. It fails loudly if the proto belongs
// to no registered chunk — that proto could not be persisted, and silently
// dropping it would corrupt the save.
func (r *ChunkRegistry) PathOf(proto *lua.FunctionProto) (chunkID, protoPath string, err error) {
	for id, rec := range r.byID {
		if p, ok := rec.paths[proto]; ok {
			return id, p, nil
		}
	}
	src := "<unknown>"
	if proto != nil {
		src = proto.SourceName
	}
	return "", "", fmt.Errorf("luabind: prototype (source %q) belongs to no registered chunk — cannot persist", src)
}

// ChunkCount reports how many distinct chunks are registered (test/debug aid).
func (r *ChunkRegistry) ChunkCount() int { return len(r.byID) }

// ProtoCount reports how many prototypes a chunk contributes (top + nested),
// or -1 if the chunk-id is unknown (test/debug aid).
func (r *ChunkRegistry) ProtoCount(chunkID string) int {
	rec, ok := r.byID[chunkID]
	if !ok {
		return -1
	}
	return len(rec.paths)
}
