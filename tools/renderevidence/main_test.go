package main

// FSV for the cheap render-evidence path (#516 leg 2). Synthetic images with
// known pixels => known checksum/grid/thumbnail (the X+X=Y discipline). SoT = the
// Evidence struct fields, asserted against hand-computed expectations.

import (
	"encoding/json"
	"image"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Regression for the duplicate-json-tag bug (vet caught it): Row/Col and
// Width/Height must each serialize under their own key, or the grid coordinates
// silently vanish from the evidence an agent reads.
func TestEvidenceJSONKeysFSV(t *testing.T) {
	b, err := json.Marshal(analyze(solid(16, 16, 1, 2, 3, 255), 2))
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)
	for _, key := range []string{`"width"`, `"height"`, `"row"`, `"col"`, `"hex"`, `"nonBlack"`} {
		if !strings.Contains(s, key) {
			t.Fatalf("evidence JSON missing key %s — duplicate-tag regression\n%s", key, s)
		}
	}
	t.Logf("FSV json keys: width/height/row/col/hex/nonBlack all present")
}

func solid(w, h int, r, g, b, a uint8) *image.RGBA {
	im := image.NewRGBA(image.Rect(0, 0, w, h))
	for i := 0; i < len(im.Pix); i += 4 {
		im.Pix[i], im.Pix[i+1], im.Pix[i+2], im.Pix[i+3] = r, g, b, a
	}
	return im
}

// A black left half, white right half — the grid must localize the split.
func leftBlackRightWhite(w, h int) *image.RGBA {
	im := image.NewRGBA(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			o := im.Pix[y*im.Stride+x*4:]
			if x >= w/2 {
				o[0], o[1], o[2], o[3] = 255, 255, 255, 255
			} else {
				o[3] = 255 // opaque black
			}
		}
	}
	return im
}

func TestBlackFrameFSV(t *testing.T) {
	ev := analyze(solid(64, 64, 0, 0, 0, 255), 4)
	t.Logf("FSV black: nonBlackFrac=%.3f regions=%d checksum=%s…", ev.NonBlackFrac, len(ev.Regions), ev.Checksum[:12])
	if ev.NonBlackFrac != 0 {
		t.Fatalf("black frame nonBlackFrac=%.3f, want 0", ev.NonBlackFrac)
	}
	if len(ev.Regions) != 16 {
		t.Fatalf("4x4 grid => 16 regions, got %d", len(ev.Regions))
	}
	for _, r := range ev.Regions {
		if r.Hex != "#000000" || r.NonBlack != 0 {
			t.Fatalf("black region %+v, want #000000 nonBlack 0", r)
		}
	}
}

func TestSolidRedFSV(t *testing.T) {
	ev := analyze(solid(40, 40, 255, 0, 0, 255), 4)
	t.Logf("FSV red: nonBlackFrac=%.3f region0=%s", ev.NonBlackFrac, ev.Regions[0].Hex)
	if ev.NonBlackFrac != 1 {
		t.Fatalf("solid red nonBlackFrac=%.3f, want 1", ev.NonBlackFrac)
	}
	for _, r := range ev.Regions {
		if r.Hex != "#ff0000" {
			t.Fatalf("red region hex=%s, want #ff0000", r.Hex)
		}
	}
}

func TestChecksumDeterministicAndSensitiveFSV(t *testing.T) {
	a := analyze(solid(32, 32, 10, 20, 30, 255), 2)
	b := analyze(solid(32, 32, 10, 20, 30, 255), 2)
	if a.Checksum != b.Checksum {
		t.Fatalf("same pixels, different checksum:\n %s\n %s", a.Checksum, b.Checksum)
	}
	t.Logf("FSV determinism: identical input => identical checksum %s…", a.Checksum[:12])

	// flip one pixel => checksum must move (sensitivity).
	im := solid(32, 32, 10, 20, 30, 255)
	im.Pix[4*100+1] ^= 0xFF
	c := analyze(im, 2)
	if c.Checksum == a.Checksum {
		t.Fatal("one changed pixel did not move the checksum")
	}
	t.Logf("FSV sensitivity: one pixel flipped => checksum %s… (changed)", c.Checksum[:12])
}

func TestGridLocalizesChangeFSV(t *testing.T) {
	ev := analyze(leftBlackRightWhite(64, 64), 4) // 4 cols: 0,1 black ; 2,3 white
	for _, r := range ev.Regions {
		wantLit := r.Col >= 2
		isLit := r.NonBlack > 0.5
		if isLit != wantLit {
			t.Fatalf("cell (%d,%d) nonBlack=%.2f hex=%s — want lit=%v", r.Row, r.Col, r.NonBlack, r.Hex, wantLit)
		}
	}
	t.Logf("FSV localization: left cols (0,1) dark, right cols (2,3) lit — grid pinpoints the split")
}

func TestThumbnailFSV(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "thumb.png")
	src := solid(1280, 720, 128, 64, 32, 255)
	tw, th, err := writeThumb(src, 128, p)
	if err != nil {
		t.Fatal(err)
	}
	if tw != 128 || th != 72 {
		t.Fatalf("thumb dims %dx%d, want 128x72 (16:9 preserved)", tw, th)
	}
	out, err := loadRGBA(p)
	if err != nil {
		t.Fatalf("reload thumb: %v", err)
	}
	if out.Rect.Dx() != 128 || out.Rect.Dy() != 72 {
		t.Fatalf("reloaded thumb %dx%d", out.Rect.Dx(), out.Rect.Dy())
	}
	// box-average of a solid image is that same color.
	c := out.Pix[:4]
	if c[0] != 128 || c[1] != 64 || c[2] != 32 {
		t.Fatalf("thumb pixel=%v, want [128 64 32]", c[:3])
	}
	if fi, _ := os.Stat(p); fi.Size() == 0 {
		t.Fatal("thumb file empty")
	}
	t.Logf("FSV thumbnail: 1280x720 -> 128x72 box-downscale, color preserved [128 64 32]")
}
