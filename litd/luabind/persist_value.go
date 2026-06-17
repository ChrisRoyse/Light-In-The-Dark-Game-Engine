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
	T   string  `json:"t"`             // "nil" | "bool" | "num" | "str" | "table"
	B   bool    `json:"b,omitempty"`   // T=="bool"
	N   float64 `json:"n,omitempty"`   // T=="num"
	S   string  `json:"s,omitempty"`   // T=="str"
	Ref int     `json:"ref,omitempty"` // T=="table": index into blob.Tables
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
// pool, so a table referenced by more than one root (e.g. two register slots
// aliasing the same table) round-trips as a single shared object rather than
// independent copies.
type valuesBlob struct {
	Roots  []sval   `json:"roots"`
	Tables []stable `json:"tables"`
}

// vEncoder interns tables by pointer identity while walking the graph.
type vEncoder struct {
	ids    map[*lua.LTable]int
	tables []stable
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

// SerializeValues encodes several LValues sharing one interned table pool, so
// identity shared ACROSS the values (not just within one) survives the round
// trip. Loud error (naming the value index) on any non-data value.
func SerializeValues(vs []lua.LValue) ([]byte, error) {
	e := &vEncoder{ids: map[*lua.LTable]int{}}
	roots := make([]sval, len(vs))
	for i, v := range vs {
		r, err := e.encode(v)
		if err != nil {
			return nil, fmt.Errorf("luabind: value %d: %w", i, err)
		}
		roots[i] = r
	}
	return json.Marshal(valuesBlob{Roots: roots, Tables: e.tables})
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
	default:
		return sval{}, fmt.Errorf("luabind: cannot serialize value of type %s in the data graph (functions/threads/userdata are handled by later persister steps)", v.Type())
	}
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

// DeserializeValues reconstructs a multi-root graph (produced by
// SerializeValues) into fresh tables on L, preserving identity shared across
// the roots.
func DeserializeValues(L *lua.LState, data []byte) ([]lua.LValue, error) {
	var b valuesBlob
	if err := json.Unmarshal(data, &b); err != nil {
		return nil, fmt.Errorf("luabind: malformed values blob: %w", err)
	}
	decode, err := decodeTables(L, b.Tables)
	if err != nil {
		return nil, err
	}
	out := make([]lua.LValue, len(b.Roots))
	for i, r := range b.Roots {
		v, err := decode(r)
		if err != nil {
			return nil, err
		}
		out[i] = v
	}
	return out, nil
}
