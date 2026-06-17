package render

// Per-map near/far clip-plane computation (camera-and-culling.md §5.1–5.3).
//
// The RTS camera does not use a lazy fixed far plane (the classic 10,000-unit
// skybox distance). Instead both planes are a closed form of the camera's zoom
// range, computed once at map load and again whenever the zoom clamps (the
// effective Z range) change, then pushed to the projection via SetNear/SetFar.
// A tight far plane keeps depth precision high — the near:far ratio stays well
// under 1:13, which is what keeps ground decals and selection rings from
// z-fighting at maximum zoom-out.
//
// Z_min / Z_max here are the camera's zoom-distance clamps (ZoomMin/ZoomMax) —
// the proxy for how close and how far the eye ever sits from its ground anchor.

const (
	// NearZoomFactor sets the near plane at a quarter of the closest zoom: close
	// enough that nothing the player can zoom to is clipped, far enough to spend
	// depth-buffer range where it matters.
	NearZoomFactor float32 = 0.25
	// FarZoomFactor sets the far plane past the farthest zoom by the visible
	// ground extent beyond the anchor plus a fixed vertical margin (tall cliffs +
	// flying units), which works out to ≈1.6× the farthest zoom distance.
	FarZoomFactor float32 = 1.6
)

// ComputeNearFar returns the near and far clip planes for a camera whose zoom
// distance ranges over [zMin, zMax]. Pure and deterministic. far is guaranteed
// strictly greater than near for any 0 < zMin <= zMax.
func ComputeNearFar(zMin, zMax float32) (near, far float32) {
	return NearZoomFactor * zMin, FarZoomFactor * zMax
}
