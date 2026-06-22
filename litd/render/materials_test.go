package render

import (
	"image"
	"image/color"
	"testing"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
)

func TestAtlasMaterialCacheIdentityFSV(t *testing.T) {
	src := mustRenderAtlasSource(t, "vigil.atlas.png", color.RGBA{80, 130, 210, 255})
	cache := NewAtlasMaterialCache()
	t.Logf("FSV atlas material cache BEFORE count=%d", cache.Count())

	high1, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	high2, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material high AFTER first=%+v secondSame=%v count=%d", cache.Snapshot(high1), high1 == high2, cache.Count())
	if high1 != high2 || cache.Count() != 1 {
		t.Fatalf("same atlas+preset must reuse one material: same=%v count=%d", high1 == high2, cache.Count())
	}
	if high1.Texture.Width() != 1024 || high1.Texture.Height() != 1024 {
		t.Fatalf("high texture dims = %dx%d", high1.Texture.Width(), high1.Texture.Height())
	}

	medium, err := cache.Material(src, litasset.AtlasPresetMedium)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material medium AFTER snapshot=%+v count=%d", cache.Snapshot(medium), cache.Count())
	if medium == high1 || cache.Count() != 2 || medium.Texture.Width() != 512 || medium.Texture.Height() != 512 {
		t.Fatalf("medium material wrong: sameAsHigh=%v count=%d dims=%dx%d", medium == high1, cache.Count(), medium.Texture.Width(), medium.Texture.Height())
	}

	src2 := mustRenderAtlasSource(t, "ember.atlas.png", color.RGBA{210, 110, 60, 255})
	other, err := cache.Material(src2, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material second atlas AFTER snapshot=%+v count=%d", cache.Snapshot(other), cache.Count())
	if other == high1 || cache.Count() != 3 {
		t.Fatalf("different atlas should create a separate material: same=%v count=%d", other == high1, cache.Count())
	}
}

func TestAtlasMaterialRuntimeSwitchFSV(t *testing.T) {
	src := mustRenderAtlasSource(t, "switch.atlas.png", color.RGBA{120, 180, 80, 255})
	cache := NewAtlasMaterialCache()
	high, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	medium, err := cache.Material(src, litasset.AtlasPresetMedium)
	if err != nil {
		t.Fatal(err)
	}
	before := cache.Count()
	highAgain, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	low, err := cache.Material(src, litasset.AtlasPresetLow)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas preset switch BEFORE count=%d high=%p medium=%p AFTER highAgain=%p low=%p count=%d", before, high.Material, medium.Material, highAgain.Material, low.Material, cache.Count())
	if highAgain != high {
		t.Fatal("switching back to high created a new material")
	}
	if before != 2 || cache.Count() != 3 || low.Texture.Width() != 256 {
		t.Fatalf("switch counts/dims wrong before=%d after=%d low=%dx%d", before, cache.Count(), low.Texture.Width(), low.Texture.Height())
	}
}

func TestAtlasMaterialEdgesFSV(t *testing.T) {
	cache := NewAtlasMaterialCache()
	if _, err := cache.Material(nil, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil source accepted")
	} else {
		t.Logf("FSV atlas material nil source BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
	src := mustRenderAtlasSource(t, "edges.atlas.png", color.RGBA{32, 64, 96, 255})
	if _, err := cache.Material(src, litasset.AtlasPreset("bad")); err == nil {
		t.Fatal("invalid preset accepted")
	} else {
		t.Logf("FSV atlas material bad preset BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
	var nilCache *AtlasMaterialCache
	if _, err := nilCache.Material(src, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil cache accepted")
	} else {
		t.Logf("FSV atlas material nil cache AFTER err=%v", err)
	}
}

func mustRenderAtlasSource(t *testing.T, name string, c color.RGBA) *litasset.AtlasSource {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, 1024, 1024))
	for y := 0; y < 1024; y++ {
		for x := 0; x < 1024; x++ {
			img.SetRGBA(x, y, c)
		}
	}
	src, err := litasset.NewAtlasSource(name, img)
	if err != nil {
		t.Fatal(err)
	}
	return src
}
