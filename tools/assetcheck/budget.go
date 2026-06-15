package main

// Triangle-budget gate (#31; tooling.md §3.2 "Triangle budget" row, §3.3;
// PRD R-RND-2). Units ≤ 1,500 triangles, buildings ≤ 4,000. The asset's
// category comes from its assets/MANIFEST entry — a model with geometry but
// no category is a finding, never a silent pass. An over-budget asset passes
// only with a reviewed waivers.toml entry (reason + expiry milestone); an
// expired waiver fails again. There is no CLI bypass flag.

import (
	"bufio"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

// triangleBudget maps a MANIFEST category to its triangle ceiling. A negative
// ceiling means "categorized but not subject to the unit/building budget"
// (terrain, props, fx) — still required to be categorized, just unbounded.
var triangleBudget = map[string]int{
	"unit":     1500,
	"building": 4000,
	"other":    -1,
}

// waiver is one reviewed exemption: a specific over-budget asset allowed to
// pass until the named milestone.
type waiver struct {
	Path   string
	Reason string
	Expiry string // milestone, e.g. "M3"
	Line   int
}

// waiverSet is the parsed waivers.toml: the project's current milestone plus
// the per-path exemptions.
type waiverSet struct {
	current     string
	haveCurrent bool
	byPath      map[string]waiver
}

func newWaiverSet() waiverSet { return waiverSet{byPath: map[string]waiver{}} }

// milestoneRank parses a milestone label "M<number>" into its ordinal. M0.5 is
// supported. ok is false for anything that is not a milestone.
func milestoneRank(m string) (float64, bool) {
	if len(m) < 2 || (m[0] != 'M' && m[0] != 'm') {
		return 0, false
	}
	f, err := strconv.ParseFloat(m[1:], 64)
	if err != nil {
		return 0, false
	}
	return f, true
}

// loadWaivers parses a waivers.toml (strict subset: comments, top-level
// current_milestone, and [[waiver]] tables). It fails closed: any malformed
// line, missing key, or duplicate path is an error, never a skipped waiver.
func loadWaivers(path string) (waiverSet, error) {
	ws := newWaiverSet()
	f, err := os.Open(path)
	if err != nil {
		return ws, err
	}
	defer f.Close()

	sc := bufio.NewScanner(f)
	var cur *waiver
	var curLine, n int

	flush := func() error {
		if cur == nil {
			return nil
		}
		if cur.Path == "" || cur.Reason == "" || cur.Expiry == "" {
			return fmt.Errorf("line %d: [[waiver]] requires path, reason, and expiry", curLine)
		}
		if prev, dup := ws.byPath[cur.Path]; dup {
			return fmt.Errorf("line %d: duplicate waiver for %q (first at line %d)", curLine, cur.Path, prev.Line)
		}
		ws.byPath[cur.Path] = *cur
		cur = nil
		return nil
	}

	for sc.Scan() {
		n++
		line := strings.TrimSpace(sc.Text())
		switch {
		case line == "" || strings.HasPrefix(line, "#"):
		case line == "[[waiver]]":
			if err := flush(); err != nil {
				return ws, err
			}
			cur = &waiver{Line: n}
			curLine = n
		default:
			k, v, ok := strings.Cut(line, "=")
			if !ok {
				return ws, fmt.Errorf("line %d: not a comment, [[waiver]], or key = \"value\": %q", n, line)
			}
			k = strings.TrimSpace(k)
			val, err := waiverString(strings.TrimSpace(v))
			if err != nil {
				return ws, fmt.Errorf("line %d: %v", n, err)
			}
			if cur == nil {
				if k != "current_milestone" {
					return ws, fmt.Errorf("line %d: unknown top-level key %q (want current_milestone)", n, k)
				}
				if ws.haveCurrent {
					return ws, fmt.Errorf("line %d: current_milestone set more than once", n)
				}
				ws.current, ws.haveCurrent = val, true
				continue
			}
			switch k {
			case "path":
				cur.Path = val
			case "reason":
				cur.Reason = val
			case "expiry":
				cur.Expiry = val
			default:
				return ws, fmt.Errorf("line %d: unknown waiver key %q", n, k)
			}
		}
	}
	if err := sc.Err(); err != nil {
		return ws, err
	}
	if err := flush(); err != nil {
		return ws, err
	}
	return ws, nil
}

// waiverString extracts a double-quoted value, tolerating a trailing comment.
func waiverString(v string) (string, error) {
	if len(v) < 2 || v[0] != '"' {
		return "", fmt.Errorf("value must be a double-quoted string: %q", v)
	}
	end := strings.IndexByte(v[1:], '"')
	if end < 0 {
		return "", fmt.Errorf("unterminated string: %q", v)
	}
	return v[1 : 1+end], nil
}

// checkBudget evaluates the triangle budget for every GLB with geometry.
// triangles maps file path → triangle count; assets is the MANIFEST keyed by
// path; ws is the waiver ledger. It returns findings (budget failures) and
// notes (waivers that were applied, for the run log). Both are deterministic.
func checkBudget(triangles map[string]int, assets map[string]manifest.Asset, ws waiverSet) (findings []finding, notes []string) {
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }

	paths := make([]string, 0, len(triangles))
	for p := range triangles {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	for _, rel := range paths {
		t := triangles[rel]
		if t <= 0 {
			continue // no triangle geometry — nothing to budget
		}
		cat := assets[rel].Category
		if cat == "" {
			add(rel, "BUDGET-UNCATEGORIZED", fmt.Sprintf("model has %d triangles but its MANIFEST entry has no category; add category = \"unit\"|\"building\"|\"other\"", t))
			continue
		}
		budget, known := triangleBudget[cat]
		if !known {
			add(rel, "BUDGET-CATEGORY", fmt.Sprintf("unknown MANIFEST category %q; allowed: unit, building, other", cat))
			continue
		}
		if budget < 0 || t <= budget {
			continue // within budget (boundary inclusive) or unbounded category
		}

		w, waived := ws.byPath[rel]
		if !waived {
			add(rel, "BUDGET-OVER", fmt.Sprintf("%d triangles exceeds %s budget of %d (R-RND-2)", t, cat, budget))
			continue
		}
		if !ws.haveCurrent {
			add(rel, "BUDGET-WAIVER", "waiver present but waivers.toml has no current_milestone; cannot evaluate expiry")
			continue
		}
		expRank, ok := milestoneRank(w.Expiry)
		if !ok {
			add(rel, "BUDGET-WAIVER", fmt.Sprintf("waiver expiry %q is not a milestone (want M<number>)", w.Expiry))
			continue
		}
		curRank, ok := milestoneRank(ws.current)
		if !ok {
			add(rel, "BUDGET-WAIVER", fmt.Sprintf("current_milestone %q is not a milestone (want M<number>)", ws.current))
			continue
		}
		if curRank > expRank {
			add(rel, "BUDGET-OVER", fmt.Sprintf("%d triangles exceeds %s budget of %d; waiver expired at %s (current %s) (R-RND-2)", t, cat, budget, w.Expiry, ws.current))
			continue
		}
		notes = append(notes, fmt.Sprintf("WAIVED %s: %d tris over %s budget %d until %s (current %s) — %s", rel, t, cat, budget, w.Expiry, ws.current, w.Reason))
	}
	return findings, notes
}
