package sim

// Spatial unit queries (#239; groups-and-enumeration.md). The JASS
// GroupEnumUnits* callback machinery collapses to slice-returning queries
// (R-EXEC-4): the sim fills a caller-provided []EntityID, the public layer
// wraps each id as a Unit. Results are ascending entity-id order so two
// runs of the same scene enumerate identically (determinism), and the
// caller's buffer is reused so the hot path allocates nothing (R-GC-2).
//
// "Unit" here means an entity carrying an Owners row — the real unit
// population, excluding missiles/effects/doodads that have no owner.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// isQueryUnit reports whether id is a live, ownable unit (has an owner row).
func (w *World) isQueryUnit(id EntityID) bool {
	return w.Ents.Alive(id) && w.Owners.Row(id) != -1
}

// sortByIndexAsc insertion-sorts dst[start:] by ascending EntityID.Index().
// In-place, no allocation; the queried slice is small (units in one area).
func sortByIndexAsc(dst []EntityID, start int) {
	for i := start + 1; i < len(dst); i++ {
		v := dst[i]
		j := i - 1
		for j >= start && dst[j].Index() > v.Index() {
			dst[j+1] = dst[j]
			j--
		}
		dst[j+1] = v
	}
}

// AppendUnitsInRange appends every live unit whose center is within radius
// of center (inclusive) to dst, ascending entity-id order, and returns the
// grown slice. Zero-alloc when dst has spare capacity.
func (w *World) AppendUnitsInRange(dst []EntityID, center fixed.Vec2, radius fixed.F64) []EntityID {
	if radius < 0 {
		return dst
	}
	start := len(dst)
	rHi, rLo := fixed.RadiusSq(radius)
	x0, x1 := bucketCoord(center.X.Sub(radius)), bucketCoord(center.X.Add(radius))
	y0, y1 := bucketCoord(center.Y.Sub(radius)), bucketCoord(center.Y.Add(radius))
	for by := y0; by <= y1; by++ {
		for bx := x0; bx <= x1; bx++ {
			for be := w.bucketHead[by*BucketGridSize+bx]; be != -1; be = w.bucketNext[be] {
				id := w.bucketID[be]
				if !w.isQueryUnit(id) {
					continue
				}
				tr := w.Transforms.Row(id)
				if tr == -1 {
					continue
				}
				dHi, dLo := fixed.DistSq(center, w.Transforms.Pos[tr])
				if dHi > rHi || (dHi == rHi && dLo > rLo) {
					continue
				}
				dst = append(dst, id)
			}
		}
	}
	sortByIndexAsc(dst, start)
	return dst
}

// AppendUnitsInRect appends every live unit whose center lies inside the
// axis-aligned rect [minx,maxx]×[miny,maxy] (inclusive) to dst, ascending
// entity-id order. Zero-alloc when dst has spare capacity.
func (w *World) AppendUnitsInRect(dst []EntityID, minx, miny, maxx, maxy fixed.F64) []EntityID {
	if maxx < minx || maxy < miny {
		return dst
	}
	start := len(dst)
	x0, x1 := bucketCoord(minx), bucketCoord(maxx)
	y0, y1 := bucketCoord(miny), bucketCoord(maxy)
	for by := y0; by <= y1; by++ {
		for bx := x0; bx <= x1; bx++ {
			for be := w.bucketHead[by*BucketGridSize+bx]; be != -1; be = w.bucketNext[be] {
				id := w.bucketID[be]
				if !w.isQueryUnit(id) {
					continue
				}
				tr := w.Transforms.Row(id)
				if tr == -1 {
					continue
				}
				p := w.Transforms.Pos[tr]
				if p.X < minx || p.X > maxx || p.Y < miny || p.Y > maxy {
					continue
				}
				dst = append(dst, id)
			}
		}
	}
	sortByIndexAsc(dst, start)
	return dst
}

// AppendAllUnits appends every live owned unit to dst, ascending
// entity-id order. Zero-alloc when dst has spare capacity.
func (w *World) AppendAllUnits(dst []EntityID) []EntityID {
	start := len(dst)
	o := w.Owners
	for i := int32(0); i < o.count; i++ {
		id := o.Entity[i]
		if w.Ents.Alive(id) {
			dst = append(dst, id)
		}
	}
	sortByIndexAsc(dst, start)
	return dst
}
