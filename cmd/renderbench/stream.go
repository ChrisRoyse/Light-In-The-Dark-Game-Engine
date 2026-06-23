package main

import (
	"encoding/json"
	"fmt"
	"hash/fnv"
	"math"
	"os"
	"path/filepath"
	"sort"

	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/math32"
)

// camKey is one recorded camera command: the (anchor, zoom, projection) triple
// the #233 spec requires. Anchor is the world look-at point, zoom is the eye
// distance along the fixed RTS view ray, projection is persp|ortho. Units are
// static across the stream — the camera path is the recorded workload. The
// segments define their *visible* unit counts (max-battle = 500 visible), so the
// path keeps that set framed; visible/culled stay constant by design and the
// per-frame series instead exposes frame-ms (compile spike → steady) and allocs.
type camKey struct {
	Anchor     [3]float32 `json:"anchor"`
	Zoom       float32    `json:"zoom"`
	Projection string     `json:"projection"`
}

// benchStream is the recorded, replay-style workload persisted under
// data/maps/bench/<segment>.stream.json. Replaying the same file twice produces
// identical draw/visible/culled columns (determinism); only frame ms varies.
type benchStream struct {
	Segment    string   `json:"segment"`
	Frames     int      `json:"frames"`
	DtPerFrame float32  `json:"dtPerFrame"`
	Camera     []camKey `json:"camera"`
}

// benchViewRay is the fixed RTS eye direction (unit vector toward the eye from
// the anchor): normalize(0, 1350, 920), so zoom=1633.7 reproduces the static
// single-frame camera at (0,1350,920).
var benchViewRay = math32.Vector3{X: 0, Y: 1350, Z: 920}

func init() { benchViewRay.Normalize() }

const benchStreamFrames = 24

// generateStream builds the deterministic recorded path for a segment: a slow
// zoom-in (far→near) with a small lateral pan over the framed army. projection is
// stamped on every key; the replay may override it per combo (the anchor/zoom
// path is the recorded part, reused across the four material×projection combos).
func generateStream(segment, projection string, frames int) benchStream {
	s := benchStream{Segment: segment, Frames: frames, DtPerFrame: 1.0 / 30.0}
	s.Camera = make([]camKey, frames)
	for f := 0; f < frames; f++ {
		t := float32(0)
		if frames > 1 {
			t = float32(f) / float32(frames-1)
		}
		zoom := 1640 - 640*t    // 1640 (far) → 1000 (near): zoom-in
		anchorX := -120 + 240*t // pan left → right
		s.Camera[f] = camKey{
			Anchor:     [3]float32{anchorX, 0, 0},
			Zoom:       zoom,
			Projection: projection,
		}
	}
	return s
}

// validate fails closed on a malformed stream rather than replaying garbage.
func (s benchStream) validate() error {
	if s.Frames <= 0 {
		return fmt.Errorf("stream %q: frames = %d, want > 0", s.Segment, s.Frames)
	}
	if len(s.Camera) != s.Frames {
		return fmt.Errorf("stream %q: %d camera keys for %d frames", s.Segment, len(s.Camera), s.Frames)
	}
	if s.DtPerFrame <= 0 {
		return fmt.Errorf("stream %q: dtPerFrame = %v, want > 0", s.Segment, s.DtPerFrame)
	}
	for i, k := range s.Camera {
		if k.Zoom <= 0 {
			return fmt.Errorf("stream %q frame %d: zoom = %v, want > 0", s.Segment, i, k.Zoom)
		}
		if k.Projection != projPersp && k.Projection != projOrtho {
			return fmt.Errorf("stream %q frame %d: bad projection %q", s.Segment, i, k.Projection)
		}
	}
	return nil
}

// streamPath is the canonical on-disk location of a segment's recorded stream.
func streamPath(segment string) string {
	return filepath.Join("data", "maps", "bench", segment+".stream.json")
}

// loadStream reads a recorded stream; generates (without writing) the canonical
// path if the file is absent so a fresh checkout is still reproducible.
func loadStream(segment, projection string) (benchStream, error) {
	p := streamPath(segment)
	b, err := os.ReadFile(p)
	if err != nil {
		if os.IsNotExist(err) {
			return generateStream(segment, projection, benchStreamFrames), nil
		}
		return benchStream{}, err
	}
	var s benchStream
	if err := json.Unmarshal(b, &s); err != nil {
		return benchStream{}, fmt.Errorf("stream %s: %w", p, err)
	}
	// The recorded file holds the canonical anchor/zoom path; the combo may run a
	// different projection, so stamp the requested one onto every key.
	for i := range s.Camera {
		s.Camera[i].Projection = projection
	}
	if err := s.validate(); err != nil {
		return benchStream{}, err
	}
	return s, nil
}

// write persists the recorded stream (used to author the committed fixtures).
func (s benchStream) write(segment string) error {
	p := streamPath(segment)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return err
	}
	b, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(p, append(b, '\n'), 0o644)
}

// applyCamKey positions the camera for one recorded key: eye = anchor + viewRay*zoom,
// look at the anchor. For ortho the size tracks zoom so the zoom-in actually zooms.
func applyCamKey(cam *camera.Camera, k camKey) {
	anchor := math32.Vector3{X: k.Anchor[0], Y: k.Anchor[1], Z: k.Anchor[2]}
	eye := math32.Vector3{
		X: anchor.X + benchViewRay.X*k.Zoom,
		Y: anchor.Y + benchViewRay.Y*k.Zoom,
		Z: anchor.Z + benchViewRay.Z*k.Zoom,
	}
	cam.SetPositionVec(&eye)
	cam.LookAt(&anchor, &math32.Vector3{X: 0, Y: 1, Z: 0})
	if cam.Projection() == camera.Orthographic {
		// 1100 size at the far zoom (1640) → proportional shrink on zoom-in.
		cam.SetSize(k.Zoom * (1100.0 / 1640.0))
	}
}

// frameStat is one per-frame measurement. The GL-derived columns are pointers so
// a headless -nogl run leaves them nil (rendered as null = n/a) instead of
// fabricating a measured value — the #233 spec's fail-closed honesty rule.
type frameStat struct {
	Frame        int     `json:"frame"`
	FrameMS      float64 `json:"frameMs"`
	OpaqueDraws  *int    `json:"opaqueDraws"`
	StateChanges *int    `json:"stateChanges"`
	Visible      *int    `json:"visible"`
	Culled       *int    `json:"culled"`
	Allocs       int64   `json:"allocs"`
	// VoiceCount is always n/a here: the render bench runs no audio admission
	// manager, so a measured voice count does not exist (wiring tracked by #538).
	VoiceCount *int `json:"voiceCount"`
}

// streamSummary aggregates the per-frame series for CI thresholds + human reading.
type streamSummary struct {
	Frames         int     `json:"frames"`
	GL             bool    `json:"gl"`
	MinFrameMS     float64 `json:"minFrameMs"`
	AvgFrameMS     float64 `json:"avgFrameMs"`
	P99FrameMS     float64 `json:"p99FrameMs"`
	MaxOpaqueDraws *int    `json:"maxOpaqueDraws"` // nil under -nogl
	MaxAllocs      int64   `json:"maxAllocs"`
}

// summarize reduces the per-frame series. gl=false marks the draw peak n/a rather
// than reporting a fabricated zero.
func summarize(stats []frameStat, gl bool) streamSummary {
	sum := streamSummary{Frames: len(stats), GL: gl}
	if len(stats) == 0 {
		return sum
	}
	ms := make([]float64, len(stats))
	sum.MinFrameMS = math.MaxFloat64
	var total float64
	maxDraws := -1
	for i, fs := range stats {
		ms[i] = fs.FrameMS
		total += fs.FrameMS
		if fs.FrameMS < sum.MinFrameMS {
			sum.MinFrameMS = fs.FrameMS
		}
		if fs.Allocs > sum.MaxAllocs {
			sum.MaxAllocs = fs.Allocs
		}
		if gl && fs.OpaqueDraws != nil && *fs.OpaqueDraws > maxDraws {
			maxDraws = *fs.OpaqueDraws
		}
	}
	sum.AvgFrameMS = total / float64(len(stats))
	sort.Float64s(ms)
	idx := int(math.Ceil(0.99*float64(len(ms)))) - 1
	if idx < 0 {
		idx = 0
	}
	sum.P99FrameMS = ms[idx]
	if gl && maxDraws >= 0 {
		sum.MaxOpaqueDraws = &maxDraws
	}
	return sum
}

func intPtr(v int) *int { return &v }

// computeStreamHash is the deterministic "sim hash" the #233 spec requires: a
// digest of the recorded command stream plus the scene definition it drives
// (draw target + overlay counts). A GL run and a headless -nogl run executing the
// same stream MUST produce the same hash — that is the parity check, independent
// of the GL-only frame ms / draw / visible / culled columns.
func computeStreamHash(segment, variant, matPath string, expectedWorldDraws, bars, blips int, s benchStream) string {
	h := fnv.New64a()
	fmt.Fprintf(h, "%s|%s|%s|%d|%d|%d|%d|%g",
		segment, variant, matPath, expectedWorldDraws, bars, blips, s.Frames, s.DtPerFrame)
	for _, k := range s.Camera {
		fmt.Fprintf(h, "|%g,%g,%g,%g,%s", k.Anchor[0], k.Anchor[1], k.Anchor[2], k.Zoom, k.Projection)
	}
	return fmt.Sprintf("%016x", h.Sum64())
}
