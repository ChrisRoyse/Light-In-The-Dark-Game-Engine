// pipeline.go is the assetgen pipeline skeleton (#56; tooling.md §5.2–5.3):
// spec -> generate -> curate -> assetcheck -> commit-with-provenance. It is
// build-time only and runs no AI itself — generation is pluggable (the image and
// TTS stages land in #57/#58); the skeleton runs end to end with a stub
// generator producing a placeholder file.
//
// Invariants enforced here, matching the issue's constraints:
//   - Raw candidates land in a scratch area, NEVER directly in assets/.
//   - Curate is a mandatory per-asset human accept with NO bypass: a missing or
//     non-interactive decision refuses to commit (DecisionNone), and an accept
//     without a sign-off name is refused at the provenance write.
//   - An accepted asset is run through the FULL assetcheck gate (in an isolated
//     staging copy) BEFORE it is committed; a finding blocks the commit and the
//     real assets/ tree is never touched on failure.
//   - Reject counts are tallied per category (R9 quality signal).
package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
)

// Decision is a curator's per-candidate verdict.
type Decision int

const (
	// DecisionNone means no decision was made (non-interactive / curate skipped).
	// It is treated as a refusal to commit — there is no bypass.
	DecisionNone Decision = iota
	DecisionAccept
	DecisionReject
)

// Generator produces a raw candidate file in scratchDir and returns its path.
// Implementations must write ONLY under scratchDir, never under assets/.
type Generator interface {
	Generate(it SpecItem, scratchDir string) (candidate string, err error)
}

// Curator reviews a candidate and returns a verdict plus, for an accept, the
// human sign-off name recorded in provenance. An empty signoff on an accept is
// refused downstream (no anonymous accept).
type Curator interface {
	Review(it SpecItem, candidate string) (Decision, string)
}

// Checker runs the asset gate over an assets directory and returns its findings
// (empty == clean). The production impl execs the real tools/assetcheck binary.
type Checker interface {
	Check(assetsDir string) (findings []string, err error)
}

// Pipeline holds the stages and the provenance constants stamped on commit.
type Pipeline struct {
	ScratchDir string
	AssetsDir  string
	Gen        Generator
	Cur        Curator
	Chk        Checker

	Pack      string // provenance: pack/source name
	Source    string // provenance: source URL or generator reference
	Retrieved string // provenance: generation date YYYY-MM-DD
	License   string // provenance: SPDX license (default CC0-1.0)

	Log io.Writer
}

// Report is the run outcome — what committed, what was rejected (per category),
// and what was accepted but blocked by the gate.
type Report struct {
	Committed []string       // committed output paths (sorted)
	Accepted  map[string]int // category -> count accepted (committed)
	Rejected  map[string]int // category -> count rejected or undecided
	Blocked   []BlockedItem  // accepted but failed assetcheck
}

// BlockedItem is an accepted candidate the gate refused.
type BlockedItem struct {
	Output   string
	Findings []string
}

func (p Pipeline) logf(format string, a ...any) {
	if p.Log != nil {
		fmt.Fprintf(p.Log, format+"\n", a...)
	}
}

// Run executes every item in items through the pipeline. It returns a Report and
// an error only for infrastructure failures (a stage erroring); a blocked or
// rejected candidate is a normal, recorded outcome, not a returned error.
func (p Pipeline) Run(items []SpecItem) (*Report, error) {
	lic := p.License
	if lic == "" {
		lic = "CC0-1.0"
	}
	rep := &Report{Accepted: map[string]int{}, Rejected: map[string]int{}}

	for _, it := range items {
		p.logf("generate: category=%s output=%s generator=%s", it.Category, it.Output, it.Generator)
		cand, err := p.Gen.Generate(it, p.ScratchDir)
		if err != nil {
			return nil, fmt.Errorf("generate %q: %w", it.Output, err)
		}
		if err := mustBeUnder(p.ScratchDir, cand); err != nil {
			return nil, fmt.Errorf("generator wrote outside scratch: %w", err)
		}
		p.logf("  candidate -> %s", cand)

		decision, signoff := p.Cur.Review(it, cand)
		switch decision {
		case DecisionAccept:
			// Validate in an isolated staging copy BEFORE touching assets/.
			findings, err := p.gateCandidate(it, cand, signoff, lic)
			if err != nil {
				return nil, fmt.Errorf("gate %q: %w", it.Output, err)
			}
			if len(findings) > 0 {
				p.logf("  BLOCKED by assetcheck (%d finding(s)) — not committed:", len(findings))
				for _, f := range findings {
					p.logf("    %s", f)
				}
				rep.Blocked = append(rep.Blocked, BlockedItem{Output: it.Output, Findings: findings})
				rep.Rejected[it.Category]++
				continue
			}
			if err := p.commit(it, cand, signoff, lic); err != nil {
				return nil, fmt.Errorf("commit %q: %w", it.Output, err)
			}
			p.logf("  committed %s (curator=%q)", it.Output, signoff)
			rep.Committed = append(rep.Committed, it.Output)
			rep.Accepted[it.Category]++
		case DecisionReject:
			p.logf("  rejected by curator — candidate discarded, assets/ untouched")
			rep.Rejected[it.Category]++
		default: // DecisionNone
			p.logf("  REFUSED: no curator decision (non-interactive / curate skipped) — there is no commit-without-sign-off bypass")
			rep.Rejected[it.Category]++
		}
	}

	sort.Strings(rep.Committed)
	return rep, nil
}

// gateCandidate stages the candidate + a provenance entry into a throwaway dir
// and runs the full asset gate there, so the real assets/ tree is never polluted
// by a candidate that fails. Returns the gate findings (empty == clean).
func (p Pipeline) gateCandidate(it SpecItem, cand, signoff, lic string) ([]string, error) {
	staging, err := os.MkdirTemp("", "assetgen-stage-")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(staging)
	if err := p.place(staging, it, cand, signoff, lic); err != nil {
		return nil, err
	}
	return p.Chk.Check(staging)
}

// commit places the candidate and its provenance entry into the real assets/.
func (p Pipeline) commit(it SpecItem, cand, signoff, lic string) error {
	return p.place(p.AssetsDir, it, cand, signoff, lic)
}

// place copies the candidate to <root>/<output> and appends its provenance entry
// to <root>/MANIFEST. AppendFile refuses an accept with no curator sign-off.
func (p Pipeline) place(root string, it SpecItem, cand, signoff, lic string) error {
	dst := filepath.Join(root, filepath.FromSlash(it.Output))
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	if err := copyFile(cand, dst); err != nil {
		return err
	}
	return AppendFile(root, Entry{
		Path: it.Output, Pack: p.Pack, Source: p.Source, License: lic,
		Retrieved: p.Retrieved, Category: it.Category,
		Generator: it.Generator, Params: specParams(it), Curator: signoff,
	})
}

// specParams folds the prompt + params + constraints into the provenance Params
// field so a committed asset records exactly how it was generated (G4.7).
func specParams(it SpecItem) string {
	s := "prompt=" + it.Prompt
	if it.Params != "" {
		s += "; " + it.Params
	}
	if it.Constraints != "" {
		s += "; constraints=" + it.Constraints
	}
	return s
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}

// mustBeUnder verifies path resolves inside dir — the guard that keeps raw
// candidates out of assets/.
func mustBeUnder(dir, path string) error {
	ad, err := filepath.Abs(dir)
	if err != nil {
		return err
	}
	ap, err := filepath.Abs(path)
	if err != nil {
		return err
	}
	rel, err := filepath.Rel(ad, ap)
	if err != nil || rel == ".." || filepath.IsAbs(rel) || hasDotDotPrefix(rel) {
		return fmt.Errorf("%q is not under %q", path, dir)
	}
	return nil
}

func hasDotDotPrefix(rel string) bool {
	return rel == ".." || (len(rel) >= 3 && rel[:3] == ".."+string(filepath.Separator))
}
