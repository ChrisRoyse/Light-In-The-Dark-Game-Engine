package hud

import (
	"errors"
	"math"
	"testing"
)

type testRegion struct {
	name   string
	anchor Anchor
	ref    RefRect
}

var testRegions = []testRegion{
	{name: "top-left", anchor: AnchorTopLeft, ref: RefRect{X: 16, Y: 16, W: 220, H: 30}},
	{name: "top-right", anchor: AnchorTopRight, ref: RefRect{X: 980, Y: 16, W: 280, H: 30}},
	{name: "bottom-left", anchor: AnchorBottomLeft, ref: RefRect{X: 16, Y: 584, W: 160, H: 120}},
	{name: "bottom-center", anchor: AnchorBottom, ref: RefRect{X: 560, Y: 594, W: 160, H: 110}},
	{name: "bottom-right", anchor: AnchorBottomRight, ref: RefRect{X: 1044, Y: 584, W: 220, H: 120}},
}

func almostEqual(a, b float64) bool {
	return math.Abs(a-b) < 0.000001
}

func TestCanvasScaleFactorsFSV(t *testing.T) {
	cases := []struct {
		w, h     int
		uiScale  float64
		want     float64
		iconCell int
	}{
		{w: 1024, h: 768, uiScale: 1, want: 1.0666666667, iconCell: 34},
		{w: 1366, h: 768, uiScale: 1, want: 1.0666666667, iconCell: 34},
		{w: 1920, h: 1080, uiScale: 1, want: 1.5, iconCell: 48},
		{w: 2560, h: 1080, uiScale: 1, want: 1.5, iconCell: 48},
		{w: 3840, h: 2160, uiScale: 1, want: 3, iconCell: 96},
	}
	for _, tc := range cases {
		c, err := NewCanvas(tc.w, tc.h, tc.uiScale)
		if err != nil {
			t.Fatalf("%dx%d NewCanvas: %v", tc.w, tc.h, err)
		}
		t.Logf("FSV canvas %dx%d BEFORE empty AFTER scale=%.6f icon32=%d", tc.w, tc.h, c.Scale, c.Snap(32))
		if !almostEqual(c.Scale, tc.want) || c.Snap(32) != tc.iconCell {
			t.Fatalf("%dx%d scale/icon got %.10f/%d want %.10f/%d", tc.w, tc.h, c.Scale, c.Snap(32), tc.want, tc.iconCell)
		}
	}
}

func TestCanvasAnchorsAndPixelSnapFSV(t *testing.T) {
	c, err := NewCanvas(1920, 1080, 1)
	if err != nil {
		t.Fatal(err)
	}
	left := c.Place(AnchorTopLeft, RefRect{X: 16, Y: 16, W: 160, H: 40})
	right := c.Place(AnchorTopRight, RefRect{X: 1100, Y: 16, W: 160, H: 40})
	center := c.Place(AnchorBottom, RefRect{X: 560, Y: 600, W: 160, H: 100})
	t.Logf("FSV anchors 1920x1080 left=%+v right=%+v center=%+v", left, right, center)

	if left != (Rect{X: 24, Y: 24, W: 240, H: 60}) {
		t.Fatalf("left anchor: %+v", left)
	}
	if right != (Rect{X: 1650, Y: 24, W: 240, H: 60}) {
		t.Fatalf("right anchor: %+v", right)
	}
	if center != (Rect{X: 840, Y: 900, W: 240, H: 150}) {
		t.Fatalf("bottom center anchor: %+v", center)
	}
}

func TestCanvasRejectsUnsupportedSizesFSV(t *testing.T) {
	valid, validErr := NewCanvas(1024, 768, 1)
	narrow, narrowErr := NewCanvas(1023, 768, 1)
	short, shortErr := NewCanvas(1280, 720, 1)
	t.Logf("FSV unsupported sizes valid=%+v validErr=%v narrowErr=%v shortErr=%v", valid, validErr, narrowErr, shortErr)

	if validErr != nil {
		t.Fatalf("1024x768 should be supported: %v", validErr)
	}
	if !errors.Is(narrowErr, ErrUnsupportedSize) {
		t.Fatalf("1023x768 should fail closed, got %v narrow=%+v", narrowErr, narrow)
	}
	if !errors.Is(shortErr, ErrUnsupportedSize) {
		t.Fatalf("1280x720 should fail below minimum height, got %v short=%+v", shortErr, short)
	}
}

func TestCanvasUIScaleClampFSV(t *testing.T) {
	low, _ := NewCanvas(1920, 1080, 0.1)
	normal, _ := NewCanvas(1920, 1080, 1.25)
	high, _ := NewCanvas(1920, 1080, 2)
	nan, _ := NewCanvas(1920, 1080, math.NaN())
	t.Logf("FSV uiScale low=%+v normal=%+v high=%+v nan=%+v", low, normal, high, nan)

	if low.UIScale != MinUIScale || !almostEqual(low.Scale, 1.125) {
		t.Fatalf("low clamp wrong: %+v", low)
	}
	if normal.UIScale != 1.25 || !almostEqual(normal.Scale, 1.875) {
		t.Fatalf("normal scale wrong: %+v", normal)
	}
	if high.UIScale != MaxUIScale || !almostEqual(high.Scale, 2.25) {
		t.Fatalf("high clamp wrong: %+v", high)
	}
	if nan.UIScale != 1 || !almostEqual(nan.Scale, 1.5) {
		t.Fatalf("NaN scale wrong: %+v", nan)
	}
}

func TestCanvasRegionsNoOverlapOrOffscreenFSV(t *testing.T) {
	cases := []struct {
		w, h    int
		uiScale float64
	}{
		{w: 1024, h: 768, uiScale: 1.5},
		{w: 1366, h: 768, uiScale: 1},
		{w: 1920, h: 1080, uiScale: 1},
		{w: 2560, h: 1080, uiScale: 1},
		{w: 3840, h: 2160, uiScale: 1},
	}
	for _, tc := range cases {
		c, err := NewCanvas(tc.w, tc.h, tc.uiScale)
		if err != nil {
			t.Fatalf("%dx%d NewCanvas: %v", tc.w, tc.h, err)
		}
		rects := make([]Rect, 0, len(testRegions))
		for _, region := range testRegions {
			r := c.Place(region.anchor, region.ref)
			t.Logf("FSV region %dx%d uiScale=%.2f %s=%+v", tc.w, tc.h, tc.uiScale, region.name, r)
			if !r.Inside(c.Width, c.Height) {
				t.Fatalf("%s offscreen at %dx%d: %+v", region.name, tc.w, tc.h, r)
			}
			for i, prev := range rects {
				if r.Overlaps(prev) {
					t.Fatalf("%s overlaps %s at %dx%d: %+v vs %+v", region.name, testRegions[i].name, tc.w, tc.h, r, prev)
				}
			}
			rects = append(rects, r)
		}
	}
}

func TestCanvasLiveResizeRecomputesFSV(t *testing.T) {
	before, err := NewCanvas(1920, 1080, 1)
	if err != nil {
		t.Fatal(err)
	}
	after, err := NewCanvas(1366, 768, 1)
	if err != nil {
		t.Fatal(err)
	}
	ref := RefRect{X: 1044, Y: 584, W: 220, H: 120}
	beforeRect := before.Place(AnchorBottomRight, ref)
	afterRect := after.Place(AnchorBottomRight, ref)
	t.Logf("FSV live resize BEFORE canvas=%+v rect=%+v AFTER canvas=%+v rect=%+v", before, beforeRect, after, afterRect)

	if beforeRect == afterRect {
		t.Fatalf("resize did not recompute rect: before=%+v after=%+v", beforeRect, afterRect)
	}
	if !afterRect.Inside(after.Width, after.Height) {
		t.Fatalf("post-resize rect offscreen: %+v", afterRect)
	}
}

func TestCanvasPlaceZeroAlloc(t *testing.T) {
	c, err := NewCanvas(2560, 1080, 1)
	if err != nil {
		t.Fatal(err)
	}
	ref := RefRect{X: 1044, Y: 584, W: 220, H: 120}
	allocs := testing.AllocsPerRun(1000, func() {
		_ = c.Place(AnchorBottomRight, ref)
	})
	t.Logf("FSV canvas Place allocs/op=%v canvas=%+v", allocs, c)
	if allocs != 0 {
		t.Fatalf("Place allocated: %v", allocs)
	}
}
