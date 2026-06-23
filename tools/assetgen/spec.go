// spec.go parses assetgen.toml — the build-time generation spec (#56;
// tooling.md §5.1). Each [[gen]] table fully specifies one asset to generate:
// its budget category, the generator/model, the prompt + parameters, the output
// path under assets/, and optional output constraints (dimensions, style tags,
// atlas target). The format is the same strict-TOML subset assets/MANIFEST uses
// (comment lines, [[gen]] headers, key = "string" pairs); anything else, or a
// missing required key, is a parse error — the spec fails closed rather than
// generating from an under-specified entry.
package main

import (
	"bufio"
	"fmt"
	"io"
	"strings"
)

// SpecItem is one [[gen]] entry.
type SpecItem struct {
	Category    string // required: triangle-budget / asset category
	Generator   string // required: generating model/tool + version
	Prompt      string // required: generation prompt
	Params      string // optional: extra generation parameters
	Output      string // required: output path relative to assets/, forward slashes
	Constraints string // optional: output constraints (e.g. "64x64;unlit;atlas=ui")
	Line        int    // line of the [[gen]] header, for diagnostics
}

var specRequired = []string{"category", "generator", "prompt", "output"}
var specOptional = []string{"params", "constraints"}

// ParseSpec reads the assetgen.toml format, returning one SpecItem per [[gen]]
// table. It errors on the first malformed line, unknown/duplicate key, missing
// required key, or duplicate output path.
func ParseSpec(r io.Reader) ([]SpecItem, error) {
	sc := bufio.NewScanner(r)
	var items []SpecItem
	var cur map[string]string
	var curLine int
	seenOut := make(map[string]int)

	flush := func() error {
		if cur == nil {
			return nil
		}
		for _, k := range specRequired {
			if strings.TrimSpace(cur[k]) == "" {
				return fmt.Errorf("line %d: [[gen]] missing required key %q", curLine, k)
			}
		}
		out := cur["output"]
		if strings.Contains(out, "\\") || strings.HasPrefix(out, "/") || strings.Contains(out, "..") {
			return fmt.Errorf("line %d: output %q must be relative with forward slashes", curLine, out)
		}
		if prev, dup := seenOut[out]; dup {
			return fmt.Errorf("line %d: duplicate output path %q (first at line %d)", curLine, out, prev)
		}
		seenOut[out] = curLine
		items = append(items, SpecItem{
			Category: cur["category"], Generator: cur["generator"], Prompt: cur["prompt"],
			Params: cur["params"], Output: out, Constraints: cur["constraints"], Line: curLine,
		})
		return nil
	}

	n := 0
	for sc.Scan() {
		n++
		line := strings.TrimSpace(sc.Text())
		switch {
		case line == "" || strings.HasPrefix(line, "#"):
		case line == "[[gen]]":
			if err := flush(); err != nil {
				return nil, err
			}
			cur, curLine = make(map[string]string), n
		default:
			k, v, ok := strings.Cut(line, "=")
			if !ok {
				return nil, fmt.Errorf("line %d: not a comment, [[gen]], or key = \"value\": %q", n, line)
			}
			if cur == nil {
				return nil, fmt.Errorf("line %d: key outside any [[gen]] table", n)
			}
			k = strings.TrimSpace(k)
			v = strings.TrimSpace(v)
			if len(v) < 2 || v[0] != '"' || v[len(v)-1] != '"' {
				return nil, fmt.Errorf("line %d: value for %q must be a double-quoted string", n, k)
			}
			v = v[1 : len(v)-1]
			if !specKeyValid(k) {
				return nil, fmt.Errorf("line %d: unknown key %q", n, k)
			}
			if _, dup := cur[k]; dup {
				return nil, fmt.Errorf("line %d: duplicate key %q in entry at line %d", n, k, curLine)
			}
			cur[k] = v
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	if err := flush(); err != nil {
		return nil, err
	}
	return items, nil
}

func specKeyValid(k string) bool {
	for _, rk := range specRequired {
		if k == rk {
			return true
		}
	}
	for _, ok := range specOptional {
		if k == ok {
			return true
		}
	}
	return false
}
