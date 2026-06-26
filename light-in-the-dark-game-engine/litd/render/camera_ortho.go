package render

import (
	"fmt"
	"strings"

	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/math32"
)

const RTSCameraPickCellSize float32 = 128

type RTSCameraProjection string

const (
	RTSCameraProjectionPerspective  RTSCameraProjection = "perspective"
	RTSCameraProjectionOrthographic RTSCameraProjection = "orthographic"
)

type RTSCameraFootprint struct {
	Projection string          `json:"projection"`
	Corners    [4]Vec3Snapshot `json:"corners"`
	MinX       float32         `json:"minX"`
	MaxX       float32         `json:"maxX"`
	MinZ       float32         `json:"minZ"`
	MaxZ       float32         `json:"maxZ"`
	Area       float32         `json:"area"`
	OK         bool            `json:"ok"`
}

type RTSCameraParity struct {
	Perspective      RTSCameraFootprint `json:"perspective"`
	Orthographic     RTSCameraFootprint `json:"orthographic"`
	AreaDeltaPct     float32            `json:"areaDeltaPct"`
	AreaTolerancePct float32            `json:"areaTolerancePct"`
	OK               bool               `json:"ok"`
}

type RTSCameraScreenPixel struct {
	X int `json:"x"`
	Y int `json:"y"`
}

type RTSCameraNDCPoint struct {
	X float32 `json:"x"`
	Y float32 `json:"y"`
}

type RTSCameraGroundPick struct {
	World     Vec3Snapshot `json:"world"`
	Projected Vec3Snapshot `json:"projected"`
	CellX     int          `json:"cellX"`
	CellZ     int          `json:"cellZ"`
	OK        bool         `json:"ok"`
}

type RTSCameraPickParity struct {
	Screen       RTSCameraScreenPixel `json:"screen"`
	NDC          RTSCameraNDCPoint    `json:"ndc"`
	CellSize     float32              `json:"cellSize"`
	Perspective  RTSCameraGroundPick  `json:"perspective"`
	Orthographic RTSCameraGroundPick  `json:"orthographic"`
	SameCell     bool                 `json:"sameCell"`
}

func ParseRTSCameraProjection(text string) (RTSCameraProjection, error) {
	switch strings.ToLower(strings.TrimSpace(text)) {
	case "", "persp", "perspective":
		return RTSCameraProjectionPerspective, nil
	case "ortho", "orthographic":
		return RTSCameraProjectionOrthographic, nil
	default:
		return "", fmt.Errorf("unknown camera projection %q", text)
	}
}

func (r *RTSCamera) SetProjectionMode(mode RTSCameraProjection) error {
	switch mode {
	case RTSCameraProjectionPerspective:
		r.Camera.SetProjection(camera.Perspective)
	case RTSCameraProjectionOrthographic:
		r.Camera.SetProjection(camera.Orthographic)
	default:
		return fmt.Errorf("unknown camera projection %q", mode)
	}
	r.applyProjectionSize()
	return nil
}

func (r *RTSCamera) applyProjectionSize() {
	if r == nil || r.Camera == nil {
		return
	}
	r.Camera.SetFov(r.cfg.FOVDeg)
	r.Camera.UpdateSize(r.orthographicUpdateDistance(r.zoom))
}

func (r *RTSCamera) calibrateOrthographicSizeScale() {
	if r == nil || r.Camera == nil {
		return
	}
	currentProjection := r.Camera.Projection()
	currentSize := r.Camera.Size()

	r.Camera.SetProjection(camera.Perspective)
	r.Camera.SetFov(r.cfg.FOVDeg)
	perspective, perspectiveOK := r.GroundFootprint()

	r.Camera.SetProjection(camera.Orthographic)
	r.Camera.UpdateSize(r.zoom)
	orthographic, orthographicOK := r.GroundFootprint()

	r.Camera.SetProjection(currentProjection)
	r.Camera.SetSize(currentSize)

	if !perspectiveOK || !orthographicOK || perspective.Area <= 0 || orthographic.Area <= 0 {
		r.orthoScale = 1
		return
	}
	r.orthoScale = math32.Sqrt(perspective.Area / orthographic.Area)
	if r.orthoScale <= 0 || math32.IsNaN(r.orthoScale) {
		r.orthoScale = 1
	}
}

func (r *RTSCamera) orthographicSizeScale() float32 {
	if r == nil || r.orthoScale <= 0 || math32.IsNaN(r.orthoScale) {
		return 1
	}
	return r.orthoScale
}

func (r *RTSCamera) orthographicUpdateDistance(zoom float32) float32 {
	return zoom * r.orthographicSizeScale()
}

func (r *RTSCamera) orthographicSizeForZoom(zoom float32) float32 {
	return 2 * r.orthographicUpdateDistance(zoom) * math32.Tan(math32.DegToRad(r.cfg.FOVDeg*0.5))
}

func (r *RTSCamera) GroundFootprint() (RTSCameraFootprint, bool) {
	points := [4]math32.Vector3{}
	ok := true
	ok = r.groundPointAtNDC(-1, -1, &points[0]) && ok
	ok = r.groundPointAtNDC(1, -1, &points[1]) && ok
	ok = r.groundPointAtNDC(1, 1, &points[2]) && ok
	ok = r.groundPointAtNDC(-1, 1, &points[3]) && ok
	if !ok {
		return RTSCameraFootprint{Projection: projectionName(r.Camera.Projection())}, false
	}

	fp := RTSCameraFootprint{
		Projection: projectionName(r.Camera.Projection()),
		Corners: [4]Vec3Snapshot{
			vec3Snapshot(points[0]),
			vec3Snapshot(points[1]),
			vec3Snapshot(points[2]),
			vec3Snapshot(points[3]),
		},
		MinX: points[0].X,
		MaxX: points[0].X,
		MinZ: points[0].Z,
		MaxZ: points[0].Z,
		OK:   true,
	}
	var shoelace float32
	for i := 0; i < len(points); i++ {
		p := points[i]
		next := points[(i+1)%len(points)]
		if p.X < fp.MinX {
			fp.MinX = p.X
		}
		if p.X > fp.MaxX {
			fp.MaxX = p.X
		}
		if p.Z < fp.MinZ {
			fp.MinZ = p.Z
		}
		if p.Z > fp.MaxZ {
			fp.MaxZ = p.Z
		}
		shoelace += p.X*next.Z - next.X*p.Z
	}
	fp.Area = math32.Abs(shoelace) * 0.5
	fp.OK = fp.Area > 0 && !math32.IsNaN(fp.Area)
	return fp, fp.OK
}

func (r *RTSCamera) ProjectionParityFootprints() RTSCameraParity {
	currentProjection := r.Camera.Projection()
	currentSize := r.Camera.Size()

	r.Camera.SetProjection(camera.Perspective)
	r.applyProjectionSize()
	perspective, perspectiveOK := r.GroundFootprint()

	r.Camera.SetProjection(camera.Orthographic)
	r.applyProjectionSize()
	orthographic, orthographicOK := r.GroundFootprint()

	r.Camera.SetProjection(currentProjection)
	r.Camera.SetSize(currentSize)

	parity := RTSCameraParity{
		Perspective:      perspective,
		Orthographic:     orthographic,
		AreaTolerancePct: 1,
	}
	if perspectiveOK && orthographicOK && perspective.Area > 0 {
		parity.AreaDeltaPct = math32.Abs(orthographic.Area-perspective.Area) / perspective.Area * 100
		parity.OK = parity.AreaDeltaPct <= parity.AreaTolerancePct
	}
	return parity
}

func (r *RTSCamera) PickParityForViewport(width, height int) RTSCameraPickParity {
	if width <= 0 {
		width = 960
	}
	if height <= 0 {
		height = 540
	}
	screen := RTSCameraScreenPixel{X: width / 2, Y: height / 2}
	ndc := RTSCameraNDCPoint{
		X: 2*float32(screen.X)/float32(width) - 1,
		Y: -2*float32(screen.Y)/float32(height) + 1,
	}

	currentProjection := r.Camera.Projection()
	currentSize := r.Camera.Size()

	r.Camera.SetProjection(camera.Perspective)
	r.applyProjectionSize()
	perspective := r.groundPickAtNDC(ndc.X, ndc.Y)

	r.Camera.SetProjection(camera.Orthographic)
	r.applyProjectionSize()
	orthographic := r.groundPickAtNDC(ndc.X, ndc.Y)

	r.Camera.SetProjection(currentProjection)
	r.Camera.SetSize(currentSize)

	return RTSCameraPickParity{
		Screen:       screen,
		NDC:          ndc,
		CellSize:     RTSCameraPickCellSize,
		Perspective:  perspective,
		Orthographic: orthographic,
		SameCell: perspective.OK && orthographic.OK &&
			perspective.CellX == orthographic.CellX &&
			perspective.CellZ == orthographic.CellZ,
	}
}

func (r *RTSCamera) groundPickAtNDC(ndcX, ndcY float32) RTSCameraGroundPick {
	var point math32.Vector3
	if !r.groundPointAtNDC(ndcX, ndcY, &point) {
		return RTSCameraGroundPick{}
	}
	projected := point
	r.Camera.Project(&projected)
	return RTSCameraGroundPick{
		World:     vec3Snapshot(point),
		Projected: vec3Snapshot(projected),
		CellX:     stableCameraCell(point.X),
		CellZ:     stableCameraCell(point.Z),
		OK:        true,
	}
}

func (r *RTSCamera) groundPointAtNDC(ndcX, ndcY float32, out *math32.Vector3) bool {
	if r == nil || r.Camera == nil || out == nil {
		return false
	}
	r.Camera.UpdateMatrixWorld()
	near := math32.Vector3{X: ndcX, Y: ndcY, Z: -1}
	far := math32.Vector3{X: ndcX, Y: ndcY, Z: 1}
	r.Camera.Unproject(&near)
	r.Camera.Unproject(&far)

	var dir math32.Vector3
	dir.SubVectors(&far, &near)
	if math32.Abs(dir.Y) <= 0.000001 {
		return false
	}
	t := (r.cfg.Anchor.Y - near.Y) / dir.Y
	*out = near
	out.Add(dir.MultiplyScalar(t))
	return true
}

func stableCameraCell(v float32) int {
	if math32.Abs(v) < 0.001 {
		v = 0
	}
	return int(math32.Floor(v / RTSCameraPickCellSize))
}
