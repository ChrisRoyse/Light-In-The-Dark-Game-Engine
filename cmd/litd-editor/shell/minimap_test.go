package shell

import (
	"image"
	"image/color"
	"path/filepath"
	"testing"
)

func TestEditorMinimapLiveUpdateUndoAndClickFSV(t *testing.T) {
	app := newCommandTestApp(t)
	if err := app.PutStartLocationCell(1, 1, 6); err != nil {
		t.Fatal(err)
	}
	if err := app.PutStartLocationCell(2, 6, 1); err != nil {
		t.Fatal(err)
	}
	beforeSnap := app.Snapshot()
	beforeImg := RenderImage(beforeSnap)
	sampleX, sampleY := 5, 4
	beforeColor := minimapPixelForCell(t, beforeImg, beforeSnap, sampleX, sampleY)
	t.Logf("FSV minimap before: sample[%d,%d]=%v starts=%+v camera=%v rect=%v", sampleX, sampleY, beforeColor, beforeSnap.World.Starts, beforeSnap.Camera.TargetCell, MinimapContentRect(beforeSnap))

	if err := app.SetTerrainBrush(BrushCliffRaise); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushSize(1); err != nil {
		t.Fatal(err)
	}
	if err := app.SetBrushStrength(1); err != nil {
		t.Fatal(err)
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		t.Fatal(err)
	}
	afterSnap := app.Snapshot()
	afterImg := RenderImage(afterSnap)
	afterColor := minimapPixelForCell(t, afterImg, afterSnap, sampleX, sampleY)
	p1x, p1y, ok := MinimapScreenPointForCell(afterSnap, 1, 6)
	if !ok {
		t.Fatal("missing minimap point for P1")
	}
	p2x, p2y, ok := MinimapScreenPointForCell(afterSnap, 6, 1)
	if !ok {
		t.Fatal("missing minimap point for P2")
	}
	p1Marker := afterImg.RGBAAt(p1x-3, p1y-3)
	p2Marker := afterImg.RGBAAt(p2x-3, p2y-3)
	t.Logf("FSV minimap after plateau: sample[%d,%d]=%v starts P1px=%d,%d marker=%v P2px=%d,%d marker=%v cliff=%s stack=%s",
		sampleX, sampleY, afterColor, p1x, p1y, p1Marker, p2x, p2y, p2Marker, cliffJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if rgbaEqual(beforeColor, afterColor) {
		t.Fatalf("plateau did not change minimap pixel at %d,%d: before=%v after=%v", sampleX, sampleY, beforeColor, afterColor)
	}
	if !rgbaEqual(p1Marker, brass) || !rgbaEqual(p2Marker, brass) {
		t.Fatalf("start markers not drawn in brass: P1=%v P2=%v want=%v", p1Marker, p2Marker, brass)
	}

	if err := app.Undo(); err != nil {
		t.Fatal(err)
	}
	undoSnap := app.Snapshot()
	undoImg := RenderImage(undoSnap)
	undoColor := minimapPixelForCell(t, undoImg, undoSnap, sampleX, sampleY)
	t.Logf("FSV minimap after undo plateau: sample[%d,%d]=%v starts=%+v cliff=%s stack=%s", sampleX, sampleY, undoColor, undoSnap.World.Starts, cliffJSON(t, app), stackJSON(t, app.StackSnapshot()))
	if !rgbaEqual(beforeColor, undoColor) {
		t.Fatalf("undo did not restore minimap pixel at %d,%d: before=%v undo=%v", sampleX, sampleY, beforeColor, undoColor)
	}

	rect := MinimapContentRect(undoSnap)
	beforeClick := undoSnap.Camera.TargetCell
	if err := app.RecenterCameraFromMinimapPixel(rect.Min.X, rect.Min.Y); err != nil {
		t.Fatal(err)
	}
	afterClick := app.Snapshot().Camera.TargetCell
	t.Logf("FSV minimap corner click: click=%d,%d beforeCamera=%v afterCamera=%v", rect.Min.X, rect.Min.Y, beforeClick, afterClick)
	if afterClick != ([2]int{0, 0}) {
		t.Fatalf("corner minimap click target=%v, want 0,0", afterClick)
	}
	outsideErr := app.RecenterCameraFromMinimapPixel(rect.Min.X-1, rect.Min.Y-1)
	afterOutside := app.Snapshot().Camera.TargetCell
	t.Logf("FSV minimap outside click: click=%d,%d err=%v afterCamera=%v", rect.Min.X-1, rect.Min.Y-1, outsideErr, afterOutside)
	if outsideErr == nil || afterOutside != afterClick {
		t.Fatalf("outside minimap click should fail without moving camera: err=%v before=%v after=%v", outsideErr, afterClick, afterOutside)
	}
}

func TestEditorMinimapSmallLargeAspectFSV(t *testing.T) {
	for _, size := range []int{64, 256} {
		app := newTestApp(t)
		if err := app.NewProjectWithSize(filepath.Join(t.TempDir(), "world"), size, size); err != nil {
			t.Fatalf("new %dx%d world: %v", size, size, err)
		}
		snap := app.Snapshot()
		img := RenderImage(snap)
		rect := MinimapContentRect(snap)
		probe := minimapPixelForCell(t, img, snap, 10, 10)
		nonGraphite := countNonColor(img, rect, graphite)
		t.Logf("FSV minimap %dx%d: rect=%v probe[10,10]=%v nonGraphite=%d starts=%+v camera=%v", size, size, rect, probe, nonGraphite, snap.World.Starts, snap.Camera.TargetCell)
		if snap.World.Width != size || snap.World.Height != size {
			t.Fatalf("snapshot size = %dx%d, want %dx%d", snap.World.Width, snap.World.Height, size, size)
		}
		if rect.Dx() != minimapMapSize || rect.Dy() != minimapMapSize {
			t.Fatalf("square %dx%d map rect=%v, want %dx%d content", size, size, rect, minimapMapSize, minimapMapSize)
		}
		if rgbaEqual(probe, graphite) || nonGraphite == 0 {
			t.Fatalf("minimap %dx%d rendered blank probe=%v nonGraphite=%d", size, size, probe, nonGraphite)
		}
	}
}

func minimapPixelForCell(t *testing.T, img interface{ RGBAAt(int, int) color.RGBA }, snap Snapshot, x, y int) color.RGBA {
	t.Helper()
	px, py, ok := MinimapScreenPointForCell(snap, x, y)
	if !ok {
		t.Fatalf("minimap point for cell %d,%d unavailable in %dx%d map", x, y, snap.World.Width, snap.World.Height)
	}
	return img.RGBAAt(px, py)
}

func countNonColor(img interface{ RGBAAt(int, int) color.RGBA }, rect image.Rectangle, c color.RGBA) int {
	count := 0
	for y := rect.Min.Y; y < rect.Max.Y; y++ {
		for x := rect.Min.X; x < rect.Max.X; x++ {
			if !rgbaEqual(img.RGBAAt(x, y), c) {
				count++
			}
		}
	}
	return count
}

func rgbaEqual(a, b color.RGBA) bool {
	return a.R == b.R && a.G == b.G && a.B == b.B && a.A == b.A
}
