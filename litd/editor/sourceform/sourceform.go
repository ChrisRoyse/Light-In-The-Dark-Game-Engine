// Package sourceform is the editor save path for the VCS-native world source
// directory (#11). It loads a source tree into editable state, writes the
// canonical diff-stable form, and exports archives through worldpack.
package sourceform

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/semver"
)

const (
	worldFile    = "world.toml"
	terrainFile  = "map/terrain.toml"
	heightFile   = "map/height.txt"
	cliffFile    = "map/cliff.txt"
	splatFile    = "map/splat.txt"
	entitiesFile = "map/entities.toml"
	doodadsFile  = "map/doodads.toml"
)

var worldIDRE = regexp.MustCompile(`^[a-z0-9-]+$`)

const (
	PlacementScaleDefault = 1000
	PlacementScaleMin     = 1
	PlacementScaleMax     = 1_000_000
)

// Metadata is the canonical world.toml state.
type Metadata struct {
	Format      int
	ID          string
	Name        string
	Description string
	Authors     []string
	Engine      string
	Players     Players
	SeedPolicy  string
	Seed        *uint64
}

// Players is the source-form players inline table.
type Players struct {
	Min       int `toml:"min"`
	Max       int `toml:"max"`
	Suggested int `toml:"suggested"`
}

// Terrain is the canonical map/terrain.toml state.
type Terrain struct {
	Width          int
	Height         int
	Tileset        string
	Biome          string
	StartLocations []StartLocation
}

// StartLocation is one author-facing player start marker. Player slots are
// editor-facing 1..8 values; runtime loaders can map them to zero-based player
// handles when they materialize a match.
type StartLocation struct {
	Player int
	Cell   [2]int
}

// CliffCell is one source-form cliff grid cell. A ramp cell joins Level and
// Level+1, matching mapdata.Cliff and the sim pathing cliff rule.
type CliffCell struct {
	Level int
	Ramp  bool
}

// SplatWeight is one source-form terrain texture blend cell. The four weights
// are normalized to sum to 255, matching mapdata.SplatWeight.
type SplatWeight struct {
	A uint8 `json:"a"`
	B uint8 `json:"b"`
	C uint8 `json:"c"`
	D uint8 `json:"d"`
}

// Entity is one placed map entity. IDs are stable and sorted on save.
type Entity struct {
	ID       uint32
	Type     string
	Player   int
	Pos      [2]int
	Rotation int
	Scale    int
}

// Doodad is one plain scenery placement. IDs are stable and sorted on save.
type Doodad struct {
	ID       uint32
	Type     string
	Pos      [2]int
	Rotation int
	Scale    int
}

// GridKind names one row-per-line map grid.
type GridKind string

const (
	GridHeight GridKind = "height"
	GridCliff  GridKind = "cliff"
	GridSplat  GridKind = "splat"
)

// World is an editable source-form tree.
type World struct {
	Dir      string
	Metadata Metadata
	Terrain  Terrain
	Height   [][]int
	Cliff    [][]CliffCell
	Splat    [][]SplatWeight
	Entities []Entity
	Doodads  []Doodad

	files map[string][]byte
	dirty bool
}

type rawWorld struct {
	Format      int      `toml:"format"`
	ID          string   `toml:"id"`
	Name        string   `toml:"name"`
	Description string   `toml:"description"`
	Authors     []string `toml:"authors"`
	Engine      string   `toml:"engine"`
	Players     Players  `toml:"players"`
	SeedPolicy  string   `toml:"seed-policy"`
	Seed        *uint64  `toml:"seed"`
}

type rawTerrain struct {
	Width   int                `toml:"width"`
	Height  int                `toml:"height"`
	Tileset string             `toml:"tileset"`
	Biome   string             `toml:"biome"`
	Start   []rawStartLocation `toml:"start"`
}

type rawStartLocation struct {
	Player int   `toml:"player"`
	Cell   []int `toml:"cell"`
}

type rawEntities struct {
	Entities []rawEntity `toml:"entities"`
}

type rawEntity struct {
	ID       uint32 `toml:"id"`
	Type     string `toml:"type"`
	Player   int    `toml:"player"`
	Pos      []int  `toml:"pos"`
	Facing   *int   `toml:"facing"`
	Rotation *int   `toml:"rotation"`
	Scale    *int   `toml:"scale"`
}

type rawDoodads struct {
	Doodads []rawDoodad `toml:"doodads"`
}

type rawDoodad struct {
	ID       uint32 `toml:"id"`
	Type     string `toml:"type"`
	Pos      []int  `toml:"pos"`
	Rotation *int   `toml:"rotation"`
	Scale    *int   `toml:"scale"`
}

// Load reads and validates a source-form world directory.
func Load(dir string) (*World, error) {
	st, err := os.Stat(dir)
	if err != nil {
		return nil, fmt.Errorf("sourceform: load %q: %w", dir, err)
	}
	if !st.IsDir() {
		return nil, fmt.Errorf("sourceform: load %q: not a directory", dir)
	}
	all, err := readTree(dir)
	if err != nil {
		return nil, err
	}
	for _, rel := range []string{worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile, doodadsFile} {
		if _, ok := all[rel]; !ok {
			return nil, fmt.Errorf("sourceform: load %q: missing required file %s", dir, rel)
		}
	}

	meta, err := parseWorld(all[worldFile])
	if err != nil {
		return nil, fmt.Errorf("%s: %w", worldFile, err)
	}
	terrain, err := parseTerrain(all[terrainFile])
	if err != nil {
		return nil, fmt.Errorf("%s: %w", terrainFile, err)
	}
	height, err := parseGrid(all[heightFile], terrain.Width, terrain.Height)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", heightFile, err)
	}
	cliff, err := parseCliffGrid(all[cliffFile], terrain.Width, terrain.Height)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", cliffFile, err)
	}
	splat, err := parseSplatGrid(all[splatFile], terrain.Width, terrain.Height)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", splatFile, err)
	}
	entities, err := parseEntities(all[entitiesFile])
	if err != nil {
		return nil, fmt.Errorf("%s: %w", entitiesFile, err)
	}
	doodads, err := parseDoodads(all[doodadsFile])
	if err != nil {
		return nil, fmt.Errorf("%s: %w", doodadsFile, err)
	}

	files := make(map[string][]byte, len(all))
	for rel, body := range all {
		switch rel {
		case worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile, doodadsFile:
			continue
		default:
			files[rel] = cloneBytes(body)
		}
	}
	return &World{
		Dir:      dir,
		Metadata: meta,
		Terrain:  terrain,
		Height:   height,
		Cliff:    cliff,
		Splat:    splat,
		Entities: entities,
		Doodads:  doodads,
		files:    files,
	}, nil
}

// Dirty reports whether the editor state has unsaved changes.
func (w *World) Dirty() bool { return w != nil && w.dirty }

// MoveEntity changes a placement position/rotation without reshuffling other rows.
func (w *World) MoveEntity(id uint32, pos [2]int, rotation int) error {
	for i := range w.Entities {
		if w.Entities[i].ID != id {
			continue
		}
		scale := w.Entities[i].Scale
		if scale == 0 {
			scale = PlacementScaleDefault
		}
		return w.SetEntityTransform(id, pos, rotation, scale)
	}
	return fmt.Errorf("sourceform: entity id %d not found", id)
}

// AddEntity inserts one entity placement without reusing IDs.
func (w *World) AddEntity(ent Entity) error {
	if w == nil {
		return fmt.Errorf("sourceform: add entity on nil world")
	}
	if err := validateEntity(ent); err != nil {
		return err
	}
	for _, existing := range w.Entities {
		if existing.ID == ent.ID {
			return fmt.Errorf("sourceform: duplicate entity id %d", ent.ID)
		}
	}
	w.Entities = append(w.Entities, ent)
	sortEntities(w.Entities)
	w.dirty = true
	return nil
}

// SetEntityTransform changes one entity's transform fields without reshuffling rows.
func (w *World) SetEntityTransform(id uint32, pos [2]int, rotation, scale int) error {
	if w == nil {
		return fmt.Errorf("sourceform: set entity transform on nil world")
	}
	if err := validatePlacementTransform("entity", id, pos, rotation, scale); err != nil {
		return err
	}
	for i := range w.Entities {
		if w.Entities[i].ID != id {
			continue
		}
		if w.Entities[i].Pos == pos && w.Entities[i].Rotation == rotation && w.Entities[i].Scale == scale {
			return nil
		}
		w.Entities[i].Pos = pos
		w.Entities[i].Rotation = rotation
		w.Entities[i].Scale = scale
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: entity id %d not found", id)
}

// DeleteEntity removes one entity placement while preserving the caller's copy
// for undo/redo restoration.
func (w *World) DeleteEntity(id uint32) error {
	if w == nil {
		return fmt.Errorf("sourceform: delete entity on nil world")
	}
	for i := range w.Entities {
		if w.Entities[i].ID != id {
			continue
		}
		w.Entities = append(w.Entities[:i], w.Entities[i+1:]...)
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: entity id %d not found", id)
}

// AddDoodad inserts one scenery placement without reusing IDs.
func (w *World) AddDoodad(d Doodad) error {
	if w == nil {
		return fmt.Errorf("sourceform: add doodad on nil world")
	}
	if err := validateDoodad(d); err != nil {
		return err
	}
	for _, existing := range w.Doodads {
		if existing.ID == d.ID {
			return fmt.Errorf("sourceform: duplicate doodad id %d", d.ID)
		}
	}
	w.Doodads = append(w.Doodads, d)
	sortDoodads(w.Doodads)
	w.dirty = true
	return nil
}

// SetDoodadTransform changes one scenery placement's transform fields.
func (w *World) SetDoodadTransform(id uint32, pos [2]int, rotation, scale int) error {
	if w == nil {
		return fmt.Errorf("sourceform: set doodad transform on nil world")
	}
	if err := validatePlacementTransform("doodad", id, pos, rotation, scale); err != nil {
		return err
	}
	for i := range w.Doodads {
		if w.Doodads[i].ID != id {
			continue
		}
		if w.Doodads[i].Pos == pos && w.Doodads[i].Rotation == rotation && w.Doodads[i].Scale == scale {
			return nil
		}
		w.Doodads[i].Pos = pos
		w.Doodads[i].Rotation = rotation
		w.Doodads[i].Scale = scale
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: doodad id %d not found", id)
}

// DeleteDoodad removes one scenery placement while preserving the caller's copy
// for undo/redo restoration.
func (w *World) DeleteDoodad(id uint32) error {
	if w == nil {
		return fmt.Errorf("sourceform: delete doodad on nil world")
	}
	for i := range w.Doodads {
		if w.Doodads[i].ID != id {
			continue
		}
		w.Doodads = append(w.Doodads[:i], w.Doodads[i+1:]...)
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: doodad id %d not found", id)
}

// SetMetadataName edits the user-facing world name.
func (w *World) SetMetadataName(name string) error {
	if w == nil {
		return fmt.Errorf("sourceform: set metadata name on nil world")
	}
	if strings.TrimSpace(name) == "" {
		return fmt.Errorf("sourceform: metadata name is required")
	}
	if w.Metadata.Name == name {
		return nil
	}
	w.Metadata.Name = name
	w.dirty = true
	return nil
}

// SetMapMetadata edits the archive-facing map metadata in one atomic operation.
func (w *World) SetMapMetadata(name, description, engineRange string, players Players, tileset, splatSet string) error {
	if w == nil {
		return fmt.Errorf("sourceform: set map metadata on nil world")
	}
	meta := w.Metadata
	terrain := w.Terrain
	meta.Name = name
	meta.Description = description
	meta.Engine = engineRange
	meta.Players = players
	terrain.Tileset = tileset
	terrain.Biome = splatSet
	if _, err := parseWorld(renderWorld(meta)); err != nil {
		return fmt.Errorf("sourceform: invalid metadata: %w", err)
	}
	if _, err := parseTerrain(renderTerrain(terrain)); err != nil {
		return fmt.Errorf("sourceform: invalid terrain metadata: %w", err)
	}
	if metadataEqual(w.Metadata, meta) && terrainEqual(w.Terrain, terrain) {
		return nil
	}
	w.Metadata = meta
	w.Terrain = terrain
	w.dirty = true
	return nil
}

// PutStartLocation inserts or replaces one player start marker.
func (w *World) PutStartLocation(start StartLocation) error {
	if w == nil {
		return fmt.Errorf("sourceform: put start location on nil world")
	}
	if err := validateStartLocation(start, w.Terrain.Width, w.Terrain.Height); err != nil {
		return err
	}
	if ok, err := w.StartLocationWalkableCell(start.Cell[0], start.Cell[1]); err != nil {
		return err
	} else if !ok {
		return fmt.Errorf("sourceform: start location player %d at %d,%d is not on walkable ground", start.Player, start.Cell[0], start.Cell[1])
	}
	for i := range w.Terrain.StartLocations {
		if w.Terrain.StartLocations[i].Player != start.Player {
			continue
		}
		if w.Terrain.StartLocations[i] == start {
			return nil
		}
		w.Terrain.StartLocations[i] = start
		sortStartLocations(w.Terrain.StartLocations)
		w.dirty = true
		return nil
	}
	next := append(append([]StartLocation(nil), w.Terrain.StartLocations...), start)
	if err := validateStartLocations(next, w.Terrain.Width, w.Terrain.Height); err != nil {
		return err
	}
	w.Terrain.StartLocations = next
	sortStartLocations(w.Terrain.StartLocations)
	w.dirty = true
	return nil
}

// AddStartLocation inserts one start marker and rejects duplicate player slots.
func (w *World) AddStartLocation(start StartLocation) error {
	if w == nil {
		return fmt.Errorf("sourceform: add start location on nil world")
	}
	if _, ok := w.StartLocationByPlayer(start.Player); ok {
		return fmt.Errorf("sourceform: duplicate start location for player %d", start.Player)
	}
	return w.PutStartLocation(start)
}

// RemoveStartLocation deletes a player start marker.
func (w *World) RemoveStartLocation(player int) error {
	if w == nil {
		return fmt.Errorf("sourceform: remove start location on nil world")
	}
	for i := range w.Terrain.StartLocations {
		if w.Terrain.StartLocations[i].Player != player {
			continue
		}
		next := append([]StartLocation(nil), w.Terrain.StartLocations[:i]...)
		next = append(next, w.Terrain.StartLocations[i+1:]...)
		if err := validateStartLocations(next, w.Terrain.Width, w.Terrain.Height); err != nil {
			return err
		}
		w.Terrain.StartLocations = next
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: start location for player %d not found", player)
}

// StartLocationByPlayer returns the marker for an author-facing player slot.
func (w *World) StartLocationByPlayer(player int) (StartLocation, bool) {
	if w == nil {
		return StartLocation{}, false
	}
	for _, start := range w.Terrain.StartLocations {
		if start.Player == player {
			return start, true
		}
	}
	return StartLocation{}, false
}

// SetGridCell edits one terrain grid cell.
func (w *World) SetGridCell(kind GridKind, x, y, value int) error {
	if kind == GridCliff {
		return w.SetCliffCell(x, y, CliffCell{Level: value})
	}
	if kind == GridSplat {
		cell, err := SplatWeightForLayer(value)
		if err != nil {
			return err
		}
		return w.SetSplatCell(x, y, cell)
	}
	grid, err := w.grid(kind)
	if err != nil {
		return err
	}
	if y < 0 || y >= len(grid) || x < 0 || len(grid) == 0 || x >= len(grid[y]) {
		return fmt.Errorf("sourceform: %s cell (%d,%d) outside %dx%d grid", kind, x, y, w.Terrain.Width, w.Terrain.Height)
	}
	if grid[y][x] == value {
		return nil
	}
	grid[y][x] = value
	w.dirty = true
	return nil
}

// SetSplatCell edits one texture-blend grid cell.
func (w *World) SetSplatCell(x, y int, cell SplatWeight) error {
	if w == nil {
		return fmt.Errorf("sourceform: set splat cell on nil world")
	}
	if err := validateSplatCell(cell); err != nil {
		return err
	}
	if y < 0 || y >= len(w.Splat) || x < 0 || x >= len(w.Splat[y]) {
		return fmt.Errorf("sourceform: splat cell (%d,%d) outside %dx%d grid", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	if w.Splat[y][x] == cell {
		return nil
	}
	w.Splat[y][x] = cell
	w.dirty = true
	return nil
}

// SetCliffCell edits one cliff/ramp grid cell.
func (w *World) SetCliffCell(x, y int, cell CliffCell) error {
	if w == nil {
		return fmt.Errorf("sourceform: set cliff cell on nil world")
	}
	if err := validateCliffCell(cell); err != nil {
		return err
	}
	if y < 0 || y >= len(w.Cliff) || x < 0 || x >= len(w.Cliff[y]) {
		return fmt.Errorf("sourceform: cliff cell (%d,%d) outside %dx%d grid", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	if w.Cliff[y][x] == cell {
		return nil
	}
	w.Cliff[y][x] = cell
	w.dirty = true
	return nil
}

// SetScript updates a Lua script file under scripts/.
func (w *World) SetScript(rel string, body []byte) error {
	clean, err := cleanRel(rel)
	if err != nil {
		return err
	}
	if !strings.HasPrefix(clean, "scripts/") || clean == "scripts/" {
		return fmt.Errorf("sourceform: script path %q must be under scripts/", rel)
	}
	return w.setPassthrough(clean, body)
}

// SetPassthroughFile updates an optional data/scripts/locale/assets file.
func (w *World) SetPassthroughFile(rel string, body []byte) error {
	clean, err := cleanRel(rel)
	if err != nil {
		return err
	}
	if !isPassthroughRel(clean) {
		return fmt.Errorf("sourceform: %q is not an editable passthrough file", rel)
	}
	return w.setPassthrough(clean, body)
}

func (w *World) setPassthrough(rel string, body []byte) error {
	if w.files == nil {
		w.files = map[string][]byte{}
	}
	if bytes.Equal(w.files[rel], body) {
		return nil
	}
	w.files[rel] = cloneBytes(body)
	w.dirty = true
	return nil
}

// Save writes the current editor state to dir in canonical source form. Passing
// an empty dir saves back to the loaded directory.
func (w *World) Save(dir string) error {
	if w == nil {
		return fmt.Errorf("sourceform: save nil world")
	}
	if dir == "" {
		dir = w.Dir
	}
	if dir == "" {
		return fmt.Errorf("sourceform: save requires a destination directory")
	}
	if err := validateWorld(w); err != nil {
		return err
	}
	files := w.renderFiles()
	rels := make([]string, 0, len(files))
	for rel := range files {
		rels = append(rels, rel)
	}
	sort.Strings(rels)
	for _, rel := range rels {
		if err := writeFileIfChanged(dir, rel, files[rel]); err != nil {
			return err
		}
	}
	w.Dir = dir
	w.dirty = false
	return nil
}

// ExportOptions controls .litdworld archive export.
type ExportOptions struct {
	EngineRange string
	Hosting     worldpack.Hosting
	Categories  map[string]string
}

// ExportArchive saves pending edits, then packs the source tree with worldpack.
func (w *World) ExportArchive(outPath string, opts ExportOptions) error {
	if w == nil {
		return fmt.Errorf("sourceform: export nil world")
	}
	if w.Dir == "" {
		return fmt.Errorf("sourceform: export requires a source directory")
	}
	if w.dirty {
		if err := w.Save(w.Dir); err != nil {
			return err
		}
	}
	opts = w.fillExportOptions(opts)
	return ExportArchive(w.Dir, outPath, opts)
}

// ExportArchive packs an already-saved source tree with the deterministic
// worldpack archive builder.
func ExportArchive(srcDir, outPath string, opts ExportOptions) error {
	return worldpack.Pack(srcDir, outPath, opts.EngineRange, opts.Hosting, opts.Categories)
}

func (w *World) fillExportOptions(opts ExportOptions) ExportOptions {
	if opts.EngineRange == "" {
		opts.EngineRange = w.Metadata.Engine
	}
	if opts.Hosting.Author == "" {
		opts.Hosting.Author = strings.Join(w.Metadata.Authors, ", ")
	}
	if opts.Hosting.Title == "" {
		opts.Hosting.Title = w.Metadata.Name
	}
	if opts.Hosting.Description == "" {
		opts.Hosting.Description = w.Metadata.Description
	}
	if opts.Hosting.Players == (worldpack.Players{}) {
		opts.Hosting.Players = worldpack.Players{
			Min:       w.Metadata.Players.Min,
			Max:       w.Metadata.Players.Max,
			Suggested: w.Metadata.Players.Suggested,
		}
	}
	if opts.Hosting.Tileset == "" {
		opts.Hosting.Tileset = w.Terrain.Tileset
	}
	if opts.Hosting.SplatSet == "" {
		opts.Hosting.SplatSet = w.Terrain.Biome
	}
	if len(opts.Hosting.StartLocations) == 0 {
		for _, start := range w.Terrain.StartLocations {
			opts.Hosting.StartLocations = append(opts.Hosting.StartLocations, worldpack.StartLocation{
				Player: start.Player,
				Cell:   start.Cell,
			})
		}
	}
	return opts
}

func (w *World) grid(kind GridKind) ([][]int, error) {
	switch kind {
	case GridHeight:
		return w.Height, nil
	default:
		return nil, fmt.Errorf("sourceform: unknown grid %q", kind)
	}
}

func readTree(root string) (map[string][]byte, error) {
	files := map[string][]byte{}
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if rel == "." {
			return nil
		}
		if isVCSMetadataRel(rel) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if d.IsDir() {
			if !isAllowedDir(rel) {
				return fmt.Errorf("sourceform: unknown directory %q", rel)
			}
			return nil
		}
		if !d.Type().IsRegular() {
			return fmt.Errorf("sourceform: %q is not a regular file", rel)
		}
		if !isAllowedRel(rel) {
			return fmt.Errorf("sourceform: unknown file %q", rel)
		}
		body, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		files[rel] = body
		return nil
	})
	if err != nil {
		return nil, err
	}
	return files, nil
}

func isVCSMetadataRel(rel string) bool {
	switch rel {
	case ".git", ".gitattributes", ".gitignore", ".gitmodules":
		return true
	default:
		return false
	}
}

func isAllowedDir(rel string) bool {
	switch rel {
	case "map", "data", "scripts", "locale", "assets":
		return true
	}
	top := strings.Split(rel, "/")[0]
	return top == "data" || top == "scripts" || top == "locale" || top == "assets"
}

func isAllowedRel(rel string) bool {
	switch rel {
	case worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile, doodadsFile:
		return true
	}
	return isPassthroughRel(rel)
}

func isPassthroughRel(rel string) bool {
	for _, prefix := range []string{"data/", "scripts/", "locale/", "assets/"} {
		if strings.HasPrefix(rel, prefix) && rel != prefix {
			return true
		}
	}
	return false
}

func cleanRel(rel string) (string, error) {
	if rel == "" {
		return "", fmt.Errorf("sourceform: empty relative path")
	}
	if filepath.IsAbs(rel) {
		return "", fmt.Errorf("sourceform: absolute path %q is not allowed", rel)
	}
	clean := filepath.ToSlash(filepath.Clean(filepath.FromSlash(rel)))
	if clean == "." || clean == ".." || strings.HasPrefix(clean, "../") {
		return "", fmt.Errorf("sourceform: unsafe relative path %q", rel)
	}
	return clean, nil
}

func parseWorld(body []byte) (Metadata, error) {
	var raw rawWorld
	md, err := toml.Decode(string(body), &raw)
	if err != nil {
		return Metadata{}, err
	}
	if err := rejectUndecoded(md); err != nil {
		return Metadata{}, err
	}
	meta := Metadata(raw)
	if meta.Format != 1 {
		return Metadata{}, fmt.Errorf("format must be 1, got %d", meta.Format)
	}
	if !worldIDRE.MatchString(meta.ID) {
		return Metadata{}, fmt.Errorf("id %q must match [a-z0-9-]+", meta.ID)
	}
	if meta.Name == "" || meta.Description == "" || meta.Engine == "" {
		return Metadata{}, fmt.Errorf("name, description, and engine are required")
	}
	if !semver.ValidRange(meta.Engine) {
		return Metadata{}, fmt.Errorf("engine range %q is not well-formed", meta.Engine)
	}
	if len(meta.Authors) == 0 {
		return Metadata{}, fmt.Errorf("authors must contain at least one entry")
	}
	if meta.Players.Min <= 0 || meta.Players.Max < meta.Players.Min || meta.Players.Max > 8 || meta.Players.Suggested < meta.Players.Min || meta.Players.Suggested > meta.Players.Max {
		return Metadata{}, fmt.Errorf("players must satisfy 0 < min <= suggested <= max <= 8")
	}
	switch meta.SeedPolicy {
	case "host":
		if meta.Seed != nil {
			return Metadata{}, fmt.Errorf("seed-policy host must not set seed")
		}
	case "fixed":
		if meta.Seed == nil {
			return Metadata{}, fmt.Errorf("seed-policy fixed requires seed")
		}
	default:
		return Metadata{}, fmt.Errorf("seed-policy must be host or fixed, got %q", meta.SeedPolicy)
	}
	return meta, nil
}

func parseTerrain(body []byte) (Terrain, error) {
	var raw rawTerrain
	md, err := toml.Decode(string(body), &raw)
	if err != nil {
		return Terrain{}, err
	}
	if err := rejectUndecoded(md); err != nil {
		return Terrain{}, err
	}
	t := Terrain{Width: raw.Width, Height: raw.Height, Tileset: raw.Tileset, Biome: raw.Biome}
	if t.Width <= 0 || t.Height <= 0 {
		return Terrain{}, fmt.Errorf("width and height must be positive")
	}
	if t.Tileset == "" {
		return Terrain{}, fmt.Errorf("tileset is required")
	}
	for _, rawStart := range raw.Start {
		if len(rawStart.Cell) != 2 {
			return Terrain{}, fmt.Errorf("start location player %d cell must have exactly two integers", rawStart.Player)
		}
		t.StartLocations = append(t.StartLocations, StartLocation{Player: rawStart.Player, Cell: [2]int{rawStart.Cell[0], rawStart.Cell[1]}})
	}
	if err := validateStartLocations(t.StartLocations, t.Width, t.Height); err != nil {
		return Terrain{}, err
	}
	sortStartLocations(t.StartLocations)
	return t, nil
}

func parseEntities(body []byte) ([]Entity, error) {
	var raw rawEntities
	md, err := toml.Decode(string(body), &raw)
	if err != nil {
		return nil, err
	}
	if err := rejectUndecoded(md); err != nil {
		return nil, err
	}
	out := make([]Entity, 0, len(raw.Entities))
	seen := map[uint32]bool{}
	for _, e := range raw.Entities {
		if e.ID == 0 {
			return nil, fmt.Errorf("entity id must be non-zero")
		}
		if seen[e.ID] {
			return nil, fmt.Errorf("duplicate entity id %d", e.ID)
		}
		seen[e.ID] = true
		if e.Type == "" {
			return nil, fmt.Errorf("entity %d type is required", e.ID)
		}
		if e.Player < 0 || e.Player > 255 {
			return nil, fmt.Errorf("entity %d player %d outside 0..255", e.ID, e.Player)
		}
		if len(e.Pos) != 2 {
			return nil, fmt.Errorf("entity %d pos must have exactly two integers", e.ID)
		}
		rotation, err := parsePlacementRotation("entity", e.ID, e.Rotation, e.Facing)
		if err != nil {
			return nil, err
		}
		scale, err := parsePlacementScale("entity", e.ID, e.Scale)
		if err != nil {
			return nil, err
		}
		out = append(out, Entity{ID: e.ID, Type: e.Type, Player: e.Player, Pos: [2]int{e.Pos[0], e.Pos[1]}, Rotation: rotation, Scale: scale})
	}
	sortEntities(out)
	return out, nil
}

func parseDoodads(body []byte) ([]Doodad, error) {
	var raw rawDoodads
	md, err := toml.Decode(string(body), &raw)
	if err != nil {
		return nil, err
	}
	if err := rejectUndecoded(md); err != nil {
		return nil, err
	}
	out := make([]Doodad, 0, len(raw.Doodads))
	seen := map[uint32]bool{}
	for _, d := range raw.Doodads {
		if d.ID == 0 {
			return nil, fmt.Errorf("doodad id must be non-zero")
		}
		if seen[d.ID] {
			return nil, fmt.Errorf("duplicate doodad id %d", d.ID)
		}
		seen[d.ID] = true
		if d.Type == "" {
			return nil, fmt.Errorf("doodad %d type is required", d.ID)
		}
		if len(d.Pos) != 2 {
			return nil, fmt.Errorf("doodad %d pos must have exactly two integers", d.ID)
		}
		rotation, err := parsePlacementRotation("doodad", d.ID, d.Rotation, nil)
		if err != nil {
			return nil, err
		}
		scale, err := parsePlacementScale("doodad", d.ID, d.Scale)
		if err != nil {
			return nil, err
		}
		out = append(out, Doodad{ID: d.ID, Type: d.Type, Pos: [2]int{d.Pos[0], d.Pos[1]}, Rotation: rotation, Scale: scale})
	}
	sortDoodads(out)
	return out, nil
}

func parsePlacementRotation(kind string, id uint32, rotation, legacyFacing *int) (int, error) {
	if rotation != nil && legacyFacing != nil {
		return 0, fmt.Errorf("%s %d must not set both rotation and legacy facing", kind, id)
	}
	v := 0
	if legacyFacing != nil {
		v = *legacyFacing
	}
	if rotation != nil {
		v = *rotation
	}
	if v < 0 || v > 65535 {
		return 0, fmt.Errorf("%s %d rotation %d outside 0..65535", kind, id, v)
	}
	return v, nil
}

func parsePlacementScale(kind string, id uint32, raw *int) (int, error) {
	scale := PlacementScaleDefault
	if raw != nil {
		scale = *raw
	}
	if scale < PlacementScaleMin || scale > PlacementScaleMax {
		return 0, fmt.Errorf("%s %d scale %d outside %d..%d", kind, id, scale, PlacementScaleMin, PlacementScaleMax)
	}
	return scale, nil
}

func parseGrid(body []byte, width, height int) ([][]int, error) {
	text := string(body)
	if strings.HasPrefix(text, "\ufeff") {
		return nil, fmt.Errorf("UTF-8 BOM is not allowed")
	}
	text = strings.TrimSuffix(text, "\n")
	if text == "" {
		if height == 0 {
			return nil, nil
		}
		return nil, fmt.Errorf("grid is empty, want %d rows", height)
	}
	lines := strings.Split(text, "\n")
	if len(lines) != height {
		return nil, fmt.Errorf("got %d rows, want %d", len(lines), height)
	}
	grid := make([][]int, height)
	for y, line := range lines {
		fields := strings.Fields(line)
		if len(fields) != width {
			return nil, fmt.Errorf("row %d has %d columns, want %d", y, len(fields), width)
		}
		row := make([]int, width)
		for x, f := range fields {
			n, err := strconv.Atoi(f)
			if err != nil {
				return nil, fmt.Errorf("row %d col %d: %w", y, x, err)
			}
			row[x] = n
		}
		grid[y] = row
	}
	return grid, nil
}

func parseCliffGrid(body []byte, width, height int) ([][]CliffCell, error) {
	text := string(body)
	if strings.HasPrefix(text, "\ufeff") {
		return nil, fmt.Errorf("UTF-8 BOM is not allowed")
	}
	text = strings.TrimSuffix(text, "\n")
	if text == "" {
		if height == 0 {
			return nil, nil
		}
		return nil, fmt.Errorf("grid is empty, want %d rows", height)
	}
	lines := strings.Split(text, "\n")
	if len(lines) != height {
		return nil, fmt.Errorf("got %d rows, want %d", len(lines), height)
	}
	grid := make([][]CliffCell, height)
	for y, line := range lines {
		fields := strings.Fields(line)
		if len(fields) != width {
			return nil, fmt.Errorf("row %d has %d columns, want %d", y, len(fields), width)
		}
		row := make([]CliffCell, width)
		for x, f := range fields {
			cell, err := parseCliffCell(f)
			if err != nil {
				return nil, fmt.Errorf("row %d col %d: %w", y, x, err)
			}
			row[x] = cell
		}
		grid[y] = row
	}
	if err := validateCliffRamps(grid); err != nil {
		return nil, err
	}
	return grid, nil
}

func parseCliffCell(text string) (CliffCell, error) {
	ramp := false
	if strings.HasPrefix(strings.ToLower(text), "r") {
		ramp = true
		text = text[1:]
	}
	n, err := strconv.Atoi(text)
	if err != nil {
		return CliffCell{}, err
	}
	cell := CliffCell{Level: n, Ramp: ramp}
	if err := validateCliffCell(cell); err != nil {
		return CliffCell{}, err
	}
	return cell, nil
}

func parseSplatGrid(body []byte, width, height int) ([][]SplatWeight, error) {
	text := string(body)
	if strings.HasPrefix(text, "\ufeff") {
		return nil, fmt.Errorf("UTF-8 BOM is not allowed")
	}
	text = strings.TrimSuffix(text, "\n")
	if text == "" {
		if height == 0 {
			return nil, nil
		}
		return nil, fmt.Errorf("grid is empty, want %d rows", height)
	}
	lines := strings.Split(text, "\n")
	if len(lines) != height {
		return nil, fmt.Errorf("got %d rows, want %d", len(lines), height)
	}
	grid := make([][]SplatWeight, height)
	for y, line := range lines {
		fields := strings.Fields(line)
		if len(fields) != width {
			return nil, fmt.Errorf("row %d has %d columns, want %d", y, len(fields), width)
		}
		row := make([]SplatWeight, width)
		for x, f := range fields {
			cell, err := parseSplatCell(f)
			if err != nil {
				return nil, fmt.Errorf("row %d col %d: %w", y, x, err)
			}
			row[x] = cell
		}
		grid[y] = row
	}
	return grid, nil
}

func parseSplatCell(text string) (SplatWeight, error) {
	parts := strings.Split(text, ",")
	if len(parts) != 4 {
		return SplatWeight{}, fmt.Errorf("splat %q must have four comma-separated weights", text)
	}
	var vals [4]uint8
	sum := 0
	for i, p := range parts {
		v, err := strconv.ParseUint(p, 10, 8)
		if err != nil {
			return SplatWeight{}, fmt.Errorf("splat %q: %w", text, err)
		}
		vals[i] = uint8(v)
		sum += int(v)
	}
	cell := SplatWeight{A: vals[0], B: vals[1], C: vals[2], D: vals[3]}
	if sum != 255 {
		return SplatWeight{}, fmt.Errorf("splat %q weights sum to %d, want 255", text, sum)
	}
	return cell, nil
}

func rejectUndecoded(md toml.MetaData) error {
	if undecoded := md.Undecoded(); len(undecoded) > 0 {
		return fmt.Errorf("unsupported key %q", strings.Join([]string(undecoded[0]), "."))
	}
	return nil
}

func validateWorld(w *World) error {
	if _, err := parseWorld(renderWorld(w.Metadata)); err != nil {
		return fmt.Errorf("sourceform: invalid metadata: %w", err)
	}
	if _, err := parseTerrain(renderTerrain(w.Terrain)); err != nil {
		return fmt.Errorf("sourceform: invalid terrain: %w", err)
	}
	if err := validateStartLocations(w.Terrain.StartLocations, w.Terrain.Width, w.Terrain.Height); err != nil {
		return err
	}
	for _, start := range w.Terrain.StartLocations {
		ok, err := w.StartLocationWalkableCell(start.Cell[0], start.Cell[1])
		if err != nil {
			return err
		}
		if !ok {
			return fmt.Errorf("sourceform: start location player %d at %d,%d is not on walkable ground", start.Player, start.Cell[0], start.Cell[1])
		}
	}
	if err := validateGrid(w.Height, w.Terrain.Width, w.Terrain.Height, "height"); err != nil {
		return err
	}
	if err := validateCliffGrid(w.Cliff, w.Terrain.Width, w.Terrain.Height); err != nil {
		return err
	}
	if err := validateSplatGrid(w.Splat, w.Terrain.Width, w.Terrain.Height); err != nil {
		return err
	}
	seen := map[uint32]bool{}
	for _, e := range w.Entities {
		if seen[e.ID] {
			return fmt.Errorf("sourceform: duplicate entity id %d", e.ID)
		}
		if err := validateEntity(e); err != nil {
			return err
		}
		seen[e.ID] = true
	}
	seenDoodads := map[uint32]bool{}
	for _, d := range w.Doodads {
		if seenDoodads[d.ID] {
			return fmt.Errorf("sourceform: duplicate doodad id %d", d.ID)
		}
		if err := validateDoodad(d); err != nil {
			return err
		}
		seenDoodads[d.ID] = true
	}
	for rel := range w.files {
		if !isPassthroughRel(rel) {
			return fmt.Errorf("sourceform: invalid passthrough file %q", rel)
		}
	}
	return nil
}

func validateStartLocations(starts []StartLocation, width, height int) error {
	if len(starts) < 1 || len(starts) > 8 {
		return fmt.Errorf("sourceform: start locations count %d outside 1..8", len(starts))
	}
	seen := map[int]bool{}
	for _, start := range starts {
		if seen[start.Player] {
			return fmt.Errorf("sourceform: duplicate start location for player %d", start.Player)
		}
		seen[start.Player] = true
		if err := validateStartLocation(start, width, height); err != nil {
			return err
		}
	}
	return nil
}

func validateStartLocation(start StartLocation, width, height int) error {
	if start.Player < 1 || start.Player > 8 {
		return fmt.Errorf("sourceform: start location player %d outside 1..8", start.Player)
	}
	if start.Cell[0] < 0 || start.Cell[1] < 0 || start.Cell[0] >= width || start.Cell[1] >= height {
		return fmt.Errorf("sourceform: start location player %d cell %d,%d outside %dx%d map", start.Player, start.Cell[0], start.Cell[1], width, height)
	}
	return nil
}

func validateEntity(e Entity) error {
	if e.ID == 0 || e.Type == "" || e.Player < 0 || e.Player > 255 {
		return fmt.Errorf("sourceform: invalid entity %+v", e)
	}
	return validatePlacementTransform("entity", e.ID, e.Pos, e.Rotation, e.Scale)
}

func validateDoodad(d Doodad) error {
	if d.ID == 0 || d.Type == "" {
		return fmt.Errorf("sourceform: invalid doodad %+v", d)
	}
	return validatePlacementTransform("doodad", d.ID, d.Pos, d.Rotation, d.Scale)
}

func validatePlacementTransform(kind string, id uint32, pos [2]int, rotation, scale int) error {
	if rotation < 0 || rotation > 65535 {
		return fmt.Errorf("sourceform: %s %d rotation %d outside 0..65535", kind, id, rotation)
	}
	if scale < PlacementScaleMin || scale > PlacementScaleMax {
		return fmt.Errorf("sourceform: %s %d scale %d outside %d..%d", kind, id, scale, PlacementScaleMin, PlacementScaleMax)
	}
	if pos[0] < 0 || pos[1] < 0 {
		return fmt.Errorf("sourceform: %s %d pos [%d,%d] must be non-negative", kind, id, pos[0], pos[1])
	}
	return nil
}

func validateGrid(grid [][]int, width, height int, name string) error {
	if len(grid) != height {
		return fmt.Errorf("sourceform: %s grid has %d rows, want %d", name, len(grid), height)
	}
	for y, row := range grid {
		if len(row) != width {
			return fmt.Errorf("sourceform: %s grid row %d has %d columns, want %d", name, y, len(row), width)
		}
	}
	return nil
}

func validateCliffGrid(grid [][]CliffCell, width, height int) error {
	if len(grid) != height {
		return fmt.Errorf("sourceform: cliff grid has %d rows, want %d", len(grid), height)
	}
	for y, row := range grid {
		if len(row) != width {
			return fmt.Errorf("sourceform: cliff grid row %d has %d columns, want %d", y, len(row), width)
		}
		for x, cell := range row {
			if err := validateCliffCell(cell); err != nil {
				return fmt.Errorf("sourceform: cliff cell (%d,%d): %w", x, y, err)
			}
		}
	}
	return validateCliffRamps(grid)
}

func validateCliffCell(cell CliffCell) error {
	if cell.Level < 0 || cell.Level > 126 {
		return fmt.Errorf("sourceform: cliff level %d outside 0..126", cell.Level)
	}
	return nil
}

func validateSplatGrid(grid [][]SplatWeight, width, height int) error {
	if len(grid) != height {
		return fmt.Errorf("sourceform: splat grid has %d rows, want %d", len(grid), height)
	}
	for y, row := range grid {
		if len(row) != width {
			return fmt.Errorf("sourceform: splat grid row %d has %d columns, want %d", y, len(row), width)
		}
		for x, cell := range row {
			if err := validateSplatCell(cell); err != nil {
				return fmt.Errorf("sourceform: splat cell (%d,%d): %w", x, y, err)
			}
		}
	}
	return nil
}

func validateSplatCell(cell SplatWeight) error {
	sum := int(cell.A) + int(cell.B) + int(cell.C) + int(cell.D)
	if sum != 255 {
		return fmt.Errorf("sourceform: splat weights sum to %d, want 255", sum)
	}
	return nil
}

// StartLocationWalkableCell reports whether a player start can stand on a cell
// using the source-form cliff pathability rule.
func (w *World) StartLocationWalkableCell(x, y int) (bool, error) {
	if w == nil {
		return false, fmt.Errorf("sourceform: start walkability on nil world")
	}
	if x < 0 || y < 0 || x >= w.Terrain.Width || y >= w.Terrain.Height {
		return false, fmt.Errorf("sourceform: start location cell %d,%d outside %dx%d map", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	if w.Terrain.Width == 1 && w.Terrain.Height == 1 {
		return true, nil
	}
	for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
		nx, ny := x+d[0], y+d[1]
		if nx < 0 || ny < 0 || nx >= w.Terrain.Width || ny >= w.Terrain.Height {
			continue
		}
		ok, err := w.CliffStepLegal(x, y, nx, ny)
		if err != nil {
			return false, err
		}
		if ok {
			return true, nil
		}
	}
	return false, nil
}

// SplatCell returns one texture-blend cell.
func (w *World) SplatCell(x, y int) (SplatWeight, error) {
	if w == nil {
		return SplatWeight{}, fmt.Errorf("sourceform: splat cell on nil world")
	}
	if y < 0 || y >= len(w.Splat) || x < 0 || x >= len(w.Splat[y]) {
		return SplatWeight{}, fmt.Errorf("sourceform: splat cell (%d,%d) outside %dx%d grid", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	return w.Splat[y][x], nil
}

// SplatWeightForLayer returns a fully painted one-hot splat cell.
func SplatWeightForLayer(layer int) (SplatWeight, error) {
	if layer < 0 || layer > 3 {
		return SplatWeight{}, fmt.Errorf("sourceform: splat layer %d outside 0..3", layer)
	}
	var weights [4]uint8
	weights[layer] = 255
	return SplatWeight{A: weights[0], B: weights[1], C: weights[2], D: weights[3]}, nil
}

// SplatWeightAt returns one of the four normalized weights by layer index.
func SplatWeightAt(cell SplatWeight, layer int) (uint8, error) {
	switch layer {
	case 0:
		return cell.A, nil
	case 1:
		return cell.B, nil
	case 2:
		return cell.C, nil
	case 3:
		return cell.D, nil
	default:
		return 0, fmt.Errorf("sourceform: splat layer %d outside 0..3", layer)
	}
}

func validateCliffRamps(grid [][]CliffCell) error {
	for y, row := range grid {
		for x, cell := range row {
			if !cell.Ramp {
				continue
			}
			lo, hi := cell.Level, cell.Level+1
			hasLo, hasHi := false, false
			for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
				nx, ny := x+d[0], y+d[1]
				if ny < 0 || ny >= len(grid) || nx < 0 || nx >= len(grid[ny]) {
					continue
				}
				nlo, nhi := cliffSpan(grid[ny][nx])
				if nlo <= lo && lo <= nhi {
					hasLo = true
				}
				if nlo <= hi && hi <= nhi {
					hasHi = true
				}
			}
			if !hasLo || !hasHi {
				return fmt.Errorf("sourceform: ramp at (%d,%d) must touch both levels %d and %d", x, y, lo, hi)
			}
		}
	}
	return nil
}

func cliffSpan(cell CliffCell) (int, int) {
	lo, hi := cell.Level, cell.Level
	if cell.Ramp {
		hi++
	}
	return lo, hi
}

// CliffStepLegal reports whether two adjacent cliff cells share at least one
// walkable level span, the same pathing rule used by the sim grid.
func (w *World) CliffStepLegal(ax, ay, bx, by int) (bool, error) {
	if w == nil {
		return false, fmt.Errorf("sourceform: cliff pathability on nil world")
	}
	a, err := w.CliffCell(ax, ay)
	if err != nil {
		return false, err
	}
	b, err := w.CliffCell(bx, by)
	if err != nil {
		return false, err
	}
	alo, ahi := cliffSpan(a)
	blo, bhi := cliffSpan(b)
	return alo <= bhi && blo <= ahi, nil
}

// CliffCell returns one cliff/ramp cell.
func (w *World) CliffCell(x, y int) (CliffCell, error) {
	if w == nil {
		return CliffCell{}, fmt.Errorf("sourceform: cliff cell on nil world")
	}
	if y < 0 || y >= len(w.Cliff) || x < 0 || x >= len(w.Cliff[y]) {
		return CliffCell{}, fmt.Errorf("sourceform: cliff cell (%d,%d) outside %dx%d grid", x, y, w.Terrain.Width, w.Terrain.Height)
	}
	return w.Cliff[y][x], nil
}

func (w *World) renderFiles() map[string][]byte {
	files := make(map[string][]byte, len(w.files)+7)
	for rel, body := range w.files {
		files[rel] = cloneBytes(body)
	}
	files[worldFile] = renderWorld(w.Metadata)
	files[terrainFile] = renderTerrain(w.Terrain)
	files[heightFile] = renderGrid(w.Height)
	files[cliffFile] = renderCliffGrid(w.Cliff)
	files[splatFile] = renderSplatGrid(w.Splat)
	files[entitiesFile] = renderEntities(w.Entities)
	files[doodadsFile] = renderDoodads(w.Doodads)
	return files
}

func renderWorld(m Metadata) []byte {
	var b strings.Builder
	fmt.Fprintf(&b, "format = %d\n", m.Format)
	fmt.Fprintf(&b, "id = %s\n", strconv.Quote(m.ID))
	fmt.Fprintf(&b, "name = %s\n", strconv.Quote(m.Name))
	fmt.Fprintf(&b, "description = %s\n", strconv.Quote(m.Description))
	fmt.Fprintf(&b, "authors = %s\n", renderStringSlice(m.Authors))
	fmt.Fprintf(&b, "engine = %s\n", strconv.Quote(m.Engine))
	fmt.Fprintf(&b, "players = { min = %d, max = %d, suggested = %d }\n", m.Players.Min, m.Players.Max, m.Players.Suggested)
	fmt.Fprintf(&b, "seed-policy = %s\n", strconv.Quote(m.SeedPolicy))
	if m.Seed != nil {
		fmt.Fprintf(&b, "seed = %d\n", *m.Seed)
	}
	return []byte(b.String())
}

func renderTerrain(t Terrain) []byte {
	var b strings.Builder
	fmt.Fprintf(&b, "width = %d\n", t.Width)
	fmt.Fprintf(&b, "height = %d\n", t.Height)
	fmt.Fprintf(&b, "tileset = %s\n", strconv.Quote(t.Tileset))
	if t.Biome != "" {
		fmt.Fprintf(&b, "biome = %s\n", strconv.Quote(t.Biome))
	}
	starts := append([]StartLocation(nil), t.StartLocations...)
	sortStartLocations(starts)
	for _, start := range starts {
		fmt.Fprintf(&b, "\n[[start]]\nplayer = %d\ncell = [%d, %d]\n", start.Player, start.Cell[0], start.Cell[1])
	}
	return []byte(b.String())
}

func renderGrid(grid [][]int) []byte {
	var b strings.Builder
	for _, row := range grid {
		for x, n := range row {
			if x > 0 {
				b.WriteByte(' ')
			}
			b.WriteString(strconv.Itoa(n))
		}
		b.WriteByte('\n')
	}
	return []byte(b.String())
}

func renderCliffGrid(grid [][]CliffCell) []byte {
	var b strings.Builder
	for _, row := range grid {
		for x, cell := range row {
			if x > 0 {
				b.WriteByte(' ')
			}
			if cell.Ramp {
				b.WriteByte('r')
			}
			b.WriteString(strconv.Itoa(cell.Level))
		}
		b.WriteByte('\n')
	}
	return []byte(b.String())
}

func renderSplatGrid(grid [][]SplatWeight) []byte {
	var b strings.Builder
	for _, row := range grid {
		for x, cell := range row {
			if x > 0 {
				b.WriteByte(' ')
			}
			b.WriteString(strconv.Itoa(int(cell.A)))
			b.WriteByte(',')
			b.WriteString(strconv.Itoa(int(cell.B)))
			b.WriteByte(',')
			b.WriteString(strconv.Itoa(int(cell.C)))
			b.WriteByte(',')
			b.WriteString(strconv.Itoa(int(cell.D)))
		}
		b.WriteByte('\n')
	}
	return []byte(b.String())
}

func renderEntities(entities []Entity) []byte {
	sorted := append([]Entity(nil), entities...)
	sortEntities(sorted)
	var b strings.Builder
	b.WriteString("# one element per line; ordered by id; ids never reused\n")
	b.WriteString("entities = [\n")
	for _, e := range sorted {
		fmt.Fprintf(&b, "  { id = %d, type = %s, player = %d, pos = [%d, %d], rotation = %d, scale = %d },\n", e.ID, strconv.Quote(e.Type), e.Player, e.Pos[0], e.Pos[1], e.Rotation, e.Scale)
	}
	b.WriteString("]\n")
	return []byte(b.String())
}

func renderDoodads(doodads []Doodad) []byte {
	sorted := append([]Doodad(nil), doodads...)
	sortDoodads(sorted)
	var b strings.Builder
	b.WriteString("# one element per line; ordered by id; ids never reused\n")
	b.WriteString("doodads = [\n")
	for _, d := range sorted {
		fmt.Fprintf(&b, "  { id = %d, type = %s, pos = [%d, %d], rotation = %d, scale = %d },\n", d.ID, strconv.Quote(d.Type), d.Pos[0], d.Pos[1], d.Rotation, d.Scale)
	}
	b.WriteString("]\n")
	return []byte(b.String())
}

func renderStringSlice(v []string) string {
	var b strings.Builder
	b.WriteByte('[')
	for i, s := range v {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(strconv.Quote(s))
	}
	b.WriteByte(']')
	return b.String()
}

func sortEntities(entities []Entity) {
	sort.Slice(entities, func(i, j int) bool { return entities[i].ID < entities[j].ID })
}

func sortDoodads(doodads []Doodad) {
	sort.Slice(doodads, func(i, j int) bool { return doodads[i].ID < doodads[j].ID })
}

func sortStartLocations(starts []StartLocation) {
	sort.Slice(starts, func(i, j int) bool { return starts[i].Player < starts[j].Player })
}

func terrainEqual(a, b Terrain) bool {
	if a.Width != b.Width || a.Height != b.Height || a.Tileset != b.Tileset || a.Biome != b.Biome {
		return false
	}
	if len(a.StartLocations) != len(b.StartLocations) {
		return false
	}
	for i := range a.StartLocations {
		if a.StartLocations[i] != b.StartLocations[i] {
			return false
		}
	}
	return true
}

func metadataEqual(a, b Metadata) bool {
	if a.Format != b.Format || a.ID != b.ID || a.Name != b.Name || a.Description != b.Description || a.Engine != b.Engine || a.Players != b.Players || a.SeedPolicy != b.SeedPolicy {
		return false
	}
	if (a.Seed == nil) != (b.Seed == nil) {
		return false
	}
	if a.Seed != nil && *a.Seed != *b.Seed {
		return false
	}
	if len(a.Authors) != len(b.Authors) {
		return false
	}
	for i := range a.Authors {
		if a.Authors[i] != b.Authors[i] {
			return false
		}
	}
	return true
}

func writeFileIfChanged(root, rel string, body []byte) error {
	p := filepath.Join(root, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return err
	}
	if old, err := os.ReadFile(p); err == nil && bytes.Equal(old, body) {
		return nil
	}
	return os.WriteFile(p, body, 0o644)
}

func cloneBytes(b []byte) []byte {
	if b == nil {
		return nil
	}
	out := make([]byte, len(b))
	copy(out, b)
	return out
}
