package main

// jassparse_body.go parses blizzard.j functions *including their bodies* into a
// statement/expression AST. The AST is rich enough for the D1/D2/D4
// deduplication heuristics (deduplication-policy.md §2–§5): it distinguishes a
// single passthrough call/return with unmodified args (D1), a single native
// call with reordered/nested/constant-defaulted args (D2), and real control
// flow or multi-statement state (D4). Stdlib only; deterministic source order.

import (
	"fmt"
	"strings"
)

// --- expression AST ---

// Expr is a parsed JASS expression node.
type Expr interface{ exprString() string }

// Ident is a bare identifier reference (variable or parameter).
type Ident struct{ Name string }

// Lit is a literal: Kind in {number,string,rawcode,hex,bool,null}.
type Lit struct {
	Kind string
	Val  string
}

// CallExpr is a function/native invocation with ordered arguments.
type CallExpr struct {
	Func string
	Args []Expr
}

// IndexExpr is an array access `base[idx]`.
type IndexExpr struct {
	Base string
	Idx  Expr
}

// Binary is a binary operation.
type Binary struct {
	Op   string
	L, R Expr
}

// Unary is a prefix operation (`-`, `not`).
type Unary struct {
	Op string
	X  Expr
}

func (e Ident) exprString() string { return e.Name }
func (e Lit) exprString() string {
	if e.Kind == "string" {
		return fmt.Sprintf("%q", e.Val)
	}
	return e.Val
}
func (e CallExpr) exprString() string {
	parts := make([]string, len(e.Args))
	for i, a := range e.Args {
		parts[i] = a.exprString()
	}
	return e.Func + "(" + strings.Join(parts, ", ") + ")"
}
func (e IndexExpr) exprString() string { return e.Base + "[" + e.Idx.exprString() + "]" }
func (e Binary) exprString() string {
	return "(" + e.L.exprString() + " " + e.Op + " " + e.R.exprString() + ")"
}
func (e Unary) exprString() string { return e.Op + e.X.exprString() }

// --- statement AST ---

// Stmt is a parsed JASS statement node.
type Stmt interface{ stmtKind() string }

// LocalStmt is `local <type> <name> [= init]`.
type LocalStmt struct {
	Type, Name string
	Init       Expr // nil if uninitialized
}

// SetStmt is `set <target>[<index>] = <value>`.
type SetStmt struct {
	Target string
	Index  Expr // nil if not an array assignment
	Value  Expr
}

// CallStmt is `call <CallExpr>`.
type CallStmt struct{ Call *CallExpr }

// ReturnStmt is `return [value]`.
type ReturnStmt struct{ Value Expr } // nil for bare return

// IfStmt is an if/elseif/else/endif chain.
type IfStmt struct {
	Cond    Expr
	Then    []Stmt
	ElseIfs []ElseIf
	Else    []Stmt
}

// ElseIf is one `elseif cond then ...` arm.
type ElseIf struct {
	Cond Expr
	Body []Stmt
}

// LoopStmt is `loop ... endloop`.
type LoopStmt struct{ Body []Stmt }

// ExitWhenStmt is `exitwhen <cond>`.
type ExitWhenStmt struct{ Cond Expr }

// DebugStmt wraps a statement guarded by the `debug` keyword.
type DebugStmt struct{ Inner Stmt }

// RawStmt captures a statement the parser does not model, preserving its text
// so unknown constructs are recorded rather than silently dropped.
type RawStmt struct{ Text string }

func (LocalStmt) stmtKind() string    { return "local" }
func (SetStmt) stmtKind() string      { return "set" }
func (CallStmt) stmtKind() string     { return "call" }
func (ReturnStmt) stmtKind() string   { return "return" }
func (IfStmt) stmtKind() string       { return "if" }
func (LoopStmt) stmtKind() string     { return "loop" }
func (ExitWhenStmt) stmtKind() string { return "exitwhen" }
func (DebugStmt) stmtKind() string    { return "debug" }
func (RawStmt) stmtKind() string      { return "raw" }

// Func is a parsed blizzard.j function with its body.
type Func struct {
	Name     string
	Params   []Param
	Returns  string
	Constant bool
	Body     []Stmt
	Line     int
}

// --- body lexer ---

type btokKind int

const (
	bEOF btokKind = iota
	bIdent
	bNumber
	bString
	bRawcode
	bOp // operators and delimiters: == != <= >= < > + - * / = , ( ) [ ]
	bNL
)

type btok struct {
	kind btokKind
	lit  string
	line int
}

func lexBody(src string) []btok {
	var toks []btok
	line := 1
	n := len(src)
	for i := 0; i < n; {
		c := src[i]
		switch {
		case c == '\n':
			toks = append(toks, btok{bNL, "\n", line})
			line++
			i++
		case c == ' ' || c == '\t' || c == '\r':
			i++
		case c == '/' && i+1 < n && src[i+1] == '/':
			for i < n && src[i] != '\n' {
				i++
			}
		case c == '"':
			j := i + 1
			for j < n && src[j] != '"' {
				if src[j] == '\\' && j+1 < n {
					j++
				}
				j++
			}
			// include surrounding quotes' contents (without quotes)
			lit := src[i+1 : min(j, n)]
			toks = append(toks, btok{bString, lit, line})
			i = j + 1
		case c == '\'':
			j := i + 1
			for j < n && src[j] != '\'' {
				j++
			}
			toks = append(toks, btok{bRawcode, src[i+1 : min(j, n)], line})
			i = j + 1
		case c == '$':
			j := i + 1
			for j < n && isHex(src[j]) {
				j++
			}
			toks = append(toks, btok{bNumber, src[i:j], line})
			i = j
		case c >= '0' && c <= '9' || (c == '.' && i+1 < n && src[i+1] >= '0' && src[i+1] <= '9'):
			j := i
			for j < n && (isIdentPart(src[j]) || src[j] == '.') {
				j++
			}
			toks = append(toks, btok{bNumber, src[i:j], line})
			i = j
		case isIdentStart(c):
			j := i + 1
			for j < n && isIdentPart(src[j]) {
				j++
			}
			toks = append(toks, btok{bIdent, src[i:j], line})
			i = j
		case c == '=' || c == '!' || c == '<' || c == '>':
			if i+1 < n && src[i+1] == '=' {
				toks = append(toks, btok{bOp, src[i : i+2], line})
				i += 2
			} else {
				toks = append(toks, btok{bOp, string(c), line})
				i++
			}
		case strings.IndexByte("+-*/,()[]", c) >= 0:
			toks = append(toks, btok{bOp, string(c), line})
			i++
		default:
			i++ // skip anything unmodeled
		}
	}
	toks = append(toks, btok{bEOF, "", line})
	return toks
}

func isHex(c byte) bool {
	return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

// --- body parser ---

type bodyParser struct {
	toks []btok
	pos  int
}

func (p *bodyParser) peek() btok { return p.toks[p.pos] }
func (p *bodyParser) advance() btok {
	t := p.toks[p.pos]
	if p.pos < len(p.toks)-1 {
		p.pos++
	}
	return t
}
func (p *bodyParser) isKw(s string) bool {
	t := p.peek()
	return t.kind == bIdent && t.lit == s
}
func (p *bodyParser) isOp(s string) bool {
	t := p.peek()
	return t.kind == bOp && t.lit == s
}
func (p *bodyParser) skipNL() {
	for p.peek().kind == bNL {
		p.advance()
	}
}

// ParseFuncs parses all top-level `function ... endfunction` definitions in
// source order. globals blocks and unmodeled top-level constructs are skipped.
func ParseFuncs(src string) []Func {
	p := &bodyParser{toks: lexBody(src)}
	var funcs []Func
	for p.peek().kind != bEOF {
		p.skipNL()
		t := p.peek()
		if t.kind != bIdent {
			p.advance()
			continue
		}
		switch t.lit {
		case "globals":
			p.skipTo("endglobals")
		case "function":
			funcs = append(funcs, p.parseFunc(false))
		case "constant":
			if p.toks[min(p.pos+1, len(p.toks)-1)].lit == "function" {
				p.advance()
				funcs = append(funcs, p.parseFunc(true))
			} else {
				p.advance()
			}
		default:
			p.advance()
		}
	}
	return funcs
}

func (p *bodyParser) skipTo(kw string) {
	for p.peek().kind != bEOF {
		t := p.advance()
		if t.kind == bIdent && t.lit == kw {
			return
		}
	}
}

func (p *bodyParser) parseFunc(constant bool) Func {
	kw := p.advance() // `function`
	f := Func{Constant: constant, Line: kw.line}
	if p.peek().kind == bIdent {
		f.Name = p.advance().lit
	}
	if p.isKw("takes") {
		p.advance()
		f.Params = p.parseParams()
	}
	if p.isKw("returns") {
		p.advance()
		if p.peek().kind == bIdent {
			f.Returns = p.advance().lit
		}
	}
	f.Body = p.parseBlock("endfunction")
	if p.isKw("endfunction") {
		p.advance()
	}
	return f
}

func (p *bodyParser) parseParams() []Param {
	if p.isKw("nothing") {
		p.advance()
		return nil
	}
	var params []Param
	for {
		if p.peek().kind != bIdent {
			break
		}
		typ := p.advance().lit
		if p.peek().kind != bIdent {
			break
		}
		nm := p.advance().lit
		params = append(params, Param{Type: typ, Name: nm})
		if p.isOp(",") {
			p.advance()
			continue
		}
		break
	}
	return params
}

// parseBlock parses statements until one of the terminator keywords is the next
// token (terminator is left unconsumed).
func (p *bodyParser) parseBlock(terms ...string) []Stmt {
	var stmts []Stmt
	for {
		p.skipNL()
		t := p.peek()
		if t.kind == bEOF {
			return stmts
		}
		if t.kind == bIdent && contains(terms, t.lit) {
			return stmts
		}
		s := p.parseStmt()
		if s != nil {
			stmts = append(stmts, s)
		}
	}
}

func contains(ss []string, s string) bool {
	for _, x := range ss {
		if x == s {
			return true
		}
	}
	return false
}

func (p *bodyParser) parseStmt() Stmt {
	t := p.peek()
	if t.kind != bIdent {
		p.advance()
		return nil
	}
	switch t.lit {
	case "local":
		return p.parseLocal()
	case "set":
		return p.parseSet()
	case "call":
		p.advance()
		e := p.parseExpr()
		if ce, ok := e.(CallExpr); ok {
			return CallStmt{Call: &ce}
		}
		return RawStmt{Text: "call " + exprStr(e)}
	case "return":
		p.advance()
		if p.peek().kind == bNL || p.peek().kind == bEOF {
			return ReturnStmt{}
		}
		return ReturnStmt{Value: p.parseExpr()}
	case "if":
		return p.parseIf()
	case "loop":
		p.advance()
		body := p.parseBlock("endloop")
		if p.isKw("endloop") {
			p.advance()
		}
		return LoopStmt{Body: body}
	case "exitwhen":
		p.advance()
		return ExitWhenStmt{Cond: p.parseExpr()}
	case "debug":
		p.advance()
		inner := p.parseStmt()
		return DebugStmt{Inner: inner}
	default:
		// unknown statement: capture text to end of line
		return p.parseRawLine()
	}
}

func (p *bodyParser) parseRawLine() Stmt {
	var parts []string
	for p.peek().kind != bNL && p.peek().kind != bEOF {
		parts = append(parts, p.advance().lit)
	}
	return RawStmt{Text: strings.Join(parts, " ")}
}

func (p *bodyParser) parseLocal() Stmt {
	p.advance() // local
	s := LocalStmt{}
	if p.peek().kind == bIdent {
		s.Type = p.advance().lit
	}
	// `array` keyword after type for array locals
	if p.isKw("array") {
		s.Type += " array"
		p.advance()
	}
	if p.peek().kind == bIdent {
		s.Name = p.advance().lit
	}
	if p.isOp("=") {
		p.advance()
		s.Init = p.parseExpr()
	}
	return s
}

func (p *bodyParser) parseSet() Stmt {
	p.advance() // set
	s := SetStmt{}
	if p.peek().kind == bIdent {
		s.Target = p.advance().lit
	}
	if p.isOp("[") {
		p.advance()
		s.Index = p.parseExpr()
		if p.isOp("]") {
			p.advance()
		}
	}
	if p.isOp("=") {
		p.advance()
		s.Value = p.parseExpr()
	}
	return s
}

func (p *bodyParser) parseIf() Stmt {
	p.advance() // if
	s := IfStmt{Cond: p.parseExpr()}
	if p.isKw("then") {
		p.advance()
	}
	s.Then = p.parseBlock("elseif", "else", "endif")
	for p.isKw("elseif") {
		p.advance()
		ei := ElseIf{Cond: p.parseExpr()}
		if p.isKw("then") {
			p.advance()
		}
		ei.Body = p.parseBlock("elseif", "else", "endif")
		s.ElseIfs = append(s.ElseIfs, ei)
	}
	if p.isKw("else") {
		p.advance()
		s.Else = p.parseBlock("endif")
	}
	if p.isKw("endif") {
		p.advance()
	}
	return s
}

// --- expression parser (precedence climbing) ---

func (p *bodyParser) parseExpr() Expr { return p.parseOr() }

func (p *bodyParser) parseOr() Expr {
	l := p.parseAnd()
	for p.isKw("or") {
		p.advance()
		l = Binary{Op: "or", L: l, R: p.parseAnd()}
	}
	return l
}
func (p *bodyParser) parseAnd() Expr {
	l := p.parseCmp()
	for p.isKw("and") {
		p.advance()
		l = Binary{Op: "and", L: l, R: p.parseCmp()}
	}
	return l
}
func (p *bodyParser) parseCmp() Expr {
	l := p.parseAdd()
	for {
		t := p.peek()
		if t.kind == bOp && (t.lit == "==" || t.lit == "!=" || t.lit == "<" || t.lit == "<=" || t.lit == ">" || t.lit == ">=") {
			p.advance()
			l = Binary{Op: t.lit, L: l, R: p.parseAdd()}
			continue
		}
		return l
	}
}
func (p *bodyParser) parseAdd() Expr {
	l := p.parseMul()
	for {
		t := p.peek()
		if t.kind == bOp && (t.lit == "+" || t.lit == "-") {
			p.advance()
			l = Binary{Op: t.lit, L: l, R: p.parseMul()}
			continue
		}
		return l
	}
}
func (p *bodyParser) parseMul() Expr {
	l := p.parseUnary()
	for {
		t := p.peek()
		if t.kind == bOp && (t.lit == "*" || t.lit == "/") {
			p.advance()
			l = Binary{Op: t.lit, L: l, R: p.parseUnary()}
			continue
		}
		return l
	}
}
func (p *bodyParser) parseUnary() Expr {
	if p.isKw("not") {
		p.advance()
		return Unary{Op: "not ", X: p.parseUnary()}
	}
	if p.isOp("-") {
		p.advance()
		return Unary{Op: "-", X: p.parseUnary()}
	}
	return p.parsePrimary()
}
func (p *bodyParser) parsePrimary() Expr {
	t := p.peek()
	switch t.kind {
	case bNumber:
		p.advance()
		return Lit{Kind: "number", Val: t.lit}
	case bString:
		p.advance()
		return Lit{Kind: "string", Val: t.lit}
	case bRawcode:
		p.advance()
		return Lit{Kind: "rawcode", Val: t.lit}
	case bOp:
		if t.lit == "(" {
			p.advance()
			e := p.parseExpr()
			if p.isOp(")") {
				p.advance()
			}
			return e // unwrap parentheses
		}
		p.advance()
		return Lit{Kind: "op", Val: t.lit}
	case bIdent:
		switch t.lit {
		case "null":
			p.advance()
			return Lit{Kind: "null", Val: "null"}
		case "true", "false":
			p.advance()
			return Lit{Kind: "bool", Val: t.lit}
		}
		p.advance()
		if p.isOp("(") {
			p.advance()
			args := p.parseArgs()
			return CallExpr{Func: t.lit, Args: args}
		}
		if p.isOp("[") {
			p.advance()
			idx := p.parseExpr()
			if p.isOp("]") {
				p.advance()
			}
			return IndexExpr{Base: t.lit, Idx: idx}
		}
		return Ident{Name: t.lit}
	default:
		p.advance()
		return Lit{Kind: "unknown", Val: t.lit}
	}
}
func (p *bodyParser) parseArgs() []Expr {
	var args []Expr
	if p.isOp(")") {
		p.advance()
		return args
	}
	for {
		args = append(args, p.parseExpr())
		if p.isOp(",") {
			p.advance()
			continue
		}
		break
	}
	if p.isOp(")") {
		p.advance()
	}
	return args
}

func exprStr(e Expr) string {
	if e == nil {
		return ""
	}
	return e.exprString()
}

// --- shape classification (for FSV dumps + downstream D-detection) ---

// Shape returns a coarse body classification used by the D1/D2/D4 heuristics:
//   - "passthrough-return": single `return Call(args...)` with all args being
//     bare parameter identifiers in order (D1 candidate).
//   - "passthrough-call": single `call Call(args...)` with all-identifier args.
//   - "single-call-modified": single call/return whose args include nested
//     calls, literals, or reordering (D2 candidate).
//   - "control-flow": contains if/loop (D4).
//   - "empty" / "other".
func (f Func) Shape() string {
	if len(f.Body) == 0 {
		return "empty"
	}
	if hasControlFlow(f.Body) {
		return "control-flow"
	}
	if len(f.Body) == 1 {
		switch s := f.Body[0].(type) {
		case ReturnStmt:
			if ce, ok := s.Value.(CallExpr); ok {
				return callShape(ce, "return")
			}
		case CallStmt:
			return callShape(*s.Call, "call")
		}
	}
	return "other"
}

func callShape(ce CallExpr, prefix string) string {
	if allBareIdents(ce.Args) {
		return "passthrough-" + prefix
	}
	return "single-call-modified"
}

func allBareIdents(args []Expr) bool {
	for _, a := range args {
		if _, ok := a.(Ident); !ok {
			return false
		}
	}
	return true
}

func hasControlFlow(stmts []Stmt) bool {
	for _, s := range stmts {
		switch s.(type) {
		case IfStmt, LoopStmt:
			return true
		}
	}
	return false
}

// DumpBody renders a function header, shape, and indented AST for FSV.
func (f Func) DumpBody() string {
	var b strings.Builder
	kw := "function"
	if f.Constant {
		kw = "constant function"
	}
	params := "nothing"
	if len(f.Params) > 0 {
		ps := make([]string, len(f.Params))
		for i, p := range f.Params {
			ps[i] = p.Type + " " + p.Name
		}
		params = strings.Join(ps, ", ")
	}
	fmt.Fprintf(&b, "%s %s takes %s returns %s  [shape=%s, stmts=%d]\n",
		kw, f.Name, params, f.Returns, f.Shape(), len(f.Body))
	for _, s := range f.Body {
		dumpStmt(&b, s, 1)
	}
	return b.String()
}

func dumpStmt(b *strings.Builder, s Stmt, indent int) {
	pad := strings.Repeat("  ", indent)
	switch v := s.(type) {
	case LocalStmt:
		init := ""
		if v.Init != nil {
			init = " = " + v.Init.exprString()
		}
		fmt.Fprintf(b, "%slocal %s %s%s\n", pad, v.Type, v.Name, init)
	case SetStmt:
		idx := ""
		if v.Index != nil {
			idx = "[" + v.Index.exprString() + "]"
		}
		fmt.Fprintf(b, "%sset %s%s = %s\n", pad, v.Target, idx, exprStr(v.Value))
	case CallStmt:
		fmt.Fprintf(b, "%scall %s\n", pad, v.Call.exprString())
	case ReturnStmt:
		fmt.Fprintf(b, "%sreturn %s\n", pad, exprStr(v.Value))
	case ExitWhenStmt:
		fmt.Fprintf(b, "%sexitwhen %s\n", pad, exprStr(v.Cond))
	case LoopStmt:
		fmt.Fprintf(b, "%sloop\n", pad)
		for _, s2 := range v.Body {
			dumpStmt(b, s2, indent+1)
		}
		fmt.Fprintf(b, "%sendloop\n", pad)
	case IfStmt:
		fmt.Fprintf(b, "%sif %s then\n", pad, exprStr(v.Cond))
		for _, s2 := range v.Then {
			dumpStmt(b, s2, indent+1)
		}
		for _, ei := range v.ElseIfs {
			fmt.Fprintf(b, "%selseif %s then\n", pad, exprStr(ei.Cond))
			for _, s2 := range ei.Body {
				dumpStmt(b, s2, indent+1)
			}
		}
		if len(v.Else) > 0 {
			fmt.Fprintf(b, "%selse\n", pad)
			for _, s2 := range v.Else {
				dumpStmt(b, s2, indent+1)
			}
		}
		fmt.Fprintf(b, "%sendif\n", pad)
	case DebugStmt:
		fmt.Fprintf(b, "%sdebug:\n", pad)
		if v.Inner != nil {
			dumpStmt(b, v.Inner, indent+1)
		}
	case RawStmt:
		fmt.Fprintf(b, "%sraw{ %s }\n", pad, v.Text)
	}
}

// CountGlobals returns the number of non-blank, non-comment declaration lines
// inside the top-level globals block(s).
func CountGlobals(src string) int {
	lines := strings.Split(src, "\n")
	in := false
	count := 0
	for _, ln := range lines {
		t := strings.TrimSpace(ln)
		switch {
		case t == "globals":
			in = true
		case t == "endglobals":
			in = false
		case in:
			if t == "" || strings.HasPrefix(t, "//") {
				continue
			}
			count++
		}
	}
	return count
}
