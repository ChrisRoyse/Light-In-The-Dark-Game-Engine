package render

import (
	"testing"
)

// #528 part 2 — the procedural VFX atlas. SoT = the generated RGBA pixel bytes
// (CPU only, no GL). The glow/impact cells must be opaque at their centre and
// transparent at the corner (a centred radial field); the aura cell must be
// HOLLOW — transparent at the centre, opaque on its mid-radius ring — which is
// what makes it read as a ring rather than a disc. Sub-rects must tile the
// atlas without overlap and stay inside [0,1].
func TestVFXAtlasImageFSV(t *testing.T) {
	img := NewVFXAtlasImage()
	if img.Bounds().Dx() != vfxAtlasSize || img.Bounds().Dy() != vfxAtlasSize {
		t.Fatalf("atlas size = %v, want %dx%d", img.Bounds(), vfxAtlasSize, vfxAtlasSize)
	}
	alphaAt := func(s VFXSprite, fx, fy float64) uint8 {
		col, row := vfxCellCoord(s)
		x := col*vfxAtlasCell + int(fx*float64(vfxAtlasCell))
		y := row*vfxAtlasCell + int(fy*float64(vfxAtlasCell))
		return img.Pix[img.PixOffset(x, y)+3]
	}

	// Centre-bright sprites: opaque core, transparent corner.
	for _, s := range []VFXSprite{VFXSpriteMissile, VFXSpriteImpact} {
		center := alphaAt(s, 0.5, 0.5)
		corner := alphaAt(s, 0.02, 0.02)
		t.Logf("FSV sprite %d: centre alpha=%d corner alpha=%d", s, center, corner)
		if center < 200 {
			t.Fatalf("sprite %d centre alpha=%d, want opaque (>=200)", s, center)
		}
		if corner > 40 {
			t.Fatalf("sprite %d corner alpha=%d, want transparent (<=40)", s, corner)
		}
	}

	// Aura is a ring: hollow centre, opaque mid-radius shell.
	auraCenter := alphaAt(VFXSpriteAura, 0.5, 0.5)
	auraRing := alphaAt(VFXSpriteAura, 0.5, 0.5+0.65*0.5) // ~0.65 radius along +y
	t.Logf("FSV aura: centre alpha=%d ring alpha=%d", auraCenter, auraRing)
	if auraCenter > 80 {
		t.Fatalf("aura centre alpha=%d, want hollow (<=80)", auraCenter)
	}
	if auraRing < 180 {
		t.Fatalf("aura ring alpha=%d, want opaque shell (>=180)", auraRing)
	}
	if auraRing <= auraCenter {
		t.Fatalf("aura not a ring: ring alpha %d <= centre alpha %d", auraRing, auraCenter)
	}

	// Sub-rects: each in [0,1], correct half-rect size, distinct cells.
	seen := map[[2]float32]bool{}
	for _, s := range []VFXSprite{VFXSpriteMissile, VFXSpriteImpact, VFXSpriteAura, VFXSpriteSpare} {
		r := VFXAtlasSubRect(s)
		if r.X < 0 || r.Y < 0 || r.Z > 1 || r.W > 1 || r.Z <= r.X || r.W <= r.Y {
			t.Fatalf("sprite %d sub-rect %v out of range / degenerate", s, r)
		}
		if w := r.Z - r.X; w < 0.49 || w > 0.51 {
			t.Fatalf("sprite %d sub-rect width=%.3f, want ~0.5", s, w)
		}
		key := [2]float32{r.X, r.Y}
		if seen[key] {
			t.Fatalf("sprite %d sub-rect origin %v collides with another cell", s, key)
		}
		seen[key] = true
	}
}
