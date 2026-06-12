// assetcheck is the build-time asset gate over assets/ (tooling.md §3).
// Usage:
//
//	go run ./tools/assetcheck [--json] [--ingest] <assets-dir>
//
// Checks (v1): file format allowlist (R-FMT-1, R-AUD-1), glTF core
// profile + extension allowlist (R-FMT-1), no Draco/Meshopt compression
// (R-FMT-3), required unit animation clips (R-AST-3), and MANIFEST
// provenance (G4.2). There are deliberately no bypass flags for format
// or provenance rules.
//
// Unit category (for the clip rule) is the assets/units/ path prefix.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

type finding struct {
	Path string `json:"path"`
	Rule string `json:"rule"`
	Msg  string `json:"msg"`
}

func (f finding) String() string { return f.Path + ": " + f.Rule + ": " + f.Msg }

var (
	rejectedModelExts = map[string]string{
		".mdx": "Warcraft III MDX", ".mdl": "Warcraft III MDL", ".fbx": "FBX",
		".obj": "Wavefront OBJ", ".dae": "COLLADA DAE",
	}
	rejectedAudioExts = map[string]string{
		".wav": "WAV", ".mp3": "MP3", ".flac": "FLAC", ".aiff": "AIFF", ".aif": "AIFF",
	}
	// fail-closed allowlist: model, audio, texture
	allowedExts = map[string]bool{".glb": true, ".ogg": true, ".png": true}

	allowedGLTFExtensions = map[string]bool{"KHR_materials_unlit": true}
	compressionExtensions = map[string]string{
		"KHR_draco_mesh_compression": "Draco",
		"EXT_meshopt_compression":    "Meshopt",
	}

	requiredUnitClips = []string{"Idle", "Walk", "Attack", "Death"}
)

func main() {
	jsonMode := flag.Bool("json", false, "emit findings as JSON for CI annotation")
	ingest := flag.Bool("ingest", false, "emit extension/clip census (R1 detection) instead of gating")
	flag.Parse()
	if flag.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: assetcheck [--json] [--ingest] <assets-dir>")
		os.Exit(2)
	}
	dir := flag.Arg(0)
	if st, err := os.Stat(dir); err != nil || !st.IsDir() {
		fmt.Fprintf(os.Stderr, "assetcheck: %s is not a directory\n", dir)
		os.Exit(2)
	}

	files, err := listFiles(dir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetcheck: walk:", err)
		os.Exit(2)
	}

	if *ingest {
		if err := census(dir, files, *jsonMode); err != nil {
			fmt.Fprintln(os.Stderr, "assetcheck: census:", err)
			os.Exit(2)
		}
		return
	}

	findings := check(dir, files)
	if *jsonMode {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if findings == nil {
			findings = []finding{}
		}
		if err := enc.Encode(findings); err != nil {
			fmt.Fprintln(os.Stderr, "assetcheck:", err)
			os.Exit(2)
		}
	} else {
		for _, f := range findings {
			fmt.Println(f)
		}
		byRule := map[string]int{}
		for _, f := range findings {
			byRule[f.Rule]++
		}
		rules := make([]string, 0, len(byRule))
		for r := range byRule {
			rules = append(rules, r)
		}
		sort.Strings(rules)
		fmt.Printf("\nassetcheck: %d files, %d findings\n", len(files), len(findings))
		for _, r := range rules {
			fmt.Printf("  %-14s %d\n", r, byRule[r])
		}
	}
	if len(findings) > 0 {
		os.Exit(1)
	}
}

func listFiles(dir string) ([]string, error) {
	var files []string
	err := filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(dir, p)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if rel == "MANIFEST" {
			return nil
		}
		files = append(files, rel)
		return nil
	})
	sort.Strings(files)
	return files, err
}

// check runs every gate rule over the file list and returns sorted findings.
func check(dir string, files []string) []finding {
	var findings []finding
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }

	for _, rel := range files {
		ext := strings.ToLower(filepath.Ext(rel))
		switch {
		case rejectedModelExts[ext] != "":
			add(rel, "FMT-MODEL", fmt.Sprintf("%s is not allowed; models are core glTF 2.0 .glb only (R-FMT-1)", rejectedModelExts[ext]))
			continue
		case rejectedAudioExts[ext] != "":
			add(rel, "FMT-AUDIO", fmt.Sprintf("%s is not allowed; audio is .ogg only (R-AUD-1)", rejectedAudioExts[ext]))
			continue
		case !allowedExts[ext]:
			add(rel, "FMT-UNKNOWN", fmt.Sprintf("extension %q is not on the allowlist (.glb/.ogg/.png)", ext))
			continue
		}

		if ext == ".glb" {
			info, err := parseGLB(filepath.Join(dir, filepath.FromSlash(rel)))
			if err != nil {
				add(rel, "GLTF-CORE", err.Error())
				continue
			}
			for _, e := range append(append([]string{}, info.ExtensionsUsed...), info.ExtensionsRequired...) {
				if name, bad := compressionExtensions[e]; bad {
					add(rel, "GLTF-COMPRESS", fmt.Sprintf("%s compression (%s) — G3N cannot decode (R-FMT-3)", name, e))
				} else if !allowedGLTFExtensions[e] {
					add(rel, "GLTF-EXT", fmt.Sprintf("extension %q not permitted; core profile allows only KHR_materials_unlit (R-FMT-1)", e))
				}
			}
			if strings.HasPrefix(rel, "units/") {
				have := map[string]bool{}
				for _, c := range info.Clips {
					have[c] = true
				}
				var missing []string
				for _, c := range requiredUnitClips {
					if !have[c] {
						missing = append(missing, c)
					}
				}
				if len(missing) > 0 {
					add(rel, "CLIP-MISSING", fmt.Sprintf("unit model missing clips %v; present: %v (R-AST-3, names exact)", missing, info.Clips))
				}
			}
		}
	}

	prov, err := manifest.Verify(dir)
	if err != nil {
		add("MANIFEST", "PROV-PARSE", err.Error())
	}
	for _, v := range prov {
		add(v.Path, v.RuleID, v.Msg)
	}

	sort.Slice(findings, func(i, j int) bool {
		if findings[i].Path != findings[j].Path {
			return findings[i].Path < findings[j].Path
		}
		return findings[i].Rule < findings[j].Rule
	})
	return findings
}

// census emits the --ingest extension/clip census (R1 detection signal).
func census(dir string, files []string, jsonMode bool) error {
	type fileCensus struct {
		Path       string   `json:"path"`
		Ext        string   `json:"ext"`
		Extensions []string `json:"gltf_extensions,omitempty"`
		Clips      []string `json:"clips,omitempty"`
		ParseError string   `json:"parse_error,omitempty"`
	}
	var rows []fileCensus
	extCount := map[string]int{}
	gltfExtCount := map[string]int{}
	clipCount := map[string]int{}

	for _, rel := range files {
		ext := strings.ToLower(filepath.Ext(rel))
		row := fileCensus{Path: rel, Ext: ext}
		extCount[ext]++
		if ext == ".glb" {
			info, err := parseGLB(filepath.Join(dir, filepath.FromSlash(rel)))
			if err != nil {
				row.ParseError = err.Error()
			} else {
				row.Extensions = append(append([]string{}, info.ExtensionsUsed...), info.ExtensionsRequired...)
				row.Clips = info.Clips
				for _, e := range row.Extensions {
					gltfExtCount[e]++
				}
				for _, c := range info.Clips {
					clipCount[c]++
				}
			}
		}
		rows = append(rows, row)
	}

	if jsonMode {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		return enc.Encode(struct {
			Files         []fileCensus   `json:"files"`
			ExtCount      map[string]int `json:"ext_count"`
			GLTFExtCount  map[string]int `json:"gltf_extension_count"`
			ClipNameCount map[string]int `json:"clip_name_count"`
		}{rows, extCount, gltfExtCount, clipCount})
	}

	fmt.Printf("census: %d files\n\nby extension:\n", len(rows))
	for _, k := range sortedKeys(extCount) {
		fmt.Printf("  %-8s %d\n", k, extCount[k])
	}
	fmt.Println("\nglTF extensions seen:")
	if len(gltfExtCount) == 0 {
		fmt.Println("  (none)")
	}
	for _, k := range sortedKeys(gltfExtCount) {
		fmt.Printf("  %-32s %d\n", k, gltfExtCount[k])
	}
	fmt.Println("\nclip names seen:")
	if len(clipCount) == 0 {
		fmt.Println("  (none)")
	}
	for _, k := range sortedKeys(clipCount) {
		fmt.Printf("  %-16s %d\n", k, clipCount[k])
	}
	for _, r := range rows {
		if r.ParseError != "" {
			fmt.Printf("\nPARSE ERROR %s: %s\n", r.Path, r.ParseError)
		}
	}
	return nil
}

func sortedKeys(m map[string]int) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}
