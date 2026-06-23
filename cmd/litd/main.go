// Command litd loads a world directory — validated data tables plus sandboxed
// Lua scripts — and runs it headless, with no engine recompile (#268). A world
// is <dir>/data/** (unit/combat/... tables, validated by litd/data) plus
// <dir>/main.lua (the entry chunk, executed inside the R-SEC-1 sandbox under an
// instruction budget). Editing the world's .lua or data and re-running needs no
// rebuild of this binary.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"sort"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/buildinfo"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
	lua "github.com/yuin/gopher-lua"
)

func main() {
	world := flag.String("world", "", "world directory to load (contains data/ and main.lua)")
	archive := flag.String("archive", "", "verified .litdworld archive to load")
	autotest := flag.Bool("autotest", false, "advance -ticks then print the sim state as JSON")
	ticks := flag.Int("ticks", 40, "ticks to advance under -autotest")
	seed := flag.Int64("seed", 1, "deterministic PRNG seed (R-SIM-2)")
	budget := flag.Int64("budget", 50_000_000, "per-eval Lua instruction budget (R-SEC-1 quota)")
	autotestOrder := flag.Bool("autotest-order", false, "under -autotest, issue a deterministic move order to the first unit before advancing")
	autotestOrderDX := flag.Float64("autotest-order-dx", 128, "x delta for the deterministic -autotest-order target")
	autotestOrderDY := flag.Float64("autotest-order-dy", 0, "y delta for the deterministic -autotest-order target")
	shot := flag.String("shot", "", "write a headless playtest screenshot after -autotest")
	flag.Parse()

	orderDelta := api.Vec2{X: *autotestOrderDX, Y: *autotestOrderDY}
	if err := run(*world, *archive, *autotest, *autotestOrder, orderDelta, *ticks, *seed, *budget, *shot); err != nil {
		fmt.Fprintln(os.Stderr, "litd:", err)
		os.Exit(1)
	}
}

func run(world, archive string, autotest, autotestOrder bool, orderDelta api.Vec2, ticks int, seed, budget int64, shot string) error {
	g, cleanup, err := loadWorldInput(world, archive, seed, budget)
	if err != nil {
		return err
	}
	defer cleanup()

	// Headless run + Source-of-Truth state dump. The interpreter is kept alive
	// (by cleanup deferral) across Advance so any handlers the world registered
	// can fire on later ticks.
	if autotest {
		order := orderState{}
		if autotestOrder {
			order = orderFirstUnit(g, orderDelta)
		}
		g.Advance(ticks)
		if shot != "" {
			if err := writePlaytestShot(shot, g, order); err != nil {
				return err
			}
			fmt.Printf("event: screenshot saved path=%s\n", shot)
		}
		printState(g, ticks, order)
	}
	return nil
}

func loadWorldInput(world, archive string, seed, budget int64) (*api.Game, func(), error) {
	switch {
	case world != "" && archive != "":
		return nil, nil, fmt.Errorf("pass either -world or -archive, not both")
	case archive != "":
		h, err := worldhost.LoadArchive(archive, engineVersion(), seed, budget)
		if err != nil {
			return nil, nil, err
		}
		return h.Game, h.Close, nil
	default:
		return loadWorld(world, seed, budget)
	}
}

// loadWorld loads a world for headless run, returning just the game handle.
// Thin wrapper over loadWorldFull for callers that don't need the Lua state /
// chunk registry (everything but the savegame round-trip).
func loadWorld(world string, seed, budget int64) (*api.Game, func(), error) {
	g, _, _, cleanup, err := loadWorldFull(world, seed, budget)
	return g, cleanup, err
}

// loadWorldFull loads a world and also returns its Lua state + chunk registry,
// so a caller can drive the production save/load container (litd/savegame.Write
// /Load, #204/#481) which bundles the sim state with the suspended Lua
// scheduler. The two handles are owned by the returned cleanup. The loader lives
// in litd/worldhost (#490) so a render harness can load worlds identically; this
// stays as the cmd/litd adapter to the 5-value shape its callers use.
func loadWorldFull(world string, seed, budget int64) (*api.Game, *lua.LState, *luabind.ChunkRegistry, func(), error) {
	h, err := worldhost.Load(world, seed, budget)
	if err != nil {
		return nil, nil, nil, nil, err
	}
	return h.Game, h.L, h.Reg, h.Close, nil
}

type unitState struct {
	ID     uint32  `json:"id"`
	Owner  int     `json:"owner"`
	X      float64 `json:"x"`
	Y      float64 `json:"y"`
	Facing float64 `json:"facing"`
	Life   float64 `json:"life"`
	Alive  bool    `json:"alive"`
}

type orderState struct {
	Issued bool    `json:"issued"`
	UnitID uint32  `json:"unitId,omitempty"`
	Before Vec2DTO `json:"before"`
	Target Vec2DTO `json:"target"`
}

type Vec2DTO struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

type effectState struct {
	ID uint32  `json:"id"`
	X  float64 `json:"x"`
	Y  float64 `json:"y"`
}

type stateDump struct {
	TimeOfDay float64       `json:"tod"`
	Ticks     int           `json:"ticks"`
	StateHash string        `json:"stateHash"`
	UnitCount int           `json:"unitCount"`
	Alive     int           `json:"alive"`
	Order     orderState    `json:"order,omitempty"`
	Units     []unitState   `json:"units"`
	Effects   []effectState `json:"effects"`
}

// printState writes the sim state as the JSON line an FSV reader inspects (the
// "state:" convention firstlight uses). Units are enumerated by a whole-map
// range query (no all-units iterator on the public surface yet).
func printState(g *api.Game, ticks int, order orderState) {
	s := gameState(g, ticks, order)
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
}

func gameState(g *api.Game, ticks int, order orderState) stateDump {
	units := sortedUnits(g)
	us := make([]unitState, 0, len(units))
	alive := 0
	for _, u := range units {
		p := u.Position()
		valid := u.Valid()
		if valid {
			alive++
		}
		us = append(us, unitState{ID: u.ID(), Owner: u.Owner().Slot(), X: p.X, Y: p.Y, Facing: u.Facing().Degrees(), Life: u.Life(), Alive: valid})
	}
	// Special effects (#529/#530): the script-spawned effect set, enumerated in
	// creation order — the SoT a reader checks to confirm a world's
	// Game_AddSpecialEffect actually produced live handles.
	effs := g.Effects()
	es := make([]effectState, 0, len(effs))
	for _, e := range effs {
		p := e.Position()
		es = append(es, effectState{ID: e.ID(), X: p.X, Y: p.Y})
	}
	return stateDump{
		TimeOfDay: g.TimeOfDay(),
		Ticks:     ticks,
		StateHash: fmt.Sprintf("0x%016x", g.StateHash()),
		UnitCount: len(us),
		Alive:     alive,
		Order:     order,
		Units:     us,
		Effects:   es,
	}
}

func orderFirstUnit(g *api.Game, delta api.Vec2) orderState {
	units := sortedUnits(g)
	if len(units) == 0 {
		return orderState{}
	}
	p := units[0].Position()
	target := api.Vec2{X: p.X + delta.X, Y: p.Y + delta.Y}
	issued := units[0].Order(api.OrderMove, api.TargetPoint(target))
	return orderState{
		Issued: issued,
		UnitID: units[0].ID(),
		Before: Vec2DTO{X: p.X, Y: p.Y},
		Target: Vec2DTO{X: target.X, Y: target.Y},
	}
}

func sortedUnits(g *api.Game) []api.Unit {
	units := append([]api.Unit(nil), g.UnitsInRange(api.Vec2{}, 1e9, nil)...)
	sort.Slice(units, func(i, j int) bool {
		pi, pj := units[i].Position(), units[j].Position()
		if pi.X != pj.X {
			return pi.X < pj.X
		}
		if pi.Y != pj.Y {
			return pi.Y < pj.Y
		}
		return units[i].Facing().Degrees() < units[j].Facing().Degrees()
	})
	return units
}

func writePlaytestShot(path string, g *api.Game, order orderState) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	const w, h = 640, 360
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	fillImage(img, color.RGBA{R: 25, G: 30, B: 32, A: 255})
	units := sortedUnits(g)
	minX, minY, maxX, maxY := 0.0, 0.0, 8192.0, 8192.0
	for _, u := range units {
		p := u.Position()
		if p.X < minX {
			minX = p.X
		}
		if p.Y < minY {
			minY = p.Y
		}
		if p.X > maxX {
			maxX = p.X
		}
		if p.Y > maxY {
			maxY = p.Y
		}
	}
	if order.Issued {
		if order.Target.X > maxX {
			maxX = order.Target.X
		}
		if order.Target.Y > maxY {
			maxY = order.Target.Y
		}
	}
	if maxX <= minX {
		maxX = minX + 1
	}
	if maxY <= minY {
		maxY = minY + 1
	}
	project := func(x, y float64) (int, int) {
		px := 32 + int((x-minX)/(maxX-minX)*float64(w-64))
		py := 32 + int((y-minY)/(maxY-minY)*float64(h-64))
		return px, py
	}
	if order.Issued {
		bx, by := project(order.Before.X, order.Before.Y)
		tx, ty := project(order.Target.X, order.Target.Y)
		drawLine(img, bx, by, tx, ty, color.RGBA{R: 91, G: 142, B: 113, A: 255})
		drawDot(img, tx, ty, 4, color.RGBA{R: 200, G: 156, B: 83, A: 255})
	}
	for _, u := range units {
		p := u.Position()
		x, y := project(p.X, p.Y)
		drawDot(img, x, y, 5, color.RGBA{R: 225, G: 232, B: 218, A: 255})
	}
	out, err := os.Create(path)
	if err != nil {
		return err
	}
	defer out.Close()
	return png.Encode(out, img)
}

func fillImage(img *image.RGBA, c color.RGBA) {
	for y := img.Rect.Min.Y; y < img.Rect.Max.Y; y++ {
		for x := img.Rect.Min.X; x < img.Rect.Max.X; x++ {
			img.SetRGBA(x, y, c)
		}
	}
}

func drawDot(img *image.RGBA, cx, cy, r int, c color.RGBA) {
	for y := cy - r; y <= cy+r; y++ {
		for x := cx - r; x <= cx+r; x++ {
			if x < img.Rect.Min.X || y < img.Rect.Min.Y || x >= img.Rect.Max.X || y >= img.Rect.Max.Y {
				continue
			}
			dx, dy := x-cx, y-cy
			if dx*dx+dy*dy <= r*r {
				img.SetRGBA(x, y, c)
			}
		}
	}
}

func drawLine(img *image.RGBA, x0, y0, x1, y1 int, c color.RGBA) {
	dx := absInt(x1 - x0)
	sx := -1
	if x0 < x1 {
		sx = 1
	}
	dy := -absInt(y1 - y0)
	sy := -1
	if y0 < y1 {
		sy = 1
	}
	err := dx + dy
	for {
		if x0 >= img.Rect.Min.X && y0 >= img.Rect.Min.Y && x0 < img.Rect.Max.X && y0 < img.Rect.Max.Y {
			img.SetRGBA(x0, y0, c)
		}
		if x0 == x1 && y0 == y1 {
			return
		}
		e2 := 2 * err
		if e2 >= dy {
			err += dy
			x0 += sx
		}
		if e2 <= dx {
			err += dx
			y0 += sy
		}
	}
}

func absInt(v int) int {
	if v < 0 {
		return -v
	}
	return v
}

func engineVersion() string {
	v := buildinfo.Get().Version
	if len(v) > 0 && v[0] == 'v' {
		v = v[1:]
	}
	if v == "" || v == "dev" {
		return "0.1.0"
	}
	return v
}
