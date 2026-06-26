package main

// changelog.go — the pure changelog renderer (#184, docs/release/versioning.md
// "Changelog"). Generate turns a list of commit subjects into a grouped
// markdown draft: conventional commits group by type; sim-semantics changes
// (scope "sim" or a breaking "!") are additionally called out under a mandatory
// Sim section (replays/saves invalidated); non-conventional subjects land under
// Uncategorized — listed, never dropped. The draft is an input to hand-editing,
// not the published artifact.

import (
	"fmt"
	"regexp"
	"strings"
)

// conventional-commit subject: type(scope)!: description (scope and ! optional).
var ccRe = regexp.MustCompile(`^(\w+)(?:\(([^)]*)\))?(!)?: (.+)$`)

type commit struct {
	typ, scope, desc string
	breaking         bool
	raw              string
}

func parse(subject string) commit {
	m := ccRe.FindStringSubmatch(subject)
	if m == nil {
		return commit{raw: subject}
	}
	return commit{typ: m[1], scope: m[2], breaking: m[3] == "!", desc: m[4], raw: subject}
}

// typeHeadings is the section order + display name for each conventional type.
var typeHeadings = []struct{ typ, heading string }{
	{"feat", "Features"},
	{"fix", "Fixes"},
	{"perf", "Performance"},
	{"refactor", "Refactors"},
	{"docs", "Documentation"},
	{"test", "Tests"},
	{"build", "Build"},
	{"ci", "CI"},
	{"chore", "Chores"},
	{"style", "Style"},
	{"revert", "Reverts"},
}

// Generate renders the changelog draft for version from commit subjects (in the
// order given — typically newest first). See the file comment for grouping
// rules.
func Generate(version string, subjects []string) string {
	byType := map[string][]commit{}
	var sim, uncategorized []commit
	for _, s := range subjects {
		s = strings.TrimSpace(s)
		if s == "" {
			continue
		}
		c := parse(s)
		if c.typ == "" {
			uncategorized = append(uncategorized, c)
			continue
		}
		byType[c.typ] = append(byType[c.typ], c)
		if c.scope == "sim" || c.breaking {
			sim = append(sim, c)
		}
	}

	var b strings.Builder
	fmt.Fprintf(&b, "# %s\n", version)
	line := func(c commit) {
		if c.scope != "" {
			fmt.Fprintf(&b, "- **%s:** %s\n", c.scope, c.desc)
		} else {
			fmt.Fprintf(&b, "- %s\n", c.desc)
		}
	}

	if len(sim) > 0 {
		b.WriteString("\n## Sim (replays/saves invalidated — MINOR bump required)\n")
		for _, c := range sim {
			line(c)
		}
	}
	for _, th := range typeHeadings {
		cs := byType[th.typ]
		if len(cs) == 0 {
			continue
		}
		fmt.Fprintf(&b, "\n## %s\n", th.heading)
		for _, c := range cs {
			line(c)
		}
	}
	if len(uncategorized) > 0 {
		b.WriteString("\n## Uncategorized\n")
		for _, c := range uncategorized {
			fmt.Fprintf(&b, "- %s\n", c.raw)
		}
	}
	return b.String()
}
