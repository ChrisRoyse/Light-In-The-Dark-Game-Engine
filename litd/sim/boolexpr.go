package sim

// Boolexpr-as-data condition tree (ADR #451, issue #457). A trigger's
// condition is a serializable boolean expression over a read-only event:
// leaves are condition HandlerRefs (#455) returning bool, interior nodes
// are And/Or/Not. Stored as a flat node arena indexed by ExprRef, never
// as a Go closure, so the condition graph is data that hashes and
// serializes (resolving the same #433/#450 closure problem the action
// side does).
//
// Evaluation short-circuits in author order (Go's && / ||) and allocates
// nothing on the heap (R-GC-1). The default semantics is AND: a trigger
// with no condition (NoExpr root) passes vacuously.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"

// exprOp is the node operator.
type exprOp uint8

const (
	exprCond exprOp = iota // leaf: a = HandlerRef of a condition; b unused
	exprAnd                // a,b = child ExprRefs
	exprOr                 // a,b = child ExprRefs
	exprNot                // a = child ExprRef; b unused
)

// exprNode is one arena node: a fixed-size value, no pointers.
type exprNode struct {
	op   exprOp
	a, b int32
}

// maxExprNodes bounds the arena on load — a corrupt save cannot drive an
// unbounded allocation. Generous for any authored condition tree.
const maxExprNodes = 1 << 20

// exprDepthLimit fails closed on a pathologically deep tree rather than
// risking a goroutine-stack overflow during evaluation.
const exprDepthLimit = 1024

// Cond appends a leaf wrapping condition handler h and returns its ref.
// This is the compile target for the `Where(pred)` sugar (ADR #451):
// a single-predicate condition is just one Cond leaf.
func (w *World) Cond(h HandlerRef) ExprRef {
	return w.appendExpr(exprNode{op: exprCond, a: int32(h)})
}

// And appends an AND node over two child exprs (all must pass).
func (w *World) And(a, b ExprRef) ExprRef {
	return w.appendExpr(exprNode{op: exprAnd, a: int32(a), b: int32(b)})
}

// Or appends an OR node over two child exprs (either may pass).
func (w *World) Or(a, b ExprRef) ExprRef {
	return w.appendExpr(exprNode{op: exprOr, a: int32(a), b: int32(b)})
}

// Not appends a NOT node negating its child expr.
func (w *World) Not(a ExprRef) ExprRef {
	return w.appendExpr(exprNode{op: exprNot, a: int32(a)})
}

// appendExpr adds a node to the arena and returns its ref. Allowed during
// Step (a runtime-authored trigger builds its condition tree from a firing
// trigger); ref assignment is append order, deterministic on replay.
func (w *World) appendExpr(n exprNode) ExprRef {
	w.exprArena = append(w.exprArena, n)
	return ExprRef(len(w.exprArena) - 1)
}

// EvalExpr evaluates the condition tree rooted at root against event e.
// A NoExpr root passes (vacuous AND). Short-circuits in author order;
// any unknown/out-of-range ref or unregistered condition handler is
// treated as false (fail-closed). When the DebugExprImpure hook is set,
// every evaluated leaf is run twice and the hook fires loudly on a
// mismatch (purity violation; execution-model.md §4).
func (w *World) EvalExpr(root ExprRef, e Event) bool {
	return w.evalExpr(root, e, 0)
}

func (w *World) evalExpr(ref ExprRef, e Event, depth int) bool {
	if ref == NoExpr {
		return true // vacuous AND — no condition means pass
	}
	if ref < 0 || int(ref) >= len(w.exprArena) || depth > exprDepthLimit {
		if w.DebugExprImpure != nil {
			w.DebugExprImpure(ref)
		}
		return false // fail-closed: corrupt ref or runaway depth
	}
	n := &w.exprArena[ref]
	switch n.op {
	case exprCond:
		fn, ok := w.ResolveHandlerRef(HandlerRef(n.a))
		if !ok {
			return false // fail-closed: condition handler not registered
		}
		r := fn(w, e)
		if w.DebugExprImpure != nil && fn(w, e) != r {
			w.DebugExprImpure(ref) // leaf is not pure — flag loudly
		}
		return r
	case exprNot:
		return !w.evalExpr(ExprRef(n.a), e, depth+1)
	case exprAnd:
		return w.evalExpr(ExprRef(n.a), e, depth+1) && w.evalExpr(ExprRef(n.b), e, depth+1)
	case exprOr:
		return w.evalExpr(ExprRef(n.a), e, depth+1) || w.evalExpr(ExprRef(n.b), e, depth+1)
	}
	return false
}

// hashExprArena folds the condition arena into h in node order (R-SIM-6):
// the count, then each node's op + operands.
func (w *World) hashExprArena(h *statehash.Hasher) {
	h.WriteU32(uint32(len(w.exprArena)))
	for i := range w.exprArena {
		n := &w.exprArena[i]
		h.WriteU8(uint8(n.op))
		h.WriteU32(uint32(n.a))
		h.WriteU32(uint32(n.b))
	}
}
