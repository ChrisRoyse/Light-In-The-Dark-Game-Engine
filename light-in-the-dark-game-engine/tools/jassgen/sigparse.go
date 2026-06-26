package main

// sigparse.go parses the manifest's goSignature strings into structured
// parameter/return lists — the foundation the -luabind dispatch generator
// (#267) needs to emit per-argument marshaling. A goSignature has the shape
// "(<params>) <returns>"; the receiver (for a method like Unit.Paused) is NOT
// in the string — it is encoded in the canonical Symbol and supplied separately.
//
// The grammar is real Go: grouped params share a trailing type ("min, max int"),
// the last param may be variadic ("opts ...UseOption"), types may contain
// commas/spaces inside brackets or func types ("cb func(a int)", "*Table[V]"),
// and returns may be empty, a single type, or a parenthesized tuple
// ("(V, bool)"). Splitting is therefore bracket-depth aware, never a naive
// strings.Split — that is exactly what made the first survey mis-tokenize.
//
// The parser only extracts STRUCTURE; deciding which type tokens are
// Lua-marshalable (and failing closed on the ones that are not — generics,
// callbacks, unenriched "(...)") is the dispatch generator's job, not this one's.

import (
	"fmt"
	"strings"
)

// goParam is one parsed parameter: its name, its Go type (with any leading
// "..." stripped), and whether it was variadic.
type goParam struct {
	Name     string
	Type     string
	Variadic bool
}

// goSig is a parsed goSignature: an optional leading type-parameter list (for
// generic functions like "[V any]() *Table[V]"), ordered params, and ordered
// return types. A non-nil TypeParams marks a generic the dispatch generator
// cannot bind directly (no instantiation at the Lua boundary) — it parses, then
// the generator skips it fail-closed.
type goSig struct {
	TypeParams []goParam
	Params     []goParam
	Returns    []string
}

// parseGoSignature parses a manifest goSignature into a goSig. It returns an
// error (fail-closed) for the unenriched placeholder "(...)", an unbalanced
// signature, or params that end with untyped names.
func parseGoSignature(sig string) (goSig, error) {
	sig = strings.TrimSpace(sig)
	if sig == "(...)" {
		return goSig{}, fmt.Errorf("goSignature is the unenriched placeholder %q", sig)
	}
	// Optional leading type-parameter list for a generic function, e.g.
	// "[V any]() *Table[V]". Strip and parse it before the value params.
	var typeParams []goParam
	if strings.HasPrefix(sig, "[") {
		end := matchBracket(sig)
		if end < 0 {
			return goSig{}, fmt.Errorf("goSignature %q has an unclosed type-parameter list", sig)
		}
		tp, err := parseParams(sig[1:end])
		if err != nil {
			return goSig{}, fmt.Errorf("type parameters: %w", err)
		}
		typeParams = tp
		sig = strings.TrimSpace(sig[end+1:])
	}
	if !strings.HasPrefix(sig, "(") {
		return goSig{}, fmt.Errorf("goSignature %q does not start with '('", sig)
	}
	// Find the ')' that closes the parameter list (bracket-depth aware).
	depth, closeIdx := 0, -1
	for i := 0; i < len(sig); i++ {
		switch sig[i] {
		case '(', '[':
			depth++
		case ')', ']':
			depth--
			if sig[i] == ')' && depth == 0 {
				closeIdx = i
			}
		}
		if closeIdx >= 0 {
			break
		}
	}
	if closeIdx < 0 {
		return goSig{}, fmt.Errorf("goSignature %q has unbalanced parentheses", sig)
	}

	params, err := parseParams(sig[1:closeIdx])
	if err != nil {
		return goSig{}, err
	}
	returns, err := parseReturns(strings.TrimSpace(sig[closeIdx+1:]))
	if err != nil {
		return goSig{}, err
	}
	return goSig{TypeParams: typeParams, Params: params, Returns: returns}, nil
}

// matchBracket returns the index of the ']' that closes the '[' at s[0]
// (bracket-depth aware), or -1 if unclosed. Assumes s[0]=='['.
func matchBracket(s string) int {
	depth := 0
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '[':
			depth++
		case ']':
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}

// parseParams splits a parameter list on top-level commas and resolves grouped
// params ("a, b int" -> a int, b int) and a trailing variadic.
func parseParams(s string) ([]goParam, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return nil, nil
	}
	toks := splitTopLevel(s, ',')
	var out []goParam
	var pending []string // names awaiting the shared trailing type
	for _, tk := range toks {
		tk = strings.TrimSpace(tk)
		if tk == "" {
			continue
		}
		sp := indexTopLevelSpace(tk)
		if sp < 0 {
			// A bare name (grouped param) — its type is the next typed token's.
			pending = append(pending, tk)
			continue
		}
		name := strings.TrimSpace(tk[:sp])
		typ := strings.TrimSpace(tk[sp+1:])
		variadic := false
		if strings.HasPrefix(typ, "...") {
			variadic = true
			typ = strings.TrimSpace(typ[3:])
		}
		for _, pn := range pending {
			out = append(out, goParam{Name: pn, Type: typ})
		}
		pending = nil
		out = append(out, goParam{Name: name, Type: typ, Variadic: variadic})
	}
	if len(pending) > 0 {
		return nil, fmt.Errorf("parameter list %q ends with untyped names %v", s, pending)
	}
	return out, nil
}

// parseReturns parses the return portion: empty, a single type, or a "(...)"
// tuple split on top-level commas.
func parseReturns(s string) ([]string, error) {
	if s == "" {
		return nil, nil
	}
	if strings.HasPrefix(s, "(") {
		if !strings.HasSuffix(s, ")") {
			return nil, fmt.Errorf("return tuple %q is not closed", s)
		}
		inner := s[1 : len(s)-1]
		var rets []string
		for _, t := range splitTopLevel(inner, ',') {
			t = strings.TrimSpace(t)
			if t != "" {
				rets = append(rets, t)
			}
		}
		return rets, nil
	}
	return []string{s}, nil
}

// (splitTopLevel is shared with dtsparse.go — bracket-depth-aware comma split.)

// indexTopLevelSpace returns the index of the first space not nested inside ()
// or [], or -1 if none — used to split a param token into name and type without
// being fooled by spaces inside "func(a int)".
func indexTopLevelSpace(s string) int {
	depth := 0
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '(', '[':
			depth++
		case ')', ']':
			depth--
		case ' ':
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}
