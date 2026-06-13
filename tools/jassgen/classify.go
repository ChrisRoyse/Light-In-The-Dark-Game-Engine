package main

// classify.go implements the mechanical D1–D5 dedup classifier
// (deduplication-policy.md §2–§6, PRD §4.2). It *proposes* a class per symbol
// from blizzard.j bodies and common.j/common.ai name+signature patterns. It
// never tombstones (G1.4 — tombstones come only from overrides) and never
// defaults: anything it cannot decide is reported `unclassified`.
//
//   D1 — BJ body is a single passthrough call/return of one symbol, args
//        unmodified and in parameter order.
//   D2 — single call with reordered / constant-defaulted / nested args, or the
//        bj_lastCreated* side-channel capture pattern.
//   D3 — native family differing only by Loc/X-Y/Counted/ById variants
//        (proposed only; always confirmed via overrides).
//   D4 — real control flow or genuine multi-statement logic.
//   D5 — enum-keyed state getter/setter (GetUnitState/SetUnitState style).

import (
	"strings"
)

// Class is a proposed dedup class.
type Class string

const (
	ClassD1           Class = "D1"
	ClassD2           Class = "D2"
	ClassD3           Class = "D3"
	ClassD4           Class = "D4"
	ClassD5           Class = "D5"
	ClassUnclassified Class = "unclassified"
)

// Classification is the verdict for one source symbol. Fields below Evidence
// are populated only when a reviewed override is applied (overrides.go).
type Classification struct {
	Name         string
	Origin       string
	Class        Class
	ClassifiedBy string // "heuristic" until an override flips it to "override"
	Family       string // D3 family stem, if any
	Evidence     string
	GoMapping     string   // canonical Go symbol (override-supplied)
	GoSignature   string   // Go signature text e.g. "() bool" (override-supplied, optional)
	CollapsesWith []string // D3: source names that collapse into GoMapping
	Package       string   // litd/api | litd/api/helpers | litd/ai
	Tombstone    string // tombstone reason enum value, if tombstoned via override
}

// stateAccessorNatives are the enum-keyed state natives that anchor D5.
var stateAccessorNatives = map[string]bool{
	"GetUnitState": true, "SetUnitState": true,
	"GetPlayerState": true, "SetPlayerState": true,
}

func isStateNativeName(name string) bool { return stateAccessorNatives[name] }

// ClassifyAll classifies blizzard.j functions plus common.j/common.ai natives.
// nativeNames / bjNames are the full symbol universes used to resolve call
// targets (native vs BJ) and to detect D3 families.
func ClassifyAll(funcs []Func, bjOrigin string, natives []Decl, nativeOrigins map[string]string) []Classification {
	nativeNames := map[string]bool{}
	var nativeNameList []string
	for _, d := range natives {
		if d.Kind == KindNative || d.Kind == KindConstantNative {
			nativeNames[d.Name] = true
			nativeNameList = append(nativeNameList, d.Name)
		}
	}
	bjNames := map[string]bool{}
	for _, f := range funcs {
		bjNames[f.Name] = true
	}
	d3 := proposeD3Families(nativeNameList, nativeNames)

	var out []Classification

	// BJ functions (have bodies).
	for _, f := range funcs {
		c := classifyBJ(f, nativeNames, bjNames)
		c.Origin = bjOrigin
		out = append(out, c)
	}

	// Natives (no bodies): D5 by name, D3 by family, else unclassified.
	for _, d := range natives {
		if d.Kind != KindNative && d.Kind != KindConstantNative {
			continue
		}
		c := Classification{Name: d.Name, Origin: nativeOrigins[d.Name], ClassifiedBy: "heuristic"}
		switch {
		case isStateNativeName(d.Name):
			c.Class = ClassD5
			c.Evidence = "enum-keyed state accessor native"
		case d3[d.Name] != "":
			c.Class = ClassD3
			c.Family = d3[d.Name]
			c.Evidence = "native family member (stem=" + d3[d.Name] + ")"
		default:
			c.Class = ClassUnclassified
			c.Evidence = "native: no mechanical dedup pattern (canonical or override-resolved)"
		}
		out = append(out, c)
	}
	return out
}

// classifyBJ classifies a single blizzard.j function from its body AST.
func classifyBJ(f Func, nativeNames, bjNames map[string]bool) Classification {
	c := Classification{Name: f.Name, ClassifiedBy: "heuristic"}

	if hasControlFlow(f.Body) {
		c.Class = ClassD4
		c.Evidence = "real control flow (if/loop)"
		return c
	}

	// bj_lastCreated* side-channel: { set G = Call(...) ; return G } -> D2.
	if len(f.Body) == 2 {
		if set, ok := f.Body[0].(SetStmt); ok {
			if ret, ok := f.Body[1].(ReturnStmt); ok {
				if call, ok := set.Value.(CallExpr); ok {
					if id, ok := ret.Value.(Ident); ok && id.Name == set.Target {
						c.Class = ClassD2
						c.Evidence = "side-channel capture: set " + set.Target + " = " + call.Func + "(...); return " + set.Target
						return c
					}
				}
			}
		}
	}

	if len(f.Body) == 1 {
		call := singleCall(f.Body[0])
		if call == nil {
			c.Class = ClassUnclassified
			c.Evidence = "single non-call statement"
			return c
		}
		// D5: routes through a state accessor native.
		if isStateNativeName(call.Func) {
			c.Class = ClassD5
			c.Evidence = "wraps state accessor " + call.Func
			return c
		}
		if allBareIdents(call.Args) && argsMatchParams(call.Args, f.Params) {
			c.Class = ClassD1
			c.Evidence = "passthrough -> " + call.Func + "(" + paramNames(f.Params) + ")"
			if !nativeNames[call.Func] {
				if bjNames[call.Func] {
					c.Evidence += " [wraps BJ; canonical resolves transitively]"
				} else {
					c.Evidence += " [callee not a known native]"
				}
			}
			return c
		}
		// reordered / literal-defaulted / nested args.
		c.Class = ClassD2
		c.Evidence = "single call " + call.Func + " with reordered/defaulted/nested args"
		return c
	}

	// >2 statements, no control flow: genuine multi-step logic.
	c.Class = ClassD4
	c.Evidence = "multi-statement logic (no single canonical call)"
	return c
}

// singleCall extracts the CallExpr from a return-of-call or a call statement.
func singleCall(s Stmt) *CallExpr {
	switch v := s.(type) {
	case ReturnStmt:
		if ce, ok := v.Value.(CallExpr); ok {
			return &ce
		}
	case CallStmt:
		return v.Call
	}
	return nil
}

// argsMatchParams reports whether args are exactly the parameters as bare
// identifiers, same count and same order (the D1 "unmodified" test).
func argsMatchParams(args []Expr, params []Param) bool {
	if len(args) != len(params) {
		return false
	}
	for i, a := range args {
		id, ok := a.(Ident)
		if !ok || id.Name != params[i].Name {
			return false
		}
	}
	return true
}

func paramNames(params []Param) string {
	names := make([]string, len(params))
	for i, p := range params {
		names[i] = p.Name
	}
	return strings.Join(names, ", ")
}

// proposeD3Families groups natives that differ only by Loc / X-Y / Counted /
// ById / ByName variants. Returns name -> family stem for every member. A
// variant only joins a family when its base form also exists as a native, which
// suppresses spurious suffix matches.
func proposeD3Families(names []string, set map[string]bool) map[string]string {
	fam := map[string]string{}
	mark := func(name, stem string) {
		fam[name] = stem
		if set[stem] {
			fam[stem] = stem
		}
	}
	for _, n := range names {
		switch {
		case strings.HasSuffix(n, "Loc") && set[strings.TrimSuffix(n, "Loc")]:
			mark(n, strings.TrimSuffix(n, "Loc"))
		case strings.HasSuffix(n, "Counted") && set[strings.TrimSuffix(n, "Counted")]:
			mark(n, strings.TrimSuffix(n, "Counted"))
		case strings.HasSuffix(n, "ById") && set[strings.TrimSuffix(n, "ById")]:
			mark(n, strings.TrimSuffix(n, "ById"))
		case strings.HasSuffix(n, "ByName") && set[strings.TrimSuffix(n, "ByName")]:
			mark(n, strings.TrimSuffix(n, "ByName"))
		case (strings.HasSuffix(n, "X") || strings.HasSuffix(n, "Y")) && len(n) > 1:
			base := n[:len(n)-1]
			var twin string
			if strings.HasSuffix(n, "X") {
				twin = base + "Y"
			} else {
				twin = base + "X"
			}
			if set[twin] {
				mark(n, base)
			}
		}
	}
	return fam
}

// TallyClasses returns per-class counts in stable order.
func TallyClasses(cs []Classification) map[Class]int {
	out := map[Class]int{}
	for _, c := range cs {
		out[c.Class]++
	}
	return out
}
