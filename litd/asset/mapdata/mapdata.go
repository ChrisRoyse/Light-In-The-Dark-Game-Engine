// Package mapdata loads immutable R-AST-1 map terrain data:
// height vertices, pathing flags, cliff/ramp cells, splat weights,
// start locations, and static doodad placements.
package mapdata

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io/fs"
	"math"
	"path"
	"sort"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	FormatVersion        = 1
	PathingScale         = 4
	MaxLightingIntensity = 8
	defaultAmbientR      = 0.82
	defaultAmbientG      = 0.88
	defaultAmbientB      = 1.00
	defaultAmbientI      = 0.62
	defaultSunR          = 1.00
	defaultSunG          = 0.96
	defaultSunB          = 0.86
	defaultSunI          = 1.05
	defaultSunAzimuth    = 180
	defaultSunElevation  = 65
)

type PathFlags uint8

const (
	PathWalkable PathFlags = 1 << iota
	PathBuildable
	PathWater
)

type Cliff struct {
	Level uint8 `json:"level"`
	Ramp  bool  `json:"ramp,omitempty"`
}

type SplatWeight struct {
	A uint8 `json:"a"`
	B uint8 `json:"b"`
	C uint8 `json:"c"`
	D uint8 `json:"d"`
}

type StartLocation struct {
	Player uint8 `json:"player"`
	X      int   `json:"x"`
	Y      int   `json:"y"`
}

// BeaconNeutral is the Owner value for a beacon that starts uncontrolled.
const BeaconNeutral = -1

// Beacon is a capturable control point (identity.md §3: holding map control is
// the light/territory win lever; the beacon-hold victory in #200 reads these).
// Coordinates are pathing-grid cells, like StartLocation. Owner is the initial
// controller: BeaconNeutral (-1) for uncontrolled, else a player index [0,15].
type Beacon struct {
	ID    uint32 `json:"id"`
	X     int    `json:"x"`
	Y     int    `json:"y"`
	Owner int    `json:"owner"`
}

type Doodad struct {
	ID           uint32 `json:"id"`
	Asset        string `json:"asset"`
	X            int    `json:"x"`
	Y            int    `json:"y"`
	Rotation     uint16 `json:"rotation"`
	Destructible bool   `json:"destructible"`
	FootprintW   int    `json:"footprintW"`
	FootprintH   int    `json:"footprintH"`
}

// Lighting is the persistent per-map gameplay lighting authored in
// terrain.toml. It is immutable after load: the renderer converts it to one
// directional sun plus one ambient light.
type Lighting struct {
	AmbientColor     [3]float32 `json:"ambientColor"`
	AmbientIntensity float32    `json:"ambientIntensity"`
	SunColor         [3]float32 `json:"sunColor"`
	SunIntensity     float32    `json:"sunIntensity"`
	SunAzimuth       float32    `json:"sunAzimuth"`
	SunElevation     float32    `json:"sunElevation"`
}

// DefaultLighting is the canonical noon-like lighting used by maps that have
// not authored an explicit [lighting] table yet.
func DefaultLighting() Lighting {
	return Lighting{
		AmbientColor:     [3]float32{defaultAmbientR, defaultAmbientG, defaultAmbientB},
		AmbientIntensity: defaultAmbientI,
		SunColor:         [3]float32{defaultSunR, defaultSunG, defaultSunB},
		SunIntensity:     defaultSunI,
		SunAzimuth:       defaultSunAzimuth,
		SunElevation:     defaultSunElevation,
	}
}

type Map struct {
	Path          string   `json:"path"`
	Width         int      `json:"width"`
	Height        int      `json:"height"`
	PathingWidth  int      `json:"pathingWidth"`
	PathingHeight int      `json:"pathingHeight"`
	Biome         string   `json:"biome"`
	Fingerprint   uint64   `json:"fingerprint"`
	Lighting      Lighting `json:"lighting"`

	pathing []PathFlags
	cliffs  []Cliff
	heights []int32
	splats  []SplatWeight
	starts  []StartLocation
	beacons []Beacon
	doodads []Doodad
}

type rawTerrain struct {
	Version      int          `toml:"version"`
	Width        int          `toml:"width"`
	Height       int          `toml:"height"`
	Biome        string       `toml:"biome"`
	PathingScale int          `toml:"pathing-scale"`
	Lighting     *rawLighting `toml:"lighting"`
	Start        []rawStart   `toml:"start"`
	Beacon       []rawBeacon  `toml:"beacon"`
}

type rawLighting struct {
	AmbientColor     []float64 `toml:"ambient-color"`
	AmbientIntensity *float64  `toml:"ambient-intensity"`
	SunColor         []float64 `toml:"sun-color"`
	SunIntensity     *float64  `toml:"sun-intensity"`
	SunAzimuth       *float64  `toml:"sun-azimuth"`
	SunElevation     *float64  `toml:"sun-elevation"`
}

type rawStart struct {
	Player int   `toml:"player"`
	Cell   []int `toml:"cell"`
}

type rawBeacon struct {
	ID    uint32 `toml:"id"`
	Cell  []int  `toml:"cell"`
	Owner *int   `toml:"owner"` // optional; omitted = neutral (a pointer so owner=0 is distinguishable from "unset")
}

type rawDoodadFile struct {
	Doodad []rawDoodad `toml:"doodad"`
}

type rawDoodad struct {
	ID           uint32 `toml:"id"`
	Asset        string `toml:"asset"`
	Cell         []int  `toml:"cell"`
	Rotation     int    `toml:"rotation"`
	Destructible bool   `toml:"destructible"`
	Footprint    []int  `toml:"footprint"`
}

func Load(fsys fs.FS, dir string) (*Map, error) {
	dir = strings.Trim(strings.TrimSpace(path.Clean(dir)), "/")
	if dir == "." || dir == "" {
		return nil, fmt.Errorf("mapdata: empty map directory")
	}
	terrainFile := path.Join(dir, "terrain.toml")
	var raw rawTerrain
	if err := readTOML(fsys, terrainFile, &raw); err != nil {
		return nil, err
	}
	if raw.Version != FormatVersion {
		return nil, fmt.Errorf("mapdata: %s: version %d unsupported, want %d", terrainFile, raw.Version, FormatVersion)
	}
	if raw.PathingScale == 0 {
		raw.PathingScale = PathingScale
	}
	if raw.PathingScale != PathingScale {
		return nil, fmt.Errorf("mapdata: %s: pathing-scale %d unsupported, want %d", terrainFile, raw.PathingScale, PathingScale)
	}
	if raw.Width <= 0 || raw.Height <= 0 || raw.Width > 512 || raw.Height > 512 {
		return nil, fmt.Errorf("mapdata: %s: dimensions %dx%d out of range", terrainFile, raw.Width, raw.Height)
	}
	if strings.TrimSpace(raw.Biome) == "" {
		return nil, fmt.Errorf("mapdata: %s: biome is required", terrainFile)
	}

	m := &Map{
		Path:          dir,
		Width:         raw.Width,
		Height:        raw.Height,
		PathingWidth:  raw.Width * PathingScale,
		PathingHeight: raw.Height * PathingScale,
		Biome:         strings.TrimSpace(raw.Biome),
	}
	var err error
	m.Lighting, err = compileLighting(terrainFile, raw.Lighting)
	if err != nil {
		return nil, err
	}
	m.pathing, err = readGrid(fsys, path.Join(dir, "pathing.txt"), m.PathingWidth, m.PathingHeight, parsePathFlags)
	if err != nil {
		return nil, err
	}
	m.cliffs, err = readGrid(fsys, path.Join(dir, "cliff.txt"), m.PathingWidth, m.PathingHeight, parseCliff)
	if err != nil {
		return nil, err
	}
	m.heights, err = readGrid(fsys, path.Join(dir, "height.txt"), m.Width+1, m.Height+1, parseHeight)
	if err != nil {
		return nil, err
	}
	m.splats, err = readGrid(fsys, path.Join(dir, "splat.txt"), m.Width, m.Height, parseSplat)
	if err != nil {
		return nil, err
	}
	m.starts, err = compileStarts(terrainFile, raw.Start, m)
	if err != nil {
		return nil, err
	}
	m.beacons, err = compileBeacons(terrainFile, raw.Beacon, m)
	if err != nil {
		return nil, err
	}
	m.doodads, err = loadDoodads(fsys, dir, m)
	if err != nil {
		return nil, err
	}
	if err := validateRamps(m); err != nil {
		return nil, err
	}
	m.Fingerprint = m.fingerprint()
	return m, nil
}

func (m *Map) PathingAt(x, y int) (PathFlags, bool) {
	if !m.inPathingBounds(x, y) {
		return 0, false
	}
	return m.pathing[y*m.PathingWidth+x], true
}

func (m *Map) CliffAt(x, y int) (Cliff, bool) {
	if !m.inPathingBounds(x, y) {
		return Cliff{}, false
	}
	return m.cliffs[y*m.PathingWidth+x], true
}

func (m *Map) HeightAtVertex(x, y int) (int32, bool) {
	if x < 0 || y < 0 || x > m.Width || y > m.Height {
		return 0, false
	}
	return m.heights[y*(m.Width+1)+x], true
}

func (m *Map) SplatAt(x, y int) (SplatWeight, bool) {
	if x < 0 || y < 0 || x >= m.Width || y >= m.Height {
		return SplatWeight{}, false
	}
	return m.splats[y*m.Width+x], true
}

func (m *Map) Starts() []StartLocation {
	return append([]StartLocation(nil), m.starts...)
}

func (m *Map) Beacons() []Beacon {
	return append([]Beacon(nil), m.beacons...)
}

func (m *Map) Doodads() []Doodad {
	return append([]Doodad(nil), m.doodads...)
}

func (m *Map) MarshalJSON() ([]byte, error) {
	type alias struct {
		Path          string          `json:"path"`
		Width         int             `json:"width"`
		Height        int             `json:"height"`
		PathingWidth  int             `json:"pathingWidth"`
		PathingHeight int             `json:"pathingHeight"`
		Biome         string          `json:"biome"`
		Fingerprint   uint64          `json:"fingerprint"`
		Lighting      Lighting        `json:"lighting"`
		Starts        []StartLocation `json:"starts"`
		Beacons       []Beacon        `json:"beacons"`
		Doodads       []Doodad        `json:"doodads"`
	}
	return json.Marshal(alias{
		Path:          m.Path,
		Width:         m.Width,
		Height:        m.Height,
		PathingWidth:  m.PathingWidth,
		PathingHeight: m.PathingHeight,
		Biome:         m.Biome,
		Fingerprint:   m.Fingerprint,
		Lighting:      m.Lighting,
		Starts:        m.Starts(),
		Beacons:       m.Beacons(),
		Doodads:       m.Doodads(),
	})
}

func (m *Map) inPathingBounds(x, y int) bool {
	return x >= 0 && y >= 0 && x < m.PathingWidth && y < m.PathingHeight
}

func readTOML(fsys fs.FS, file string, v any) error {
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return fmt.Errorf("mapdata: read %s: %w", file, err)
	}
	md, err := toml.Decode(string(blob), v)
	if err != nil {
		return fmt.Errorf("mapdata: %s: %w", file, err)
	}
	for _, un := range md.Undecoded() {
		return fmt.Errorf("mapdata: %s: unknown field %q", file, un.String())
	}
	return nil
}

func readGrid[T any](fsys fs.FS, file string, width, height int, parse func(string) (T, error)) ([]T, error) {
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return nil, fmt.Errorf("mapdata: read %s: %w", file, err)
	}
	out := make([]T, 0, width*height)
	rows := bytes.Split(bytes.TrimRight(blob, "\n"), []byte{'\n'})
	logicalY := 0
	for physical, raw := range rows {
		line := strings.TrimSpace(string(raw))
		if line == "" {
			return nil, fmt.Errorf("mapdata: %s: physical row %d is empty", file, physical)
		}
		repeat := 1
		if strings.HasPrefix(line, "@repeat ") {
			fields := strings.Fields(line)
			if len(fields) < 3 {
				return nil, fmt.Errorf("mapdata: %s: physical row %d bad @repeat directive", file, physical)
			}
			n, err := strconv.Atoi(fields[1])
			if err != nil || n <= 0 {
				return nil, fmt.Errorf("mapdata: %s: physical row %d bad @repeat count %q", file, physical, fields[1])
			}
			repeat = n
			line = strings.Join(fields[2:], " ")
		}
		rowValues, err := parseGridRow(file, logicalY, line, width, parse)
		if err != nil {
			return nil, err
		}
		for i := 0; i < repeat; i++ {
			out = append(out, rowValues...)
		}
		logicalY += repeat
		if logicalY > height {
			return nil, fmt.Errorf("mapdata: %s: got more than %d rows after physical row %d", file, height, physical)
		}
	}
	if logicalY != height {
		return nil, fmt.Errorf("mapdata: %s: got %d rows, want %d", file, logicalY, height)
	}
	return out, nil
}

func parseGridRow[T any](file string, y int, line string, width int, parse func(string) (T, error)) ([]T, error) {
	fields := strings.Fields(line)
	out := make([]T, 0, width)
	rowN := 0
	for _, field := range fields {
		atom, count, err := splitRepeat(field)
		if err != nil {
			return nil, fmt.Errorf("mapdata: %s: row %d: %w", file, y, err)
		}
		v, err := parse(atom)
		if err != nil {
			return nil, fmt.Errorf("mapdata: %s: row %d col %d: %w", file, y, rowN, err)
		}
		for i := 0; i < count; i++ {
			out = append(out, v)
		}
		rowN += count
	}
	if rowN != width {
		return nil, fmt.Errorf("mapdata: %s: row %d got %d values, want %d", file, y, rowN, width)
	}
	return out, nil
}

func splitRepeat(token string) (string, int, error) {
	i := strings.LastIndexByte(token, '*')
	if i < 0 {
		return token, 1, nil
	}
	atom, ntext := token[:i], token[i+1:]
	if atom == "" || ntext == "" {
		return "", 0, fmt.Errorf("bad repeat token %q", token)
	}
	n, err := strconv.Atoi(ntext)
	if err != nil || n <= 0 {
		return "", 0, fmt.Errorf("bad repeat count in %q", token)
	}
	return atom, n, nil
}

func parsePathFlags(atom string) (PathFlags, error) {
	v, err := strconv.ParseUint(atom, 0, 8)
	if err != nil {
		return 0, fmt.Errorf("path flags %q: %w", atom, err)
	}
	f := PathFlags(v)
	if f&^(PathWalkable|PathBuildable|PathWater) != 0 {
		return 0, fmt.Errorf("path flags %q contain unknown bits", atom)
	}
	if f&PathWater != 0 && f&(PathWalkable|PathBuildable) != 0 {
		return 0, fmt.Errorf("water path flags %q must not also be walk/build", atom)
	}
	return f, nil
}

func parseCliff(atom string) (Cliff, error) {
	ramp := false
	if strings.HasPrefix(strings.ToLower(atom), "r") {
		ramp = true
		atom = atom[1:]
	}
	v, err := strconv.ParseUint(atom, 10, 8)
	if err != nil {
		return Cliff{}, fmt.Errorf("cliff %q: %w", atom, err)
	}
	if v > 126 {
		return Cliff{}, fmt.Errorf("cliff level %d exceeds max 126", v)
	}
	return Cliff{Level: uint8(v), Ramp: ramp}, nil
}

func parseHeight(atom string) (int32, error) {
	v, err := strconv.ParseInt(atom, 10, 32)
	if err != nil {
		return 0, fmt.Errorf("height %q: %w", atom, err)
	}
	return int32(v), nil
}

func parseSplat(atom string) (SplatWeight, error) {
	parts := strings.Split(atom, ",")
	if len(parts) != 4 {
		return SplatWeight{}, fmt.Errorf("splat %q must have four comma-separated weights", atom)
	}
	var vals [4]uint8
	sum := 0
	for i, p := range parts {
		v, err := strconv.ParseUint(p, 10, 8)
		if err != nil {
			return SplatWeight{}, fmt.Errorf("splat %q: %w", atom, err)
		}
		vals[i] = uint8(v)
		sum += int(v)
	}
	if sum != 255 {
		return SplatWeight{}, fmt.Errorf("splat %q weights sum to %d, want 255", atom, sum)
	}
	return SplatWeight{A: vals[0], B: vals[1], C: vals[2], D: vals[3]}, nil
}

func compileLighting(file string, raw *rawLighting) (Lighting, error) {
	if raw == nil {
		return DefaultLighting(), nil
	}
	ambientColor, err := compileColor3(file, "ambient-color", raw.AmbientColor)
	if err != nil {
		return Lighting{}, err
	}
	sunColor, err := compileColor3(file, "sun-color", raw.SunColor)
	if err != nil {
		return Lighting{}, err
	}
	ambientIntensity, err := compileScalar(file, "ambient-intensity", raw.AmbientIntensity, 0, MaxLightingIntensity)
	if err != nil {
		return Lighting{}, err
	}
	sunIntensity, err := compileScalar(file, "sun-intensity", raw.SunIntensity, 0, MaxLightingIntensity)
	if err != nil {
		return Lighting{}, err
	}
	sunAzimuth, err := compileScalar(file, "sun-azimuth", raw.SunAzimuth, 0, 360)
	if err != nil {
		return Lighting{}, err
	}
	if sunAzimuth == 360 {
		return Lighting{}, fmt.Errorf("mapdata: %s: lighting sun-azimuth 360 out of range [0,360)", file)
	}
	sunElevation, err := compileScalar(file, "sun-elevation", raw.SunElevation, -90, 90)
	if err != nil {
		return Lighting{}, err
	}
	return Lighting{
		AmbientColor:     ambientColor,
		AmbientIntensity: ambientIntensity,
		SunColor:         sunColor,
		SunIntensity:     sunIntensity,
		SunAzimuth:       sunAzimuth,
		SunElevation:     sunElevation,
	}, nil
}

func compileColor3(file, field string, raw []float64) ([3]float32, error) {
	if raw == nil {
		return [3]float32{}, fmt.Errorf("mapdata: %s: lighting %s is required", file, field)
	}
	if len(raw) != 3 {
		return [3]float32{}, fmt.Errorf("mapdata: %s: lighting %s must have exactly 3 components, got %d", file, field, len(raw))
	}
	var out [3]float32
	for i, v := range raw {
		if err := validateFiniteRange(file, fmt.Sprintf("%s[%d]", field, i), v, 0, 1, "range [0,1]"); err != nil {
			return [3]float32{}, err
		}
		out[i] = float32(v)
	}
	return out, nil
}

func compileScalar(file, field string, raw *float64, lo, hi float64) (float32, error) {
	if raw == nil {
		return 0, fmt.Errorf("mapdata: %s: lighting %s is required", file, field)
	}
	rangeText := fmt.Sprintf("range [%g,%g]", lo, hi)
	if field == "sun-azimuth" {
		rangeText = "range [0,360)"
	}
	if err := validateFiniteRange(file, field, *raw, lo, hi, rangeText); err != nil {
		return 0, err
	}
	return float32(*raw), nil
}

func validateFiniteRange(file, field string, v, lo, hi float64, rangeText string) error {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return fmt.Errorf("mapdata: %s: lighting %s must be finite", file, field)
	}
	if v < lo || v > hi {
		return fmt.Errorf("mapdata: %s: lighting %s %g out of %s", file, field, v, rangeText)
	}
	return nil
}

func compileStarts(file string, raw []rawStart, m *Map) ([]StartLocation, error) {
	if len(raw) == 0 {
		return nil, fmt.Errorf("mapdata: %s: at least one [[start]] is required", file)
	}
	out := make([]StartLocation, 0, len(raw))
	seen := map[int]bool{}
	for _, r := range raw {
		if r.Player < 0 || r.Player > 15 {
			return nil, fmt.Errorf("mapdata: %s: start player %d out of range [0,15]", file, r.Player)
		}
		if seen[r.Player] {
			return nil, fmt.Errorf("mapdata: %s: duplicate start player %d", file, r.Player)
		}
		seen[r.Player] = true
		x, y, err := cell2(r.Cell)
		if err != nil {
			return nil, fmt.Errorf("mapdata: %s: start player %d: %w", file, r.Player, err)
		}
		flags, ok := m.PathingAt(x, y)
		if !ok {
			return nil, fmt.Errorf("mapdata: %s: start player %d cell (%d,%d) out of bounds", file, r.Player, x, y)
		}
		if flags&PathBuildable == 0 || flags&PathWater != 0 {
			return nil, fmt.Errorf("mapdata: %s: start player %d cell (%d,%d) is not buildable ground", file, r.Player, x, y)
		}
		out = append(out, StartLocation{Player: uint8(r.Player), X: x, Y: y})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Player < out[j].Player })
	return out, nil
}

// compileBeacons validates the [[beacon]] table from terrain.toml: unique ids,
// coordinates inside the pathing grid, and an initial owner of either
// BeaconNeutral (the default when [owner] is omitted) or a player index in
// [0,15]. Out-of-bounds or out-of-range owners are hard errors, never silently
// clamped — a misplaced control point would silently change who wins.
func compileBeacons(file string, raw []rawBeacon, m *Map) ([]Beacon, error) {
	out := make([]Beacon, 0, len(raw))
	seen := map[uint32]bool{}
	for _, r := range raw {
		if seen[r.ID] {
			return nil, fmt.Errorf("mapdata: %s: duplicate beacon id %d", file, r.ID)
		}
		seen[r.ID] = true
		x, y, err := cell2(r.Cell)
		if err != nil {
			return nil, fmt.Errorf("mapdata: %s: beacon %d: %w", file, r.ID, err)
		}
		if !m.inPathingBounds(x, y) {
			return nil, fmt.Errorf("mapdata: %s: beacon %d cell (%d,%d) out of bounds", file, r.ID, x, y)
		}
		owner := BeaconNeutral
		if r.Owner != nil {
			owner = *r.Owner
			if owner < 0 || owner > 15 {
				return nil, fmt.Errorf("mapdata: %s: beacon %d owner %d out of range [0,15]", file, r.ID, owner)
			}
		}
		out = append(out, Beacon{ID: r.ID, X: x, Y: y, Owner: owner})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out, nil
}

func loadDoodads(fsys fs.FS, dir string, m *Map) ([]Doodad, error) {
	file := path.Join(dir, "doodads.toml")
	var raw rawDoodadFile
	if err := readTOML(fsys, file, &raw); err != nil {
		return nil, err
	}
	out := make([]Doodad, 0, len(raw.Doodad))
	seen := map[uint32]bool{}
	for _, r := range raw.Doodad {
		if seen[r.ID] {
			return nil, fmt.Errorf("mapdata: %s: duplicate doodad id %d", file, r.ID)
		}
		seen[r.ID] = true
		if strings.TrimSpace(r.Asset) == "" {
			return nil, fmt.Errorf("mapdata: %s: doodad %d asset is required", file, r.ID)
		}
		asset := strings.Trim(r.Asset, "/")
		if strings.Contains(asset, "..") {
			return nil, fmt.Errorf("mapdata: %s: doodad %d asset %q must be relative", file, r.ID, r.Asset)
		}
		if _, err := fs.Stat(fsys, path.Join("assets", asset)); err != nil {
			return nil, fmt.Errorf("mapdata: %s: doodad %d asset %q: %w", file, r.ID, r.Asset, err)
		}
		x, y, err := cell2(r.Cell)
		if err != nil {
			return nil, fmt.Errorf("mapdata: %s: doodad %d: %w", file, r.ID, err)
		}
		if !m.inPathingBounds(x, y) {
			return nil, fmt.Errorf("mapdata: %s: doodad %d cell (%d,%d) out of bounds", file, r.ID, x, y)
		}
		if r.Rotation < 0 || r.Rotation > 65535 {
			return nil, fmt.Errorf("mapdata: %s: doodad %d rotation %d out of range [0,65535]", file, r.ID, r.Rotation)
		}
		fpW, fpH := 1, 1
		if len(r.Footprint) > 0 {
			if len(r.Footprint) != 2 || r.Footprint[0] <= 0 || r.Footprint[1] <= 0 {
				return nil, fmt.Errorf("mapdata: %s: doodad %d footprint must be [w,h] with positive values", file, r.ID)
			}
			fpW, fpH = r.Footprint[0], r.Footprint[1]
		}
		if x+fpW > m.PathingWidth || y+fpH > m.PathingHeight {
			return nil, fmt.Errorf("mapdata: %s: doodad %d footprint leaves map at (%d,%d)", file, r.ID, x, y)
		}
		out = append(out, Doodad{
			ID:           r.ID,
			Asset:        asset,
			X:            x,
			Y:            y,
			Rotation:     uint16(r.Rotation),
			Destructible: r.Destructible,
			FootprintW:   fpW,
			FootprintH:   fpH,
		})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out, nil
}

func cell2(v []int) (int, int, error) {
	if len(v) != 2 {
		return 0, 0, fmt.Errorf("cell must be [x,y]")
	}
	return v[0], v[1], nil
}

func validateRamps(m *Map) error {
	for y := 0; y < m.PathingHeight; y++ {
		for x := 0; x < m.PathingWidth; x++ {
			c := m.cliffs[y*m.PathingWidth+x]
			if !c.Ramp {
				continue
			}
			lo, hi := c.Level, c.Level+1
			hasLo, hasHi := false, false
			for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
				nx, ny := x+d[0], y+d[1]
				if !m.inPathingBounds(nx, ny) {
					continue
				}
				n := m.cliffs[ny*m.PathingWidth+nx]
				nlo, nhi := n.Level, n.Level
				if n.Ramp {
					nhi++
				}
				if nlo <= lo && lo <= nhi {
					hasLo = true
				}
				if nlo <= hi && hi <= nhi {
					hasHi = true
				}
			}
			if !hasLo || !hasHi {
				return fmt.Errorf("mapdata: %s/cliff.txt: ramp at (%d,%d) must touch both levels %d and %d", m.Path, x, y, lo, hi)
			}
		}
	}
	return nil
}

func (m *Map) fingerprint() uint64 {
	h := statehash.New()
	h.WriteU32(FormatVersion)
	h.WriteU32(uint32(m.Width))
	h.WriteU32(uint32(m.Height))
	h.WriteU32(uint32(PathingScale))
	writeString(h, m.Biome)
	writeLighting(h, m.Lighting)
	h.WriteU32(uint32(len(m.pathing)))
	for _, f := range m.pathing {
		h.WriteU8(uint8(f))
	}
	h.WriteU32(uint32(len(m.cliffs)))
	for _, c := range m.cliffs {
		h.WriteU8(c.Level)
		h.WriteBool(c.Ramp)
	}
	h.WriteU32(uint32(len(m.heights)))
	for _, v := range m.heights {
		h.WriteI64(int64(v))
	}
	h.WriteU32(uint32(len(m.splats)))
	for _, s := range m.splats {
		h.WriteU8(s.A)
		h.WriteU8(s.B)
		h.WriteU8(s.C)
		h.WriteU8(s.D)
	}
	h.WriteU32(uint32(len(m.starts)))
	for _, s := range m.starts {
		h.WriteU8(s.Player)
		h.WriteU32(uint32(s.X))
		h.WriteU32(uint32(s.Y))
	}
	h.WriteU32(uint32(len(m.beacons)))
	for _, b := range m.beacons {
		h.WriteU32(b.ID)
		h.WriteU32(uint32(b.X))
		h.WriteU32(uint32(b.Y))
		h.WriteI64(int64(b.Owner))
	}
	h.WriteU32(uint32(len(m.doodads)))
	for _, d := range m.doodads {
		h.WriteU32(d.ID)
		writeString(h, d.Asset)
		h.WriteU32(uint32(d.X))
		h.WriteU32(uint32(d.Y))
		h.WriteU16(d.Rotation)
		h.WriteBool(d.Destructible)
		h.WriteU32(uint32(d.FootprintW))
		h.WriteU32(uint32(d.FootprintH))
	}
	return h.Sum64()
}

func writeString(h *statehash.Hasher, s string) {
	h.WriteU32(uint32(len(s)))
	h.WriteBytes([]byte(s))
}

func writeLighting(h *statehash.Hasher, l Lighting) {
	for _, v := range l.AmbientColor {
		writeFloat32(h, v)
	}
	writeFloat32(h, l.AmbientIntensity)
	for _, v := range l.SunColor {
		writeFloat32(h, v)
	}
	writeFloat32(h, l.SunIntensity)
	writeFloat32(h, l.SunAzimuth)
	writeFloat32(h, l.SunElevation)
}

func writeFloat32(h *statehash.Hasher, v float32) {
	h.WriteU32(math.Float32bits(v))
}
