package render

import (
	"fmt"

	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
)

// PortraitTarget is an offscreen color+depth framebuffer for rendering a unit
// portrait (#193): a small subject scene (a unit model + a tight portrait camera +
// a key light) is rendered into this target's color texture, which the HUD portrait
// panel then samples — so the portrait is a live 3D render, not a baked sprite, and
// it costs one extra render pass rather than perturbing the main scene.
//
// It owns the GL framebuffer, its RGBA color texture, and a depth+stencil
// renderbuffer (the same attachment scheme g3n's Postprocessor uses). Construction
// is fail-closed: an incomplete framebuffer is an error, never a silently-broken
// target. The color texture id is exposed (Texture()) for the HUD to bind, and the
// rendered pixels are read back (ReadPixels) as the verifiable source of truth.
type PortraitTarget struct {
	gs     *gls.GLS
	fbo    uint32
	tex    uint32
	depth  uint32
	width  int32
	height int32
}

// NewPortraitTarget builds an offscreen target of the given pixel size. Requires a
// current GL context. Fail-closed on a non-positive size or an incomplete
// framebuffer.
func NewPortraitTarget(gs *gls.GLS, width, height int32) (*PortraitTarget, error) {
	if gs == nil {
		return nil, fmt.Errorf("render: portrait target needs a GL state")
	}
	if width <= 0 || height <= 0 {
		return nil, fmt.Errorf("render: portrait target size %dx%d must be positive", width, height)
	}
	p := &PortraitTarget{gs: gs, width: width, height: height}

	p.fbo = gs.GenFramebuffer()
	gs.BindFramebuffer(p.fbo)

	p.tex = gs.GenTexture()
	gs.BindTexture(gls.TEXTURE_2D, p.tex)
	gs.TexImage2D(gls.TEXTURE_2D, 0, gls.RGBA, width, height, gls.RGBA, gls.UNSIGNED_BYTE, nil)
	gs.TexParameteri(gls.TEXTURE_2D, gls.TEXTURE_WRAP_S, gls.CLAMP_TO_EDGE)
	gs.TexParameteri(gls.TEXTURE_2D, gls.TEXTURE_WRAP_T, gls.CLAMP_TO_EDGE)
	gs.TexParameteri(gls.TEXTURE_2D, gls.TEXTURE_MIN_FILTER, gls.LINEAR)
	gs.TexParameteri(gls.TEXTURE_2D, gls.TEXTURE_MAG_FILTER, gls.LINEAR)
	gs.BindTexture(gls.TEXTURE_2D, 0)
	gs.FramebufferTexture2D(gls.COLOR_ATTACHMENT0, gls.TEXTURE_2D, p.tex)

	p.depth = gs.GenRenderbuffer()
	gs.BindRenderbuffer(p.depth)
	gs.RenderbufferStorage(gls.DEPTH24_STENCIL8, int(width), int(height))
	gs.BindRenderbuffer(0)
	gs.FramebufferRenderbuffer(gls.DEPTH_STENCIL_ATTACHMENT, p.depth)

	if st := gs.CheckFramebufferStatus(); st != gls.FRAMEBUFFER_COMPLETE {
		gs.BindFramebuffer(0)
		p.Dispose()
		return nil, fmt.Errorf("render: portrait framebuffer incomplete (status 0x%x)", st)
	}
	gs.BindFramebuffer(0)
	return p, nil
}

// Texture is the GL id of the color attachment, for the HUD to bind as the portrait
// panel's source.
func (p *PortraitTarget) Texture() uint32 { return p.tex }

// Size returns the target dimensions in pixels.
func (p *PortraitTarget) Size() (int32, int32) { return p.width, p.height }

// Render draws scene through cam into the offscreen target, clearing first to bg.
// It leaves the default framebuffer (0) bound; the caller restores the main
// viewport for subsequent passes. Depth testing is enabled for the pass.
func (p *PortraitTarget) Render(rend *renderer.Renderer, scene core.INode, cam camera.ICamera, bg math32.Color4) error {
	p.gs.BindFramebuffer(p.fbo)
	p.gs.Viewport(0, 0, p.width, p.height)
	p.gs.Enable(gls.DEPTH_TEST)
	p.gs.ClearColor(bg.R, bg.G, bg.B, bg.A)
	p.gs.Clear(gls.COLOR_BUFFER_BIT | gls.DEPTH_BUFFER_BIT)
	err := rend.Render(scene, cam)
	p.gs.BindFramebuffer(0)
	return err
}

// ReadPixels reads the color buffer back as RGBA bytes (4 per pixel, GL's
// bottom-up row order). This is the FSV source of truth for what was rendered.
func (p *PortraitTarget) ReadPixels() []byte {
	p.gs.BindFramebuffer(p.fbo)
	data := p.gs.ReadPixels(0, 0, int(p.width), int(p.height), gls.RGBA, gls.UNSIGNED_BYTE)
	p.gs.BindFramebuffer(0)
	return data
}

// Dispose releases the GL objects. Safe to call on a partially-built target.
func (p *PortraitTarget) Dispose() {
	if p.gs == nil {
		return
	}
	if p.tex != 0 {
		p.gs.DeleteTextures(p.tex)
		p.tex = 0
	}
	if p.depth != 0 {
		p.gs.DeleteRenderbuffers(p.depth)
		p.depth = 0
	}
	if p.fbo != 0 {
		p.gs.DeleteFramebuffers(p.fbo)
		p.fbo = 0
	}
}

// PortraitCoverage summarizes a readback for FSV: the fraction of pixels that
// differ from the background (i.e. the subject actually rendered) and the mean
// color of those subject pixels. A background-only readback yields Coverage 0.
type PortraitCoverage struct {
	Total    int     `json:"total"`
	Subject  int     `json:"subject"`
	Coverage float64 `json:"coverage"`
	MeanR    float64 `json:"meanR"`
	MeanG    float64 `json:"meanG"`
	MeanB    float64 `json:"meanB"`
}

// AnalyzePortrait classifies each RGBA pixel as subject vs background by distance
// from bg (any channel differing by more than tol/255). Pure — no GL — so the
// classification itself is unit-testable on synthetic pixels.
func AnalyzePortrait(rgba []byte, bg math32.Color4, tol uint8) PortraitCoverage {
	bgR := uint8(clamp01(bg.R) * 255)
	bgG := uint8(clamp01(bg.G) * 255)
	bgB := uint8(clamp01(bg.B) * 255)
	var cov PortraitCoverage
	var sumR, sumG, sumB int
	for i := 0; i+3 < len(rgba); i += 4 {
		cov.Total++
		r, g, b := rgba[i], rgba[i+1], rgba[i+2]
		if diffExceeds(r, bgR, tol) || diffExceeds(g, bgG, tol) || diffExceeds(b, bgB, tol) {
			cov.Subject++
			sumR += int(r)
			sumG += int(g)
			sumB += int(b)
		}
	}
	if cov.Total > 0 {
		cov.Coverage = float64(cov.Subject) / float64(cov.Total)
	}
	if cov.Subject > 0 {
		cov.MeanR = float64(sumR) / float64(cov.Subject) / 255
		cov.MeanG = float64(sumG) / float64(cov.Subject) / 255
		cov.MeanB = float64(sumB) / float64(cov.Subject) / 255
	}
	return cov
}

func diffExceeds(a, b, tol uint8) bool {
	if a > b {
		return a-b > tol
	}
	return b-a > tol
}
