package luabind

// #264 persister, step 2: the LValue data-graph serializer. Registry slots and
// closed upvalue values are LValues, most of them tables, so a faithful
// table-graph (de)serializer is a prerequisite for the full suspended-state
// serializer. This layer handles the DATA subset — nil, boolean, number,
// string, and table — with pointer-identity interning so shared subtables and
// cycles survive the round trip as the same object, not as duplicated copies.
//
// Functions, threads (nested coroutines), and userdata are NOT data — they
// carry execution/host state and are serialized in later steps (by chunk-id +
// proto-path, by nested LState snapshot, and by litd handle id+generation
// respectively). Encountering one here is a LOUD save-time error: per the
// issue, an unserializable value reachable from a saved state must never be
// silently dropped.
//
// The wire form is JSON with ordered arrays (no Go maps), so the blob is
// deterministic byte-for-byte and human-inspectable (chunk-ids and the like
// appear as plain strings when later steps extend `sval`).

import (
	"encoding/json"
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// sval tags one serialized value. T selects which payload field is live.
type sval struct {
	T   string  `json:"t"`             // "nil" | "bool" | "num" | "str" | "table" | "func"
	B   bool    `json:"b,omitempty"`   // T=="bool"
	N   float64 `json:"n,omitempty"`   // T=="num"
	S   string  `json:"s,omitempty"`   // T=="str"
	Ref int     `json:"ref,omitempty"` // T=="table": index into Tables; T=="func": index into Funcs
}

// supval is one serialized upvalue of a closure: OPEN (aliases the owning
// thread's register at Index) or CLOSED (owns Val, serialized through the same
// graph and so able to reference shared tables/closures).
type supval struct {
	Open  bool `json:"open,omitempty"`
	Index int  `json:"index,omitempty"`
	Val   sval `json:"val,omitempty"`
}

// sfunc is one interned closure: a proto reference (chunk-id + proto-path,
// never bytecode) plus its upvalues.
type sfunc struct {
	Chunk  string   `json:"chunk"`
	Proto  string   `json:"proto"`
	Upvals []supval `json:"upvals,omitempty"`
}

// stable is one interned table: its entries in deterministic ForEach order
// (array part, then string keys, then other keys — insertion order within
// each), plus its metatable (a table ref, or nil).
type stable struct {
	Keys []sval `json:"keys"`
	Vals []sval `json:"vals"`
	Meta *sval  `json:"meta,omitempty"`
}

// valueBlob is the serialized data graph: a root value plus the interned table
// pool it references. Table refs are indices into Tables.
type valueBlob struct {
	Root   sval     `json:"root"`
	Tables []stable `json:"tables"`
}

// valuesBlob is the multi-root form: several values sharing ONE interned table
// (and closure) pool, so an object referenced by more than one root (e.g. two
// register slots aliasing the same table, or two closures sharing an upvalue)
// round-trips as a single shared object rather than independent copies.
type valuesBlob struct {
	Roots  []sval   `json:"roots"`
	Tables []stable `json:"tables"`
	Funcs  []sfunc  `json:"funcs,omitempty"`
}

// vEncoder interns tables (and, when reg+owner are set, closures) by pointer
// identity while walking the graph. reg/owner are nil for the data-only public
// API (SerializeValue[s]), which rejects functions; the thread persister sets
// them to serialize register closures.
type vEncoder struct {
	ids    map[*lua.LTable]int
	tables []stable

	reg   *ChunkRegistry
	owner *lua.LState
	fnIDs map[*lua.LFunction]int
	funcs []sfunc
}

// SerializeValue encodes a data-subset LValue graph to a deterministic blob.
// Loud error on any non-data value (function/thread/userdata/channel) or a
// non-table metatable.
func SerializeValue(v lua.LValue) ([]byte, error) {
	e := &vEncoder{ids: map[*lua.LTable]int{}}
	root, err := e.encode(v)
	if err != nil {
		return nil, err
	}
	return json.Marshal(valueBlob{Root: root, Tables: e.tables})
}

func (e *vEncoder) encode(v lua.LValue) (sval, error) {
	switch x := v.(type) {
	case *lua.LNilType:
		return sval{T: "nil"}, nil
	case lua.LBool:
		return sval{T: "bool", B: bool(x)}, nil
	case lua.LNumber:
		return sval{T: "num", N: float64(x)}, nil
	case lua.LString:
		return sval{T: "str", S: string(x)}, nil
	case *lua.LTable:
		return e.encodeTable(x)
	case *lua.LFunction:
		return e.encodeFunc(x)
	default:
		return sval{}, fmt.Errorf("luabind: cannot serialize value of type %s in the data graph (threads/userdata are handled by later persister steps)", v.Type())
	}
}

// encodeFunc interns a closure as a proto reference plus its upvalues. Only
// available when the encoder has a registry+owner (the thread persister); the
// data-only API leaves reg nil and rejects functions.
func (e *vEncoder) encodeFunc(fn *lua.LFunction) (sval, error) {
	if e.reg == nil {
		return sval{}, fmt.Errorf("luabind: cannot serialize a function in the data graph (no chunk registry — use the thread persister)")
	}
	if id, ok := e.fnIDs[fn]; ok {
		return sval{T: "func", Ref: id}, nil // already interned (shared / cyclic)
	}
	if fn.IsG || fn.Proto == nil {
		return sval{}, fmt.Errorf("luabind: cannot serialize a Go-function value")
	}
	chunk, path, err := e.reg.PathOf(fn.Proto)
	if err != nil {
		return sval{}, err
	}
	id := len(e.funcs)
	e.fnIDs[fn] = id
	e.funcs = append(e.funcs, sfunc{}) // reserve before recursing (cycles)

	views, err := fn.LitdUpvalueViews(e.owner)
	if err != nil {
		return sval{}, err
	}
	ups := make([]supval, len(views))
	for i, vw := range views {
		if vw.Closed {
			ev, err := e.encode(vw.Value)
			if err != nil {
				return sval{}, err
			}
			ups[i] = supval{Val: ev}
			continue
		}
		ups[i] = supval{Open: true, Index: vw.Index}
	}
	e.funcs[id] = sfunc{Chunk: chunk, Proto: path, Upvals: ups}
	return sval{T: "func", Ref: id}, nil
}

// serializeRegisters encodes a thread's register values as one shared graph,
// resolving closures through reg and classifying their upvalues relative to
// owner (the thread being saved). Loud error on any value that cannot be
// persisted.
func serializeRegisters(reg *ChunkRegistry, owner *lua.LState, vs []lua.LValue) ([]byte, error) {
	e := &vEncoder{ids: map[*lua.LTable]int{}, fnIDs: map[*lua.LFunction]int{}, reg: reg, owner: owner}
	roots := make([]sval, len(vs))
	for i, v := range vs {
		r, err := e.encode(v)
		if err != nil {
			return nil, fmt.Errorf("luabind: value %d: %w", i, err)
		}
		roots[i] = r
	}
	return json.Marshal(valuesBlob{Roots: roots, Tables: e.tables, Funcs: e.funcs})
}

func (e *vEncoder) encodeTable(t *lua.LTable) (sval, error) {
	if id, ok := e.ids[t]; ok {
		return sval{T: "table", Ref: id}, nil // already interned (shared / cyclic)
	}
	id := len(e.tables)
	e.ids[t] = id
	e.tables = append(e.tables, stable{}) // reserve the slot before recursing (cycles)

	var keys, vals []sval
	var encErr error
	t.ForEach(func(k, v lua.LValue) {
		if encErr != nil {
			return
		}
		ek, err := e.encode(k)
		if err != nil {
			encErr = err
			return
		}
		ev, err := e.encode(v)
		if err != nil {
			encErr = err
			return
		}
		keys = append(keys, ek)
		vals = append(vals, ev)
	})
	if encErr != nil {
		return sval{}, encErr
	}

	rec := stable{Keys: keys, Vals: vals}
	if t.Metatable != nil && t.Metatable != lua.LNil {
		mt, ok := t.Metatable.(*lua.LTable)
		if !ok {
			return sval{}, fmt.Errorf("luabind: cannot serialize a non-table metatable (%s)", t.Metatable.Type())
		}
		emt, err := e.encodeTable(mt)
		if err != nil {
			return sval{}, err
		}
		rec.Meta = &emt
	}
	e.tables[id] = rec
	return sval{T: "table", Ref: id}, nil
}

// decodeTables allocates and wires the interned table pool, returning the pool
// and a decoder closure over it. Sharing and cycles are restored by allocating
// every table first (pass 1), then wiring entries (pass 2) — so a ref always
// resolves to the same object.
func decodeTables(L *lua.LState, tables []stable) (func(sval) (lua.LValue, error), error) {
	pool := make([]*lua.LTable, len(tables))
	for i := range tables {
		pool[i] = L.NewTable()
	}
	decode := func(s sval) (lua.LValue, error) {
		switch s.T {
		case "nil":
			return lua.LNil, nil
		case "bool":
			return lua.LBool(s.B), nil
		case "num":
			return lua.LNumber(s.N), nil
		case "str":
			return lua.LString(s.S), nil
		case "table":
			if s.Ref < 0 || s.Ref >= len(pool) {
				return nil, fmt.Errorf("luabind: table ref %d out of range (%d tables)", s.Ref, len(pool))
			}
			return pool[s.Ref], nil
		default:
			return nil, fmt.Errorf("luabind: unknown serialized value tag %q", s.T)
		}
	}
	for i, st := range tables {
		if len(st.Keys) != len(st.Vals) {
			return nil, fmt.Errorf("luabind: table %d has %d keys but %d vals", i, len(st.Keys), len(st.Vals))
		}
		tbl := pool[i]
		for j := range st.Keys {
			k, err := decode(st.Keys[j])
			if err != nil {
				return nil, err
			}
			v, err := decode(st.Vals[j])
			if err != nil {
				return nil, err
			}
			tbl.RawSet(k, v)
		}
		if st.Meta != nil {
			mv, err := decode(*st.Meta)
			if err != nil {
				return nil, err
			}
			tbl.Metatable = mv
		}
	}
	return decode, nil
}

// graphDecoder reconstructs a function-aware multi-root graph (produced by
// serializeRegisters). Tables and closures are allocated up front into pools so
// refs and cycles resolve; closure upvalues are wired separately, after the
// owning thread exists, via wireUpvalues.
type graphDecoder struct {
	parent    *lua.LState
	blob      *valuesBlob
	tablePool []*lua.LTable
	fnPool    []*lua.LFunction
}

func newGraphDecoder(parent *lua.LState, reg *ChunkRegistry, blob *valuesBlob) (*graphDecoder, error) {
	d := &graphDecoder{parent: parent, blob: blob}
	d.tablePool = make([]*lua.LTable, len(blob.Tables))
	for i := range blob.Tables {
		d.tablePool[i] = parent.NewTable()
	}
	d.fnPool = make([]*lua.LFunction, len(blob.Funcs))
	for i, sf := range blob.Funcs {
		proto, err := reg.ResolveProto(sf.Chunk, sf.Proto)
		if err != nil {
			return nil, fmt.Errorf("luabind: closure %d: %w", i, err)
		}
		d.fnPool[i] = parent.LitdMakeClosure(proto)
	}
	// Wire table entries now (closures may be stored but their upvalues wait).
	for i, st := range blob.Tables {
		if len(st.Keys) != len(st.Vals) {
			return nil, fmt.Errorf("luabind: table %d has %d keys but %d vals", i, len(st.Keys), len(st.Vals))
		}
		tbl := d.tablePool[i]
		for j := range st.Keys {
			k, err := d.decode(st.Keys[j])
			if err != nil {
				return nil, err
			}
			val, err := d.decode(st.Vals[j])
			if err != nil {
				return nil, err
			}
			tbl.RawSet(k, val)
		}
		if st.Meta != nil {
			mv, err := d.decode(*st.Meta)
			if err != nil {
				return nil, err
			}
			tbl.Metatable = mv
		}
	}
	return d, nil
}

func (d *graphDecoder) decode(s sval) (lua.LValue, error) {
	switch s.T {
	case "nil":
		return lua.LNil, nil
	case "bool":
		return lua.LBool(s.B), nil
	case "num":
		return lua.LNumber(s.N), nil
	case "str":
		return lua.LString(s.S), nil
	case "table":
		if s.Ref < 0 || s.Ref >= len(d.tablePool) {
			return nil, fmt.Errorf("luabind: table ref %d out of range (%d tables)", s.Ref, len(d.tablePool))
		}
		return d.tablePool[s.Ref], nil
	case "func":
		if s.Ref < 0 || s.Ref >= len(d.fnPool) {
			return nil, fmt.Errorf("luabind: func ref %d out of range (%d funcs)", s.Ref, len(d.fnPool))
		}
		return d.fnPool[s.Ref], nil
	default:
		return nil, fmt.Errorf("luabind: unknown serialized value tag %q", s.T)
	}
}

func (d *graphDecoder) roots() ([]lua.LValue, error) {
	out := make([]lua.LValue, len(d.blob.Roots))
	for i, r := range d.blob.Roots {
		v, err := d.decode(r)
		if err != nil {
			return nil, err
		}
		out[i] = v
	}
	return out, nil
}

// wireUpvalues binds each reconstructed closure's upvalues against th. Open
// upvalues go through th.LitdBindOpenUpvalue (so they alias th's live registers
// and shared cells coincide); closed upvalues take their decoded value.
func (d *graphDecoder) wireUpvalues(th *lua.LState) error {
	for i, sf := range d.blob.Funcs {
		fn := d.fnPool[i]
		for j, up := range sf.Upvals {
			if up.Open {
				th.LitdBindOpenUpvalue(fn, j, up.Index)
				continue
			}
			val, err := d.decode(up.Val)
			if err != nil {
				return fmt.Errorf("luabind: closure %d upvalue %d: %w", i, j, err)
			}
			fn.LitdSetUpvalueClosed(j, val)
		}
	}
	return nil
}

// DeserializeValue reconstructs a data graph (produced by SerializeValue) into
// fresh tables on L.
func DeserializeValue(L *lua.LState, data []byte) (lua.LValue, error) {
	var b valueBlob
	if err := json.Unmarshal(data, &b); err != nil {
		return nil, fmt.Errorf("luabind: malformed value blob: %w", err)
	}
	decode, err := decodeTables(L, b.Tables)
	if err != nil {
		return nil, err
	}
	return decode(b.Root)
}
