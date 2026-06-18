package worldarchive

import "strings"

// Engine-version range support, mirroring the worldpack/assetcheck grammar: a
// space-separated list of comparators "(>=|<=|>|<|=)?MAJOR.MINOR.PATCH", or "*".

// validEngineRange reports whether r is a well-formed range.
func validEngineRange(r string) bool {
	r = strings.TrimSpace(r)
	if r == "" {
		return false
	}
	if r == "*" {
		return true
	}
	for _, tok := range strings.Fields(r) {
		if _, ok := parseComparator(tok); !ok {
			return false
		}
	}
	return true
}

// satisfiesRange reports whether semver v satisfies every comparator in r.
func satisfiesRange(v, r string) bool {
	r = strings.TrimSpace(r)
	if r == "*" {
		return true
	}
	ver, ok := parseSemver(v)
	if !ok {
		return false
	}
	for _, tok := range strings.Fields(r) {
		c, ok := parseComparator(tok)
		if !ok {
			return false
		}
		cmp := cmpSemver(ver, c.bound)
		switch c.op {
		case ">=":
			if cmp < 0 {
				return false
			}
		case "<=":
			if cmp > 0 {
				return false
			}
		case ">":
			if cmp <= 0 {
				return false
			}
		case "<":
			if cmp >= 0 {
				return false
			}
		case "=":
			if cmp != 0 {
				return false
			}
		}
	}
	return true
}

type comparator struct {
	op    string
	bound [3]int
}

func parseComparator(tok string) (comparator, bool) {
	op := "="
	rest := tok
	for _, o := range []string{">=", "<=", ">", "<", "="} {
		if strings.HasPrefix(tok, o) {
			op, rest = o, tok[len(o):]
			break
		}
	}
	b, ok := parseSemver(rest)
	if !ok {
		return comparator{}, false
	}
	return comparator{op: op, bound: b}, true
}

func parseSemver(v string) ([3]int, bool) {
	var out [3]int
	parts := strings.Split(v, ".")
	if len(parts) != 3 {
		return out, false
	}
	for i, p := range parts {
		if p == "" {
			return out, false
		}
		n := 0
		for _, ch := range p {
			if ch < '0' || ch > '9' {
				return out, false
			}
			n = n*10 + int(ch-'0')
		}
		out[i] = n
	}
	return out, true
}

func cmpSemver(a, b [3]int) int {
	for i := 0; i < 3; i++ {
		switch {
		case a[i] < b[i]:
			return -1
		case a[i] > b[i]:
			return 1
		}
	}
	return 0
}
