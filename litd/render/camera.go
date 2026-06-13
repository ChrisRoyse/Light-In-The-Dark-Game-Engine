package render

import (
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/math32"
)

const (
	RTSCameraPitchFromVerticalDeg float32 = 34
	RTSCameraYawDeg               float32 = 0
	RTSCameraRollDeg              float32 = 0
	RTSCameraFOVDeg               float32 = 45
	RTSCameraDefaultZoom          float32 = 1650
)

type Vec3Snapshot struct {
	X float32 `json:"x"`
	Y float32 `json:"y"`
	Z float32 `json:"z"`
}

type RTSCameraConfig struct {
	Aspect               float32
	Anchor               math32.Vector3
	Zoom                 float32
	ZoomMin              float32
	ZoomMax              float32
	PitchFromVerticalDeg float32
	YawDeg               float32
	FOVDeg               float32
	Near                 float32
	Far                  float32
}

type RTSCameraSnapshot struct {
	Projection           string       `json:"projection"`
	Aspect               float32      `json:"aspect"`
	Anchor               Vec3Snapshot `json:"anchor"`
	Eye                  Vec3Snapshot `json:"eye"`
	Target               Vec3Snapshot `json:"target"`
	UnitOffset           Vec3Snapshot `json:"unitOffset"`
	ZoomRequested        float32      `json:"zoomRequested"`
	Zoom                 float32      `json:"zoom"`
	ZoomMin              float32      `json:"zoomMin"`
	ZoomDefault          float32      `json:"zoomDefault"`
	ZoomMax              float32      `json:"zoomMax"`
	OrthoSize            float32      `json:"orthoSize"`
	OrthoSizeMin         float32      `json:"orthoSizeMin"`
	OrthoSizeMax         float32      `json:"orthoSizeMax"`
	OrthoSizeScale       float32      `json:"orthoSizeScale"`
	PitchFromVerticalDeg float32      `json:"pitchFromVerticalDeg"`
	YawDeg               float32      `json:"yawDeg"`
	RollDeg              float32      `json:"rollDeg"`
	FOVDeg               float32      `json:"fovDeg"`
	Near                 float32      `json:"near"`
	Far                  float32      `json:"far"`
}

type RTSCameraLockProbe struct {
	Before                        RTSCameraSnapshot `json:"before"`
	AttemptedYawDeg               float32           `json:"attemptedYawDeg"`
	AttemptedPitchFromVerticalDeg float32           `json:"attemptedPitchFromVerticalDeg"`
	AttemptedRollDeg              float32           `json:"attemptedRollDeg"`
	After                         RTSCameraSnapshot `json:"after"`
	Unchanged                     bool              `json:"unchanged"`
}

type RTSCameraDump struct {
	Snapshot         RTSCameraSnapshot   `json:"snapshot"`
	LockProbe        RTSCameraLockProbe  `json:"lockProbe"`
	Footprint        RTSCameraFootprint  `json:"footprint"`
	ProjectionParity RTSCameraParity     `json:"projectionParity"`
	PickParity       RTSCameraPickParity `json:"pickParity"`
	OK               bool                `json:"ok"`
	Errors           []string            `json:"errors,omitempty"`
}

type RTSCamera struct {
	Camera *camera.Camera

	cfg           RTSCameraConfig
	zoomRequested float32
	zoom          float32
	orthoScale    float32
	unitOffset    math32.Vector3
	eye           math32.Vector3
	target        math32.Vector3
	up            math32.Vector3
}

func DefaultRTSCameraConfig(aspect float32) RTSCameraConfig {
	zoomMin := RTSCameraDefaultZoom * 0.7
	zoomMax := RTSCameraDefaultZoom * 1.4
	return RTSCameraConfig{
		Aspect:               aspect,
		Zoom:                 RTSCameraDefaultZoom,
		ZoomMin:              zoomMin,
		ZoomMax:              zoomMax,
		PitchFromVerticalDeg: RTSCameraPitchFromVerticalDeg,
		YawDeg:               RTSCameraYawDeg,
		FOVDeg:               RTSCameraFOVDeg,
		Near:                 0.25 * zoomMin,
		Far:                  1.6 * zoomMax,
	}
}

func NewRTSCamera(cfg RTSCameraConfig) *RTSCamera {
	cfg = normalizeRTSCameraConfig(cfg)
	r := &RTSCamera{
		cfg:           cfg,
		zoomRequested: cfg.Zoom,
		zoom:          clampZoom(cfg.Zoom, cfg.ZoomMin, cfg.ZoomMax),
		up:            math32.Vector3{Y: 1},
	}
	r.unitOffset = fixedRTSCameraUnitOffset(cfg.PitchFromVerticalDeg, cfg.YawDeg)
	r.Camera = camera.NewPerspective(cfg.Aspect, cfg.Near, cfg.Far, cfg.FOVDeg, camera.Vertical)
	r.Apply()
	r.calibrateOrthographicSizeScale()
	r.applyProjectionSize()
	return r
}

func (r *RTSCamera) SetAspect(aspect float32) {
	r.cfg.Aspect = aspect
	r.Camera.SetAspect(aspect)
	r.calibrateOrthographicSizeScale()
	r.applyProjectionSize()
}

func (r *RTSCamera) SetAnchor(anchor math32.Vector3) {
	r.cfg.Anchor = anchor
	r.Apply()
}

func (r *RTSCamera) SetZoomRequested(zoom float32) {
	r.zoomRequested = zoom
	r.zoom = clampZoom(zoom, r.cfg.ZoomMin, r.cfg.ZoomMax)
	r.Apply()
}

func (r *RTSCamera) Apply() {
	r.target = r.cfg.Anchor
	r.eye.Set(
		r.cfg.Anchor.X+r.unitOffset.X*r.zoom,
		r.cfg.Anchor.Y+r.unitOffset.Y*r.zoom,
		r.cfg.Anchor.Z+r.unitOffset.Z*r.zoom,
	)
	r.Camera.SetPosition(r.eye.X, r.eye.Y, r.eye.Z)
	r.Camera.LookAt(&r.target, &r.up)
	r.applyProjectionSize()
}

func (r *RTSCamera) TrySetAngles(_, _, _ float32) bool {
	return false
}

func (r *RTSCamera) Snapshot() RTSCameraSnapshot {
	return RTSCameraSnapshot{
		Projection:           projectionName(r.Camera.Projection()),
		Aspect:               r.cfg.Aspect,
		Anchor:               vec3Snapshot(r.cfg.Anchor),
		Eye:                  vec3Snapshot(r.eye),
		Target:               vec3Snapshot(r.target),
		UnitOffset:           vec3Snapshot(r.unitOffset),
		ZoomRequested:        r.zoomRequested,
		Zoom:                 r.zoom,
		ZoomMin:              r.cfg.ZoomMin,
		ZoomDefault:          RTSCameraDefaultZoom,
		ZoomMax:              r.cfg.ZoomMax,
		OrthoSize:            r.Camera.Size(),
		OrthoSizeMin:         r.orthographicSizeForZoom(r.cfg.ZoomMin),
		OrthoSizeMax:         r.orthographicSizeForZoom(r.cfg.ZoomMax),
		OrthoSizeScale:       r.orthographicSizeScale(),
		PitchFromVerticalDeg: r.cfg.PitchFromVerticalDeg,
		YawDeg:               r.cfg.YawDeg,
		RollDeg:              RTSCameraRollDeg,
		FOVDeg:               r.cfg.FOVDeg,
		Near:                 r.cfg.Near,
		Far:                  r.cfg.Far,
	}
}

func (r *RTSCamera) DumpWithLockProbe(yawDeg, pitchFromVerticalDeg, rollDeg float32) RTSCameraDump {
	return r.DumpWithLockProbeForViewport(yawDeg, pitchFromVerticalDeg, rollDeg, 0, 0)
}

func (r *RTSCamera) DumpWithLockProbeForViewport(yawDeg, pitchFromVerticalDeg, rollDeg float32, width, height int) RTSCameraDump {
	before := r.Snapshot()
	_ = r.TrySetAngles(yawDeg, pitchFromVerticalDeg, rollDeg)
	r.Apply()
	after := r.Snapshot()
	probe := RTSCameraLockProbe{
		Before:                        before,
		AttemptedYawDeg:               yawDeg,
		AttemptedPitchFromVerticalDeg: pitchFromVerticalDeg,
		AttemptedRollDeg:              rollDeg,
		After:                         after,
		Unchanged:                     snapshotsEqual(before, after),
	}
	footprint, footprintOK := r.GroundFootprint()
	parity := r.ProjectionParityFootprints()
	pickParity := r.PickParityForViewport(width, height)
	dump := RTSCameraDump{
		Snapshot:         after,
		LockProbe:        probe,
		Footprint:        footprint,
		ProjectionParity: parity,
		PickParity:       pickParity,
		OK:               true,
	}
	if after.Zoom < after.ZoomMin || after.Zoom > after.ZoomMax {
		dump.OK = false
		dump.Errors = append(dump.Errors, "zoom outside clamp")
	}
	if after.Projection == "orthographic" && (after.OrthoSize < after.OrthoSizeMin || after.OrthoSize > after.OrthoSizeMax) {
		dump.OK = false
		dump.Errors = append(dump.Errors, "orthographic size outside clamp")
	}
	if after.PitchFromVerticalDeg != RTSCameraPitchFromVerticalDeg || after.YawDeg != RTSCameraYawDeg || after.RollDeg != RTSCameraRollDeg {
		dump.OK = false
		dump.Errors = append(dump.Errors, "camera angles mutated")
	}
	if !probe.Unchanged {
		dump.OK = false
		dump.Errors = append(dump.Errors, "script angle mutation changed camera")
	}
	if !footprintOK || footprint.Area <= 0 {
		dump.OK = false
		dump.Errors = append(dump.Errors, "ground footprint unavailable")
	}
	if !parity.OK {
		dump.OK = false
		dump.Errors = append(dump.Errors, "projection footprint parity failed")
	}
	if !pickParity.SameCell {
		dump.OK = false
		dump.Errors = append(dump.Errors, "projection pick parity failed")
	}
	return dump
}

func normalizeRTSCameraConfig(cfg RTSCameraConfig) RTSCameraConfig {
	if cfg.Aspect <= 0 || math32.IsNaN(cfg.Aspect) {
		cfg.Aspect = 16.0 / 9.0
	}
	if cfg.ZoomMin <= 0 || math32.IsNaN(cfg.ZoomMin) {
		cfg.ZoomMin = RTSCameraDefaultZoom * 0.7
	}
	if cfg.ZoomMax < cfg.ZoomMin || math32.IsNaN(cfg.ZoomMax) {
		cfg.ZoomMax = RTSCameraDefaultZoom * 1.4
	}
	if cfg.Zoom <= 0 || math32.IsNaN(cfg.Zoom) {
		cfg.Zoom = RTSCameraDefaultZoom
	}
	if cfg.PitchFromVerticalDeg == 0 || math32.IsNaN(cfg.PitchFromVerticalDeg) {
		cfg.PitchFromVerticalDeg = RTSCameraPitchFromVerticalDeg
	}
	if math32.IsNaN(cfg.YawDeg) {
		cfg.YawDeg = RTSCameraYawDeg
	}
	if cfg.FOVDeg <= 0 || math32.IsNaN(cfg.FOVDeg) {
		cfg.FOVDeg = RTSCameraFOVDeg
	}
	if cfg.Near <= 0 || math32.IsNaN(cfg.Near) {
		cfg.Near = 0.25 * cfg.ZoomMin
	}
	if cfg.Far <= cfg.Near || math32.IsNaN(cfg.Far) {
		cfg.Far = 1.6 * cfg.ZoomMax
	}
	return cfg
}

func fixedRTSCameraUnitOffset(pitchFromVerticalDeg, yawDeg float32) math32.Vector3 {
	pitch := math32.DegToRad(pitchFromVerticalDeg)
	yaw := math32.DegToRad(yawDeg)
	horizontal := math32.Sin(pitch)
	return math32.Vector3{
		X: horizontal * math32.Sin(yaw),
		Y: math32.Cos(pitch),
		Z: horizontal * math32.Cos(yaw),
	}
}

func clampZoom(zoom, min, max float32) float32 {
	if math32.IsNaN(zoom) {
		return RTSCameraDefaultZoom
	}
	if zoom < min {
		return min
	}
	if zoom > max {
		return max
	}
	return zoom
}

func projectionName(proj camera.Projection) string {
	if proj == camera.Orthographic {
		return "orthographic"
	}
	return "perspective"
}

func vec3Snapshot(v math32.Vector3) Vec3Snapshot {
	return Vec3Snapshot{X: v.X, Y: v.Y, Z: v.Z}
}

func snapshotsEqual(a, b RTSCameraSnapshot) bool {
	return a == b
}
