// Package lualint is the static Lua sandbox-safety lint (#37; M6 D-20), shared
// by the archive validator (tools/assetcheck) and the in-engine archive read
// path (litd/asset/worldarchive) so both enforce the SAME rule with one
// implementation. A minimal Lua lexer walks the source skipping comments and
// string literals, then flags any reference to a sandbox-forbidden global
// (io, os, net) or a code-loading function (require, loadfile, dofile). Because
// strings and comments are skipped, a literal like "ghost" (containing "os") is
// never a false positive, and a field access like t.os (preceded by '.'/':')
// is not the global os.
package lualint

import (
	"fmt"
	"sort"
)

var forbidden = map[string]string{
	"io":       "sandbox-forbidden global",
	"os":       "sandbox-forbidden global",
	"net":      "sandbox-forbidden global",
	"require":  "code-loading function",
	"loadfile": "code-loading function",
	"dofile":   "code-loading function",
}

// SandboxLint returns one message per forbidden reference, sorted by line. An
// empty slice means the source is sandbox-safe by this static check.
func SandboxLint(src []byte) []string {
	type hit struct {
		line int
		msg  string
	}
	var hits []hit
	line := 1
	i, n := 0, len(src)

	isIdentStart := func(c byte) bool {
		return c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
	}
	isIdent := func(c byte) bool {
		return isIdentStart(c) || (c >= '0' && c <= '9')
	}
	// longBracketLevel: at src[i]=='[', returns the '=' level if this opens a
	// long bracket ([[ , [=[ , ...), else -1.
	longBracketLevel := func(p int) int {
		if p >= n || src[p] != '[' {
			return -1
		}
		q := p + 1
		for q < n && src[q] == '=' {
			q++
		}
		if q < n && src[q] == '[' {
			return q - (p + 1)
		}
		return -1
	}
	// skipLongBracket advances past a long bracket body of the given level,
	// starting just after the opening bracket; updates line; returns new index.
	skipLongBracket := func(p, level int) int {
		for p < n {
			if src[p] == ']' {
				q := p + 1
				eqs := 0
				for q < n && src[q] == '=' {
					q++
					eqs++
				}
				if eqs == level && q < n && src[q] == ']' {
					return q + 1
				}
			}
			if src[p] == '\n' {
				line++
			}
			p++
		}
		return p
	}

	for i < n {
		c := src[i]
		switch {
		case c == '\n':
			line++
			i++
		case c == '-' && i+1 < n && src[i+1] == '-': // comment
			if lvl := longBracketLevel(i + 2); lvl >= 0 {
				i = skipLongBracket(i+3+lvl, lvl) // skip --[=*[ ... ]=*]
			} else {
				for i < n && src[i] != '\n' {
					i++
				}
			}
		case c == '"' || c == '\'': // short string
			q := c
			i++
			for i < n && src[i] != q {
				if src[i] == '\\' && i+1 < n {
					if src[i+1] == '\n' {
						line++
					}
					i += 2
					continue
				}
				if src[i] == '\n' {
					line++
				}
				i++
			}
			i++ // closing quote
		case c == '[' && longBracketLevel(i) >= 0: // long string
			lvl := longBracketLevel(i)
			i = skipLongBracket(i+2+lvl, lvl)
		case isIdentStart(c): // identifier
			start := i
			for i < n && isIdent(src[i]) {
				i++
			}
			word := string(src[start:i])
			if reason, bad := forbidden[word]; bad && !precededByField(src, start) {
				hits = append(hits, hit{line, fmt.Sprintf("%q (%s) referenced at line %d", word, reason, line)})
			}
		default:
			i++
		}
	}

	sort.Slice(hits, func(a, b int) bool { return hits[a].line < hits[b].line })
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		out = append(out, h.msg)
	}
	return out
}

// precededByField reports whether the identifier at start is a field access
// (immediately preceded, ignoring spaces/tabs, by '.' or ':').
func precededByField(src []byte, start int) bool {
	j := start - 1
	for j >= 0 && (src[j] == ' ' || src[j] == '\t') {
		j--
	}
	return j >= 0 && (src[j] == '.' || src[j] == ':')
}
