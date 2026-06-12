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
	return check(f.dir, files)
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

func TestEmptyAssetsDirPasses(t *testing.T) {
	f := newFixture(t)
	if got := f.run(t); len(got) != 0 {
		t.Fatalf("empty assets dir should pass, got %v", got)
	}
}
