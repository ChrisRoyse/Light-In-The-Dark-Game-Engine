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
	findings, _ := check(f.dir, files, "", newWaiverSet())
	return findings
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
	findings, _ := check(f.dir, files, subdir, newWaiverSet())
	return findings
}

func rules(fs []finding) []string {
	var r []string
	for _, f := range fs {
		r = append(r, f.Rule)
	}
	return r
}

// monoVorbisOgg builds a minimal valid mono Vorbis Ogg (identification header on a
// BOS page + an EOS page carrying `frames` as the final granule). channels=1,
// rate=44100 satisfies the world-SFX layout rule (#228).
func monoVorbisOgg(channels byte, rate uint32, frames int64) []byte {
	page := func(headerType byte, granule int64, seq uint32, packet []byte) []byte {
		var segs []byte
		n := len(packet)
		for n >= 255 {
			segs = append(segs, 255)
			n -= 255
		}
		segs = append(segs, byte(n))
		var b bytes.Buffer
		b.WriteString("OggS")
		b.WriteByte(0)
		b.WriteByte(headerType)
		binary.Write(&b, binary.LittleEndian, granule)
		binary.Write(&b, binary.LittleEndian, uint32(0xCAFE))
		binary.Write(&b, binary.LittleEndian, seq)
		binary.Write(&b, binary.LittleEndian, uint32(0))
		b.WriteByte(byte(len(segs)))
		b.Write(segs)
		b.Write(packet)
		return b.Bytes()
	}
	var id bytes.Buffer
	id.WriteByte(1)
	id.WriteString("vorbis")
	binary.Write(&id, binary.LittleEndian, uint32(0))
	id.WriteByte(channels)
	binary.Write(&id, binary.LittleEndian, rate)
	binary.Write(&id, binary.LittleEndian, int32(0))
	binary.Write(&id, binary.LittleEndian, int32(0))
	binary.Write(&id, binary.LittleEndian, int32(0))
	id.WriteByte(0xB8)
	id.WriteByte(1)
	return append(page(0x02, 0, 0, id.Bytes()), page(0x04, frames, 1, []byte{})...)
}

// opusInOgg builds an Ogg whose identification header is OpusHead (wrong codec).
func opusInOgg(channels byte) []byte {
	page := func(headerType byte, granule int64, seq uint32, packet []byte) []byte {
		var b bytes.Buffer
		b.WriteString("OggS")
		b.WriteByte(0)
		b.WriteByte(headerType)
		binary.Write(&b, binary.LittleEndian, granule)
		binary.Write(&b, binary.LittleEndian, uint32(0xBEEF))
		binary.Write(&b, binary.LittleEndian, seq)
		binary.Write(&b, binary.LittleEndian, uint32(0))
		b.WriteByte(1)
		b.WriteByte(byte(len(packet)))
		b.Write(packet)
		return b.Bytes()
	}
	var h bytes.Buffer
	h.WriteString("OpusHead")
	h.WriteByte(1)
	h.WriteByte(channels)
	binary.Write(&h, binary.LittleEndian, uint16(0))
	binary.Write(&h, binary.LittleEndian, uint32(48000))
	binary.Write(&h, binary.LittleEndian, uint16(0))
	h.WriteByte(0)
	return append(page(0x02, 0, 0, h.Bytes()), page(0x04, 48000, 1, []byte{})...)
}

func rulesContain(fs []finding, rule string) bool {
	for _, f := range fs {
		if f.Rule == rule {
			return true
		}
	}
	return false
}

// #228 FSV: each audio rule rejects its violation in a constructed fixture tree.
func TestAudioStereoWorldSFXRejectedFSV(t *testing.T) {
	f := newFixture(t)
	f.add(t, "sfx/loud.ogg", monoVorbisOgg(2, 44100, 100), true) // stereo in a world dir
	got := f.run(t)
	if !rulesContain(got, "AUD-CHAN") {
		t.Fatalf("stereo world SFX must yield AUD-CHAN, got %v", got)
	}
	t.Logf("FSV #228 assetcheck: stereo world SFX → AUD-CHAN")
}

func TestAudioOpusRejectedFSV(t *testing.T) {
	f := newFixture(t)
	f.add(t, "music/theme.ogg", opusInOgg(2), true) // Opus codec inside .ogg
	got := f.run(t)
	if !rulesContain(got, "AUD-CODEC") {
		t.Fatalf("Opus-in-ogg must yield AUD-CODEC (codec check, not extension), got %v", got)
	}
	t.Logf("FSV #228 assetcheck: Opus-in-ogg → AUD-CODEC")
}

func TestAudioBudgetFSV(t *testing.T) {
	const mb = 1024 * 1024
	// Decoded bytes = frames × 1ch × 2. Choose granules straddling the 48 MB cap.
	over := int64(49*mb/2 + 1)  // ~49 MB decoded
	under := int64(47 * mb / 2) // ~47 MB decoded

	fOver := newFixture(t)
	fOver.add(t, "sfx/big.ogg", monoVorbisOgg(1, 44100, over), true)
	if got := fOver.run(t); !rulesContain(got, "AUD-BUDGET") {
		t.Fatalf("49 MB resident set must yield AUD-BUDGET, got %v", got)
	}
	fUnder := newFixture(t)
	fUnder.add(t, "sfx/ok.ogg", monoVorbisOgg(1, 44100, under), true)
	if got := fUnder.run(t); rulesContain(got, "AUD-BUDGET") {
		t.Fatalf("47 MB resident set must pass the budget, got %v", got)
	}
	t.Logf("FSV #228 assetcheck: ~49 MB → AUD-BUDGET; ~47 MB → passes (cap 48 MB)")
}

func TestCleanAssetsPass(t *testing.T) {
	f := newFixture(t)
	f.add(t, "units/knight.glb", buildGLB(t, gltfDoc([]string{"KHR_materials_unlit"}, []string{"Idle", "Walk", "Attack", "Death", "Spell"})), true)
	f.add(t, "sfx/click.ogg", monoVorbisOgg(1, 44100, 4410), true)
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
	// data: URIs are self-contained and must pass (valid embedded 8x8 PNG;
	// alone in the tree so the atlas uniqueness rule does not fire)
	doc2 := gltfDoc(nil, nil)
	doc2["images"] = []map[string]any{{"uri": "data:image/png;base64," + tinyPNGB64(t, 8, 8)}}
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
	writeLocale(t, data, "en", goodLocaleTOML(t))
	writeLocale(t, data, "xx", pseudoLocaleTOML(t))
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
	missing := strings.Replace(goodLocaleTOML(t), `"hud.queue.prefix" = "queue v"`+"\n", "", 1)
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
	writeLocale(t, data, "en", goodLocaleTOML(t)+`"hud.extra.unused" = "unused"`+"\n")
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
	writeLocale(t, data, "en", goodLocaleTOML(t))
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

func TestDataCommandCardRejectedFSV(t *testing.T) {
	_, data := newDataFixture(t)
	writeLocale(t, data, "en", goodLocaleTOML(t))
	bad := strings.Replace(goodCommandCardTOML(t), `opcode = "move"`, `opcode = "teleport"`, 1)
	if err := os.WriteFile(filepath.Join(data, "hud", "command-card.toml"), []byte(bad), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(data)
	if err != nil {
		t.Fatal(err)
	}
	got := checkData(data, files, "")
	t.Logf("FSV bad command-card findings=%v", got)
	if len(got) != 1 || got[0].Rule != "COMMAND-CARD" || !strings.Contains(got[0].Msg, "teleport") {
		t.Fatalf("bad command-card opcode should be rejected, got %v", got)
	}
}

func TestDataMapDataScopedPassAndRejectFSV(t *testing.T) {
	root := t.TempDir()
	data := filepath.Join(root, "data")
	mapDir := filepath.Join(data, "maps", "test64")
	writeTinyMapData(t, root, mapDir, true)
	files, err := listFiles(mapDir)
	if err != nil {
		t.Fatal(err)
	}
	for i := range files {
		files[i] = filepath.ToSlash(filepath.Join("maps/test64", filepath.FromSlash(files[i])))
	}
	got := checkData(data, files, "maps/test64")
	t.Logf("FSV scoped mapdata pass findings=%v", got)
	if len(got) != 0 {
		t.Fatalf("scoped mapdata should pass without global hud/locale files, got %v", got)
	}

	root = t.TempDir()
	data = filepath.Join(root, "data")
	mapDir = filepath.Join(data, "maps", "test64")
	writeTinyMapData(t, root, mapDir, false)
	files, err = listFiles(mapDir)
	if err != nil {
		t.Fatal(err)
	}
	for i := range files {
		files[i] = filepath.ToSlash(filepath.Join("maps/test64", filepath.FromSlash(files[i])))
	}
	got = checkData(data, files, "maps/test64")
	t.Logf("FSV scoped mapdata missing asset findings=%v", got)
	if len(got) != 1 || got[0].Rule != "MAPDATA" || !strings.Contains(got[0].Msg, "tree_single_A.glb") {
		t.Fatalf("missing map doodad asset should be MAPDATA, got %v", got)
	}
}

func newDataFixture(t *testing.T) (root, data string) {
	t.Helper()
	root = t.TempDir()
	data = filepath.Join(root, "data")
	if err := os.MkdirAll(filepath.Join(data, "locale"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(data, "hud"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(data, "hud", "command-card.toml"), []byte(goodCommandCardTOML(t)), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "litd", "render", "hud"), 0o755); err != nil {
		t.Fatal(err)
	}
	return root, data
}

func writeTinyMapData(t *testing.T, root, mapDir string, withAsset bool) {
	t.Helper()
	if err := os.MkdirAll(mapDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if withAsset {
		asset := filepath.Join(root, "assets", "kaykit-hexagon", "tree_single_A.glb")
		if err := os.MkdirAll(filepath.Dir(asset), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(asset, []byte("stub"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	files := map[string]string{
		"terrain.toml": `version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]
`,
		"pathing.txt": "3*4\n3*4\n3*4\n3*4\n",
		"cliff.txt":   "0*4\n0*4\n0*4\n0*4\n",
		"height.txt":  "0*2\n0*2\n",
		"splat.txt":   "255,0,0,0\n",
		"doodads.toml": `[[doodad]]
id = 1
asset = "kaykit-hexagon/tree_single_A.glb"
cell = [1, 1]
rotation = 0
destructible = true
`,
	}
	for name, body := range files {
		if err := os.WriteFile(filepath.Join(mapDir, name), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func writeLocale(t *testing.T, data, tag, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(data, "locale", tag+".toml"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func goodLocaleTOML(t *testing.T) string {
	t.Helper()
	return readRepoText(t, filepath.Join("..", "..", "data", "locale", "en.toml"))
}

func pseudoLocaleTOML(t *testing.T) string {
	t.Helper()
	return readRepoText(t, filepath.Join("..", "..", "data", "locale", "xx.toml"))
}

func goodCommandCardTOML(t *testing.T) string {
	t.Helper()
	return readRepoText(t, filepath.Join("..", "..", "data", "hud", "command-card.toml"))
}

func readRepoText(t *testing.T, path string) string {
	t.Helper()
	blob, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(blob)
}

// TestSoundClassUnclassifiedRejectedFSV — #428 fail-closed gate: a sound entry
// lacking domain/priority classification is a build error with a precise message,
// same posture as the glTF catalog and map tables. A fully-classified table is
// accepted (the gate is not blanket-blind). SoT = the findings assetcheck emits
// over data/audio (the data-table validation pass).
func TestSoundClassUnclassifiedRejectedFSV(t *testing.T) {
	write := func(body string) []finding {
		t.Helper()
		root := t.TempDir()
		data := filepath.Join(root, "data")
		if err := os.MkdirAll(filepath.Join(data, "audio"), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(data, "audio", "sounds.toml"), []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		files, err := listFiles(data)
		if err != nil {
			t.Fatal(err)
		}
		return checkData(data, files, "audio")
	}

	// Valid: fully-classified entry → no SOUND-CLASS finding.
	if got := write("[[sound]]\ncue=\"footman_attack\"\ndomain=\"world\"\npriority=\"attackimpact\"\nogg=\"sfx/a.ogg\"\n"); rulesContain(got, "SOUND-CLASS") {
		t.Fatalf("classified sound table produced SOUND-CLASS findings: %+v", got)
	}

	// Invalid: entry with no domain → SOUND-CLASS build error, exact message.
	got := write("[[sound]]\ncue=\"footman_attack\"\npriority=\"attackimpact\"\nogg=\"sfx/a.ogg\"\n")
	if len(got) != 1 || got[0].Rule != "SOUND-CLASS" || !strings.Contains(got[0].Msg, `sound "footman_attack" has missing/invalid domain`) {
		t.Fatalf("unclassified sound should be rejected with one SOUND-CLASS finding, got %+v", got)
	}
	t.Logf("FSV #428 assetcheck gate: unclassified sound rejected — [%s] %s", got[0].Rule, got[0].Msg)
}
