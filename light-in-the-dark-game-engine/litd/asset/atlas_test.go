package asset

import (
	"image"
	"image/color"
	"testing"
)

func TestAtlasPresetSizesFSV(t *testing.T) {
	for _, tc := range []struct {
		text string
		size int
	}{
		{text: "high", size: 1024},
		{text: "medium", size: 512},
		{text: "low", size: 256},
		{text: " HIGH ", size: 1024},
	} {
		preset, err := ParseAtlasPreset(tc.text)
		if err != nil {
			t.Fatalf("ParseAtlasPreset(%q): %v", tc.text, err)
		}
		got, err := preset.Size()
		t.Logf("FSV atlas preset input=%q parsed=%s size=%d err=%v", tc.text, preset, got, err)
		if err != nil || got != tc.size {
			t.Fatalf("preset %q size=%d, %v; want %d nil", tc.text, got, err, tc.size)
		}
	}
	if _, err := ParseAtlasPreset("ultra"); err == nil {
		t.Fatal("invalid preset accepted")
	} else {
		t.Logf("FSV invalid preset input=%q err=%v", "ultra", err)
	}
}

func TestAtlasSourceValidationFSV(t *testing.T) {
	ok := testAtlasImage(1024, 1024)
	src, err := NewAtlasSource("vigil.atlas.png", ok)
	t.Logf("FSV atlas source valid BEFORE name=vigil.atlas.png dims=1024x1024 AFTER src=%+v err=%v", srcSummary(src), err)
	if err != nil || src.Width != 1024 || src.Height != 1024 || src.SHA256 == "" {
		t.Fatalf("valid atlas rejected: src=%+v err=%v", src, err)
	}

	for _, tc := range []struct {
		name string
		w, h int
	}{
		{name: "too-small.atlas.png", w: 512, h: 512},
		{name: "non-square.atlas.png", w: 1024, h: 512},
		{name: "npot.atlas.png", w: 1000, h: 1024},
	} {
		before := tc
		_, err := NewAtlasSource(tc.name, testAtlasImage(tc.w, tc.h))
		t.Logf("FSV atlas source invalid BEFORE=%+v AFTER err=%v", before, err)
		if err == nil {
			t.Fatalf("%s %dx%d accepted", tc.name, tc.w, tc.h)
		}
	}
}

func TestAtlasDownsampleBoxFSV(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 1024, 1024))
	fillRect(img, image.Rect(0, 0, 512, 512), color.RGBA{255, 0, 0, 255})
	fillRect(img, image.Rect(512, 0, 1024, 512), color.RGBA{0, 255, 0, 255})
	fillRect(img, image.Rect(0, 512, 512, 1024), color.RGBA{0, 0, 255, 255})
	fillRect(img, image.Rect(512, 512, 1024, 1024), color.RGBA{255, 255, 0, 255})
	src, err := NewAtlasSource("quadrants.atlas.png", img)
	if err != nil {
		t.Fatal(err)
	}

	for _, preset := range []AtlasPreset{AtlasPresetHigh, AtlasPresetMedium, AtlasPresetLow} {
		out, upload, err := BuildAtlasUpload(src, preset)
		t.Logf("FSV atlas downsample preset=%s upload=%+v err=%v", preset, upload, err)
		if err != nil {
			t.Fatalf("%s: %v", preset, err)
		}
		if out.Bounds().Dx() != upload.Width || out.Bounds().Dy() != upload.Height {
			t.Fatalf("upload dims disagree with image: image=%v upload=%+v", out.Bounds(), upload)
		}
		wantSize, _ := preset.Size()
		if upload.Width != wantSize || upload.Height != wantSize {
			t.Fatalf("%s upload size=%dx%d want %dx%d", preset, upload.Width, upload.Height, wantSize, wantSize)
		}
		checkPixel(t, out, upload.Width/4, upload.Height/4, color.RGBA{255, 0, 0, 255})
		checkPixel(t, out, 3*upload.Width/4, upload.Height/4, color.RGBA{0, 255, 0, 255})
		checkPixel(t, out, upload.Width/4, 3*upload.Height/4, color.RGBA{0, 0, 255, 255})
		checkPixel(t, out, 3*upload.Width/4, 3*upload.Height/4, color.RGBA{255, 255, 0, 255})
	}
}

func TestAtlasUploadEdgesFSV(t *testing.T) {
	_, _, nilErr := BuildAtlasUpload(nil, AtlasPresetHigh)
	t.Logf("FSV atlas upload nil source BEFORE nil AFTER err=%v", nilErr)
	if nilErr == nil {
		t.Fatal("nil source accepted")
	}

	src, err := NewAtlasSource("edge.atlas.png", testAtlasImage(1024, 1024))
	if err != nil {
		t.Fatal(err)
	}
	_, _, presetErr := BuildAtlasUpload(src, AtlasPreset("tiny"))
	t.Logf("FSV atlas upload invalid preset BEFORE preset=tiny AFTER err=%v", presetErr)
	if presetErr == nil {
		t.Fatal("invalid preset accepted")
	}
}

func testAtlasImage(w, h int) *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	fillRect(img, img.Bounds(), color.RGBA{32, 64, 96, 255})
	return img
}

func fillRect(img *image.RGBA, r image.Rectangle, c color.RGBA) {
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			img.SetRGBA(x, y, c)
		}
	}
}

func checkPixel(t *testing.T, img *image.RGBA, x, y int, want color.RGBA) {
	t.Helper()
	got := img.RGBAAt(x, y)
	t.Logf("FSV atlas pixel (%d,%d) got=%+v want=%+v", x, y, got, want)
	if got != want {
		t.Fatalf("pixel (%d,%d) got %+v want %+v", x, y, got, want)
	}
}

func srcSummary(src *AtlasSource) any {
	if src == nil {
		return nil
	}
	return struct {
		Name   string
		Width  int
		Height int
		SHA256 string
	}{src.Name, src.Width, src.Height, src.SHA256[:16]}
}
