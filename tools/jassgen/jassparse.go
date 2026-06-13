// Package main implements jassgen, the JASS API parser and generator.
//
// jassparse.go is a small recursive-descent parser for the JASS declaration
// subset that appears in common.j and blizzard.j: type aliases, native
// declarations (plain and constant), and their `takes ... returns ...`
// signatures. It uses only the Go standard library (tooling.md §1) and emits
// declarations in source order so re-runs are byte-identical (R-AST-4).
package main

import (
	"fmt"
	"strings"
)

// DeclKind enumerates the top-level declaration shapes the parser recognizes.
type DeclKind string

const (
	// KindType is a `type X extends Y` handle-hierarchy alias.
	KindType DeclKind = "type"
	// KindNative is a `native Name takes ... returns ...` declaration.
	KindNative DeclKind = "native"
	// KindConstantNative is a `constant native Name takes ... returns ...`.
	KindConstantNative DeclKind = "constant native"
)

// Param is one `type name` pair from a `takes` list.
type Param struct {
	Type string
	Name string
}

// Decl is a parsed top-level declaration.
type Decl struct {
	Kind     DeclKind
	Name     string
	Line     int    // 1-based source line of the declaration keyword
	Extends  string // type decls only
	Constant bool   // native decls: true for `constant native`
	Params   []Param
	Returns  string
}

// Signature reconstructs a normalized, single-spaced declaration string. It is
// deterministic and used as the verbatim line in -dump-decls output.
func (d Decl) Signature() string {
	switch d.Kind {
	case KindType:
		return fmt.Sprintf("type %s extends %s", d.Name, d.Extends)
	case KindNative, KindConstantNative:
		var b strings.Builder
		if d.Constant {
			b.WriteString("constant ")
		}
		b.WriteString("native ")
		b.WriteString(d.Name)
		b.WriteString(" takes ")
		if len(d.Params) == 0 {
			b.WriteString("nothing")
		} else {
			for i, p := range d.Params {
				if i > 0 {
					b.WriteString(", ")
				}
				b.WriteString(p.Type)
				b.WriteByte(' ')
				b.WriteString(p.Name)
			}
		}
		b.WriteString(" returns ")
		b.WriteString(d.Returns)
		return b.String()
	default:
		return string(d.Kind) + " " + d.Name
	}
}

// --- lexer ---

type tokKind int

const (
	tEOF tokKind = iota
	tIdent
	tComma
	tEq
	tOther
)

type token struct {
	kind tokKind
	lit  string
	line int
}

func isIdentStart(r byte) bool {
	return r == '_' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z')
}

func isIdentPart(r byte) bool {
	return isIdentStart(r) || (r >= '0' && r <= '9')
}

// lex tokenizes JASS source. `//` line comments are dropped. Identifiers,
// commas, and `=` are distinguished; every other rune becomes tOther so the
// parser can skip constructs it does not model (operators, literals, etc.).
func lex(src string) []token {
	var toks []token
	line := 1
	for i := 0; i < len(src); {
		c := src[i]
		switch {
		case c == '\n':
			line++
			i++
		case c == ' ' || c == '\t' || c == '\r':
			i++
		case c == '/' && i+1 < len(src) && src[i+1] == '/':
			// line comment: skip to newline (newline handled next loop)
			for i < len(src) && src[i] != '\n' {
				i++
			}
		case c == ',':
			toks = append(toks, token{tComma, ",", line})
			i++
		case c == '=':
			toks = append(toks, token{tEq, "=", line})
			i++
		case isIdentStart(c):
			j := i + 1
			for j < len(src) && isIdentPart(src[j]) {
				j++
			}
			toks = append(toks, token{tIdent, src[i:j], line})
			i = j
		default:
			toks = append(toks, token{tOther, string(c), line})
			i++
		}
	}
	toks = append(toks, token{tEOF, "", line})
	return toks
}

// --- parser ---

type parser struct {
	toks []token
	pos  int
}

func (p *parser) peek() token    { return p.toks[p.pos] }
func (p *parser) peekN(n int) token {
	if p.pos+n < len(p.toks) {
		return p.toks[p.pos+n]
	}
	return p.toks[len(p.toks)-1]
}
func (p *parser) next() token {
	t := p.toks[p.pos]
	if p.pos < len(p.toks)-1 {
		p.pos++
	}
	return t
}
func (p *parser) atEOF() bool { return p.peek().kind == tEOF }

func (p *parser) isIdent(lit string) bool {
	t := p.peek()
	return t.kind == tIdent && t.lit == lit
}

// expectIdent consumes the next token, requiring it be an identifier. It
// returns the literal and ok=false on mismatch so callers fail closed rather
// than fabricate a declaration.
func (p *parser) expectIdent() (string, bool) {
	t := p.peek()
	if t.kind != tIdent {
		return "", false
	}
	p.next()
	return t.lit, true
}

// ParseDecls parses all top-level type and native declarations from src in
// source order. Unrecognized constructs (globals blocks, function bodies,
// operators, bare constants) are skipped, never silently turned into decls.
func ParseDecls(src string) []Decl {
	p := &parser{toks: lex(src)}
	var decls []Decl
	for !p.atEOF() {
		t := p.peek()
		if t.kind != tIdent {
			p.next()
			continue
		}
		switch t.lit {
		case "globals":
			p.skipUntil("endglobals")
		case "function":
			p.skipUntil("endfunction")
		case "type":
			if d, ok := p.parseType(); ok {
				decls = append(decls, d)
			}
		case "native":
			if d, ok := p.parseNative(false); ok {
				decls = append(decls, d)
			}
		case "constant":
			// `constant native ...` is a native; any other `constant`
			// (a typed global constant) is skipped token-by-token.
			if p.peekN(1).kind == tIdent && p.peekN(1).lit == "native" {
				p.next() // consume `constant`
				if d, ok := p.parseNative(true); ok {
					decls = append(decls, d)
				}
			} else {
				p.next()
			}
		default:
			p.next()
		}
	}
	return decls
}

// skipUntil consumes tokens through the next identifier equal to lit (inclusive).
func (p *parser) skipUntil(lit string) {
	for !p.atEOF() {
		t := p.next()
		if t.kind == tIdent && t.lit == lit {
			return
		}
	}
}

func (p *parser) parseType() (Decl, bool) {
	kw := p.next() // `type`
	name, ok := p.expectIdent()
	if !ok {
		return Decl{}, false
	}
	d := Decl{Kind: KindType, Name: name, Line: kw.line}
	if p.isIdent("extends") {
		p.next()
		if base, ok := p.expectIdent(); ok {
			d.Extends = base
		}
	}
	return d, true
}

func (p *parser) parseNative(constant bool) (Decl, bool) {
	kw := p.next() // `native`
	name, ok := p.expectIdent()
	if !ok {
		return Decl{}, false
	}
	d := Decl{Name: name, Line: kw.line, Constant: constant}
	if constant {
		d.Kind = KindConstantNative
	} else {
		d.Kind = KindNative
	}
	if !p.isIdent("takes") {
		return Decl{}, false
	}
	p.next() // `takes`
	params, ok := p.parseParams()
	if !ok {
		return Decl{}, false
	}
	d.Params = params
	if !p.isIdent("returns") {
		return Decl{}, false
	}
	p.next() // `returns`
	ret, ok := p.expectIdent()
	if !ok {
		return Decl{}, false
	}
	d.Returns = ret
	return d, true
}

// parseParams parses a `takes` list: either the single keyword `nothing`
// (empty param slice) or one-or-more `type name` pairs separated by commas.
func (p *parser) parseParams() ([]Param, bool) {
	if p.isIdent("nothing") {
		p.next()
		return nil, true
	}
	var params []Param
	for {
		typ, ok := p.expectIdent()
		if !ok {
			return nil, false
		}
		nm, ok := p.expectIdent()
		if !ok {
			return nil, false
		}
		params = append(params, Param{Type: typ, Name: nm})
		if p.peek().kind == tComma {
			p.next()
			continue
		}
		return params, true
	}
}

// Counts is a per-kind tally of parsed declarations.
type Counts struct {
	Types           int
	PlainNatives    int
	ConstantNatives int
}

// TotalNatives is plain + constant native declarations.
func (c Counts) TotalNatives() int { return c.PlainNatives + c.ConstantNatives }

// Tally computes per-kind counts over decls.
func Tally(decls []Decl) Counts {
	var c Counts
	for _, d := range decls {
		switch d.Kind {
		case KindType:
			c.Types++
		case KindNative:
			c.PlainNatives++
		case KindConstantNative:
			c.ConstantNatives++
		}
	}
	return c
}
