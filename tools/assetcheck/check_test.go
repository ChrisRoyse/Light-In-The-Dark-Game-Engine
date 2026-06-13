package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// buildGLB assembles a minimal valid GLB container around a glTF JSON doc.
func buildGLB(t *testing.T, doc map[string]any) []byte {
	t.Helper()
	j, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	for len(j)%4 != 0 {
		j = append(j, ' ')
	}
	var b bytes.Buffer
	w := func(v uint32) { binary.Write(&b, binary.LittleEndian, v) }
	w(glbMagic)
	w(2)
	w(uint32(12 + 8 + len(j)))
	w(uint32(len(j)))
	w(glbChunkJSON)
	b.Write(j)
	return b.Bytes()
}

func gltfDoc(extUsed []string, clips []string) map[string]any {
	doc := map[string]any{"asset": map[string]any{"version": "2.0"}}
	if extUsed != nil {
		doc["extensionsUsed"] = extUsed
	}
	if clips != nil {
		var anims []map[string]any
		for _, c := range clips {
			anims = append(anims, map[string]any{"name": c})
		}
		doc["animations"] = anims
	}
	return doc
}

// fixture writes a file under dir and appends a matching MANIFEST entry.
type fixture struct {
	dir      string
	manifest bytes.Buffer
}

func newFixture(t *testing.T) *fixture {
	f := &fixture{dir: t.TempDir()}
	f.manifest.WriteString("# test ledger\n")
	return f
}

func (f *fixture) add(t *testing.T, rel string, content []byte, listed bool) {
	t.Helper()
	p := filepath.Join(f.dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, content, 0o644); err != nil {
		t.Fatal(err)
	}
	if listed {
		sum := sha256.Sum256(content)
		fmt.Fprintf(&f.manifest, "[[asset]]\npath = %q\npack = \"T\"\nsource = \"https://example.com\"\nlicense = \"CC0-1.0\"\nretrieved = \"2026-06-11\"\nsha256 = %q\n", rel, hex.EncodeToString(sum[:]))
	}
}

func (f *fixture) run(t *testing.T) []finding {
	t.Helper()
	if err := os.WriteFile(filepath.Join(f.dir, "MANIFEST"), f.manifest.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(f.dir)
	if err != nil {
		t.Fatal(err)
	}
	return check(f.dir, files, "")
}

func (f *fixture) runSubdir(t *testing.T, subdir string) []finding {
	t.Helper()
	if err := os.WriteFile(filepath.Join(f.dir, "MANIFEST"), f.manifest.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(filepath.Join(f.dir, filepath.FromSlash(subdir)))
	if err != nil {
		t.Fatal(err)
	}
	for i := range files {
		files[i] = filepath.ToSlash(filepath.Join(subdir, filepath.FromSlash(files[i])))
	}
	return check(f.dir, files, subdir)
}

func rules(fs []finding) []string {
	var r []string
	for _, f := range fs {
		r = append(r, f.Rule)
	}
	return r
}

func TestCleanAssetsPass(t *testing.T) {
	f := newFixture(t)
	f.add(t, "units/knight.glb", buildGLB(t, gltfDoc([]string{"KHR_materials_unlit"}, []string{"Idle", "Walk", "Attack", "Death", "Spell"})), true)
	f.add(t, "sfx/click.ogg", []byte("OggS-synthetic"), true)
	f.add(t, "atlas/vigil.png", []byte("PNG-synthetic"), true)
	if got := f.run(t); len(got) != 0 {
		t.Fatalf("clean fixture should pass, got %v", got)
	}
}

func TestUIAtlasPassesFromAssetsRootAndSubdir(t *testing.T) {
	f := newFixture(t)
	f.add(t, "ui/litd-default-ui.atlas.png", []byte("PNG-synthetic"), true)
	if got := f.run(t); len(got) != 0 {
		t.Fatalf("single UI atlas should pass from root, got %v", got)
	}
	if got := f.runSubdir(t, "ui"); len(got) != 0 {
		t.Fatalf("single UI atlas should pass from assets/ui, got %v", got)
	}
}

func TestUILooseIconRejected(t *testing.T) {
	f := newFixture(t)
	f.add(t, "ui/litd-default-ui.atlas.png", []byte("PNG-synthetic"), true)
	f.add(t, "ui/loose-command-icon.png", []byte("PNG-synthetic"), true)
	got := f.runSubdir(t, "ui")
	t.Logf("FSV loose UI icon findings=%v", got)
	if len(got) != 1 || got[0].Path != "ui/loose-command-icon.png" || got[0].Rule != "UI-ATLAS" {
		t.Fatalf("want loose icon UI-ATLAS, got %v", got)
	}
}

func TestSubdirCheckIgnoresUnrelatedManifestEntries(t *testing.T) {
	f := newFixture(t)
	f.add(t, "ui/litd-default-ui.atlas.png", []byte("PNG-synthetic"), true)
	f.manifest.WriteString("[[asset]]\npath = \"units/missing.glb\"\npack = \"T\"\nsource = \"https://example.com\"\nlicense = \"CC0-1.0\"\nretrieved = \"2026-06-11\"\nsha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n")
	if got := f.runSubdir(t, "ui"); len(got) != 0 {
		t.Fatalf("subdir check should ignore unrelated manifest entries, got %v", got)
	}
}

func TestRejectedModelFormats(t *testing.T) {
	f := newFixture(t)
	for _, ext := range []string{"mdx", "mdl", "fbx", "obj", "dae"} {
		f.add(t, "units/bad."+ext, []byte("x"), true)
	}
	got := f.run(t)
	if len(got) != 5 {
		t.Fatalf("want 5 FMT-MODEL findings, got %v", got)
	}
	for _, fd := range got {
		if fd.Rule != "FMT-MODEL" {
			t.Fatalf("want FMT-MODEL, got %v", fd)
		}
	}
}

func TestRejectedAudioAndUnknown(t *testing.T) {
	f := newFixture(t)
	f.add(t, "sfx/bad.wav", []byte("x"), true)
	f.add(t, "stray.zip", []byte("x"), true)
	got := f.run(t)
	if len(got) != 2 || got[0].Rule != "FMT-AUDIO" || got[1].Rule != "FMT-UNKNOWN" {
		t.Fatalf("want [FMT-AUDIO FMT-UNKNOWN], got %v", got)
	}
}

func TestDisallowedExtension(t *testing.T) {
	f := newFixture(t)
	f.add(t, "props/rock.glb", buildGLB(t, gltfDoc([]string{"KHR_texture_transform"}, nil)), true)
	got := f.run(t)
	if len(got) != 1 || got[0].Rule != "GLTF-EXT" {
		t.Fatalf("want GLTF-EXT naming KHR_texture_transform, got %v", got)
	}
}

func TestCompressionRejected(t *testing.T) {
	f := newFixture(t)
	f.add(t, "props/d.glb", buildGLB(t, gltfDoc([]string{"KHR_draco_mesh_compression"}, nil)), true)
	f.add(t, "props/m.glb", buildGLB(t, gltfDoc([]string{"EXT_meshopt_compression"}, nil)), true)
	got := f.run(t)
	if len(got) != 2 || got[0].Rule != "GLTF-COMPRESS" || got[1].Rule != "GLTF-COMPRESS" {
		t.Fatalf("want 2 GLTF-COMPRESS, got %v", got)
	}
}

func TestUnitMissingDeathClip(t *testing.T) {
	f := newFixture(t)
	f.add(t, "units/wisp.glb", buildGLB(t, gltfDoc(nil, []string{"Idle", "Walk", "Attack"})), true)
	got := f.run(t)
	if len(got) != 1 || got[0].Rule != "CLIP-MISSING" {
		t.Fatalf("want CLIP-MISSING, got %v", got)
	}
}

func TestNonUnitNeedsNoClips(t *testing.T) {
	f := newFixture(t)
	f.add(t, "buildings/hall.glb", buildGLB(t, gltfDoc(nil, nil)), true)
	if got := f.run(t); len(got) != 0 {
		t.Fatalf("buildings need no clips in v1, got %v", got)
	}
}

func TestProvenanceWiredIn(t *testing.T) {
	f := newFixture(t)
	f.add(t, "units/orphan.glb", buildGLB(t, gltfDoc(nil, []string{"Idle", "Walk", "Attack", "Death"})), false)
	got := f.run(t)
	if len(got) != 1 || got[0].Rule != "PROV-UNLISTED" {
		t.Fatalf("want PROV-UNLISTED, got %v", got)
	}
}

func TestCorruptGLB(t *testing.T) {
	f := newFixture(t)
	f.add(t, "units/corrupt.glb", []byte("not a glb at all"), true)
	got := f.run(t)
	if len(got) != 1 || got[0].Rule != "GLTF-CORE" {
		t.Fatalf("want GLTF-CORE, got %v", got)
	}
}

func TestExternalURIRejected(t *testing.T) {
	f := newFixture(t)
	doc := gltfDoc(nil, nil)
	doc["images"] = []map[string]any{{"uri": "Textures/colormap.png"}}
	f.add(t, "props/external.glb", buildGLB(t, doc), true)
	// data: URIs are self-contained and must pass
	doc2 := gltfDoc(nil, nil)
	doc2["images"] = []map[string]any{{"uri": "data:image/png;base64,AAAA"}}
	f.add(t, "props/datauri.glb", buildGLB(t, doc2), true)
	got := f.run(t)
	if len(got) != 1 || got[0].Rule != "GLTF-URI" || got[0].Path != "props/external.glb" {
		t.Fatalf("want one GLTF-URI for external.glb only, got %v", got)
	}
}

func TestEmptyAssetsDirPasses(t *testing.T) {
	f := newFixture(t)
	if got := f.run(t); len(got) != 0 {
		t.Fatalf("empty assets dir should pass, got %v", got)
	}
}

func TestDataLocalePassesFSV(t *testing.T) {
	root, data := newDataFixture(t)
	writeLocale(t, data, "en", goodLocaleTOML())
	writeLocale(t, data, "xx", pseudoLocaleTOML())
	files, err := listFiles(data)
	if err != nil {
		t.Fatal(err)
	}
	got := checkData(data, files, "")
	t.Logf("FSV data locale pass root=%s files=%v findings=%v", root, files, got)
	if len(got) != 0 {
		t.Fatalf("clean locale data should pass, got %v", got)
	}
}

func TestDataLocaleMissingAndUnusedRejectedFSV(t *testing.T) {
	_, data := newDataFixture(t)
	missing := strings.Replace(goodLocaleTOML(), `"hud.queue.prefix" = "queue v"`+"\n", "", 1)
	writeLocale(t, data, "en", missing)
	files, err := listFiles(data)
	if err != nil {
		t.Fatal(err)
	}
	got := checkData(data, files, "")
	t.Logf("FSV missing locale findings=%v", got)
	if len(got) != 1 || got[0].Rule != "LOCALE-MISSING" || !strings.Contains(got[0].Msg, "hud.queue.prefix") {
		t.Fatalf("missing locale key should be named, got %v", got)
	}

	_, data = newDataFixture(t)
	writeLocale(t, data, "en", goodLocaleTOML()+`"hud.extra.unused" = "unused"`+"\n")
	files, err = listFiles(data)
	if err != nil {
		t.Fatal(err)
	}
	got = checkData(data, files, "")
	t.Logf("FSV unused locale findings=%v", got)
	if len(got) != 1 || got[0].Rule != "LOCALE-UNUSED" || !strings.Contains(got[0].Msg, "hud.extra.unused") {
		t.Fatalf("unused locale key should be named, got %v", got)
	}
}

func TestDataHardcodedHUDLabelRejectedFSV(t *testing.T) {
	root, data := newDataFixture(t)
	writeLocale(t, data, "en", goodLocaleTOML())
	hudDir := filepath.Join(root, "litd", "render", "hud")
	if err := os.MkdirAll(hudDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(hudDir, "bad.go"), []byte(`package hud

func leak() {
	gui.NewLabel("leaked literal")
}
`), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(data)
	if err != nil {
		t.Fatal(err)
	}
	got := checkData(data, files, "")
	t.Logf("FSV hard-coded HUD label findings=%v", got)
	if len(got) != 1 || got[0].Rule != "STRING-LINT" || !strings.Contains(got[0].Path, "bad.go:4") {
		t.Fatalf("hard-coded GUI label should be rejected with file:line, got %v", got)
	}
}

func newDataFixture(t *testing.T) (root, data string) {
	t.Helper()
	root = t.TempDir()
	data = filepath.Join(root, "data")
	if err := os.MkdirAll(filepath.Join(data, "locale"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "litd", "render", "hud"), 0o755); err != nil {
		t.Fatal(err)
	}
	return root, data
}

func writeLocale(t *testing.T, data, tag, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(data, "locale", tag+".toml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func goodLocaleTOML() string {
	return `[strings]
"hud.resource.gold" = "G"
"hud.resource.lumber" = "L"
"hud.resource.food" = "F"
"hud.vital.life" = "HP"
"hud.vital.mana" = "MP"
"hud.selection.prefix" = "selection v"
"hud.queue.prefix" = "queue v"
"hud.groups.prefix" = "groups v"
"hud.menu.ok_true" = "HUD ok"
"hud.menu.ok_false" = "HUD error"
"hud.widget.idle_worker" = "idle worker"
"hud.widget.minimap" = "minimap"
`
}

func pseudoLocaleTOML() string {
	return `[strings]
"hud.resource.gold" = "[xx.01]"
"hud.resource.lumber" = "[xx.02]"
"hud.resource.food" = "[xx.03]"
"hud.vital.life" = "[xx.04]"
"hud.vital.mana" = "[xx.05]"
"hud.selection.prefix" = "[xx.06]"
"hud.queue.prefix" = "[xx.07]"
"hud.groups.prefix" = "[xx.08]"
"hud.menu.ok_true" = "[xx.09]"
"hud.menu.ok_false" = "[xx.10]"
"hud.widget.idle_worker" = "[xx.11]"
"hud.widget.minimap" = "[xx.12]"
`
}
