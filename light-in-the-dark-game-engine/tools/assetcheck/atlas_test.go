package main

// #32 atlas/texture FSV. SoT = assetcheck findings over synthetic GLBs with
// real embedded PNGs whose dimensions we control via tinyPNG. Dimensions are
// cross-checked against the raw IHDR bytes (imageDims), so the count is
// X+X=Y verifiable: a PNG encoded at 1024×1024 decodes to exactly 1024×1024.

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"image"
	"image/color"
	"image/png"
	"testing"
)

// tinyPNG encodes a real w×h PNG. A pixel offset keeps otherwise-identical
// sizes byte-distinct when we need "unique" textures.
func tinyPNG(t *testing.T, w, h, salt int) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	img.Set(0, 0, color.RGBA{uint8(salt), uint8(salt >> 8), 0, 255})
	var b bytes.Buffer
	if err := png.Encode(&b, img); err != nil {
		t.Fatal(err)
	}
	return b.Bytes()
}

// glbWithTextures builds a GLB embedding the given images in its BIN chunk,
// each referenced by an image→bufferView. This is the real on-disk container
// the gate parses.
func glbWithTextures(t *testing.T, imgs [][]byte) []byte {
	t.Helper()
	var bin bytes.Buffer
	var views []map[string]any
	for _, im := range imgs {
		off := bin.Len()
		bin.Write(im)
		for bin.Len()%4 != 0 {
			bin.WriteByte(0)
		}
		views = append(views, map[string]any{"buffer": 0, "byteOffset": off, "byteLength": len(im)})
	}
	var images []map[string]any
	for i := range imgs {
		images = append(images, map[string]any{"mimeType": "image/png", "bufferView": i, "name": "tex"})
	}
	doc := map[string]any{
		"asset":       map[string]any{"version": "2.0"},
		"bufferViews": views,
		"images":      images,
		"buffers":     []map[string]any{{"byteLength": bin.Len()}},
	}
	return assembleGLB(t, doc, bin.Bytes())
}

// assembleGLB writes a JSON chunk + optional BIN chunk into a GLB container.
func assembleGLB(t *testing.T, doc map[string]any, bin []byte) []byte {
	t.Helper()
	j := mustJSON(t, doc)
	for len(j)%4 != 0 {
		j = append(j, ' ')
	}
	for len(bin)%4 != 0 {
		bin = append(bin, 0)
	}
	var b bytes.Buffer
	w := func(v uint32) { b.Write([]byte{byte(v), byte(v >> 8), byte(v >> 16), byte(v >> 24)}) }
	total := 12 + 8 + len(j)
	if len(bin) > 0 {
		total += 8 + len(bin)
	}
	w(glbMagic)
	w(2)
	w(uint32(total))
	w(uint32(len(j)))
	w(glbChunkJSON)
	b.Write(j)
	if len(bin) > 0 {
		w(uint32(len(bin)))
		w(glbChunkBIN)
		b.Write(bin)
	}
	return b.Bytes()
}

func mustJSON(t *testing.T, doc map[string]any) []byte {
	t.Helper()
	j, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	return j
}

// TestImageDimsCrossCheck — independent decode: a PNG encoded at a known size
// reports exactly that size from its IHDR bytes.
func TestImageDimsCrossCheck(t *testing.T) {
	for _, c := range []struct{ w, h int }{{1, 1}, {1024, 1024}, {2048, 512}, {256, 256}} {
		b := tinyPNG(t, c.w, c.h, 0)
		gw, gh, err := imageDims(b)
		if err != nil {
			t.Fatalf("%dx%d: %v", c.w, c.h, err)
		}
		// raw IHDR cross-check (PNG: width@16, height@20, big-endian)
		rawW := int(b[16])<<24 | int(b[17])<<16 | int(b[18])<<8 | int(b[19])
		rawH := int(b[20])<<24 | int(b[21])<<16 | int(b[22])<<8 | int(b[23])
		t.Logf("FSV imageDims %dx%d: decoded=%dx%d rawIHDR=%dx%d", c.w, c.h, gw, gh, rawW, rawH)
		if gw != c.w || gh != c.h || rawW != c.w || rawH != c.h {
			t.Fatalf("dim mismatch want %dx%d got dec %dx%d raw %dx%d", c.w, c.h, gw, gh, rawW, rawH)
		}
	}
}

// TestAtlasSizeBoundary — 1024² passes (inclusive), 2048×1024 fails, non-square
// 2048×512 fails on the long edge. SoT = atlas findings from the real GLB.
func TestAtlasSizeBoundary(t *testing.T) {
	cases := []struct {
		rel     string
		w, h    int
		wantBad bool
	}{
		{"units/ok.glb", 1024, 1024, false},
		{"units/big.glb", 2048, 1024, true},
		{"units/wide.glb", 2048, 512, true},
		{"units/empty.glb", 0, 0, false}, // zero textures -> handled below
	}
	for _, c := range cases {
		f := newBudgetFixture(t)
		if c.rel == "units/empty.glb" {
			f.add(t, c.rel, buildGLB(t, gltfDoc(nil, nil)), "unit") // no images
		} else {
			f.add(t, c.rel, glbWithTextures(t, [][]byte{tinyPNG(t, c.w, c.h, 0)}), "unit")
		}
		raw, notes := f.run(t, newWaiverSet())
		var got []finding
		for _, fd := range raw {
			if len(fd.Rule) >= 5 && fd.Rule[:5] == "ATLAS" {
				got = append(got, fd)
			}
		}
		t.Logf("FSV atlas %s (%dx%d): findings=%v notes=%v", c.rel, c.w, c.h, got, notes)
		if c.wantBad {
			if len(got) != 1 || got[0].Rule != "ATLAS-SIZE" {
				t.Fatalf("%s: want one ATLAS-SIZE, got %v", c.rel, got)
			}
		} else if len(got) != 0 {
			t.Fatalf("%s: want pass, got %v", c.rel, got)
		}
	}
}

// TestAtlasUniqueVsShared — two models embedding the SAME atlas bytes are
// shared (pass); a third model with its own texture is flagged ATLAS-UNIQUE.
func TestAtlasUniqueVsShared(t *testing.T) {
	f := newBudgetFixture(t)
	shared := tinyPNG(t, 1024, 1024, 1) // identical bytes in two models
	f.add(t, "units/a.glb", glbWithTextures(t, [][]byte{shared}), "unit")
	f.add(t, "units/b.glb", glbWithTextures(t, [][]byte{shared}), "unit")
	f.add(t, "units/lone.glb", glbWithTextures(t, [][]byte{tinyPNG(t, 256, 256, 2)}), "unit")

	// sanity: the two shared blobs really are identical bytes.
	h1 := sha256.Sum256(shared)
	t.Logf("FSV shared atlas sha256=%s (referenced by a.glb and b.glb)", hex.EncodeToString(h1[:])[:16])

	raw, notes := f.run(t, newWaiverSet())
	var got []finding
	for _, fd := range raw {
		if len(fd.Rule) >= 5 && fd.Rule[:5] == "ATLAS" {
			got = append(got, fd)
		}
	}
	t.Logf("FSV unique-vs-shared: findings=%v notes=%v", got, notes)
	if len(got) != 1 || got[0].Rule != "ATLAS-UNIQUE" || got[0].Path != "units/lone.glb" {
		t.Fatalf("want one ATLAS-UNIQUE for lone.glb, got %v", got)
	}
}

// TestAtlasNoSharedNoFlag — a single textured model in a tree with no shared
// atlas is NOT flagged (sharing can't be expected of a lone model).
func TestAtlasNoSharedNoFlag(t *testing.T) {
	f := newBudgetFixture(t)
	f.add(t, "units/solo.glb", glbWithTextures(t, [][]byte{tinyPNG(t, 512, 512, 3)}), "unit")
	raw, notes := f.run(t, newWaiverSet())
	for _, fd := range raw {
		if len(fd.Rule) >= 5 && fd.Rule[:5] == "ATLAS" {
			t.Fatalf("lone textured model should not be flagged, got %v", raw)
		}
	}
	t.Logf("FSV no-shared-no-flag: notes=%v (no ATLAS findings)", notes)
}

// tinyPNGB64 is used by check_test's data-URI fixture.
func tinyPNGB64(t *testing.T, w, h int) string {
	return base64.StdEncoding.EncodeToString(tinyPNG(t, w, h, 0))
}
