package main

// audit.go produces the dedup audit report (deduplication-policy.md §8,
// tooling.md §2.3): a machine-readable audit-report.json of CI-gate counters
// plus a human audit-report.md. It enforces the M2 invariants — totality (one
// record per source function), uniqueness (mapped XOR tombstoned), and the
// unclassified==0 / unmapped==0 gates — and scaffolds the M5 gates
// (duplicateTargets, helperShadowsCore, reverse-closure).
//
// The report tells the truth about the current state: until overrides map or
// tombstone every symbol, the unmapped/unclassified gates are RED and -audit
// exits nonzero. That is the M2 backlog, not a tooling defect.

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

// AuditReport is the machine-readable counter block; each field is a CI gate or
// census signal.
type AuditReport struct {
	Total                        int            `json:"total"`        // all source functions
	TotalCore                    int            `json:"totalCore"`    // common.j + blizzard.j
	ByRule                       map[string]int `json:"byRule"`       // D1..D5 over all classifications
	Mapped                       int            `json:"mapped"`       // disposition=mapped
	Tombstoned                   int            `json:"tombstoned"`   // disposition=tombstoned
	Collapsed                    int            `json:"collapsed"`    // D3 members resolved via a canonical entry's collapsesWith
	Unclassified                 int            `json:"unclassified"` // M2 gate: must be 0
	Unmapped                     int            `json:"unmapped"`     // M2 gate: must be 0
	TombstonesByReason           map[string]int `json:"tombstonesByReason"`
	DuplicateTargets             []string       `json:"duplicateTargets"`             // M5 gate (emitted now)
	HelperShadowsCore            []string       `json:"helperShadowsCore"`            // M5 gate (scaffold)
	CommonaiCapabilityTombstones int            `json:"commonaiCapabilityTombstones"` // must be 0 (D-2026-06-11-6)
	FeatureTags                  map[string]int `json:"featureTags"`                  // R4 census rollup
	Violations                   []string       `json:"violations"`                   // M2 gate breaches
}

// ComputeAudit derives the counter block from the full classification universe
// (all source symbols) and the emitted manifest (resolved entries only).
func ComputeAudit(cs []Classification, m Manifest) AuditReport {
	r := AuditReport{
		ByRule:             map[string]int{},
		TombstonesByReason: map[string]int{},
		FeatureTags:        map[string]int{},
		DuplicateTargets:   []string{},
		HelperShadowsCore:  []string{},
	}
	r.Total = len(cs)
	for _, c := range cs {
		if c.Origin == "common" || c.Origin == "blizzard" {
			r.TotalCore++
		}
		if validClass(string(c.Class)) {
			r.ByRule[string(c.Class)]++
		} else {
			r.Unclassified++
		}
	}

	// Disposition + uniqueness from the manifest (validated, mapped XOR tombstone).
	symbolUse := map[string]int{}
	for _, f := range m.Functions {
		switch f.Disposition {
		case "mapped":
			r.Mapped++
			if f.GoMapping != nil {
				symbolUse[f.GoMapping.Symbol]++
			}
		case "tombstoned":
			r.Tombstoned++
			if f.Tombstone != nil {
				r.TombstonesByReason[f.Tombstone.Reason]++
			}
			if f.Origin == "commonai" {
				r.CommonaiCapabilityTombstones++
			}
		}
		for _, tag := range f.FeatureTags {
			r.FeatureTags[tag]++
		}
	}

	// D3 collapse members are resolved by their canonical entry's collapsesWith,
	// not by a separate mapped/tombstoned record (deduplication-policy.md §4: a
	// coordinate/location/axis family collapses onto one symbol). They are real
	// source functions, so without crediting them here they would count as
	// unmapped forever and the M2 gate could never go green for any collapsed
	// family — even though the dedup decision for them is made. They are NOT
	// independent mapped entries, so they never inflate duplicateTargets (that
	// census counts manifest GoMapping symbols only). Credit each member that is
	// a real source symbol and is not already mapped/tombstoned in its own right.
	resolved := map[string]bool{}
	for _, f := range m.Functions {
		if f.Disposition == "mapped" || f.Disposition == "tombstoned" {
			resolved[f.Name] = true
		}
	}
	sourceNames := map[string]bool{}
	for _, c := range cs {
		sourceNames[c.Name] = true
	}
	collapsed := map[string]bool{}
	for _, f := range m.Functions {
		if f.Disposition != "mapped" || f.GoMapping == nil {
			continue
		}
		for _, member := range f.GoMapping.CollapsesWith {
			if !resolved[member] && sourceNames[member] {
				collapsed[member] = true
			}
		}
	}
	r.Collapsed = len(collapsed)

	// unmapped = source symbols neither mapped, tombstoned, nor collapsed.
	r.Unmapped = r.Total - r.Mapped - r.Tombstoned - r.Collapsed

	// duplicateTargets (M5 scaffold): a canonical symbol claimed by >1 entry.
	for sym, n := range symbolUse {
		if n > 1 {
			r.DuplicateTargets = append(r.DuplicateTargets, fmt.Sprintf("%s (×%d)", sym, n))
		}
	}
	sort.Strings(r.DuplicateTargets)

	r.Violations = auditViolations(r, cs)
	return r
}

// auditViolations returns the M2 gate breaches in stable order.
func auditViolations(r AuditReport, cs []Classification) []string {
	var v []string
	if r.Unclassified != 0 {
		v = append(v, fmt.Sprintf("unclassified=%d (M2 gate requires 0)", r.Unclassified))
	}
	if r.Unmapped != 0 {
		v = append(v, fmt.Sprintf("unmapped=%d (M2 gate requires 0; every symbol mapped or tombstoned)", r.Unmapped))
	}
	if r.CommonaiCapabilityTombstones != 0 {
		v = append(v, fmt.Sprintf("commonaiCapabilityTombstones=%d (D-2026-06-11-6 requires 0)", r.CommonaiCapabilityTombstones))
	}
	if len(r.DuplicateTargets) != 0 {
		v = append(v, fmt.Sprintf("duplicateTargets=%d: %s", len(r.DuplicateTargets), strings.Join(r.DuplicateTargets, ", ")))
	}
	// totality: every (origin,name) is unique (exactly one record per function).
	seen := map[string]bool{}
	dup := 0
	for _, c := range cs {
		k := c.Origin + "." + c.Name
		if seen[k] {
			dup++
		}
		seen[k] = true
	}
	if dup != 0 {
		v = append(v, fmt.Sprintf("totality: %d duplicate (origin,name) records", dup))
	}
	return v
}

// MarshalAuditJSON renders the counter block deterministically.
func MarshalAuditJSON(r AuditReport) ([]byte, error) {
	b, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(b, '\n'), nil
}

// RenderAuditMarkdown renders the human report (deduplication-policy.md §8 layout).
func RenderAuditMarkdown(r AuditReport) string {
	var b strings.Builder
	b.WriteString("# LitD API audit\n\n")
	b.WriteString("> Generated by jassgen. Do not edit by hand — regenerate with `go run ./tools/jassgen -audit`.\n\n")
	fmt.Fprintf(&b, "Source functions ......... %d  (common.j+blizzard.j = %d, +commonai = %d)\n", r.Total, r.TotalCore, r.Total)
	fmt.Fprintf(&b, "  Classified (D1) .......   %d\n", r.ByRule["D1"])
	fmt.Fprintf(&b, "  Classified (D2) .......   %d\n", r.ByRule["D2"])
	fmt.Fprintf(&b, "  Classified (D3) .......   %d\n", r.ByRule["D3"])
	fmt.Fprintf(&b, "  Classified (D4) .......   %d\n", r.ByRule["D4"])
	fmt.Fprintf(&b, "  Classified (D5) .......   %d\n", r.ByRule["D5"])
	fmt.Fprintf(&b, "  Unclassified ..........   %d\n", r.Unclassified)
	fmt.Fprintf(&b, "Disposition\n")
	fmt.Fprintf(&b, "  Mapped ................   %d\n", r.Mapped)
	fmt.Fprintf(&b, "  Tombstoned ............   %d", r.Tombstoned)
	if len(r.TombstonesByReason) > 0 {
		var parts []string
		for _, k := range []string{"superseded", "gameplay-irrelevant", "deprecated", "deferred-v2"} {
			if n := r.TombstonesByReason[k]; n > 0 {
				parts = append(parts, fmt.Sprintf("%s %d", k, n))
			}
		}
		fmt.Fprintf(&b, "  (%s)", strings.Join(parts, ", "))
	}
	b.WriteString("\n")
	fmt.Fprintf(&b, "  Collapsed (D3 members) .   %d\n", r.Collapsed)
	fmt.Fprintf(&b, "  Unmapped ..............   %d\n", r.Unmapped)
	fmt.Fprintf(&b, "commonai capability tombstones: %d (must be 0)\n", r.CommonaiCapabilityTombstones)
	fmt.Fprintf(&b, "duplicate canonical targets:    %d (M5 gate)\n\n", len(r.DuplicateTargets))

	b.WriteString("## VIOLATIONS\n\n")
	if len(r.Violations) == 0 {
		b.WriteString("(none — all M2 gates green)\n")
	} else {
		for _, v := range r.Violations {
			fmt.Fprintf(&b, "- %s\n", v)
		}
	}
	return b.String()
}
