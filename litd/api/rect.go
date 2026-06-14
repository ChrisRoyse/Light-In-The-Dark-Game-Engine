package litd

// Rect value-type surface (regions-rects-locations.md; R-API-2).
//
// JASS's rect was a heap handle with a create/remove lifetime and
// per-axis getters (GetRectMinX, GetRectCenterY, ...). LitD collapses
// the whole family onto the Rect value (handles.go): construction is a
// struct literal or one of the two constructors here, the getters are
// Vec2-returning methods, and there is no RemoveRect — GC owns the
// value, so the entire heap-lifetime native set evaporates.
//
// A Rect is normalized lazily by the accessors: Min/Max/Center/Contains
// treat the bounds as an unordered pair, so a Rect built with swapped
// corners still answers containment correctly. All methods are pure
// value-in/value-out and allocate nothing (R-API-2).

// NewRect builds a rectangle spanning the two corner points. The corners
// may be given in any order. JASS: Rect (the (minx,miny,maxx,maxy)
// constructor, coordinates collapsed to Vec2 per R-API-2).
func NewRect(a, b Vec2) Rect {
	return Rect{MinX: a.X, MinY: a.Y, MaxX: b.X, MaxY: b.Y}.normalized()
}

// RectAround builds a rectangle of width w and height h centered on c.
// Negative sizes are treated as their magnitudes. JASS:
// RectFromCenterSizeBJ (center+size constructor, D2).
func RectAround(c Vec2, w, h float64) Rect {
	if w < 0 {
		w = -w
	}
	if h < 0 {
		h = -h
	}
	return Rect{
		MinX: c.X - w/2, MinY: c.Y - h/2,
		MaxX: c.X + w/2, MaxY: c.Y + h/2,
	}
}

// normalized returns the rect with Min ≤ Max on both axes.
func (r Rect) normalized() Rect {
	if r.MinX > r.MaxX {
		r.MinX, r.MaxX = r.MaxX, r.MinX
	}
	if r.MinY > r.MaxY {
		r.MinY, r.MaxY = r.MaxY, r.MinY
	}
	return r
}

// Min returns the lower-left corner. JASS: GetRectMinX/GetRectMinY
// collapsed to one Vec2 getter (D3/D5: per-axis getters become fields).
func (r Rect) Min() Vec2 {
	r = r.normalized()
	return Vec2{X: r.MinX, Y: r.MinY}
}

// Max returns the upper-right corner. JASS: GetRectMaxX/GetRectMaxY.
func (r Rect) Max() Vec2 {
	r = r.normalized()
	return Vec2{X: r.MaxX, Y: r.MaxY}
}

// Center returns the midpoint. JASS: GetRectCenterX/GetRectCenterY and
// the GetRectCenter location BJ, collapsed to one Vec2 getter.
func (r Rect) Center() Vec2 {
	return Vec2{X: (r.MinX + r.MaxX) / 2, Y: (r.MinY + r.MaxY) / 2}
}

// Width returns the horizontal extent. JASS: GetRectWidthBJ.
func (r Rect) Width() float64 {
	if r.MaxX >= r.MinX {
		return r.MaxX - r.MinX
	}
	return r.MinX - r.MaxX
}

// Height returns the vertical extent. JASS: GetRectHeightBJ.
func (r Rect) Height() float64 {
	if r.MaxY >= r.MinY {
		return r.MaxY - r.MinY
	}
	return r.MinY - r.MaxY
}

// Contains reports whether p lies within the rectangle, edges inclusive.
// JASS: RectContainsCoords (and RectContainsLoc, the location variant).
func (r Rect) Contains(p Vec2) bool {
	r = r.normalized()
	return p.X >= r.MinX && p.X <= r.MaxX && p.Y >= r.MinY && p.Y <= r.MaxY
}

// Offset returns the rectangle translated by d. JASS: the MoveRectTo /
// MoveRectToLoc family — because a Rect is a value with no identity,
// "moving" a rect re-issues a translated value rather than mutating a
// handle (regions-rects-locations.md hazard 4).
func (r Rect) Offset(d Vec2) Rect {
	return Rect{
		MinX: r.MinX + d.X, MinY: r.MinY + d.Y,
		MaxX: r.MaxX + d.X, MaxY: r.MaxY + d.Y,
	}
}
