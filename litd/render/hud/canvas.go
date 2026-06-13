package hud

import (
	"errors"
	"math"
)

const (
	ReferenceWidth  = 1280
	ReferenceHeight = 720
	MinWidth        = 1024
	MinHeight       = 768
	MinUIScale      = 0.75
	MaxUIScale      = 1.50
)

var ErrUnsupportedSize = errors.New("hud: unsupported canvas size")

type Anchor uint8

const (
	AnchorTopLeft Anchor = iota
	AnchorTop
	AnchorTopRight
	AnchorLeft
	AnchorCenter
	AnchorRight
	AnchorBottomLeft
	AnchorBottom
	AnchorBottomRight
)

func (a Anchor) String() string {
	switch a {
	case AnchorTopLeft:
		return "top-left"
	case AnchorTop:
		return "top"
	case AnchorTopRight:
		return "top-right"
	case AnchorLeft:
		return "left"
	case AnchorCenter:
		return "center"
	case AnchorRight:
		return "right"
	case AnchorBottomLeft:
		return "bottom-left"
	case AnchorBottom:
		return "bottom"
	case AnchorBottomRight:
		return "bottom-right"
	default:
		return "unknown"
	}
}

type Rect struct {
	X int `json:"x"`
	Y int `json:"y"`
	W int `json:"w"`
	H int `json:"h"`
}

func (r Rect) Right() int  { return r.X + r.W }
func (r Rect) Bottom() int { return r.Y + r.H }

func (r Rect) Inside(width, height int) bool {
	return r.X >= 0 && r.Y >= 0 && r.Right() <= width && r.Bottom() <= height
}

func (r Rect) Overlaps(o Rect) bool {
	return r.X < o.Right() && r.Right() > o.X && r.Y < o.Bottom() && r.Bottom() > o.Y
}

type RefRect struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
	W float64 `json:"w"`
	H float64 `json:"h"`
}

type Canvas struct {
	Width   int     `json:"width"`
	Height  int     `json:"height"`
	UIScale float64 `json:"uiScale"`
	Scale   float64 `json:"scale"`
}

func NewCanvas(width, height int, uiScale float64) (Canvas, error) {
	if width < MinWidth || height < MinHeight || width*3 < height*4 {
		return Canvas{}, ErrUnsupportedSize
	}
	uiScale = ClampUIScale(uiScale)
	return Canvas{
		Width:   width,
		Height:  height,
		UIScale: uiScale,
		Scale:   (float64(height) / ReferenceHeight) * uiScale,
	}, nil
}

func ClampUIScale(v float64) float64 {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return 1
	}
	if v < MinUIScale {
		return MinUIScale
	}
	if v > MaxUIScale {
		return MaxUIScale
	}
	return v
}

func (c Canvas) Place(anchor Anchor, ref RefRect) Rect {
	w := c.Snap(ref.W)
	h := c.Snap(ref.H)
	x := c.placeX(anchor, ref, w)
	y := c.placeY(anchor, ref, h)
	return Rect{X: x, Y: y, W: w, H: h}
}

func (c Canvas) Snap(refUnits float64) int {
	return int(math.Round(refUnits * c.Scale))
}

func (c Canvas) placeX(anchor Anchor, ref RefRect, w int) int {
	switch anchor {
	case AnchorTopRight, AnchorRight, AnchorBottomRight:
		rightMargin := c.Snap(ReferenceWidth - ref.X - ref.W)
		return c.Width - rightMargin - w
	case AnchorTop, AnchorCenter, AnchorBottom:
		refCenter := ref.X + ref.W/2
		screenCenter := float64(c.Width) / 2
		return int(math.Round(screenCenter + (refCenter-ReferenceWidth/2)*c.Scale - float64(w)/2))
	default:
		return c.Snap(ref.X)
	}
}

func (c Canvas) placeY(anchor Anchor, ref RefRect, h int) int {
	switch anchor {
	case AnchorBottomLeft, AnchorBottom, AnchorBottomRight:
		bottomMargin := c.Snap(ReferenceHeight - ref.Y - ref.H)
		return c.Height - bottomMargin - h
	case AnchorLeft, AnchorCenter, AnchorRight:
		refCenter := ref.Y + ref.H/2
		screenCenter := float64(c.Height) / 2
		return int(math.Round(screenCenter + (refCenter-ReferenceHeight/2)*c.Scale - float64(h)/2))
	default:
		return c.Snap(ref.Y)
	}
}
