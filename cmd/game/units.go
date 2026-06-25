package main

// Unit model rendering (#531 follow-up): the slice draws each live sim unit as a
// real, animated glTF model instead of a placeholder box. The binding flows
// data → worldhost → here: a unit type declares its art (data.Unit.Model), the
// host retains it (Host.UnitModels), and this file preloads each distinct model
// ONCE and attaches a fresh posed instance the moment a unit of that type spawns —
// the "preload the full asset, then just drop the unit in fully visual" path. The
// sim stays model-agnostic (presentation never enters the hashing core, PRD §4.1);
// everything here reads sim state and renders, never mutating it.
//
// When a declared model path is not yet provisioned in assets/ (the firstclash art
// is still WIP — #670), the resolver substitutes an existing CC0 model by category
// and logs the substitution. That is a presentation placeholder for missing ART,
// not a cover-up of broken gameplay: the unit, its orders, and its combat are all
// real and running; only the mesh is a stand-in until the canonical asset lands.

import (
	"fmt"
	"os"
	"path/filepath"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/g3n/engine/animation"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/loader/gltf"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

// assetsRoot is the repo-root asset tree the model paths resolve against (cmd/game
// is run from the repo root, like the other binaries).
const assetsRoot = "assets"

// unitVisual is the rendered representation of one sim unit: its scene node and the
// looping idle clip (nil for a model with no animation, e.g. a static building),
// advanced each frame so models breathe rather than freeze in their bind pose.
type unitVisual struct {
	node core.INode
	idle *animation.Animation
}

// Category fallback models — real CC0 assets that exist in the tree, used when a
// unit type's declared model is not yet provisioned. Buildings read as team-colored
// castles, the hero as a knight, and rank-and-file as varied adventurers so a base
// reads as a crowd rather than clones.
var (
	fallbackBuildingBlue = "kaykit-hexagon/building_castle_blue.glb"
	fallbackBuildingRed  = "kaykit-hexagon/building_castle_red.glb"
	fallbackHero         = "kaykit-adventurers/Knight.glb"
	fallbackUnits        = []string{
		"kaykit-adventurers/Rogue.glb",
		"kaykit-adventurers/Mage.glb",
		"kaykit-adventurers/Barbarian.glb",
		"kaykit-adventurers/Rogue_Hooded.glb",
	}
)

// buildModelIndex builds the unit-type → declared-model map from the loaded world
// (worldhost retains it) and allocates the per-frame visual bookkeeping. Idempotent
// and cheap to call again after a quickload swaps the host (#204).
func (gm *game) buildModelIndex() {
	if gm.modelDocs == nil {
		gm.modelDocs = make(map[string]*gltf.GLTF)
	}
	if gm.modelWarned == nil {
		gm.modelWarned = make(map[string]bool)
	}
	gm.seen = make(map[uint32]bool)
	gm.unitViz = make(map[uint32]*unitVisual)
	gm.typeModel = make(map[api.UnitType]string)
	if gm.host == nil {
		return
	}
	for code, model := range gm.host.UnitModels {
		if ut := gm.g.UnitType(code); ut.Valid() {
			gm.typeModel[ut] = model
		}
	}
}

// preloadModels parses + caches the GLB for every model a spawned unit could need:
// each declared model that is actually on disk, plus the full fallback set. Warming
// the doc cache up front means the first spawn of a type drops in with no parse
// hitch (the user's "preload the full asset" step). Safe to call once at startup.
func (gm *game) preloadModels() {
	warm := make(map[string]bool)
	add := func(rel string) {
		if rel == "" || warm[rel] {
			return
		}
		warm[rel] = true
		if _, err := gm.docFor(rel); err != nil {
			fmt.Printf("event: model preload failed %q: %v\n", rel, err)
		}
	}
	for _, declared := range gm.typeModel {
		if declared != "" && fileExists(filepath.Join(assetsRoot, declared)) {
			add(declared)
		}
	}
	add(fallbackBuildingBlue)
	add(fallbackBuildingRed)
	add(fallbackHero)
	for _, m := range fallbackUnits {
		add(m)
	}
	fmt.Printf("event: models preloaded count=%d\n", len(warm))
}

// resolveModel maps a unit to the model asset to render: its declared art if that
// file is provisioned, else an existing CC0 stand-in chosen by category (building /
// hero / rank-and-file) and team. Substitutions are logged once per declared path.
func (gm *game) resolveModel(declared string, u api.Unit) (rel string, height float32) {
	building := u.MoveSpeed() == 0
	h := float32(1.5)
	if building {
		h = 3.2
	} else if u.Life() >= 1000 {
		h = 1.9
	}
	if declared != "" && fileExists(filepath.Join(assetsRoot, declared)) {
		return declared, h
	}
	if declared != "" && !gm.modelWarned[declared] {
		gm.modelWarned[declared] = true
		fmt.Printf("event: model %q not provisioned — substituting CC0 placeholder (see #670)\n", declared)
	}
	return categoryFallback(building, u.Owner().Slot(), u.Life() >= 1000, u.ID()), h
}

// categoryFallback picks an existing CC0 model for a unit by category — team-colored
// castle for a building, knight for a hero (high life), one of a rotating set of
// adventurers (keyed by id) for rank-and-file. Pure so it is unit-testable headless;
// the FSV asserts every value it can return is a real file under assets/.
func categoryFallback(building bool, slot int, hero bool, id uint32) string {
	switch {
	case building && slot == 1:
		return fallbackBuildingRed
	case building:
		return fallbackBuildingBlue
	case hero:
		return fallbackHero
	default:
		return fallbackUnits[int(id)%len(fallbackUnits)]
	}
}

// buildUnitVisual loads (from the warmed cache) a fresh model instance for u, posed
// to frame 0 of its idle clip so it is never shown in a raw T-pose. On a hard load
// failure it falls back to a visible team-tinted box (logged), so a missing/broken
// asset is loud and still leaves the unit on screen rather than invisible.
func (gm *game) buildUnitVisual(u api.Unit) (core.INode, *animation.Animation) {
	rel, height := gm.resolveModel(gm.typeModel[u.Type()], u)
	node, idle, err := gm.loadModelInstance(rel, height)
	if err != nil {
		fmt.Printf("event: unit model %q load failed: %v — box placeholder\n", rel, err)
		return gm.placeholderBox(u), nil
	}
	if idle != nil {
		idle.Update(0) // pose immediately; the live loop then advances it each frame
	}
	return node, idle
}

// loadModelInstance returns a fresh scene node + looping idle clip for a model,
// parsing the GLB at most once (cached in modelDocs) and creating a NEW scene each
// call so instances never share nodes. A model with no "Idle" clip (e.g. a static
// building) returns a nil animation, which is fine — it just renders static.
func (gm *game) loadModelInstance(rel string, height float32) (core.INode, *animation.Animation, error) {
	doc, err := gm.docFor(rel)
	if err != nil {
		return nil, nil, err
	}
	scene := 0
	if doc.Scene != nil {
		scene = *doc.Scene
	}
	inode, err := doc.LoadScene(scene)
	if err != nil {
		return nil, nil, fmt.Errorf("load scene: %w", err)
	}
	node := normalizeModel(inode, height)
	var idle *animation.Animation
	if a, err := doc.LoadAnimationByName("Idle"); err == nil && a != nil {
		a.SetLoop(true)
		idle = a
	}
	return node, idle, nil
}

// docFor parses a GLB once and caches the document for reuse across instances.
func (gm *game) docFor(rel string) (*gltf.GLTF, error) {
	if doc := gm.modelDocs[rel]; doc != nil {
		return doc, nil
	}
	doc, err := gltf.ParseBin(filepath.Join(assetsRoot, rel))
	if err != nil {
		return nil, err
	}
	gm.modelDocs[rel] = doc
	return doc, nil
}

// placeholderBox is the visible last-resort marker when a model cannot be loaded at
// all: a team-tinted box, so the unit is still on screen (never invisible) and the
// failure is obvious rather than silent.
func (gm *game) placeholderBox(u api.Unit) core.INode {
	col := teamColors[0]
	if slot := u.Owner().Slot(); slot >= 0 && slot < len(teamColors) {
		col = teamColors[slot]
	}
	box := graphic.NewMesh(geometry.NewBox(0.6, 1.2, 0.6), material.NewStandard(&col))
	box.SetPosition(0, 0.6, 0)
	return box
}

// updateSelectionCap moves the single reused selection marker to the selected unit,
// or hides it when nothing is selected. Created lazily on first use.
func (gm *game) updateSelectionCap(have bool, x, z float32) {
	if gm.selCap == nil {
		capMat := material.NewStandard(&math32.Color{R: 1, G: 0.95, B: 0.4})
		capMat.SetEmissiveColor(&math32.Color{R: 0.9, G: 0.8, B: 0.2})
		gm.selCap = graphic.NewMesh(geometry.NewSphere(0.28, 12, 8), capMat)
		gm.unitsRoot.Add(gm.selCap)
	}
	gm.selCap.SetVisible(have)
	if have {
		gm.selCap.SetPosition(x, 2.0, z)
	}
}

// animateUnits advances every live unit's idle clip by dt seconds so the models are
// continuously animated. Presentation only — never touches sim state.
func (gm *game) animateUnits(dtSec float32) {
	for _, v := range gm.unitViz {
		if v.idle != nil {
			v.idle.Update(dtSec)
		}
	}
}

// resetUnitVisuals tears down all unit nodes (used on quickload, which swaps the
// host and renumbers units). The parsed-doc cache survives — same assets reload
// instantly — but typeModel/unitViz are rebuilt against the new host.
func (gm *game) resetUnitVisuals() {
	for _, v := range gm.unitViz {
		gm.unitsRoot.Remove(v.node)
	}
	if gm.selCap != nil {
		gm.unitsRoot.Remove(gm.selCap)
		gm.selCap = nil
	}
	gm.unitViz = nil // forces buildModelIndex on the next rebuildUnits
}

// normalizeModel wraps model so its bounding box is scaled to `height` world units
// on its largest axis, its base sits at y=0, and its XZ center is on the wrapper
// origin — so SetPosition places the unit's feet on the ground. (Same convention as
// cmd/firstlight.)
func normalizeModel(inode core.INode, height float32) *core.Node {
	model := inode.GetNode()
	model.UpdateMatrixWorld() // bbox needs world matrices, stale until first render
	bb := model.BoundingBox()
	var center, size math32.Vector3
	bb.Center(&center)
	bb.Size(&size)
	max := size.X
	if size.Y > max {
		max = size.Y
	}
	if size.Z > max {
		max = size.Z
	}
	scale := float32(1)
	if max > 0.001 {
		scale = height / max
	}
	model.SetScale(scale, scale, scale)
	model.SetPosition(-center.X*scale, -bb.Min.Y*scale, -center.Z*scale)
	wrapper := core.NewNode()
	wrapper.Add(model)
	return wrapper
}

// fileExists reports whether path is a readable regular file.
func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}
