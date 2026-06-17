package main

import (
	"strings"
	"testing"
)

// TestGenerateGroupingFSV pins the renderer against a known commit set — known
// input, known output. SoT = the produced markdown.
func TestGenerateGroupingFSV(t *testing.T) {
	subjects := []string{
		"feat(luabind): runtime world loader",
		"fix(sim): correct tick rounding",
		"feat(sim)!: change PRNG call order",
		"docs: update README",
		"Merge branch 'topic' into main", // non-conventional
		"perf(render): frustum cull chunks",
		"random brain-dump commit", // non-conventional
	}
	out := Generate("v0.2.0", subjects)
	t.Logf("FSV changelog:\n%s", out)

	must := func(sub string) {
		if !strings.Contains(out, sub) {
			t.Errorf("changelog missing %q\n---\n%s", sub, out)
		}
	}
	must("# v0.2.0")
	// Sim callout lists the sim-scoped fix AND the breaking sim feat.
	must("## Sim (replays/saves invalidated")
	must("- **sim:** correct tick rounding")
	must("- **sim:** change PRNG call order")
	// Type sections.
	must("## Features")
	must("- **luabind:** runtime world loader")
	must("## Performance")
	must("- **render:** frustum cull chunks")
	must("## Documentation")
	must("- update README")
	// Non-conventional subjects are listed verbatim, never dropped.
	must("## Uncategorized")
	must("- Merge branch 'topic' into main")
	must("- random brain-dump commit")

	// Ordering: Sim section precedes Features (callout first).
	if strings.Index(out, "## Sim") > strings.Index(out, "## Features") {
		t.Error("Sim section must precede Features")
	}
}

// TestGenerateEmptyAndWhitespace: blank subjects are skipped; an all-blank set
// yields just the heading (no empty sections).
func TestGenerateEmptyAndWhitespace(t *testing.T) {
	out := Generate("v0.0.1", []string{"", "   ", "\t"})
	if strings.TrimSpace(out) != "# v0.0.1" {
		t.Errorf("empty input should yield just the heading, got:\n%q", out)
	}
}

// TestParseConventional checks the subject parser on representative forms.
func TestParseConventional(t *testing.T) {
	cases := []struct {
		in              string
		typ, scope      string
		breaking        bool
		nonConventional bool
	}{
		{"feat(api): add verb", "feat", "api", false, false},
		{"fix: bug", "fix", "", false, false},
		{"feat(sim)!: breaking", "feat", "sim", true, false},
		{"chore(deps)!: bump", "chore", "deps", true, false},
		{"not a conventional commit", "", "", false, true},
		{"WIP", "", "", false, true},
	}
	for _, c := range cases {
		got := parse(c.in)
		if c.nonConventional {
			if got.typ != "" {
				t.Errorf("parse(%q): expected non-conventional, got type %q", c.in, got.typ)
			}
			continue
		}
		if got.typ != c.typ || got.scope != c.scope || got.breaking != c.breaking {
			t.Errorf("parse(%q) = {typ:%q scope:%q breaking:%v}, want {%q %q %v}",
				c.in, got.typ, got.scope, got.breaking, c.typ, c.scope, c.breaking)
		}
	}
}
