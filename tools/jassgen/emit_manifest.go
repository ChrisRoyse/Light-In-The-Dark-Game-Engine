package main

// emit_manifest.go builds api-manifest.json (schema v1, tooling.md §2.4). The
// payload is deterministic — slices sorted by stable keys, no timestamps — so
// re-running yields a byte-identical file (tooling §1 reproducibility). A
// hand-written validator (ValidateManifest) enforces the published JSON Schema:
// the "zero exotic deps" principle (tooling §1) rules out a JSON-Schema engine,
// so the schema's required-fields/enums/allOf-conditionals are encoded in Go.
//
// Only schema-complete entries are emitted: a D1-D5 classification plus either a
// goMapping (disposition=mapped) or a tombstone (disposition=tombstoned).
// Unresolved symbols are counted and reported — that backlog is what the M2
// audit gate (#8) drives to zero.

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"sort"
)

// Manifest is the api-manifest.json instance (schema v1).
type Manifest struct {
	SchemaVersion int             `json:"schemaVersion"`
	Sources       []SourceEntry   `json:"sources"`
	Functions     []FunctionEntry `json:"functions"`
}

// SourceEntry records a vendored input file and its content hash.
type SourceEntry struct {
	File      string `json:"file"`
	SHA256    string `json:"sha256"`
	DeclCount int    `json:"declCount"`
}

// FunctionEntry is one source-function record.
type FunctionEntry struct {
	Name           string      `json:"name"`
	Origin         string      `json:"origin"`
	Signature      Signature   `json:"signature"`
	Classification string      `json:"classification"`
	ClassifiedBy   string      `json:"classifiedBy"`
	Disposition    string      `json:"disposition"`
	GoMapping      *GoMapping  `json:"goMapping,omitempty"`
	Tombstone      *TombstoneT `json:"tombstone,omitempty"`
	FeatureTags    []string    `json:"featureTags,omitempty"`
}

// Signature is the param list + return type.
type Signature struct {
	Params  []ParamEntry `json:"params"`
	Returns string       `json:"returns"`
}

// ParamEntry is one parameter with its JASS and (optional) TS type.
type ParamEntry struct {
	Name     string `json:"name"`
	JassType string `json:"jassType"`
	TSType   string `json:"tsType,omitempty"`
}

// GoMapping is the canonical Go symbol an entry maps to.
type GoMapping struct {
	Symbol        string   `json:"symbol"`
	Package       string   `json:"package"`
	GoSignature   string   `json:"goSignature,omitempty"`
	CollapsesWith []string `json:"collapsesWith,omitempty"`
	Notes         string   `json:"notes,omitempty"`
}

// TombstoneT is the tombstone record for a dropped symbol.
type TombstoneT struct {
	Reason string `json:"reason"`
	Detail string `json:"detail"`
}

var originFile = map[string]string{
	"common":   "common.j",
	"blizzard": "blizzard.j",
	"commonai": "commonai",
}

// BuildManifest assembles a Manifest from classifications, per-symbol signatures
// (from the merge pass), and source file metadata. It returns the manifest plus
// the count of classifications skipped because they are not yet schema-complete.
func BuildManifest(cs []Classification, sigs map[string]MergedEntry, sources []SourceEntry) (Manifest, int) {
	m := Manifest{SchemaVersion: 1, Sources: sources}
	skipped := 0
	for _, c := range cs {
		entry, ok := toFunctionEntry(c, sigs)
		if !ok {
			skipped++
			continue
		}
		m.Functions = append(m.Functions, entry)
	}
	sort.Slice(m.Functions, func(i, j int) bool {
		if m.Functions[i].Origin != m.Functions[j].Origin {
			return m.Functions[i].Origin < m.Functions[j].Origin
		}
		return m.Functions[i].Name < m.Functions[j].Name
	})
	return m, skipped
}

// toFunctionEntry converts a resolved classification into a schema entry, or
// reports ok=false if it is not yet emittable (no D1-D5 class, or mapped with no
// goMapping, or no disposition resolvable).
func toFunctionEntry(c Classification, sigs map[string]MergedEntry) (FunctionEntry, bool) {
	if !validClass(string(c.Class)) {
		return FunctionEntry{}, false
	}
	tombstoned := c.Tombstone != ""
	mapped := c.GoMapping != ""
	if !tombstoned && !mapped {
		return FunctionEntry{}, false
	}

	e := FunctionEntry{
		Name:           c.Name,
		Origin:         originFile[c.Origin],
		Signature:      signatureFor(c.Name, sigs),
		Classification: string(c.Class),
		ClassifiedBy:   c.ClassifiedBy,
	}
	if tombstoned {
		e.Disposition = "tombstoned"
		detail := c.Evidence
		if len(detail) > len("tombstone: ") && detail[:len("tombstone: ")] == "tombstone: " {
			detail = detail[len("tombstone: "):]
		}
		e.Tombstone = &TombstoneT{Reason: c.Tombstone, Detail: detail}
	} else {
		e.Disposition = "mapped"
		e.GoMapping = &GoMapping{Symbol: c.GoMapping, Package: c.Package, GoSignature: c.GoSignature, CollapsesWith: c.CollapsesWith}
	}
	return e, true
}

func signatureFor(name string, sigs map[string]MergedEntry) Signature {
	me, ok := sigs[name]
	if !ok {
		return Signature{Params: []ParamEntry{}, Returns: ""}
	}
	params := make([]ParamEntry, 0, len(me.Params))
	for _, p := range me.Params {
		params = append(params, ParamEntry{Name: p.Name, JassType: p.JassType, TSType: p.TSType})
	}
	ret := me.JassReturns
	if ret == "" {
		ret = "nothing"
	}
	return Signature{Params: params, Returns: ret}
}

// MarshalManifest renders the manifest as deterministic, indented JSON with a
// trailing newline.
func MarshalManifest(m Manifest) ([]byte, error) {
	b, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return nil, err
	}
	return append(b, '\n'), nil
}

// ValidateManifest enforces the published schema (tooling.md §2.4).
func ValidateManifest(m Manifest) error {
	if m.SchemaVersion != 1 {
		return fmt.Errorf("schemaVersion = %d, want const 1", m.SchemaVersion)
	}
	for i, s := range m.Sources {
		if s.File == "" || s.SHA256 == "" {
			return fmt.Errorf("sources[%d]: file and sha256 are required", i)
		}
		if s.DeclCount < 0 {
			return fmt.Errorf("sources[%d]: declCount must be >= 0", i)
		}
	}
	originOK := map[string]bool{"common.j": true, "blizzard.j": true, "commonai": true}
	classOK := map[string]bool{"D1": true, "D2": true, "D3": true, "D4": true, "D5": true}
	byOK := map[string]bool{"heuristic": true, "override": true}
	pkgOK := map[string]bool{"litd/api": true, "litd/api/helpers": true, "litd/ai": true}
	dispOK := map[string]bool{"mapped": true, "tombstoned": true}
	reasonOK := tombstoneReasons

	for i, f := range m.Functions {
		where := fmt.Sprintf("functions[%d] %q", i, f.Name)
		if f.Name == "" {
			return fmt.Errorf("%s: name is required", where)
		}
		if !originOK[f.Origin] {
			return fmt.Errorf("%s: origin %q outside enum [common.j blizzard.j commonai]", where, f.Origin)
		}
		if f.Signature.Returns == "" {
			return fmt.Errorf("%s: signature.returns is required", where)
		}
		for j, p := range f.Signature.Params {
			if p.Name == "" || p.JassType == "" {
				return fmt.Errorf("%s: param[%d] requires name and jassType", where, j)
			}
		}
		if !classOK[f.Classification] {
			return fmt.Errorf("%s: classification %q outside enum [D1..D5]", where, f.Classification)
		}
		if !byOK[f.ClassifiedBy] {
			return fmt.Errorf("%s: classifiedBy %q outside enum [heuristic override]", where, f.ClassifiedBy)
		}
		if !dispOK[f.Disposition] {
			return fmt.Errorf("%s: disposition %q outside enum [mapped tombstoned]", where, f.Disposition)
		}
		// allOf conditionals (+ defense-in-depth converse).
		switch f.Disposition {
		case "mapped":
			if f.GoMapping == nil {
				return fmt.Errorf("%s: disposition=mapped requires goMapping", where)
			}
			if f.GoMapping.Symbol == "" || !pkgOK[f.GoMapping.Package] {
				return fmt.Errorf("%s: goMapping needs symbol + package in [litd/api litd/api/helpers litd/ai], got %q/%q", where, f.GoMapping.Symbol, f.GoMapping.Package)
			}
			if f.Tombstone != nil {
				return fmt.Errorf("%s: mapped entry must not carry a tombstone", where)
			}
		case "tombstoned":
			if f.Tombstone == nil {
				return fmt.Errorf("%s: disposition=tombstoned requires tombstone", where)
			}
			if !reasonOK[f.Tombstone.Reason] {
				return fmt.Errorf("%s: tombstone.reason %q outside enum", where, f.Tombstone.Reason)
			}
			if f.Tombstone.Detail == "" {
				return fmt.Errorf("%s: tombstone.detail is required", where)
			}
			if f.GoMapping != nil {
				return fmt.Errorf("%s: tombstoned entry must not carry a goMapping", where)
			}
		}
	}
	return nil
}

// sourceMeta hashes a file and returns a SourceEntry.
func sourceMeta(path, file string, declCount int) (SourceEntry, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return SourceEntry{}, err
	}
	sum := sha256.Sum256(b)
	return SourceEntry{File: file, SHA256: fmt.Sprintf("%x", sum), DeclCount: declCount}, nil
}
