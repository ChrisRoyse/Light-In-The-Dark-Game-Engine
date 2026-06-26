package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"time"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapgrid"
	simpath "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

type summary struct {
	Map        mapSummary       `json:"map"`
	Counts     mapCounts        `json:"counts"`
	Starts     []startSummary   `json:"starts"`
	Beacons    []beaconSummary  `json:"beacons"`
	Symmetry   symmetrySummary  `json:"symmetry"`
	Paths      []pathSummary    `json:"paths"`
	Footprints []footprintCheck `json:"footprints"`
	Images     []imageRecord    `json:"images"`
}

type mapSummary struct {
	Path          string `json:"path"`
	Width         int    `json:"width"`
	Height        int    `json:"height"`
	PathingWidth  int    `json:"pathingWidth"`
	PathingHeight int    `json:"pathingHeight"`
	Biome         string `json:"biome"`
	Fingerprint   string `json:"fingerprint"`
}

type mapCounts struct {
	WalkableCells  int `json:"walkableCells"`
	BuildableCells int `json:"buildableCells"`
	HighCells      int `json:"highCells"`
	RampCells      int `json:"rampCells"`
	Doodads        int `json:"doodads"`
}

type startSummary struct {
	Player int `json:"player"`
	X      int `json:"x"`
	Y      int `json:"y"`
	Level  int `json:"level"`
}

type beaconSummary struct {
	ID    uint32 `json:"id"`
	X     int    `json:"x"`
	Y     int    `json:"y"`
	Owner int    `json:"owner"`
	Level int    `json:"level"`
	Ramp  bool   `json:"ramp"`
}

type symmetrySummary struct {
	StartMirror      bool     `json:"startMirror"`
	StartMirrorPair  string   `json:"startMirrorPair,omitempty"`
	BeaconMirror     bool     `json:"beaconMirror"`
	BeaconMirrorPair string   `json:"beaconMirrorPair,omitempty"`
	CenterBeacons    []uint32 `json:"centerBeacons,omitempty"`
}

type pathSummary struct {
	FromPlayer int  `json:"fromPlayer"`
	ToPlayer   int  `json:"toPlayer"`
	Steps      int  `json:"steps"`
	Reachable  bool `json:"reachable"`
}

type footprintCheck struct {
	Player      int    `json:"player"`
	X           int    `json:"x"`
	Y           int    `json:"y"`
	Size        int    `json:"size"`
	Clear       bool   `json:"clear"`
	Blocked     int    `json:"blocked"`
	Description string `json:"description"`
}

type imageRecord struct {
	Kind   string `json:"kind"`
	Player int    `json:"player"`
	Path   string `json:"path"`
	Width  int    `json:"width"`
	Height int    `json:"height"`
	Bytes  int64  `json:"bytes"`
	SHA256 string `json:"sha256"`
}

type viewRect struct {
	x0, y0, x1, y1 int
}

func main() {
	if err := run(os.Args[1:]); err != nil {
		fmt.Fprintf(os.Stderr, "mapshot: %v\n", err)
		os.Exit(1)
	}
}

func run(args []string) error {
	fs := flag.NewFlagSet("mapshot", flag.ContinueOnError)
	mapPath := fs.String("map", "data/maps/firstflame", "map directory to load")
	outDir := fs.String("out", "artifacts/mapshot", "output directory")
	scale := fs.Int("scale", 4, "pixels per pathing cell [1,16]")
	cropRadius := fs.Int("crop-radius", 48, "pathing-cell radius for per-start crops")
	footprint := fs.Int("footprint", 16, "pathing-cell square footprint to verify at each start")
	if err := fs.Parse(args); err != nil {
		return err
	}
	if *scale < 1 || *scale > 16 {
		return fmt.Errorf("-scale must be in [1,16], got %d", *scale)
	}
	if *cropRadius < 8 || *cropRadius > 256 {
		return fmt.Errorf("-crop-radius must be in [8,256], got %d", *cropRadius)
	}
	if *footprint < 1 || *footprint > 128 {
		return fmt.Errorf("-footprint must be in [1,128], got %d", *footprint)
	}

	loadStart := time.Now()
	m, rel, err := loadMap(*mapPath)
	if err != nil {
		return err
	}
	loadDuration := time.Since(loadStart)
	fmt.Printf("event: map loaded path=%s dims=%dx%d pathing=%dx%d fingerprint=0x%016x load_ms=%.3f\n",
		rel, m.Width, m.Height, m.PathingWidth, m.PathingHeight, m.Fingerprint, float64(loadDuration.Microseconds())/1000)
	sum, err := analyze(m, rel, *footprint)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(*outDir, 0o755); err != nil {
		return fmt.Errorf("mkdir %s: %w", *outDir, err)
	}

	base := strings.TrimSuffix(filepath.Base(filepath.Clean(*mapPath)), string(filepath.Separator))
	if base == "." || base == "" {
		base = "map"
	}
	overview := filepath.Join(*outDir, base+"-overhead.png")
	if rec, err := writePNG(overview, "overhead", -1, renderMap(m, *scale, nil)); err != nil {
		return err
	} else {
		sum.Images = append(sum.Images, rec)
		fmt.Printf("event: screenshot saved path=%s kind=overhead\n", overview)
	}
	for _, st := range m.Starts() {
		v := cropAround(st.X, st.Y, *cropRadius, m.PathingWidth, m.PathingHeight)
		p := filepath.Join(*outDir, fmt.Sprintf("%s-start-p%d.png", base, st.Player))
		if rec, err := writePNG(p, "start", int(st.Player), renderMap(m, *scale, &v)); err != nil {
			return err
		} else {
			sum.Images = append(sum.Images, rec)
			fmt.Printf("event: screenshot saved path=%s kind=start player=%d\n", p, st.Player)
		}
	}

	summaryPath := filepath.Join(*outDir, base+"-summary.json")
	body, err := json.MarshalIndent(sum, "", "  ")
	if err != nil {
		return err
	}
	body = append(body, '\n')
	if err := os.WriteFile(summaryPath, body, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", summaryPath, err)
	}
	fmt.Printf("event: summary saved path=%s\n", summaryPath)
	fmt.Printf("state: %s\n", body)
	return nil
}

func loadMap(p string) (*mapdata.Map, string, error) {
	clean := filepath.Clean(strings.TrimSpace(p))
	if clean == "." || clean == "" {
		return nil, "", fmt.Errorf("empty -map")
	}
	var root string
	var rel string
	if filepath.IsAbs(clean) {
		root = string(filepath.Separator)
		rel = strings.TrimPrefix(filepath.ToSlash(clean), "/")
	} else {
		root = "."
		rel = filepath.ToSlash(clean)
	}
	m, err := mapdata.Load(os.DirFS(root), rel)
	if err != nil {
		return nil, rel, err
	}
	return m, rel, nil
}

func analyze(m *mapdata.Map, rel string, footprint int) (summary, error) {
	if m == nil {
		return summary{}, fmt.Errorf("nil map")
	}
	g, err := mapgrid.GridFromMap(m)
	if err != nil {
		return summary{}, err
	}
	out := summary{
		Map: mapSummary{
			Path:          rel,
			Width:         m.Width,
			Height:        m.Height,
			PathingWidth:  m.PathingWidth,
			PathingHeight: m.PathingHeight,
			Biome:         m.Biome,
			Fingerprint:   fmt.Sprintf("0x%016x", m.Fingerprint),
		},
	}
	for y := 0; y < m.PathingHeight; y++ {
		for x := 0; x < m.PathingWidth; x++ {
			pf, _ := m.PathingAt(x, y)
			if pf&mapdata.PathWalkable != 0 {
				out.Counts.WalkableCells++
			}
			if pf&mapdata.PathBuildable != 0 {
				out.Counts.BuildableCells++
			}
			c, _ := m.CliffAt(x, y)
			if c.Level > 0 {
				out.Counts.HighCells++
			}
			if c.Ramp {
				out.Counts.RampCells++
			}
		}
	}
	out.Counts.Doodads = len(m.Doodads())

	for _, st := range m.Starts() {
		c, _ := m.CliffAt(st.X, st.Y)
		out.Starts = append(out.Starts, startSummary{Player: int(st.Player), X: st.X, Y: st.Y, Level: int(c.Level)})
		out.Footprints = append(out.Footprints, checkFootprint(g, m.PathingWidth, m.PathingHeight, int(st.Player), st.X, st.Y, footprint))
	}
	for _, b := range m.Beacons() {
		c, _ := m.CliffAt(b.X, b.Y)
		out.Beacons = append(out.Beacons, beaconSummary{ID: b.ID, X: b.X, Y: b.Y, Owner: b.Owner, Level: int(c.Level), Ramp: c.Ramp})
	}
	out.Symmetry = symmetry(m)
	out.Paths = pathChecks(g, m)
	return out, nil
}

func symmetry(m *mapdata.Map) symmetrySummary {
	out := symmetrySummary{}
	starts := m.Starts()
	if len(starts) == 2 {
		a, b := starts[0], starts[1]
		out.StartMirror = int(a.X)+int(b.X) == m.PathingWidth && int(a.Y) == int(b.Y)
		out.StartMirrorPair = fmt.Sprintf("p%d(%d,%d)<->p%d(%d,%d)", a.Player, a.X, a.Y, b.Player, b.X, b.Y)
	}
	beacons := m.Beacons()
	if len(beacons) == 3 {
		for _, b := range beacons {
			if b.X*2 == m.PathingWidth && b.Y*2 == m.PathingHeight {
				out.CenterBeacons = append(out.CenterBeacons, b.ID)
			}
		}
		for i := range beacons {
			for j := i + 1; j < len(beacons); j++ {
				a, b := beacons[i], beacons[j]
				if a.X+b.X == m.PathingWidth && a.Y+b.Y == m.PathingHeight {
					out.BeaconMirror = true
					out.BeaconMirrorPair = fmt.Sprintf("b%d(%d,%d)<->b%d(%d,%d)", a.ID, a.X, a.Y, b.ID, b.X, b.Y)
				}
			}
		}
	}
	return out
}

func pathChecks(g *simpath.Grid, m *mapdata.Map) []pathSummary {
	starts := m.Starts()
	if len(starts) != 2 {
		return nil
	}
	a, b := starts[0], starts[1]
	ab := bfsDist(g, m.PathingWidth, m.PathingHeight, a.X, a.Y, b.X, b.Y)
	ba := bfsDist(g, m.PathingWidth, m.PathingHeight, b.X, b.Y, a.X, a.Y)
	return []pathSummary{
		{FromPlayer: int(a.Player), ToPlayer: int(b.Player), Steps: ab, Reachable: ab >= 0},
		{FromPlayer: int(b.Player), ToPlayer: int(a.Player), Steps: ba, Reachable: ba >= 0},
	}
}

func bfsDist(g *simpath.Grid, w, h, sx, sy, gx, gy int) int {
	if sx < 0 || sy < 0 || gx < 0 || gy < 0 || sx >= w || sy >= h || gx >= w || gy >= h {
		return -1
	}
	dist := make([]int, w*h)
	for i := range dist {
		dist[i] = -1
	}
	dist[sy*w+sx] = 0
	queue := []int{sy*w + sx}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		cx, cy := cur%w, cur/w
		if cx == gx && cy == gy {
			return dist[cur]
		}
		for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
			nx, ny := cx+d[0], cy+d[1]
			if nx < 0 || ny < 0 || nx >= w || ny >= h || dist[ny*w+nx] != -1 {
				continue
			}
			if !g.StepLegal(int32(cx), int32(cy), int32(nx), int32(ny)) {
				continue
			}
			dist[ny*w+nx] = dist[cur] + 1
			queue = append(queue, ny*w+nx)
		}
	}
	return -1
}

func checkFootprint(g *simpath.Grid, w, h, player, x, y, size int) footprintCheck {
	half := size / 2
	x0, y0 := x-half, y-half
	out := footprintCheck{Player: player, X: x, Y: y, Size: size, Clear: true, Description: "all cells walkable+buildable"}
	for yy := y0; yy < y0+size; yy++ {
		for xx := x0; xx < x0+size; xx++ {
			if xx < 0 || yy < 0 || xx >= w || yy >= h {
				out.Blocked++
				out.Clear = false
				out.Description = "footprint extends out of map"
				continue
			}
			f := g.FlagsAt(int32(xx), int32(yy))
			if f&(simpath.Walkable|simpath.Buildable) != simpath.Walkable|simpath.Buildable {
				out.Blocked++
				out.Clear = false
				out.Description = "one or more cells are not walkable+buildable"
			}
		}
	}
	return out
}

func cropAround(x, y, radius, w, h int) viewRect {
	v := viewRect{x0: x - radius, y0: y - radius, x1: x + radius, y1: y + radius}
	if v.x0 < 0 {
		v.x0 = 0
	}
	if v.y0 < 0 {
		v.y0 = 0
	}
	if v.x1 > w {
		v.x1 = w
	}
	if v.y1 > h {
		v.y1 = h
	}
	return v
}

func renderMap(m *mapdata.Map, scale int, view *viewRect) *image.RGBA {
	v := viewRect{x0: 0, y0: 0, x1: m.PathingWidth, y1: m.PathingHeight}
	if view != nil {
		v = *view
	}
	img := image.NewRGBA(image.Rect(0, 0, (v.x1-v.x0)*scale, (v.y1-v.y0)*scale))
	for y := v.y0; y < v.y1; y++ {
		for x := v.x0; x < v.x1; x++ {
			fillCell(img, x-v.x0, y-v.y0, scale, cellColor(m, x, y))
		}
	}
	drawGuideLines(img, m, scale, v)
	for _, b := range m.Beacons() {
		drawBeacon(img, b.X-v.x0, b.Y-v.y0, scale, b.ID)
	}
	for _, st := range m.Starts() {
		drawStart(img, st.X-v.x0, st.Y-v.y0, scale, int(st.Player))
	}
	return img
}

func cellColor(m *mapdata.Map, x, y int) color.RGBA {
	pf, _ := m.PathingAt(x, y)
	c, _ := m.CliffAt(x, y)
	if pf&mapdata.PathWalkable == 0 {
		return color.RGBA{23, 25, 29, 255}
	}
	if c.Ramp {
		return color.RGBA{218, 166, 64, 255}
	}
	if c.Level > 0 {
		return color.RGBA{126, 104, 61, 255}
	}
	if pf&mapdata.PathBuildable != 0 {
		return color.RGBA{74, 95, 73, 255}
	}
	return color.RGBA{49, 67, 56, 255}
}

func fillCell(img *image.RGBA, x, y, scale int, c color.RGBA) {
	px, py := x*scale, y*scale
	for yy := py; yy < py+scale; yy++ {
		for xx := px; xx < px+scale; xx++ {
			img.SetRGBA(xx, yy, c)
		}
	}
}

func drawGuideLines(img *image.RGBA, m *mapdata.Map, scale int, v viewRect) {
	line := color.RGBA{39, 43, 47, 255}
	for y := v.y0; y < v.y1; y++ {
		if y%mapdata.PathingScale != 0 {
			continue
		}
		py := (y - v.y0) * scale
		for x := 0; x < img.Bounds().Dx(); x++ {
			img.SetRGBA(x, py, line)
		}
	}
	for x := v.x0; x < v.x1; x++ {
		if x%mapdata.PathingScale != 0 {
			continue
		}
		px := (x - v.x0) * scale
		for y := 0; y < img.Bounds().Dy(); y++ {
			img.SetRGBA(px, y, line)
		}
	}
	center := color.RGBA{210, 210, 190, 255}
	if m.PathingWidth/2 >= v.x0 && m.PathingWidth/2 < v.x1 {
		px := (m.PathingWidth/2 - v.x0) * scale
		for y := 0; y < img.Bounds().Dy(); y++ {
			img.SetRGBA(px, y, center)
		}
	}
	if m.PathingHeight/2 >= v.y0 && m.PathingHeight/2 < v.y1 {
		py := (m.PathingHeight/2 - v.y0) * scale
		for x := 0; x < img.Bounds().Dx(); x++ {
			img.SetRGBA(x, py, center)
		}
	}
}

func drawStart(img *image.RGBA, cellX, cellY, scale, player int) {
	if cellX < 0 || cellY < 0 || cellX*scale >= img.Bounds().Dx() || cellY*scale >= img.Bounds().Dy() {
		return
	}
	c := color.RGBA{77, 145, 255, 255}
	if player%2 == 1 {
		c = color.RGBA{242, 82, 82, 255}
	}
	cx, cy := cellX*scale, cellY*scale
	r := 5 * scale
	for y := cy - r; y <= cy+r; y++ {
		for x := cx - r; x <= cx+r; x++ {
			if x < 0 || y < 0 || x >= img.Bounds().Dx() || y >= img.Bounds().Dy() {
				continue
			}
			if abs(x-cx) == r || abs(y-cy) == r || abs(x-cx) == abs(y-cy) {
				img.SetRGBA(x, y, c)
			}
		}
	}
}

func drawBeacon(img *image.RGBA, cellX, cellY, scale int, id uint32) {
	if cellX < 0 || cellY < 0 || cellX*scale >= img.Bounds().Dx() || cellY*scale >= img.Bounds().Dy() {
		return
	}
	cx, cy := cellX*scale, cellY*scale
	r := 5 * scale
	fill := color.RGBA{255, 224, 89, 255}
	ring := color.RGBA{255, 255, 225, 255}
	if id == 1 {
		fill = color.RGBA{255, 241, 128, 255}
	}
	for y := cy - r; y <= cy+r; y++ {
		for x := cx - r; x <= cx+r; x++ {
			if x < 0 || y < 0 || x >= img.Bounds().Dx() || y >= img.Bounds().Dy() {
				continue
			}
			d2 := (x-cx)*(x-cx) + (y-cy)*(y-cy)
			if d2 <= r*r {
				if d2 >= (r-scale)*(r-scale) {
					img.SetRGBA(x, y, ring)
				} else {
					img.SetRGBA(x, y, fill)
				}
			}
		}
	}
}

func writePNG(path, kind string, player int, img image.Image) (imageRecord, error) {
	f, err := os.Create(path)
	if err != nil {
		return imageRecord{}, fmt.Errorf("create %s: %w", path, err)
	}
	if err := png.Encode(f, img); err != nil {
		_ = f.Close()
		return imageRecord{}, fmt.Errorf("encode %s: %w", path, err)
	}
	if err := f.Close(); err != nil {
		return imageRecord{}, fmt.Errorf("close %s: %w", path, err)
	}
	return inspectImage(path, kind, player)
}

func inspectImage(path, kind string, player int) (imageRecord, error) {
	body, err := os.ReadFile(path)
	if err != nil {
		return imageRecord{}, fmt.Errorf("read %s: %w", path, err)
	}
	cfg, _, err := image.DecodeConfig(bytes.NewReader(body))
	if err != nil {
		return imageRecord{}, fmt.Errorf("decode config %s: %w", path, err)
	}
	st, err := os.Stat(path)
	if err != nil {
		return imageRecord{}, fmt.Errorf("stat %s: %w", path, err)
	}
	sum := sha256.Sum256(body)
	return imageRecord{
		Kind: kind, Player: player, Path: path, Width: cfg.Width, Height: cfg.Height,
		Bytes: st.Size(), SHA256: hex.EncodeToString(sum[:]),
	}, nil
}

func abs(v int) int {
	if v < 0 {
		return -v
	}
	return v
}
