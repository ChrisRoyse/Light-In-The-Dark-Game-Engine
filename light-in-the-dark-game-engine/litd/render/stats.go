package render

import (
	"encoding/json"
	"io"
	"os"
	"path/filepath"

	"github.com/g3n/engine/renderer"
)

// FrameStats is the stable LitD render-stat surface used by dumps, gates,
// and debug overlays. Values describe the last rendered frame.
type FrameStats struct {
	GraphicMaterials int `json:"graphicMaterials"`
	Lights           int `json:"lights"`
	Panels           int `json:"panels"`
	Others           int `json:"others"`

	VisibleGraphics int `json:"visibleGraphics"`
	CulledGraphics  int `json:"culledGraphics"`

	DrawCalls            int `json:"drawCalls"`
	OpaqueDrawCalls      int `json:"opaqueDrawCalls"`
	TransparentDrawCalls int `json:"transparentDrawCalls"`
	GUIDrawCalls         int `json:"guiDrawCalls"`

	StateChanges      int `json:"stateChanges"`
	OpaqueStates      int `json:"opaqueStates"`
	TransparentStates int `json:"transparentStates"`
	GUIStates         int `json:"guiStates"`
}

// ReadFrameStats copies the renderer's most recent per-frame stats.
func ReadFrameStats(r *renderer.Renderer) FrameStats {
	return FromRendererStats(r.Stats())
}

// FromRendererStats maps the vendored renderer stats to the stable LitD schema.
func FromRendererStats(s renderer.Stats) FrameStats {
	return FrameStats{
		GraphicMaterials:     s.GraphicMats,
		Lights:               s.Lights,
		Panels:               s.Panels,
		Others:               s.Others,
		VisibleGraphics:      s.VisibleGraphics,
		CulledGraphics:       s.CulledGraphics,
		DrawCalls:            s.DrawCalls,
		OpaqueDrawCalls:      s.OpaqueDrawCalls,
		TransparentDrawCalls: s.TransparentDrawCalls,
		GUIDrawCalls:         s.GUIDrawCalls,
		StateChanges:         s.StateChanges,
		OpaqueStates:         s.OpaqueStates,
		TransparentStates:    s.TransparentStates,
		GUIStates:            s.GUIStates,
	}
}

// DumpFrameStats writes an indented JSON snapshot. Dumping is off the render
// hot path and may allocate.
func DumpFrameStats(w io.Writer, stats FrameStats) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(stats)
}

// DumpFrameStatsFile writes an indented JSON snapshot to path.
func DumpFrameStatsFile(path string, stats FrameStats) error {
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return DumpFrameStats(f, stats)
}
