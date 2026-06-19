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
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/assetcatalog"
	litlocale "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
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

	// glTF extension allow/deny lists now live in the shared assetcatalog package
	// (#411) so assetcheck and the in-engine archive loader use one source.
	allowedGLTFExtensions = assetcatalog.AllowedGLTFExtensions
	compressionExtensions = assetcatalog.CompressionExtensions

	requiredUnitClips = []string{"Idle", "Walk", "Attack", "Death"}
)

func main() {
	// `assetcheck archive [--json] <file>` is a subcommand with its own flags,
	// dispatched before the default-dir flag set (#37).
	if len(os.Args) >= 2 && os.Args[1] == "archive" {
		runArchiveCmd(os.Args[2:])
		return
	}

	jsonMode := flag.Bool("json", false, "emit findings as JSON for CI annotation")
	ingest := flag.Bool("ingest", false, "emit extension/clip census (R1 detection) instead of gating")
	waiversPath := flag.String("waivers", "", "path to triangle-budget waivers.toml (default: <assets-root>/waivers.toml if present)")
	flag.Parse()

	if flag.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: assetcheck [--json] [--ingest] <assets-dir>\n       assetcheck archive [--json] <archive.litdworld>")
		os.Exit(2)
	}
	dir := flag.Arg(0)
	if st, err := os.Stat(dir); err != nil || !st.IsDir() {
		fmt.Fprintf(os.Stderr, "assetcheck: %s is not a directory\n", dir)
		os.Exit(2)
	}
	root, prefix, dataMode, err := resolveCheckRoot(dir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetcheck:", err)
		os.Exit(2)
	}

	files, err := listFiles(filepath.Join(root, filepath.FromSlash(prefix)))
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetcheck: walk:", err)
		os.Exit(2)
	}
	if prefix != "" {
		for i := range files {
			files[i] = filepath.ToSlash(filepath.Join(prefix, filepath.FromSlash(files[i])))
		}
	}

	if *ingest && dataMode {
		fmt.Fprintln(os.Stderr, "assetcheck: --ingest is only supported for assets/")
		os.Exit(2)
	}
	if *ingest {
		if err := census(root, files, *jsonMode); err != nil {
			fmt.Fprintln(os.Stderr, "assetcheck: census:", err)
			os.Exit(2)
		}
		return
	}

	var findings []finding
	var notes []string
	if dataMode {
		findings = checkData(root, files, prefix)
	} else {
		ws := newWaiverSet()
		if *waiversPath != "" {
			// An explicitly-named ledger that fails to load is fatal — the
			// budget gate fails closed rather than silently ignoring waivers.
			loaded, lerr := loadWaivers(*waiversPath)
			if lerr != nil {
				fmt.Fprintln(os.Stderr, "assetcheck: waivers:", lerr)
				os.Exit(2)
			}
			ws = loaded
		}
		findings, notes = check(root, files, prefix, ws)
	}
	for _, n := range notes {
		fmt.Fprintln(os.Stderr, "assetcheck:", n)
	}
	emitFindings(findings, *jsonMode, len(files))
	if len(findings) > 0 {
		os.Exit(1)
	}
}

// emitFindings prints findings as JSON or as the human gate report.
func emitFindings(findings []finding, jsonMode bool, fileCount int) {
	if jsonMode {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if findings == nil {
			findings = []finding{}
		}
		if err := enc.Encode(findings); err != nil {
			fmt.Fprintln(os.Stderr, "assetcheck:", err)
			os.Exit(2)
		}
		return
	}
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
	fmt.Printf("\nassetcheck: %d files, %d findings\n", fileCount, len(findings))
	for _, r := range rules {
		fmt.Printf("  %-14s %d\n", r, byRule[r])
	}
}

func resolveCheckRoot(dir string) (root string, prefix string, dataMode bool, err error) {
	root, prefix, err = resolveAssetRoot(dir)
	if err == nil {
		return root, prefix, false, nil
	}
	root, prefix, err = resolveDataRoot(dir)
	if err == nil {
		return root, prefix, true, nil
	}
	return "", "", false, fmt.Errorf("no MANIFEST or data/ root found at %s or its parents", dir)
}

func resolveAssetRoot(dir string) (root string, prefix string, err error) {
	dir, err = filepath.Abs(dir)
	if err != nil {
		return "", "", err
	}
	for cur := dir; ; cur = filepath.Dir(cur) {
		if _, statErr := os.Stat(filepath.Join(cur, "MANIFEST")); statErr == nil {
			rel, relErr := filepath.Rel(cur, dir)
			if relErr != nil {
				return "", "", relErr
			}
			if rel == "." {
				rel = ""
			}
			return cur, filepath.ToSlash(rel), nil
		}
		next := filepath.Dir(cur)
		if next == cur {
			break
		}
	}
	return "", "", fmt.Errorf("no MANIFEST found at %s or its parents", dir)
}

func resolveDataRoot(dir string) (root string, prefix string, err error) {
	dir, err = filepath.Abs(dir)
	if err != nil {
		return "", "", err
	}
	for cur := dir; ; cur = filepath.Dir(cur) {
		if filepath.Base(cur) == "data" {
			rel, relErr := filepath.Rel(cur, dir)
			if relErr != nil {
				return "", "", relErr
			}
			if rel == "." {
				rel = ""
			}
			return cur, filepath.ToSlash(rel), nil
		}
		next := filepath.Dir(cur)
		if next == cur {
			break
		}
	}
	return "", "", fmt.Errorf("no data root found")
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

// check runs every gate rule over the file list and returns sorted findings
// plus informational notes (e.g. applied triangle-budget waivers).
func check(dir string, files []string, prefix string, ws waiverSet) ([]finding, []string) {
	var findings []finding
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }
	triangles := map[string]int{}
	textures := map[string][]textureInfo{}

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
			triangles[rel] = info.Triangles
			if tis := decodeTextures(info); tis != nil {
				textures[rel] = tis
			}
			for _, u := range info.ExternalURIs {
				add(rel, "GLTF-URI", fmt.Sprintf("external resource reference (%s) — committed GLBs must be self-contained", u))
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
	checkUIAtlas(files, add)
	atlasFindings, atlasNotes := checkAtlas(textures)
	findings = append(findings, atlasFindings...)

	prov, err := manifest.VerifyPrefix(dir, prefix)
	if err != nil {
		add("MANIFEST", "PROV-PARSE", err.Error())
	}
	for _, v := range prov {
		add(v.Path, v.RuleID, v.Msg)
	}

	// Triangle budget (#31). If the MANIFEST parses, run the budget gate over
	// the GLBs we counted; a parse error was already reported as PROV-PARSE.
	notes := atlasNotes
	if assets, lerr := manifest.Load(dir); lerr == nil {
		byPath := make(map[string]manifest.Asset, len(assets))
		for _, a := range assets {
			byPath[a.Path] = a
		}
		bf, bn := checkBudget(triangles, byPath, ws)
		findings = append(findings, bf...)
		notes = append(notes, bn...)
		findings = append(findings, checkGeneratedProvenance(assets, prefix)...)
	}

	sort.Slice(findings, func(i, j int) bool {
		if findings[i].Path != findings[j].Path {
			return findings[i].Path < findings[j].Path
		}
		return findings[i].Rule < findings[j].Rule
	})
	return findings, notes
}

func checkUIAtlas(files []string, add func(path, rule, msg string)) {
	uiPNGs := 0
	atlases := 0
	for _, rel := range files {
		if !strings.HasPrefix(rel, "ui/") {
			continue
		}
		if strings.ToLower(filepath.Ext(rel)) != ".png" {
			continue
		}
		uiPNGs++
		if strings.HasSuffix(filepath.Base(rel), ".atlas.png") {
			atlases++
			continue
		}
		add(rel, "UI-ATLAS", "UI PNG is not atlas-resident; all HUD iconography must be packed into one *.atlas.png")
	}
	if uiPNGs > 0 && atlases != 1 {
		add("ui", "UI-ATLAS", fmt.Sprintf("expected exactly one UI *.atlas.png, found %d", atlases))
	}
}

func checkData(dir string, files []string, prefix string) []finding {
	var findings []finding
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }

	for _, rel := range files {
		if isMapDataFile(rel) {
			continue
		}
		if strings.ToLower(filepath.Ext(rel)) != ".toml" {
			add(rel, "DATA-FMT", "data files must be TOML in this validation pass")
		}
	}
	if prefix == "" || prefix == "maps" || strings.HasPrefix(prefix, "maps/") {
		checkMapDataTables(dir, files, add)
	}
	if prefix == "" || prefix == "hud" || strings.HasPrefix(prefix, "hud/") {
		checkCommandCardTables(dir, files, add)
	}
	if prefix == "" || prefix == "locale" || strings.HasPrefix(prefix, "locale/") {
		checkLocaleTables(dir, files, add)
	}
	if prefix == "" {
		checkHardcodedRenderLabels(filepath.Dir(dir), add)
	}

	sort.Slice(findings, func(i, j int) bool {
		if findings[i].Path != findings[j].Path {
			return findings[i].Path < findings[j].Path
		}
		return findings[i].Rule < findings[j].Rule
	})
	return findings
}

func isMapDataFile(rel string) bool {
	if !strings.HasPrefix(rel, "maps/") {
		return false
	}
	switch path.Base(rel) {
	case "terrain.toml", "doodads.toml", "pathing.txt", "cliff.txt", "height.txt", "splat.txt":
		return true
	default:
		return false
	}
}

func checkMapDataTables(dataRoot string, files []string, add func(path, rule, msg string)) {
	dirs := map[string]bool{}
	for _, rel := range files {
		if isMapDataFile(rel) {
			dirs[path.Dir(rel)] = true
		}
	}
	repoRoot := filepath.Dir(dataRoot)
	for _, dir := range sortedBoolKeys(dirs) {
		if _, err := litmapdata.Load(os.DirFS(repoRoot), path.Join("data", dir)); err != nil {
			add(dir, "MAPDATA", err.Error())
		}
	}
}

func checkCommandCardTables(dir string, files []string, add func(path, rule, msg string)) {
	have := false
	for _, rel := range files {
		if rel != lithud.DefaultCommandCardPath {
			continue
		}
		have = true
		table, err := lithud.ReadCommandCardTable(os.DirFS(dir), rel)
		if err != nil {
			add(rel, "COMMAND-CARD", err.Error())
			continue
		}
		required := map[string]bool{}
		for _, key := range litlocale.RequiredKeys() {
			required[key] = true
		}
		for _, key := range table.LocaleKeys() {
			if !required[key] {
				add(rel, "COMMAND-CARD", fmt.Sprintf("locale key %q is not in the extracted string-key set", key))
			}
		}
	}
	if !have {
		add(lithud.DefaultCommandCardPath, "COMMAND-CARD", "default command-card table is required")
	}
}

func checkLocaleTables(dir string, files []string, add func(path, rule, msg string)) {
	haveEN := false
	required := litlocale.RequiredKeys()
	for _, rel := range files {
		if !strings.HasPrefix(rel, "locale/") || strings.ToLower(filepath.Ext(rel)) != ".toml" {
			continue
		}
		tag := strings.TrimSuffix(strings.TrimPrefix(rel, "locale/"), ".toml")
		if tag == "en" {
			haveEN = true
		}
		table, err := litlocale.Read(os.DirFS(dir), tag)
		if err != nil {
			add(rel, "LOCALE-PARSE", err.Error())
			continue
		}
		for _, v := range litlocale.ValidateTable(rel, table, required) {
			add(v.Path, v.Rule, v.Msg)
		}
	}
	if !haveEN {
		add("locale/en.toml", "LOCALE-MISSING", "English locale table is required")
	}
}

func checkHardcodedRenderLabels(repoRoot string, add func(path, rule, msg string)) {
	for _, rel := range []string{"litd/render"} {
		root := filepath.Join(repoRoot, filepath.FromSlash(rel))
		_ = filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
			if err != nil || d.IsDir() || filepath.Ext(path) != ".go" || strings.HasSuffix(path, "_test.go") {
				return nil
			}
			checkHardcodedRenderLabelsFile(repoRoot, path, add)
			return nil
		})
	}
}

func checkHardcodedRenderLabelsFile(repoRoot, file string, add func(path, rule, msg string)) {
	fset := token.NewFileSet()
	node, err := parser.ParseFile(fset, file, nil, 0)
	rel, relErr := filepath.Rel(repoRoot, file)
	if relErr != nil {
		rel = file
	}
	rel = filepath.ToSlash(rel)
	if err != nil {
		add(rel, "STRING-LINT", err.Error())
		return
	}
	ast.Inspect(node, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok || len(call.Args) == 0 || !isStringLiteral(call.Args[0]) {
			return true
		}
		if isScreenTextCall(call.Fun) {
			pos := fset.Position(call.Pos())
			add(fmt.Sprintf("%s:%d", rel, pos.Line), "STRING-LINT", "hard-coded GUI text literal must use a locale key")
		}
		return true
	})
}

func isStringLiteral(expr ast.Expr) bool {
	lit, ok := expr.(*ast.BasicLit)
	return ok && lit.Kind == token.STRING
}

func isScreenTextCall(fun ast.Expr) bool {
	switch f := fun.(type) {
	case *ast.SelectorExpr:
		return f.Sel.Name == "SetText" || f.Sel.Name == "NewLabel"
	case *ast.Ident:
		return f.Name == "NewLabel"
	}
	return false
}

// census emits the --ingest extension/clip census (R1 detection signal).
func census(dir string, files []string, jsonMode bool) error {
	type fileCensus struct {
		Path       string   `json:"path"`
		Ext        string   `json:"ext"`
		Extensions []string `json:"gltf_extensions,omitempty"`
		Clips      []string `json:"clips,omitempty"`
		Textures   []string `json:"textures,omitempty"` // "WxH" per embedded texture
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
				for _, ti := range decodeTextures(info) {
					if ti.Err != nil {
						row.Textures = append(row.Textures, "?x?")
					} else {
						row.Textures = append(row.Textures, fmt.Sprintf("%dx%d", ti.Width, ti.Height))
					}
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

func sortedBoolKeys(m map[string]bool) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}
