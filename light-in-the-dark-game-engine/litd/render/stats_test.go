package render

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/g3n/engine/renderer"
)

func TestFrameStatsFromRendererStatsFSV(t *testing.T) {
	before := FromRendererStats(renderer.Stats{})
	source := renderer.Stats{
		GraphicMats:          5,
		Lights:               2,
		Panels:               1,
		Others:               3,
		VisibleGraphics:      5,
		CulledGraphics:       1,
		DrawCalls:            7,
		OpaqueDrawCalls:      5,
		TransparentDrawCalls: 1,
		GUIDrawCalls:         1,
		StateChanges:         4,
		OpaqueStates:         2,
		TransparentStates:    1,
		GUIStates:            1,
	}
	after := FromRendererStats(source)
	t.Logf("FSV adapter before=%+v after=%+v", before, after)

	if after.GraphicMaterials != 5 || after.VisibleGraphics != 5 || after.CulledGraphics != 1 {
		t.Fatalf("object counters not copied: %+v", after)
	}
	if after.DrawCalls != 7 || after.OpaqueDrawCalls != 5 || after.TransparentDrawCalls != 1 || after.GUIDrawCalls != 1 {
		t.Fatalf("draw counters not copied: %+v", after)
	}
	if after.StateChanges != 4 || after.OpaqueStates != 2 || after.TransparentStates != 1 || after.GUIStates != 1 {
		t.Fatalf("state counters not copied: %+v", after)
	}
}

func TestFrameStatsDumpFileReadbackFSV(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "stats.json")
	stats := FrameStats{
		GraphicMaterials: 1,
		VisibleGraphics:  1,
		CulledGraphics:   0,
		DrawCalls:        1,
		OpaqueDrawCalls:  1,
		StateChanges:     1,
		OpaqueStates:     1,
	}
	before, beforeErr := os.ReadFile(path)
	t.Logf("FSV dump before path=%s exists=%v bytes=%q", path, beforeErr == nil, before)

	if err := DumpFrameStatsFile(path, stats); err != nil {
		t.Fatalf("DumpFrameStatsFile: %v", err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read stats source-of-truth: %v", err)
	}
	t.Logf("FSV dump after path=%s bytes=%s", path, data)

	var got FrameStats
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal dumped stats: %v", err)
	}
	if got != stats {
		t.Fatalf("dump readback got %+v want %+v", got, stats)
	}
}

func TestFrameStatsDumpEdgesFSV(t *testing.T) {
	cases := []struct {
		name string
		in   FrameStats
		want string
	}{
		{name: "empty", in: FrameStats{}, want: `"drawCalls": 0`},
		{name: "single-visible", in: FrameStats{VisibleGraphics: 1, DrawCalls: 1, OpaqueDrawCalls: 1}, want: `"visibleGraphics": 1`},
		{name: "culled", in: FrameStats{VisibleGraphics: 1, CulledGraphics: 1, DrawCalls: 1}, want: `"culledGraphics": 1`},
		{name: "state-split", in: FrameStats{StateChanges: 2, OpaqueStates: 1, TransparentStates: 1}, want: `"transparentStates": 1`},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var buf bytes.Buffer
			before := buf.String()
			if err := DumpFrameStats(&buf, tc.in); err != nil {
				t.Fatalf("DumpFrameStats: %v", err)
			}
			after := buf.String()
			t.Logf("FSV edge %s before=%q after=%s", tc.name, before, after)
			if !bytes.Contains(buf.Bytes(), []byte(tc.want)) {
				t.Fatalf("dump missing %s: %s", tc.want, after)
			}
		})
	}
}

func TestFrameStatsCopyZeroAlloc(t *testing.T) {
	source := renderer.Stats{
		GraphicMats:          5,
		VisibleGraphics:      5,
		CulledGraphics:       1,
		DrawCalls:            7,
		OpaqueDrawCalls:      5,
		TransparentDrawCalls: 1,
		GUIDrawCalls:         1,
		StateChanges:         4,
		OpaqueStates:         2,
		TransparentStates:    1,
		GUIStates:            1,
	}
	allocs := testing.AllocsPerRun(1000, func() {
		_ = FromRendererStats(source)
	})
	t.Logf("FSV FrameStats copy allocs/op=%v source=%+v", allocs, source)
	if allocs != 0 {
		t.Fatalf("FromRendererStats allocated: %v", allocs)
	}
}
