// Package semver is the SINGLE engine-version range matcher shared by every
// part of the system that decides whether a given engine build may run a given
// world archive (#180): the archive verifier (litd/asset/worldarchive), the
// assetcheck archive linter (tools/assetcheck), the hub index/intake, and the
// client loader. Having exactly one parser is the point — two implementations
// could drift and make contradictory compatibility claims (D-2026-06-11-14;
// "no two parsers" constraint). The grammar is the one documented in
// docs/specs/world-archive-v1.md.
//
// Grammar: a range is a space-separated list of comparators, each
// "(>=|<=|>|<|=)?MAJOR.MINOR.PATCH" (a bare version means "="), or the wildcard
// "*". A version satisfies the range iff it satisfies EVERY comparator.
// Matching is exact and total: every (version, range) pair is compatible or
// incompatible — never "unknown". Prerelease/build metadata is not part of the
// grammar; versions are three non-negative integers.
package semver

import (
	"strconv"
	"strings"
)

// ops are matched longest-first so ">=" / "<=" win over ">" / "<".
var ops = []string{">=", "<=", ">", "<", "="}

// ValidRange reports whether r is a well-formed range (the form intake accepts).
// A malformed range is rejected here, at intake/lint time — never silently at
// play time.
func ValidRange(r string) bool {
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

// Satisfies reports whether engine version v (MAJOR.MINOR.PATCH) satisfies every
// comparator in range r. "*" admits all versions. A malformed v never satisfies
// a concrete range (fail-closed). r need not be pre-validated — any malformed
// comparator yields false.
func Satisfies(v, r string) bool {
	r = strings.TrimSpace(r)
	if r == "*" {
		return true
	}
	ver, ok := parse(v)
	if !ok {
		return false
	}
	for _, tok := range strings.Fields(r) {
		c, ok := parseComparator(tok)
		if !ok {
			return false
		}
		cmp := compare(ver, c.bound)
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
	for _, o := range ops {
		if strings.HasPrefix(tok, o) {
			op, rest = o, tok[len(o):]
			break
		}
	}
	b, ok := parse(rest)
	if !ok {
		return comparator{}, false
	}
	return comparator{op: op, bound: b}, true
}

// parse reads MAJOR.MINOR.PATCH into a comparable triple of non-negative ints.
func parse(v string) ([3]int, bool) {
	var out [3]int
	parts := strings.Split(v, ".")
	if len(parts) != 3 {
		return out, false
	}
	for i, p := range parts {
		if p == "" {
			return out, false
		}
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 {
			return out, false
		}
		out[i] = n
	}
	return out, true
}

// compare returns -1/0/+1 comparing a to b component-wise (major, minor, patch).
func compare(a, b [3]int) int {
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
