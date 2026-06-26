package hud

import (
	"bytes"
	"encoding/json"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestPerfOverlayZeroDataFirstFrameFSV(t *testing.T) {
	overlay := NewDefaultPerfOverlay()
	before := overlay.Snapshot()
	t.Logf("BEFORE zero-data overlay: visible=%v samples=%d drawCalls=%d", before.Visible, before.Samples, before.DrawCalls)

	overlay.SetVisible(true)
	overlay.Update(PerfInput{})
	after := overlay.Snapshot()
	t.Logf("AFTER zero-data overlay: visible=%v samples=%d drawCalls=%d rows=%+v", after.Visible, after.Samples, after.DrawCalls, after.Rows)

	if !after.Visible || after.DrawCalls != 1 || after.DrawCalls > after.DrawCallBudget {
		t.Fatalf("overlay visibility/draw budget wrong: %+v", after)
	}
	for _, row := range after.Rows {
		if row.Current != 0 || row.Worst != 0 {
			t.Fatalf("zero-data row %s current/worst=%d/%d", row.Name, row.Current, row.Worst)
		}
	}

	path := filepath.Join(t.TempDir(), "perf-zero.png")
	writeOverlayPNG(t, path, overlay)
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("SoT zero-data PNG path=%s bytes=%d header=% x", path, len(raw), raw[:8])
	if !bytes.HasPrefix(raw, []byte{0x89, 'P', 'N', 'G'}) {
		t.Fatalf("overlay PNG source-of-truth has wrong header: % x", raw[:8])
	}
}

func TestPerfOverlayCounterRowsAndBudgetFSV(t *testing.T) {
	overlay := NewDefaultPerfOverlay()
	overlay.SetVisible(true)
	in := PerfInput{
		Tick:        42,
		Frame:       99,
		TickNS:      2_250_000,
		PhaseNS:     [perfOverlayPhases]int64{100_000, 200_000, 300_000, 1_500_000, 400_000, 250_000, 125_000},
		FrameNS:     16_670_000,
		FPS:         60,
		DrawCalls:   18,
		Batches:     11,
		Instances:   500,
		AllocsFrame: 0,
		AllocsTick:  0,
		HeapBytes:   42 << 20,
		Units:       500,
		Missiles:    2,
		Buffs:       1,
	}
	overlay.Update(in)
	snap := overlay.Snapshot()
	t.Logf("AFTER populated overlay: frame=%d tick=%d rows=%+v", snap.Frame, snap.Tick, snap.Rows)

	assertRow(t, snap, "TICK", 225, 225, "TICK 2.25MS W 2.25")
	assertRow(t, snap, "PHASE", 150, 150, "PHASE 1.50MS W 1.50")
	assertRow(t, snap, "FRAME", 1667, 1667, "FRAME 16.67MS 60FPS W 16.67")
	assertRow(t, snap, "DRAW", 18, 18, "DRAW 18 W 18")
	assertRow(t, snap, "ALLOC", 0, 0, "ALLOC 0 W 0")
	assertRow(t, snap, "HEAP", 420, 420, "HEAP 42.0MB W 42.0")
	assertRow(t, snap, "ENT", 503, 503, "ENT 500/2/1 W 503")
	if snap.DrawCalls != 1 || snap.DrawCalls > PerfOverlayDrawCallCap {
		t.Fatalf("overlay draw calls=%d budget=%d", snap.DrawCalls, PerfOverlayDrawCallCap)
	}

	path := filepath.Join(t.TempDir(), "perf-snapshot.json")
	raw, err := json.MarshalIndent(snap, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, raw, 0o644); err != nil {
		t.Fatal(err)
	}
	readback, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("SoT populated snapshot %s:\n%s", path, readback)
	if !strings.Contains(string(readback), `"name": "DRAW"`) || !strings.Contains(string(readback), `"current": 18`) {
		t.Fatalf("snapshot readback missing draw row: %s", readback)
	}
}

func TestPerfOverlayToggleSpamFSV(t *testing.T) {
	overlay := NewDefaultPerfOverlay()
	t.Logf("BEFORE toggle spam: visible=%v toggles=%d", overlay.Visible(), overlay.ToggleCount())
	for i := 0; i < 20; i++ {
		overlay.Toggle()
	}
	t.Logf("AFTER 20 toggles: visible=%v toggles=%d", overlay.Visible(), overlay.ToggleCount())
	if overlay.Visible() || overlay.ToggleCount() != 20 {
		t.Fatalf("toggle spam state wrong visible=%v toggles=%d", overlay.Visible(), overlay.ToggleCount())
	}
	overlay.Toggle()
	overlay.Update(PerfInput{Frame: 21, DrawCalls: 3})
	snap := overlay.Snapshot()
	t.Logf("AFTER final visible toggle: visible=%v toggles=%d drawCalls=%d rows=%+v", snap.Visible, snap.Toggles, snap.DrawCalls, snap.Rows)
	if !snap.Visible || snap.Toggles != 21 || snap.DrawCalls != 1 {
		t.Fatalf("final toggle state wrong: %+v", snap)
	}
}

func TestPerfOverlayStress500ZeroAllocFSV(t *testing.T) {
	overlay := NewDefaultPerfOverlay()
	overlay.SetVisible(true)
	in := PerfInput{
		TickNS:      9_800_000,
		PhaseNS:     [perfOverlayPhases]int64{200_000, 300_000, 400_000, 7_500_000, 800_000, 400_000, 200_000},
		FrameNS:     16_000_000,
		DrawCalls:   512,
		Batches:     512,
		Instances:   501,
		AllocsFrame: 0,
		HeapBytes:   96 << 20,
		Units:       500,
	}
	overlay.Update(in)
	before := overlay.Snapshot()
	t.Logf("BEFORE zero-alloc churn: samples=%d drawRow=%+v entRow=%+v", before.Samples, rowByName(before, "DRAW"), rowByName(before, "ENT"))

	allocs := testing.AllocsPerRun(1000, func() {
		in.Frame++
		in.Tick++
		in.DrawCalls++
		overlay.Update(in)
	})
	after := overlay.Snapshot()
	t.Logf("AFTER zero-alloc churn: allocs/op=%v samples=%d drawRow=%+v entRow=%+v", allocs, after.Samples, rowByName(after, "DRAW"), rowByName(after, "ENT"))

	if allocs != 0 {
		t.Fatalf("PerfOverlay.Update allocated %v/op; want 0", allocs)
	}
	if got := rowByName(after, "ENT"); got.Current != 500 || got.Worst != 500 {
		t.Fatalf("stress entity row=%+v want current/worst 500", got)
	}
	if got := rowByName(after, "DRAW"); got.Current <= 512 || got.Worst != got.Current {
		t.Fatalf("draw churn row not updated: %+v", got)
	}
}

func assertRow(t *testing.T, snap PerfOverlayDump, name string, current, worst int64, text string) {
	t.Helper()
	row := rowByName(snap, name)
	if row.Name == "" {
		t.Fatalf("row %s missing in %+v", name, snap.Rows)
	}
	if row.Current != current || row.Worst != worst || row.Text != text {
		t.Fatalf("row %s got current=%d worst=%d text=%q want current=%d worst=%d text=%q",
			name, row.Current, row.Worst, row.Text, current, worst, text)
	}
}

func rowByName(snap PerfOverlayDump, name string) PerfOverlayRowDump {
	for _, row := range snap.Rows {
		if row.Name == name {
			return row
		}
	}
	return PerfOverlayRowDump{}
}

func writeOverlayPNG(t *testing.T, path string, overlay *PerfOverlay) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	if err := png.Encode(f, overlay.Image()); err != nil {
		t.Fatal(err)
	}
}
