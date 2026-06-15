package main

// Generated-asset provenance gate (#35; tooling.md §3.2 Provenance row, §5.3;
// G4.2/G4.7). The base ledger rules (every file listed, no stale entries,
// hashes match, license in policy) live in the manifest package and run via
// VerifyPrefix. This file adds the rules specific to *generated* assets:
//
//   - A generated asset (provenance = "generated") must additionally carry a
//     generator (model/tool + version), generation params (or an assetgen.toml
//     reference), and a curator sign-off — any missing field fails (PROV-GENFIELD).
//   - A free-commercial license requires a curator sign-off (PROV-SIGNOFF).
//
// There is no bypass flag, consistent with the rest of the provenance surface.

import (
	"fmt"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

// checkGeneratedProvenance validates the G4.7 extra fields over the MANIFEST
// entries in scope. Findings are unsorted; check() sorts the merged set.
func checkGeneratedProvenance(assets []manifest.Asset, prefix string) []finding {
	var findings []finding
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }

	prefix = strings.Trim(prefix, "/")
	if prefix != "" {
		prefix += "/"
	}

	for _, a := range assets {
		if prefix != "" && !strings.HasPrefix(a.Path, prefix) {
			continue
		}
		// A free-commercial license is allowed only with a human sign-off.
		if a.License == "free-commercial" && a.Curator == "" {
			add(a.Path, "PROV-SIGNOFF", "free-commercial license requires a curator sign-off (curator field)")
		}
		if a.Provenance == "" {
			continue // downloaded asset — base provenance rules suffice
		}
		if a.Provenance != "generated" {
			add(a.Path, "PROV-GENFIELD", fmt.Sprintf("unknown provenance %q; use \"generated\" or omit for downloaded", a.Provenance))
			continue
		}
		for _, fld := range []struct{ name, val string }{
			{"generator", a.Generator},
			{"params", a.Params},
			{"curator", a.Curator},
		} {
			if fld.val == "" {
				add(a.Path, "PROV-GENFIELD", fmt.Sprintf("generated asset missing required %q field (G4.7)", fld.name))
			}
		}
	}
	return findings
}
