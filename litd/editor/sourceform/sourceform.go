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
)

const (
	worldFile    = "world.toml"
	terrainFile  = "map/terrain.toml"
	heightFile   = "map/height.txt"
	cliffFile    = "map/cliff.txt"
	splatFile    = "map/splat.txt"
	entitiesFile = "map/entities.toml"
)

var worldIDRE = regexp.MustCompile(`^[a-z0-9-]+$`)

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
	Width   int
	Height  int
	Tileset string
	Biome   string
}

// Entity is one placed map entity. IDs are stable and sorted on save.
type Entity struct {
	ID     uint32
	Type   string
	Player int
	Pos    [2]int
	Facing int
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
	Cliff    [][]int
	Splat    [][]int
	Entities []Entity

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
	Width   int    `toml:"width"`
	Height  int    `toml:"height"`
	Tileset string `toml:"tileset"`
	Biome   string `toml:"biome"`
}

type rawEntities struct {
	Entities []rawEntity `toml:"entities"`
}

type rawEntity struct {
	ID     uint32 `toml:"id"`
	Type   string `toml:"type"`
	Player int    `toml:"player"`
	Pos    []int  `toml:"pos"`
	Facing int    `toml:"facing"`
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
	for _, rel := range []string{worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile} {
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
	cliff, err := parseGrid(all[cliffFile], terrain.Width, terrain.Height)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", cliffFile, err)
	}
	splat, err := parseGrid(all[splatFile], terrain.Width, terrain.Height)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", splatFile, err)
	}
	entities, err := parseEntities(all[entitiesFile])
	if err != nil {
		return nil, fmt.Errorf("%s: %w", entitiesFile, err)
	}

	files := make(map[string][]byte, len(all))
	for rel, body := range all {
		switch rel {
		case worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile:
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
		files:    files,
	}, nil
}

// Dirty reports whether the editor state has unsaved changes.
func (w *World) Dirty() bool { return w != nil && w.dirty }

// MoveEntity changes a placement position/facing without reshuffling other rows.
func (w *World) MoveEntity(id uint32, pos [2]int, facing int) error {
	for i := range w.Entities {
		if w.Entities[i].ID != id {
			continue
		}
		if w.Entities[i].Pos == pos && w.Entities[i].Facing == facing {
			return nil
		}
		w.Entities[i].Pos = pos
		w.Entities[i].Facing = facing
		w.dirty = true
		return nil
	}
	return fmt.Errorf("sourceform: entity id %d not found", id)
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

// SetGridCell edits one terrain grid cell.
func (w *World) SetGridCell(kind GridKind, x, y, value int) error {
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
	return ExportArchive(w.Dir, outPath, opts)
}

// ExportArchive packs an already-saved source tree with the deterministic
// worldpack archive builder.
func ExportArchive(srcDir, outPath string, opts ExportOptions) error {
	return worldpack.Pack(srcDir, outPath, opts.EngineRange, opts.Hosting, opts.Categories)
}

func (w *World) grid(kind GridKind) ([][]int, error) {
	switch kind {
	case GridHeight:
		return w.Height, nil
	case GridCliff:
		return w.Cliff, nil
	case GridSplat:
		return w.Splat, nil
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
	case worldFile, terrainFile, heightFile, cliffFile, splatFile, entitiesFile:
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
	if len(meta.Authors) == 0 {
		return Metadata{}, fmt.Errorf("authors must contain at least one entry")
	}
	if meta.Players.Min <= 0 || meta.Players.Max < meta.Players.Min || meta.Players.Suggested < meta.Players.Min || meta.Players.Suggested > meta.Players.Max {
		return Metadata{}, fmt.Errorf("players must satisfy 0 < min <= suggested <= max")
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
	t := Terrain(raw)
	if t.Width <= 0 || t.Height <= 0 {
		return Terrain{}, fmt.Errorf("width and height must be positive")
	}
	if t.Tileset == "" {
		return Terrain{}, fmt.Errorf("tileset is required")
	}
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
		out = append(out, Entity{ID: e.ID, Type: e.Type, Player: e.Player, Pos: [2]int{e.Pos[0], e.Pos[1]}, Facing: e.Facing})
	}
	sortEntities(out)
	return out, nil
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
	if err := validateGrid(w.Height, w.Terrain.Width, w.Terrain.Height, "height"); err != nil {
		return err
	}
	if err := validateGrid(w.Cliff, w.Terrain.Width, w.Terrain.Height, "cliff"); err != nil {
		return err
	}
	if err := validateGrid(w.Splat, w.Terrain.Width, w.Terrain.Height, "splat"); err != nil {
		return err
	}
	seen := map[uint32]bool{}
	for _, e := range w.Entities {
		if e.ID == 0 || e.Type == "" || e.Player < 0 || e.Player > 255 {
			return fmt.Errorf("sourceform: invalid entity %+v", e)
		}
		if seen[e.ID] {
			return fmt.Errorf("sourceform: duplicate entity id %d", e.ID)
		}
		seen[e.ID] = true
	}
	for rel := range w.files {
		if !isPassthroughRel(rel) {
			return fmt.Errorf("sourceform: invalid passthrough file %q", rel)
		}
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

func (w *World) renderFiles() map[string][]byte {
	files := make(map[string][]byte, len(w.files)+6)
	for rel, body := range w.files {
		files[rel] = cloneBytes(body)
	}
	files[worldFile] = renderWorld(w.Metadata)
	files[terrainFile] = renderTerrain(w.Terrain)
	files[heightFile] = renderGrid(w.Height)
	files[cliffFile] = renderGrid(w.Cliff)
	files[splatFile] = renderGrid(w.Splat)
	files[entitiesFile] = renderEntities(w.Entities)
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

func renderEntities(entities []Entity) []byte {
	sorted := append([]Entity(nil), entities...)
	sortEntities(sorted)
	var b strings.Builder
	b.WriteString("# one element per line; ordered by id; ids never reused\n")
	b.WriteString("entities = [\n")
	for _, e := range sorted {
		fmt.Fprintf(&b, "  { id = %d, type = %s, player = %d, pos = [%d, %d], facing = %d },\n", e.ID, strconv.Quote(e.Type), e.Player, e.Pos[0], e.Pos[1], e.Facing)
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
