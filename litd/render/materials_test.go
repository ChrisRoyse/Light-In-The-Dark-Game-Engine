package render

import (
	"image"
	"image/color"
	"testing"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

func TestAtlasMaterialCacheIdentityFSV(t *testing.T) {
	src := mustRenderAtlasSource(t, "vigil.atlas.png", color.RGBA{80, 130, 210, 255})
	cache := NewAtlasMaterialCache()
	t.Logf("FSV atlas material cache BEFORE count=%d", cache.Count())

	high1, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	high2, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material high AFTER first=%+v secondSame=%v count=%d", cache.Snapshot(high1), high1 == high2, cache.Count())
	if high1 != high2 || cache.Count() != 1 {
		t.Fatalf("same atlas+preset must reuse one material: same=%v count=%d", high1 == high2, cache.Count())
	}
	if high1.Texture.Width() != 1024 || high1.Texture.Height() != 1024 {
		t.Fatalf("high texture dims = %dx%d", high1.Texture.Width(), high1.Texture.Height())
	}

	medium, err := cache.Material(src, litasset.AtlasPresetMedium)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material medium AFTER snapshot=%+v count=%d", cache.Snapshot(medium), cache.Count())
	if medium == high1 || cache.Count() != 2 || medium.Texture.Width() != 512 || medium.Texture.Height() != 512 {
		t.Fatalf("medium material wrong: sameAsHigh=%v count=%d dims=%dx%d", medium == high1, cache.Count(), medium.Texture.Width(), medium.Texture.Height())
	}

	src2 := mustRenderAtlasSource(t, "ember.atlas.png", color.RGBA{210, 110, 60, 255})
	other, err := cache.Material(src2, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas material second atlas AFTER snapshot=%+v count=%d", cache.Snapshot(other), cache.Count())
	if other == high1 || cache.Count() != 3 {
		t.Fatalf("different atlas should create a separate material: same=%v count=%d", other == high1, cache.Count())
	}
}

func TestAtlasMaterialRuntimeSwitchFSV(t *testing.T) {
	src := mustRenderAtlasSource(t, "switch.atlas.png", color.RGBA{120, 180, 80, 255})
	cache := NewAtlasMaterialCache()
	high, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	medium, err := cache.Material(src, litasset.AtlasPresetMedium)
	if err != nil {
		t.Fatal(err)
	}
	before := cache.Count()
	highAgain, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	low, err := cache.Material(src, litasset.AtlasPresetLow)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV atlas preset switch BEFORE count=%d high=%p medium=%p AFTER highAgain=%p low=%p count=%d", before, high.Material, medium.Material, highAgain.Material, low.Material, cache.Count())
	if highAgain != high {
		t.Fatal("switching back to high created a new material")
	}
	if before != 2 || cache.Count() != 3 || low.Texture.Width() != 256 {
		t.Fatalf("switch counts/dims wrong before=%d after=%d low=%dx%d", before, cache.Count(), low.Texture.Width(), low.Texture.Height())
	}
}

func TestAtlasMaterialEdgesFSV(t *testing.T) {
	cache := NewAtlasMaterialCache()
	if _, err := cache.Material(nil, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil source accepted")
	} else {
		t.Logf("FSV atlas material nil source BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
	src := mustRenderAtlasSource(t, "edges.atlas.png", color.RGBA{32, 64, 96, 255})
	if _, err := cache.Material(src, litasset.AtlasPreset("bad")); err == nil {
		t.Fatal("invalid preset accepted")
	} else {
		t.Logf("FSV atlas material bad preset BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
	var nilCache *AtlasMaterialCache
	if _, err := nilCache.Material(src, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil cache accepted")
	} else {
		t.Logf("FSV atlas material nil cache AFTER err=%v", err)
	}
}

func TestPBRAtlasMaterialDefaultsFSV(t *testing.T) {
	src := mustRenderAtlasSource(t, "pbr-vigil.atlas.png", color.RGBA{140, 170, 205, 255})
	cache := NewPBRAtlasMaterialCache()
	t.Logf("FSV pbr atlas BEFORE count=%d", cache.Count())
	entry, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	again, err := cache.Material(src, litasset.AtlasPresetHigh)
	if err != nil {
		t.Fatal(err)
	}
	snap := cache.Snapshot(entry)
	defines := entry.Material.GetMaterial().ShaderDefines
	t.Logf("FSV pbr atlas AFTER snapshot=%+v same=%v textures=%d defines=%v", snap, entry == again, entry.Material.GetMaterial().TextureCount(), defines)
	if entry != again || cache.Count() != 1 {
		t.Fatalf("same atlas+preset should reuse one PBR material: same=%v count=%d", entry == again, cache.Count())
	}
	if snap.Factors.MetallicFactor != DefaultPBRMetallicFactor || snap.Factors.RoughnessFactor != DefaultPBRRoughnessFactor {
		t.Fatalf("PBR factors wrong: %+v", snap.Factors)
	}
	if !snap.Factors.BaseColorMap || snap.Factors.MetallicRoughnessMap || snap.Factors.NormalMap || snap.Factors.OcclusionMap || snap.Factors.EmissiveMap {
		t.Fatalf("PBR map flags violate core atlas-only path: %+v", snap.Factors)
	}
	if entry.Material.GetMaterial().TextureCount() != 1 {
		t.Fatalf("PBR material texture count=%d, want only base-color atlas", entry.Material.GetMaterial().TextureCount())
	}
	if _, ok := defines["HAS_BASECOLORMAP"]; !ok {
		t.Fatalf("PBR material missing base-color map define: %v", defines)
	}
	for _, forbidden := range []string{"HAS_METALROUGHNESSMAP", "HAS_NORMALMAP", "HAS_OCCLUSIONMAP", "HAS_EMISSIVEMAP"} {
		if _, ok := defines[forbidden]; ok {
			t.Fatalf("PBR material has forbidden map define %s: %v", forbidden, defines)
		}
	}
}

func TestPBRMaterialEdgesFSV(t *testing.T) {
	if _, _, err := NewPBRMaterial(nil, PBRMaterialOptions{}); err == nil {
		t.Fatal("nil base-color texture accepted")
	} else {
		t.Logf("FSV pbr material nil texture AFTER err=%v", err)
	}

	src := mustRenderAtlasSource(t, "pbr-emissive.atlas.png", color.RGBA{60, 90, 150, 255})
	cache := NewPBRAtlasMaterialCache()
	entry, err := cache.Material(src, litasset.AtlasPresetMedium)
	if err != nil {
		t.Fatal(err)
	}
	emissive := [3]float32{0.2, 0.7, 1.0}
	mat, factors, err := NewPBRMaterial(entry.Texture, PBRMaterialOptions{EmissiveFactor: emissive})
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV pbr material emissive BEFORE base=%+v AFTER factors=%+v textures=%d defines=%v", entry.Factors, factors, mat.GetMaterial().TextureCount(), mat.GetMaterial().ShaderDefines)
	if factors.EmissiveFactor != emissive || !factors.BaseColorMap || factors.EmissiveMap {
		t.Fatalf("emissive-factor PBR flags wrong: %+v", factors)
	}
	if mat.GetMaterial().TextureCount() != 1 {
		t.Fatalf("emissive-factor material should still bind only base-color map, got %d textures", mat.GetMaterial().TextureCount())
	}
	if _, ok := mat.GetMaterial().ShaderDefines["HAS_EMISSIVEMAP"]; ok {
		t.Fatalf("emissive factor must not create emissive map define: %v", mat.GetMaterial().ShaderDefines)
	}

	var nilCache *PBRAtlasMaterialCache
	if _, err := nilCache.Material(src, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil PBR cache accepted")
	} else {
		t.Logf("FSV pbr material nil cache AFTER err=%v", err)
	}
	if _, err := cache.Material(nil, litasset.AtlasPresetHigh); err == nil {
		t.Fatal("nil source accepted")
	} else {
		t.Logf("FSV pbr material nil source BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
	if _, err := cache.Material(src, litasset.AtlasPreset("bad")); err == nil {
		t.Fatal("invalid preset accepted")
	} else {
		t.Logf("FSV pbr material bad preset BEFORE count=%d AFTER err=%v count=%d", cache.Count(), err, cache.Count())
	}
}

func mustRenderAtlasSource(t *testing.T, name string, c color.RGBA) *litasset.AtlasSource {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, 1024, 1024))
	for y := 0; y < 1024; y++ {
		for x := 0; x < 1024; x++ {
			img.SetRGBA(x, y, c)
		}
	}
	src, err := litasset.NewAtlasSource(name, img)
	if err != nil {
		t.Fatal(err)
	}
	return src
}

func TestTeamColorPaletteFSV(t *testing.T) {
	for slot := 0; slot < TeamColorSlots; slot++ {
		c, err := TeamColor(slot)
		t.Logf("FSV team palette slot=%d color=%+v err=%v", slot, c, err)
		if err != nil {
			t.Fatalf("slot %d rejected: %v", slot, err)
		}
		if c.R < 0 || c.R > 1 || c.G < 0 || c.G > 1 || c.B < 0 || c.B > 1 {
			t.Fatalf("slot %d color out of normalized range: %+v", slot, c)
		}
	}
	for _, slot := range []int{-1, TeamColorSlots} {
		if _, err := TeamColor(slot); err == nil {
			t.Fatalf("invalid slot %d accepted", slot)
		} else {
			t.Logf("FSV invalid team slot=%d err=%v", slot, err)
		}
	}
}

func TestTeamColorMeshStateFSV(t *testing.T) {
	mat := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	mesh, err := NewTeamColorMesh(geometry.NewPlane(1, 1), mat, 2)
	if err != nil {
		t.Fatal(err)
	}
	before := mesh.TeamColorState()
	t.Logf("FSV team mesh BEFORE state=%+v materials=%d", before, len(mesh.Materials()))
	if len(mesh.Materials()) != 1 || mesh.Materials()[0].IGraphic() != mesh {
		t.Fatalf("team mesh material ownership wrong")
	}
	if got := mesh.GetGraphic().ShaderDefines["LITD_TEAMCOLOR"]; got != "1" {
		t.Fatalf("team mesh shader define = %q, want 1", got)
	}
	if _, ok := mat.GetMaterial().ShaderDefines["LITD_TEAMCOLOR"]; ok {
		t.Fatalf("team-color define leaked into shared material")
	}

	mesh.SetPresentationScalars(1.5, -1, 0.35)
	if err := mesh.SetTeamColorZone(TeamColorZone{MinU: 0.1, MinV: 0.2, MaxU: 0.45, MaxV: 0.9}); err != nil {
		t.Fatal(err)
	}
	if err := mesh.SetTeamSlot(NeutralTeamSlot); err != nil {
		t.Fatal(err)
	}
	after := mesh.TeamColorState()
	t.Logf("FSV team mesh AFTER state=%+v", after)
	if after.Slot != NeutralTeamSlot || after.HitFlash != 1 || after.FadeAlpha != 0 || after.FogDim != 0.35 {
		t.Fatalf("team mesh state wrong after updates: %+v", after)
	}
	if after.Zone.MinU != 0.1 || after.Zone.MinV != 0.2 || after.Zone.MaxU != 0.45 || after.Zone.MaxV != 0.9 {
		t.Fatalf("zone not applied: %+v", after.Zone)
	}

	zoneBefore := after.Zone
	if err := mesh.SetTeamColorZone(TeamColorZone{MinU: 0.6, MinV: 0, MaxU: 0.4, MaxV: 1}); err == nil {
		t.Fatal("invalid zone accepted")
	} else {
		t.Logf("FSV invalid team zone BEFORE=%+v AFTER err=%v state=%+v", zoneBefore, err, mesh.TeamColorState().Zone)
	}
	if mesh.TeamColorState().Zone != zoneBefore {
		t.Fatalf("invalid zone mutated state: %+v -> %+v", zoneBefore, mesh.TeamColorState().Zone)
	}
}

func TestTeamColorMeshCloneAndEdgesFSV(t *testing.T) {
	mesh, err := NewTeamColorMesh(geometry.NewPlane(1, 1), material.NewStandard(&math32.Color{R: 1, G: 1, B: 1}), 0)
	if err != nil {
		t.Fatal(err)
	}
	mesh.SetTeamColor(math32.Color{R: 0.25, G: 0.5, B: 0.75})
	mesh.SetTeamColorEnabled(false)
	clone := mesh.Clone().(*TeamColorMesh)
	t.Logf("FSV team clone original=%+v clone=%+v", mesh.TeamColorState(), clone.TeamColorState())
	if clone.TeamColorState() != mesh.TeamColorState() {
		t.Fatalf("clone state mismatch: original=%+v clone=%+v", mesh.TeamColorState(), clone.TeamColorState())
	}
	if len(clone.Materials()) != 1 || clone.Materials()[0].IGraphic() != clone {
		t.Fatalf("clone material ownership wrong")
	}
	if _, err := NewTeamColorMesh(geometry.NewPlane(1, 1), nil, TeamColorSlots); err == nil {
		t.Fatal("invalid constructor slot accepted")
	} else {
		t.Logf("FSV team constructor invalid slot err=%v", err)
	}
}
