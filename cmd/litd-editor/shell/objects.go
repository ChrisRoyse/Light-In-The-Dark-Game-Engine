package shell

import (
	"fmt"
	"io/fs"
	"path"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

type ObjectKind string

const (
	ObjectKindUnit   ObjectKind = "unit"
	ObjectKindDoodad ObjectKind = "doodad"
)

type ObjectPaletteItem struct {
	Kind   ObjectKind `json:"kind"`
	Type   string     `json:"type"`
	Source string     `json:"source"`
	Active bool       `json:"active,omitempty"`
}

type ObjectSelection struct {
	Kind                ObjectKind `json:"kind"`
	Type                string     `json:"type"`
	Owner               int        `json:"owner"`
	Rotation            int        `json:"rotation"`
	Scale               int        `json:"scale"`
	OverrideWalkability bool       `json:"overrideWalkability"`
}

type ObjectSnapshot struct {
	Palette   []ObjectPaletteItem `json:"palette"`
	Selection ObjectSelection     `json:"selection"`
	Units     []sourceform.Entity `json:"units,omitempty"`
	Doodads   []sourceform.Doodad `json:"doodads,omitempty"`
}

type rawPaletteDoodadFile struct {
	Doodad []rawPaletteDoodad `toml:"doodad"`
}

type rawPaletteDoodad struct {
	ID           uint32 `toml:"id"`
	Asset        string `toml:"asset"`
	Cell         []int  `toml:"cell"`
	Rotation     int    `toml:"rotation"`
	Destructible bool   `toml:"destructible"`
	Footprint    []int  `toml:"footprint"`
}

func (a *App) LoadObjectPalette(fsys fs.FS) error {
	if fsys == nil {
		return fmt.Errorf("editor objects: nil data filesystem")
	}
	tables, err := data.Load(fsys)
	if err != nil {
		return fmt.Errorf("editor objects: load unit palette: %w", err)
	}
	items := make([]ObjectPaletteItem, 0, len(tables.Units))
	units := make(map[string]data.Unit, len(tables.Units))
	for _, u := range tables.Units {
		items = append(items, ObjectPaletteItem{Kind: ObjectKindUnit, Type: u.ID, Source: "data/units"})
		units[u.ID] = u
	}
	doodads, err := loadDoodadPalette(fsys)
	if err != nil {
		return err
	}
	items = append(items, doodads...)
	if len(items) == 0 {
		return fmt.Errorf("editor objects: data tables produced an empty object palette")
	}
	sort.Slice(items, func(i, j int) bool {
		if items[i].Kind != items[j].Kind {
			return objectKindRank(items[i].Kind) < objectKindRank(items[j].Kind)
		}
		return items[i].Type < items[j].Type
	})
	a.objectPalette = items
	a.unitData = units
	if a.objectSelection.Type == "" || !a.paletteContains(a.objectSelection.Kind, a.objectSelection.Type) {
		a.objectSelection = ObjectSelection{Kind: items[0].Kind, Type: items[0].Type, Scale: sourceform.PlacementScaleDefault}
	}
	if a.objectSelection.Scale == 0 {
		a.objectSelection.Scale = sourceform.PlacementScaleDefault
	}
	return nil
}

func objectKindRank(kind ObjectKind) int {
	switch kind {
	case ObjectKindUnit:
		return 0
	case ObjectKindDoodad:
		return 1
	default:
		return 2
	}
}

func loadDoodadPalette(fsys fs.FS) ([]ObjectPaletteItem, error) {
	entries, err := fs.ReadDir(fsys, "maps")
	if err != nil {
		return nil, fmt.Errorf("editor objects: read doodad palette maps directory: %w", err)
	}
	seen := map[string]string{}
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		rel := path.Join("maps", e.Name(), "doodads.toml")
		body, err := fs.ReadFile(fsys, rel)
		if err != nil {
			return nil, fmt.Errorf("editor objects: read doodad palette %s: %w", rel, err)
		}
		var raw rawPaletteDoodadFile
		md, err := toml.Decode(string(body), &raw)
		if err != nil {
			return nil, fmt.Errorf("editor objects: decode doodad palette %s: %w", rel, err)
		}
		if err := rejectPaletteUndecoded(md); err != nil {
			return nil, fmt.Errorf("editor objects: decode doodad palette %s: %w", rel, err)
		}
		for _, d := range raw.Doodad {
			if strings.TrimSpace(d.Asset) == "" {
				return nil, fmt.Errorf("editor objects: %s: doodad %d asset is required", rel, d.ID)
			}
			if _, ok := seen[d.Asset]; !ok {
				seen[d.Asset] = rel
			}
		}
	}
	types := make([]string, 0, len(seen))
	for typ := range seen {
		types = append(types, typ)
	}
	sort.Strings(types)
	out := make([]ObjectPaletteItem, 0, len(types))
	for _, typ := range types {
		out = append(out, ObjectPaletteItem{Kind: ObjectKindDoodad, Type: typ, Source: seen[typ]})
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("editor objects: no doodad palette rows found under data/maps")
	}
	return out, nil
}

func rejectPaletteUndecoded(md toml.MetaData) error {
	if undecoded := md.Undecoded(); len(undecoded) > 0 {
		return fmt.Errorf("unsupported key %q", strings.Join([]string(undecoded[0]), "."))
	}
	return nil
}

func (a *App) ObjectSnapshot() ObjectSnapshot {
	selection := a.objectSelection
	if selection.Scale == 0 {
		selection.Scale = sourceform.PlacementScaleDefault
	}
	palette := make([]ObjectPaletteItem, len(a.objectPalette))
	for i, item := range a.objectPalette {
		palette[i] = item
		palette[i].Active = item.Kind == selection.Kind && item.Type == selection.Type
	}
	var units []sourceform.Entity
	var doodads []sourceform.Doodad
	if a.world != nil {
		units = append([]sourceform.Entity(nil), a.world.Entities...)
		doodads = append([]sourceform.Doodad(nil), a.world.Doodads...)
	}
	return ObjectSnapshot{
		Palette:   palette,
		Selection: selection,
		Units:     units,
		Doodads:   doodads,
	}
}

func (a *App) SelectObject(kind ObjectKind, typeID string) error {
	if !a.paletteContains(kind, typeID) {
		return fmt.Errorf("editor objects: %s %q is not in the loaded palette", kind, typeID)
	}
	a.objectSelection = a.ensureObjectSelection()
	a.objectSelection.Kind = kind
	a.objectSelection.Type = typeID
	a.errText = ""
	a.status = fmt.Sprintf("Object selected: %s %s", kind, typeID)
	return nil
}

func (a *App) SetObjectOwner(owner int) error {
	if owner < 0 || owner > 255 {
		return fmt.Errorf("editor objects: owner %d outside 0..255", owner)
	}
	a.objectSelection = a.ensureObjectSelection()
	a.objectSelection.Owner = owner
	a.errText = ""
	a.status = fmt.Sprintf("Object owner: %d", owner)
	return nil
}

func (a *App) SetObjectRotation(rotation int) error {
	if rotation < 0 || rotation > 65535 {
		return fmt.Errorf("editor objects: rotation %d outside 0..65535", rotation)
	}
	a.objectSelection = a.ensureObjectSelection()
	a.objectSelection.Rotation = rotation
	a.errText = ""
	a.status = fmt.Sprintf("Object rotation: %d", rotation)
	return nil
}

func (a *App) SetObjectScale(scale int) error {
	a.objectSelection = a.ensureObjectSelection()
	a.objectSelection.Scale = ClampPlacementScale(scale)
	a.errText = ""
	a.status = fmt.Sprintf("Object scale: %d", a.objectSelection.Scale)
	return nil
}

func (a *App) SetObjectWalkabilityOverride(enabled bool) {
	a.objectSelection = a.ensureObjectSelection()
	a.objectSelection.OverrideWalkability = enabled
	a.errText = ""
	a.status = fmt.Sprintf("Object walkability override: %v", enabled)
}

func (a *App) PlaceSelectedObjectCell(x, y int) error {
	selection := a.ensureObjectSelection()
	switch selection.Kind {
	case ObjectKindUnit:
		_, err := a.PlaceUnitCell(selection.Type, selection.Owner, x, y, selection.Rotation, selection.Scale, selection.OverrideWalkability)
		return err
	case ObjectKindDoodad:
		_, err := a.PlaceDoodadCell(selection.Type, x, y, selection.Rotation, selection.Scale)
		return err
	default:
		return fmt.Errorf("editor objects: no object selected")
	}
}

func (a *App) PlaceUnitCell(typeID string, owner, x, y, rotation, scale int, overrideWalkability bool) (sourceform.Entity, error) {
	if a.world == nil {
		return sourceform.Entity{}, fmt.Errorf("editor shell: no project loaded")
	}
	if !a.paletteContains(ObjectKindUnit, typeID) {
		return sourceform.Entity{}, fmt.Errorf("editor objects: unit %q is not in the loaded palette", typeID)
	}
	if owner < 0 || owner > 255 {
		return sourceform.Entity{}, fmt.Errorf("editor objects: owner %d outside 0..255", owner)
	}
	if err := validateObjectCell(a.world, x, y); err != nil {
		return sourceform.Entity{}, err
	}
	if rotation < 0 || rotation > 65535 {
		return sourceform.Entity{}, fmt.Errorf("editor objects: rotation %d outside 0..65535", rotation)
	}
	if !overrideWalkability {
		ok, err := a.UnitPlacementWalkableCell(typeID, x, y)
		if err != nil {
			return sourceform.Entity{}, err
		}
		if !ok {
			err := fmt.Errorf("editor objects: unit placement rejected at blocked pathing footprint for cell %d,%d", x, y)
			a.errText = err.Error()
			a.status = a.errText
			return sourceform.Entity{}, err
		}
	}
	ent := sourceform.Entity{
		ID:       nextEntityID(a.world.Entities),
		Type:     typeID,
		Player:   owner,
		Pos:      placementPosForCell(x, y),
		Rotation: rotation,
		Scale:    ClampPlacementScale(scale),
	}
	if err := a.executeCommand(entityPlaceCommand{after: ent}); err != nil {
		return sourceform.Entity{}, err
	}
	a.mode = ModeObjects
	a.status = fmt.Sprintf("Placed unit #%d %s at %d,%d", ent.ID, ent.Type, x, y)
	return ent, nil
}

func (a *App) PlaceDoodadCell(typeID string, x, y, rotation, scale int) (sourceform.Doodad, error) {
	if a.world == nil {
		return sourceform.Doodad{}, fmt.Errorf("editor shell: no project loaded")
	}
	if !a.paletteContains(ObjectKindDoodad, typeID) {
		return sourceform.Doodad{}, fmt.Errorf("editor objects: doodad %q is not in the loaded palette", typeID)
	}
	if err := validateObjectCell(a.world, x, y); err != nil {
		return sourceform.Doodad{}, err
	}
	if rotation < 0 || rotation > 65535 {
		return sourceform.Doodad{}, fmt.Errorf("editor objects: rotation %d outside 0..65535", rotation)
	}
	d := sourceform.Doodad{
		ID:       nextDoodadID(a.world.Doodads),
		Type:     typeID,
		Pos:      placementPosForCell(x, y),
		Rotation: rotation,
		Scale:    ClampPlacementScale(scale),
	}
	if err := a.executeCommand(doodadPlaceCommand{after: d}); err != nil {
		return sourceform.Doodad{}, err
	}
	a.mode = ModeObjects
	a.status = fmt.Sprintf("Placed doodad #%d %s at %d,%d", d.ID, d.Type, x, y)
	return d, nil
}

func (a *App) TransformDoodad(id uint32, pos [2]int, rotation, scale int) error {
	if a.world == nil {
		return fmt.Errorf("editor shell: no project loaded")
	}
	before, err := doodadByID(a.world, id)
	if err != nil {
		return err
	}
	scale = ClampPlacementScale(scale)
	return a.executeCommand(doodadTransformCommand{id: id, beforePos: before.Pos, beforeRotation: before.Rotation, beforeScale: before.Scale, afterPos: pos, afterRotation: rotation, afterScale: scale})
}

func (a *App) DeleteEntity(id uint32) error {
	if a.world == nil {
		return fmt.Errorf("editor shell: no project loaded")
	}
	ent, err := entityByID(a.world, id)
	if err != nil {
		return err
	}
	return a.executeCommand(entityDeleteCommand{before: ent})
}

func (a *App) DeleteDoodad(id uint32) error {
	if a.world == nil {
		return fmt.Errorf("editor shell: no project loaded")
	}
	d, err := doodadByID(a.world, id)
	if err != nil {
		return err
	}
	return a.executeCommand(doodadDeleteCommand{before: d})
}

func (a *App) UnitPlacementWalkableCell(typeID string, x, y int) (bool, error) {
	if a.world == nil {
		return false, fmt.Errorf("editor shell: no project loaded")
	}
	if err := validateObjectCell(a.world, x, y); err != nil {
		return false, err
	}
	u, ok := a.unitData[typeID]
	if !ok {
		return false, fmt.Errorf("editor objects: unit %q metadata is not loaded", typeID)
	}
	px, py, fw, fh, requireBuildable, airborne, err := unitPlacementPathingFootprint(x, y, u)
	if err != nil {
		return false, err
	}
	if airborne {
		return true, nil
	}
	return a.world.PathingFootprintClear(px, py, fw, fh, requireBuildable)
}

func unitPlacementPathingFootprint(x, y int, u data.Unit) (px, py, width, height int, requireBuildable, airborne bool, err error) {
	if u.Footprint > 0 {
		return x * sourceform.PathingScale, y * sourceform.PathingScale, int(u.Footprint), int(u.Footprint), true, false, nil
	}
	switch u.Pathing {
	case data.PathingGround:
		radius := int(u.CollisionClass)
		cx, cy := sourceform.TerrainCellCenterPathingCell(x, y)
		side := radius*2 + 1
		return cx - radius, cy - radius, side, side, false, false, nil
	case data.PathingAir:
		return 0, 0, 0, 0, false, true, nil
	default:
		return 0, 0, 0, 0, false, false, fmt.Errorf("editor objects: unit %q has unsupported pathing class %d", u.ID, u.Pathing)
	}
}

func ClampPlacementScale(scale int) int {
	if scale < sourceform.PlacementScaleMin {
		return sourceform.PlacementScaleMin
	}
	if scale > sourceform.PlacementScaleMax {
		return sourceform.PlacementScaleMax
	}
	return scale
}

func (a *App) ensureObjectSelection() ObjectSelection {
	selection := a.objectSelection
	if selection.Scale == 0 {
		selection.Scale = sourceform.PlacementScaleDefault
	}
	if selection.Type == "" && len(a.objectPalette) > 0 {
		selection.Kind = a.objectPalette[0].Kind
		selection.Type = a.objectPalette[0].Type
	}
	return selection
}

func (a *App) paletteContains(kind ObjectKind, typeID string) bool {
	for _, item := range a.objectPalette {
		if item.Kind == kind && item.Type == typeID {
			return true
		}
	}
	return false
}

func validateObjectCell(w *sourceform.World, x, y int) error {
	if w == nil {
		return fmt.Errorf("editor shell: no project loaded")
	}
	if x < 0 || y < 0 || x >= w.Terrain.Width || y >= w.Terrain.Height {
		return fmt.Errorf("editor objects: cell %d,%d outside %dx%d map", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	return nil
}

func placementPosForCell(x, y int) [2]int {
	return [2]int{x * editorTerrainCellWorldUnit, y * editorTerrainCellWorldUnit}
}

func nextEntityID(entities []sourceform.Entity) uint32 {
	var max uint32
	for _, ent := range entities {
		if ent.ID > max {
			max = ent.ID
		}
	}
	return max + 1
}

func nextDoodadID(doodads []sourceform.Doodad) uint32 {
	var max uint32
	for _, d := range doodads {
		if d.ID > max {
			max = d.ID
		}
	}
	return max + 1
}

func entityByID(w *sourceform.World, id uint32) (sourceform.Entity, error) {
	if w == nil {
		return sourceform.Entity{}, fmt.Errorf("editor shell: no project loaded")
	}
	for _, ent := range w.Entities {
		if ent.ID == id {
			return ent, nil
		}
	}
	return sourceform.Entity{}, fmt.Errorf("editor objects: entity id %d not found", id)
}

func doodadByID(w *sourceform.World, id uint32) (sourceform.Doodad, error) {
	if w == nil {
		return sourceform.Doodad{}, fmt.Errorf("editor shell: no project loaded")
	}
	for _, d := range w.Doodads {
		if d.ID == id {
			return d, nil
		}
	}
	return sourceform.Doodad{}, fmt.Errorf("editor objects: doodad id %d not found", id)
}
