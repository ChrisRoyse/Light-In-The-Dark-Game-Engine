package main

// dtsparse.go parses the mechanical TypeScript *declaration* subset emitted in
// war3-types core/*.d.ts: `declare [const] function Name(p: T, ...): R;` and
// `declare interface Name extends Base { ... }`. There is no general TypeScript
// compiler (tooling.md §2.2 step 2) — the files are line-oriented and regular.
// Output is used to enrich the .j entries with tsType, nullability, and the
// handle subtype hierarchy.

import "strings"

// DTSParam is one TypeScript parameter: its name, raw type text, and whether
// the type union admits null (or the param is optional).
type DTSParam struct {
	Name     string
	TSType   string
	Nullable bool
}

// DTSDecl is a parsed .d.ts declaration (function or interface).
type DTSDecl struct {
	Kind    string // "function" | "interface"
	Name    string
	Params  []DTSParam
	Returns string // function return type text
	Extends string // interface base type
	Line    int
}

// ParseDTS parses all `declare function` and `declare interface` lines from a
// .d.ts source, in source order. Lines that are neither are skipped.
func ParseDTS(src string) []DTSDecl {
	var out []DTSDecl
	for i, raw := range strings.Split(src, "\n") {
		line := strings.TrimSpace(raw)
		if !strings.HasPrefix(line, "declare ") {
			continue
		}
		rest := strings.TrimPrefix(line, "declare ")
		switch {
		case strings.HasPrefix(rest, "interface "):
			if d, ok := parseDTSInterface(rest, i+1); ok {
				out = append(out, d)
			}
		case strings.HasPrefix(rest, "function "), strings.HasPrefix(rest, "const function "):
			rest = strings.TrimPrefix(rest, "const ")
			if d, ok := parseDTSFunction(rest, i+1); ok {
				out = append(out, d)
			}
		}
	}
	return out
}

func parseDTSInterface(rest string, line int) (DTSDecl, bool) {
	// rest = "interface NAME [extends BASE] { ... }"
	rest = strings.TrimPrefix(rest, "interface ")
	brace := strings.IndexByte(rest, '{')
	head := rest
	if brace >= 0 {
		head = rest[:brace]
	}
	head = strings.TrimSpace(head)
	d := DTSDecl{Kind: "interface", Line: line}
	if idx := strings.Index(head, " extends "); idx >= 0 {
		d.Name = strings.TrimSpace(head[:idx])
		d.Extends = strings.TrimSpace(head[idx+len(" extends "):])
	} else {
		d.Name = head
	}
	if d.Name == "" {
		return DTSDecl{}, false
	}
	return d, true
}

func parseDTSFunction(rest string, line int) (DTSDecl, bool) {
	// rest = "function NAME(PARAMS): RET;"
	rest = strings.TrimPrefix(rest, "function ")
	open := strings.IndexByte(rest, '(')
	if open < 0 {
		return DTSDecl{}, false
	}
	name := strings.TrimSpace(rest[:open])
	close := matchParen(rest, open)
	if close < 0 {
		return DTSDecl{}, false
	}
	paramStr := rest[open+1 : close]
	d := DTSDecl{Kind: "function", Name: name, Line: line}
	d.Params = parseDTSParams(paramStr)

	// after ')': expect ": RET ;"
	tail := strings.TrimSpace(rest[close+1:])
	tail = strings.TrimSuffix(tail, ";")
	tail = strings.TrimSpace(tail)
	if strings.HasPrefix(tail, ":") {
		d.Returns = strings.TrimSpace(strings.TrimPrefix(tail, ":"))
	}
	return d, true
}

// matchParen returns the index of the ')' that matches the '(' at open,
// accounting for nested () [] {}; -1 if unbalanced. Angle brackets are NOT
// tracked: TypeScript arrow types (`() => void`) put a '>' in '=>' that has no
// matching '<', so treating '<>' as a bracket pair corrupts the depth count.
func matchParen(s string, open int) int {
	depth := 0
	for i := open; i < len(s); i++ {
		switch s[i] {
		case '(', '[', '{':
			depth++
		case ')', ']', '}':
			depth--
			if depth == 0 && s[i] == ')' {
				return i
			}
		}
	}
	return -1
}

// parseDTSParams splits a parameter list on top-level commas (ignoring commas
// nested inside ()<>[]{}), then parses each `name[?]: type`.
func parseDTSParams(s string) []DTSParam {
	s = strings.TrimSpace(s)
	if s == "" {
		return nil
	}
	var params []DTSParam
	for _, raw := range splitTopLevel(s, ',') {
		raw = strings.TrimSpace(raw)
		if raw == "" {
			continue
		}
		colon := strings.IndexByte(raw, ':')
		if colon < 0 {
			params = append(params, DTSParam{Name: raw})
			continue
		}
		name := strings.TrimSpace(raw[:colon])
		typ := strings.TrimSpace(raw[colon+1:])
		optional := strings.HasSuffix(name, "?")
		name = strings.TrimSuffix(name, "?")
		params = append(params, DTSParam{
			Name:     name,
			TSType:   typ,
			Nullable: optional || typeIsNullable(typ),
		})
	}
	return params
}

// typeIsNullable reports whether a union type text admits null/undefined.
func typeIsNullable(t string) bool {
	for _, alt := range splitTopLevel(t, '|') {
		a := strings.TrimSpace(alt)
		if a == "null" || a == "undefined" {
			return true
		}
	}
	return false
}

// splitTopLevel splits s on sep at bracket-depth 0. Angle brackets are not
// tracked (see matchParen) so arrow types do not skew the depth.
func splitTopLevel(s string, sep byte) []string {
	var parts []string
	depth := 0
	start := 0
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '(', '[', '{':
			depth++
		case ')', ']', '}':
			depth--
		case sep:
			if depth == 0 {
				parts = append(parts, s[start:i])
				start = i + 1
			}
		}
	}
	parts = append(parts, s[start:])
	return parts
}

// DTSHierarchy builds the handle subtype map (type -> immediate base) from
// parsed interface declarations.
func DTSHierarchy(decls []DTSDecl) map[string]string {
	h := map[string]string{}
	for _, d := range decls {
		if d.Kind == "interface" && d.Extends != "" {
			h[d.Name] = d.Extends
		}
	}
	return h
}
