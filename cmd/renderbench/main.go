package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"runtime"
	"time"

	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/window"
)

const (
	variantBaseline = "baseline"
	variantFloor    = "floor"

	// #233 slice 3 — material path and camera projection, the two axes of the
	// {PBR,unlit} x {persp,ortho} combo matrix.
	matPBR    = "pbr"
	matUnlit  = "unlit"
	projPersp = "persp"
	projOrtho = "ortho"
)

type benchScenario struct {
	Name            string
	RigidInstances  int
	RigidModelTypes int
	SkinnedUnits    int
	Columns         int
	// #233 M4 bench segment fields. Segment is the named workload class
	// (typical / max-battle / stress); Lights is the count of extra point lights
	// the segment adds (the stress segment's 8-light spell-storm). Zero on the
	// legacy #107 instancing scenes.
	Segment string
	Lights  int
}

type benchDump struct {
	Scene              string                             `json:"scene"`
	Segment            string                             `json:"segment,omitempty"`
	Lights             int                                `json:"lights,omitempty"`
	Variant            string                             `json:"variant"`
	MaterialPath       string                             `json:"materialPath"`
	Projection         string                             `json:"projection"`
	RigidInstances     int                                `json:"rigidInstances"`
	RigidModelTypes    int                                `json:"rigidModelTypes"`
	SkinnedUnits       int                                `json:"skinnedUnits"`
	Policy             litrender.InstancingPolicySnapshot `json:"policy"`
	Stats              litrender.FrameStats               `json:"stats"`
	ExpectedWorldDraws int                                `json:"expectedWorldDraws"`
	FrameMS            float64                            `json:"frameMs"`
	SubmissionMS       float64                            `json:"submissionMs"`
	AnimationSamples   []animationSample                  `json:"animationSamples"`
	DeathTrace         []animationSample                  `json:"deathTrace"`
	// #233 slice 2 — max-battle overlay layers (all active per fog-of-war §7).
	OverlayFog     bool `json:"overlayFog,omitempty"`
	OverlayBars    int  `json:"overlayBars,omitempty"`
	OverlayMinimap bool `json:"overlayMinimap,omitempty"`
	MinimapBlips   int  `json:"minimapBlips,omitempty"`
	// #233 slice 4 — recorded command-stream replay. GL=false marks a headless
	// -nogl run whose draw/state columns are n/a (nil), not fabricated.
	GL                bool          `json:"gl"`
	Frames            int           `json:"frames"`
	StreamHash        string        `json:"streamHash"`
	PerFrame          []frameStat   `json:"perFrame,omitempty"`
	Summary           streamSummary `json:"summary"`
	VisualDescription string        `json:"visualDescription"`
	OK                bool          `json:"ok"`
	Errors            []string      `json:"errors,omitempty"`
}

type animationSample struct {
	Index int     `json:"index"`
	State string  `json:"state"`
	Clip  string  `json:"clip"`
	Time  float32 `json:"time"`
	Fade  float32 `json:"fade"`
}

func main() {
	sceneName := flag.String("scene", "battle500", "benchmark scene: battle500 or battle1000")
	variant := flag.String("variant", variantFloor, "variant: baseline or floor")
	matPath := flag.String("material", matUnlit, "material path: pbr or unlit")
	projection := flag.String("projection", projPersp, "camera projection: persp or ortho")
	nogl := flag.Bool("nogl", false, "headless: replay the stream without GL; draw/ms/visible/culled columns are n/a")
	genStream := flag.Bool("genstream", false, "author the canonical recorded streams under data/maps/bench/ and exit")
	shotPath := flag.String("shot", "", "write screenshot PNG (last replay frame)")
	dumpPath := flag.String("dump", "", "write benchmark JSON")
	flag.Parse()

	if *genStream {
		for _, seg := range []string{"typical", "max-battle", "stress"} {
			s := generateStream(seg, projPersp, benchStreamFrames)
			if err := s.write(seg); err != nil {
				fmt.Fprintf(os.Stderr, "renderbench: genstream %s: %v\n", seg, err)
				os.Exit(1)
			}
			fmt.Printf("renderbench: wrote %s (%d frames)\n", streamPath(seg), s.Frames)
		}
		return
	}

	sc, err := scenarioFor(*sceneName)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderbench: %v\n", err)
		os.Exit(1)
	}
	v := *variant
	if v != variantBaseline && v != variantFloor {
		fmt.Fprintf(os.Stderr, "renderbench: unknown variant %q\n", v)
		os.Exit(1)
	}
	if *matPath != matPBR && *matPath != matUnlit {
		fmt.Fprintf(os.Stderr, "renderbench: unknown material path %q\n", *matPath)
		os.Exit(1)
	}
	if *projection != projPersp && *projection != projOrtho {
		fmt.Fprintf(os.Stderr, "renderbench: unknown projection %q\n", *projection)
		os.Exit(1)
	}

	stream, err := loadStream(sc.Name, *projection)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderbench: stream: %v\n", err)
		os.Exit(1)
	}

	if *nogl {
		if err := runNoGL(sc, v, *matPath, *projection, stream, *dumpPath); err != nil {
			fmt.Fprintf(os.Stderr, "renderbench: nogl: %v\n", err)
			os.Exit(1)
		}
		return
	}

	a := app.App(1024, 576, "LitD renderbench")
	scene := core.NewNode()
	const aspect = 1024.0 / 576.0
	var cam *camera.Camera
	if *projection == projOrtho {
		// Ortho size frames the ~1400-unit field along the vertical axis.
		cam = camera.NewOrthographic(aspect, 250, 3800, 1100, camera.Vertical)
	} else {
		cam = camera.New(aspect)
		cam.SetNear(250)
		cam.SetFar(3800)
	}
	cam.SetPosition(0, 1350, 920)
	cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	scene.Add(cam)
	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.85))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.45)
	sun.SetPosition(-300, 800, 600)
	scene.Add(sun)

	dump, err := buildScene(scene, sc, v, *matPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderbench: %v\n", err)
		os.Exit(1)
	}
	dump.Projection = *projection
	dump.GL = true
	dump.Frames = stream.Frames
	dump.StreamHash = computeStreamHash(sc.Name, v, *matPath, dump.ExpectedWorldDraws, dump.OverlayBars, dump.MinimapBlips, stream)

	a.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := a.GetSize()
		a.Gls().Viewport(0, 0, int32(w), int32(h))
		cam.SetAspect(float32(w) / float32(h))
	})
	w, h := a.GetSize()
	a.Gls().Viewport(0, 0, int32(w), int32(h))

	frame := 0
	a.Run(func(rend *renderer.Renderer, dt time.Duration) {
		if frame >= stream.Frames {
			finishGL(a, dump, *shotPath, *dumpPath)
			os.Exit(0)
		}
		applyCamKey(cam, stream.Camera[frame])

		var m0 runtime.MemStats
		runtime.ReadMemStats(&m0)
		frameStart := time.Now()
		a.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
		if err := rend.Render(scene, cam); err != nil {
			fmt.Fprintf(os.Stderr, "renderbench: render: %v\n", err)
			os.Exit(1)
		}
		frameMS := float64(time.Since(frameStart).Nanoseconds()) / 1e6
		var m1 runtime.MemStats
		runtime.ReadMemStats(&m1)
		fs := litrender.ReadFrameStats(rend)
		dump.PerFrame = append(dump.PerFrame, frameStat{
			Frame:        frame,
			FrameMS:      frameMS,
			OpaqueDraws:  intPtr(fs.OpaqueDrawCalls),
			StateChanges: intPtr(fs.StateChanges),
			Visible:      intPtr(fs.VisibleGraphics),
			Culled:       intPtr(fs.CulledGraphics),
			Allocs:       int64(m1.Mallocs - m0.Mallocs),
			VoiceCount:   nil, // n/a: render bench runs no audio admission manager
		})
		// Last-frame stats stand in for the legacy single-frame top-level fields.
		dump.Stats = fs
		dump.FrameMS = frameMS
		// Screenshot the final (most zoomed-in) keyframe.
		if frame == stream.Frames-1 && *shotPath != "" {
			if err := screenshot(a, *shotPath); err != nil {
				fmt.Fprintf(os.Stderr, "renderbench: screenshot: %v\n", err)
				os.Exit(1)
			}
		}
		frame++
	})
}

// finishGL validates the replayed run, summarizes the per-frame series, and
// writes the dump. Called once after the last stream frame.
func finishGL(a *app.Application, dump *benchDump, shotPath, dumpPath string) {
	validateDump(dump)
	dump.Summary = summarize(dump.PerFrame, true)
	if dumpPath != "" {
		if err := writeJSON(dumpPath, dump); err != nil {
			fmt.Fprintf(os.Stderr, "renderbench: dump: %v\n", err)
			os.Exit(1)
		}
	}
	fmt.Printf("renderbench: scene=%s variant=%s mat=%s proj=%s frames=%d hash=%s lastStats=%+v expectedWorldDraws=%d avgFrameMs=%.3f p99FrameMs=%.3f ok=%v shot=%s dump=%s\n",
		dump.Scene, dump.Variant, dump.MaterialPath, dump.Projection, dump.Frames, dump.StreamHash,
		dump.Stats, dump.ExpectedWorldDraws, dump.Summary.AvgFrameMS, dump.Summary.P99FrameMS, dump.OK, shotPath, dumpPath)
}

// runNoGL replays the same command stream with no GL context. The deterministic
// scene definition (draw target, overlay counts, stream hash) is computed exactly
// as in the GL run; the GL-only columns (frame ms, draws, visible, culled) are
// left n/a (nil) — never fabricated. Parity is the matching stream hash.
func runNoGL(sc benchScenario, variant, matPath, projection string, stream benchStream, dumpPath string) error {
	dump, err := buildScene(core.NewNode(), sc, variant, matPath)
	if err != nil {
		return err
	}
	dump.Projection = projection
	dump.GL = false
	dump.Frames = stream.Frames
	dump.StreamHash = computeStreamHash(sc.Name, variant, matPath, dump.ExpectedWorldDraws, dump.OverlayBars, dump.MinimapBlips, stream)
	for f := 0; f < stream.Frames; f++ {
		dump.PerFrame = append(dump.PerFrame, frameStat{
			Frame:   f,
			FrameMS: 0, // n/a without GL; summary frame-ms stats stay 0
		})
	}
	dump.Summary = summarize(dump.PerFrame, false)
	if dumpPath != "" {
		if err := writeJSON(dumpPath, dump); err != nil {
			return err
		}
	}
	fmt.Printf("renderbench: scene=%s variant=%s mat=%s proj=%s frames=%d hash=%s gl=false (draw/ms/visible/culled = n/a) expectedWorldDraws=%d dump=%s\n",
		dump.Scene, dump.Variant, dump.MaterialPath, dump.Projection, dump.Frames, dump.StreamHash, dump.ExpectedWorldDraws, dumpPath)
	return nil
}

func scenarioFor(name string) (benchScenario, error) {
	switch name {
	case "battle500":
		return benchScenario{Name: name, RigidInstances: 440, RigidModelTypes: 12, SkinnedUnits: 60, Columns: 25}, nil
	case "battle1000":
		return benchScenario{Name: name, RigidInstances: 880, RigidModelTypes: 12, SkinnedUnits: 120, Columns: 40}, nil
	// #233 M4 acceptance segments. Counts: typical ~200 units (normal play
	// density), max-battle 500 visible units (overlays added in a later slice),
	// stress 1000 units + an 8-light spell-storm. Rigid+skinned split mirrors the
	// battle scenes so the instancing floor stays the measured workload.
	case "typical":
		return benchScenario{Name: name, Segment: "typical", RigidInstances: 160, RigidModelTypes: 12, SkinnedUnits: 40, Columns: 16}, nil
	case "max-battle":
		return benchScenario{Name: name, Segment: "max-battle", RigidInstances: 440, RigidModelTypes: 12, SkinnedUnits: 60, Columns: 25}, nil
	case "stress":
		return benchScenario{Name: name, Segment: "stress", RigidInstances: 880, RigidModelTypes: 12, SkinnedUnits: 120, Columns: 40, Lights: 8}, nil
	default:
		return benchScenario{}, fmt.Errorf("unknown scene %q", name)
	}
}

func buildScene(scene *core.Node, sc benchScenario, variant, matPath string) (*benchDump, error) {
	if variant != variantBaseline && variant != variantFloor {
		return nil, fmt.Errorf("unknown variant %q", variant)
	}
	if matPath != matPBR && matPath != matUnlit {
		return nil, fmt.Errorf("unknown material path %q", matPath)
	}
	if sc.Columns <= 0 {
		return nil, fmt.Errorf("scene %q has non-positive column count %d", sc.Name, sc.Columns)
	}
	policy, err := litrender.PlanRigidOnlyInstancing(sc.RigidInstances, sc.RigidModelTypes, sc.SkinnedUnits)
	if err != nil {
		return nil, err
	}
	dump := &benchDump{
		Scene:             sc.Name,
		Segment:           sc.Segment,
		Lights:            sc.Lights,
		Variant:           variant,
		MaterialPath:      matPath,
		RigidInstances:    sc.RigidInstances,
		RigidModelTypes:   sc.RigidModelTypes,
		SkinnedUnits:      sc.SkinnedUnits,
		Policy:            policy,
		VisualDescription: "Rigid content is rendered as buildings/doodads/projectiles; skinned units remain individual per-draw actor nodes driven by AnimDriver clip state. Full GLB skin sampling remains tracked in #308.",
		OK:                true,
	}
	if variant == variantBaseline {
		dump.ExpectedWorldDraws = policy.BaselineWorldDraws + 1
	} else {
		dump.ExpectedWorldDraws = policy.FloorWorldDraws + 1
	}

	overlays := sc.Segment == "max-battle"
	if overlays {
		// Fog-of-war ground: the plane is a FogTerrainMesh dimmed by a synthetic
		// three-zone fog texture, so the keyframe shows hidden/explored/visible
		// bands under the army (the fog overlay the segment requires).
		addFogGround(scene)
	} else {
		groundMat := material.NewStandard(&math32.Color{R: 0.09, G: 0.12, B: 0.10})
		ground := graphic.NewMesh(geometry.NewPlane(1700, 980), groundMat)
		ground.SetRotationX(-math32.Pi / 2)
		scene.Add(ground)
	}

	if variant == variantBaseline {
		addRigidBaseline(scene, sc, matPath)
	} else {
		if err := addRigidFloor(scene, sc, matPath); err != nil {
			return nil, err
		}
	}
	dump.AnimationSamples, dump.DeathTrace = addSkinnedPerDraw(scene, sc, matPath)
	addSpellStormLights(scene, sc.Lights)

	if overlays {
		dump.OverlayFog = true
		dump.OverlayBars = addHealthBars(scene, sc)
		dump.OverlayMinimap, dump.MinimapBlips = addMinimap(scene, sc)
	}
	return dump, nil
}

// benchField is the world rectangle the fog/minimap overlays cover — a square
// around the rigid army (zBase -460) and the skinned front row (zBase 340).
const (
	benchFieldMinX, benchFieldMinZ = float32(-700), float32(-700)
	benchFieldMaxX, benchFieldMaxZ = float32(700), float32(700)
)

// benchFogGrid is a synthetic three-zone fog source (hidden | explored | visible
// by world X), mirroring the #161 fogscout fixture so the bench fog reads the
// same hidden→explored→visible ramp without a live sim.
type benchFogGrid struct{ size int32 }

func (g benchFogGrid) FogStateAt(_ uint8, x, _ int32) uint8 {
	if x < 0 || x >= g.size {
		return 0
	}
	switch {
	case x < g.size/3:
		return 0 // hidden
	case x < 2*g.size/3:
		return 1 // explored
	default:
		return 2 // visible
	}
}

func addFogGround(scene *core.Node) {
	origin := math32.Vector2{X: benchFieldMinX, Y: benchFieldMinZ}
	size := math32.Vector2{X: benchFieldMaxX - benchFieldMinX, Y: benchFieldMaxZ - benchFieldMinZ}
	fog := litrender.NewFogTexture(1)
	fog.Update(benchFogGrid{size: int32(fog.Size())}, 1)
	// A world-baked ground quad (identity model matrix) spanning the field, in the
	// XZ plane. Built directly in world space so the fog UV (world XZ) is correct.
	geom := geometry.NewPlane(size.X, size.Y)
	mat := material.NewStandard(&math32.Color{R: 0.30, G: 0.40, B: 0.26})
	ground := litrender.NewFogTerrainMesh(geom, mat, fog, origin, size)
	ground.SetRotationX(-math32.Pi / 2)
	ground.SetPosition((benchFieldMinX+benchFieldMaxX)/2, 0, (benchFieldMinZ+benchFieldMaxZ)/2)
	scene.Add(ground)
}

// addHealthBars draws camera-facing health-bar billboards over the front-row
// skinned units, fill mapped to the green→red ramp. Returns the bar count.
func addHealthBars(scene *core.Node, sc benchScenario) int {
	pool := litrender.NewHealthBarPool(sc.SkinnedUnits)
	n := 0
	for i := 0; i < sc.SkinnedUnits; i++ {
		x, z := gridPos(i, sc.Columns, sc.SkinnedUnits, 26, 340)
		fill := float32(i%5+1) / 5 // 0.2..1.0 spread across the row
		idx, ok := pool.Acquire(math32.Vector3{X: x, Y: 44, Z: z}, fill, 1)
		if !ok {
			break
		}
		b := pool.At(idx)
		barMat := material.NewStandard(&math32.Color{R: b.Color.R, G: b.Color.G, B: b.Color.B})
		barMat.SetEmissiveColor(&math32.Color{R: b.Color.R, G: b.Color.G, B: b.Color.B})
		barMat.SetUseLights(material.UseLightNone)
		// Transparent (opacity 1) so the bars render in the alpha pass and do not
		// inflate the opaque world-draw count the #107 instancing invariant asserts.
		barMat.SetTransparent(true)
		bar := graphic.NewSprite(28*b.Fill+2, 5, barMat) // width scales with fill
		bar.SetPosition(b.Anchor.X, b.Anchor.Y, b.Anchor.Z)
		scene.Add(bar)
		n++
	}
	return n
}

// addMinimap composites a unit-blip minimap and shows it as a HUD image in the
// bottom-left corner. Returns whether it was added and the blip count.
func addMinimap(scene *core.Node, sc benchScenario) (bool, int) {
	mm := litrender.NewMinimap(benchFieldMinX, benchFieldMinZ, benchFieldMaxX, benchFieldMaxZ)
	mm.Clear()
	blips := 0
	plot := func(total int, spacing, zBase float32, c litrender.RGBA) {
		for i := 0; i < total; i++ {
			x, z := gridPos(i, sc.Columns, total, spacing, zBase)
			mm.PlotBlip(x, z, 2, c, true)
			blips++
		}
	}
	plot(sc.RigidInstances, 34, -460, litrender.RGBA{R: 0.8, G: 0.5, B: 0.2, A: 1})
	plot(sc.SkinnedUnits, 26, 340, litrender.RGBA{R: 0.3, G: 0.7, B: 1, A: 1})
	mm.Upload()
	img := gui.NewImageFromTex(mm.EnsureTexture())
	img.SetSize(180, 180)
	img.SetPosition(12, 576-192) // bottom-left of the 1024x576 frame
	scene.Add(img)
	return true, blips
}

// addSpellStormLights adds the stress segment's spell-storm point lights, spread
// in a ring over the unit field so several units fall in multiple light ranges
// (the worst case for the lighting loop). Deterministic placement (no RNG).
func addSpellStormLights(scene *core.Node, n int) {
	if n <= 0 {
		return
	}
	colors := []math32.Color{
		{R: 0.9, G: 0.5, B: 0.2}, {R: 0.3, G: 0.6, B: 1.0}, {R: 0.8, G: 0.2, B: 0.9}, {R: 0.2, G: 0.9, B: 0.5},
	}
	const radius = 420
	for i := 0; i < n; i++ {
		ang := 2 * math32.Pi * float32(i) / float32(n)
		c := colors[i%len(colors)]
		l := light.NewPoint(&c, 900)
		l.SetLinearDecay(1)
		l.SetQuadraticDecay(1)
		l.SetPosition(radius*math32.Cos(ang), 120, 200+radius*math32.Sin(ang))
		scene.Add(l)
	}
}

func addRigidBaseline(scene *core.Node, sc benchScenario, matPath string) {
	geom := geometry.NewBox(22, 36, 22)
	mats := rigidMaterials(sc.RigidModelTypes, matPath)
	for i := 0; i < sc.RigidInstances; i++ {
		x, z := gridPos(i, sc.Columns, sc.RigidInstances, 34, -460)
		mesh := graphic.NewMesh(geom, mats[i%len(mats)])
		mesh.SetPosition(x, 18, z)
		scene.Add(mesh)
	}
}

func addRigidFloor(scene *core.Node, sc benchScenario, matPath string) error {
	mats := rigidMaterials(sc.RigidModelTypes, matPath)
	geom := geometry.NewBox(22, 36, 22)
	for typ := 0; typ < sc.RigidModelTypes; typ++ {
		count := sc.RigidInstances / sc.RigidModelTypes
		if typ < sc.RigidInstances%sc.RigidModelTypes {
			count++
		}
		if count == 0 {
			continue
		}
		mesh := graphic.NewInstancedMesh(geom, mats[typ], 0)
		scene.Add(mesh)
		buf, err := litrender.NewInstanceBuffer(mesh, count)
		if err != nil {
			return err
		}
		buf.BeginFrame()
		if err := buf.SetCount(count); err != nil {
			return err
		}
		for n := 0; n < count; n++ {
			i := typ + n*sc.RigidModelTypes
			x, z := gridPos(i, sc.Columns, sc.RigidInstances, 34, -460)
			var m math32.Matrix4
			m.MakeTranslation(x, 18, z)
			if err := buf.SetInstance(n, &m, typ%litrender.TeamColorSlots); err != nil {
				return err
			}
		}
	}
	return nil
}

func addSkinnedPerDraw(scene *core.Node, sc benchScenario, matPath string) ([]animationSample, []animationSample) {
	clips := litrender.ClipSet{
		litrender.ClipIdle:   {Duration: 1.0, Loop: true},
		litrender.ClipWalk:   {Duration: 1.0, Loop: true},
		litrender.ClipAttack: {Duration: 0.6, Loop: false, ImpactTime: 0.3},
		litrender.ClipDeath:  {Duration: 0.8, Loop: false},
	}
	states := make([]litrender.SimAnimState, sc.SkinnedUnits)
	visible := make([]bool, sc.SkinnedUnits)
	for i := range states {
		visible[i] = true
		switch i % 4 {
		case 1:
			states[i] = litrender.StateMove
		case 2:
			states[i] = litrender.StateAttack
		case 3:
			states[i] = litrender.StateDead
		default:
			states[i] = litrender.StateIdle
		}
	}
	driver := litrender.NewAnimDriver(sc.SkinnedUnits)
	driver.Update(states, visible, 0, clips, 0.5)
	driver.Update(states, visible, 0.35, clips, 0.5)

	geom := geometry.NewBox(18, 30, 18)
	mats := []material.IMaterial{
		benchMat(matPath, math32.Color{R: 0.26, G: 0.45, B: 0.95}),
		benchMat(matPath, math32.Color{R: 0.26, G: 0.78, B: 0.40}),
		benchMat(matPath, math32.Color{R: 0.96, G: 0.62, B: 0.20}),
		benchMat(matPath, math32.Color{R: 0.50, G: 0.50, B: 0.50}),
	}
	samples := make([]animationSample, 0, 4)
	for i := 0; i < sc.SkinnedUnits; i++ {
		x, z := gridPos(i, sc.Columns, sc.SkinnedUnits, 26, 340)
		state := states[i]
		mesh := graphic.NewMesh(geom, mats[int(state)])
		clip := driver.Clip(i)
		switch state {
		case litrender.StateMove:
			mesh.SetRotationX(-0.18)
			mesh.SetPosition(x, 15, z)
		case litrender.StateAttack:
			mesh.SetRotationZ(0.28)
			mesh.SetPosition(x, 15, z)
		case litrender.StateDead:
			mesh.SetScale(1, 0.2, 1.4)
			mesh.SetPosition(x, 3, z)
		default:
			mesh.SetPosition(x, 15, z)
		}
		scene.Add(mesh)
		if i < 4 {
			samples = append(samples, animationSample{
				Index: i,
				State: stateName(state),
				Clip:  clipName(clip),
				Time:  driver.Time(i),
				Fade:  driver.Fade(i),
			})
		}
	}

	death := litrender.NewAnimDriver(1)
	deathStates := []litrender.SimAnimState{litrender.StateDead}
	deathVis := []bool{true}
	trace := make([]animationSample, 0, 3)
	for _, dt := range []float32{0, 0.8, 0.25} {
		death.Update(deathStates, deathVis, dt, clips, 0.5)
		trace = append(trace, animationSample{
			Index: 0,
			State: stateName(litrender.StateDead),
			Clip:  clipName(death.Clip(0)),
			Time:  death.Time(0),
			Fade:  death.Fade(0),
		})
	}
	return samples, trace
}

func rigidMaterials(n int, matPath string) []material.IMaterial {
	mats := make([]material.IMaterial, n)
	for i := range mats {
		c := math32.Color{R: 0.35 + 0.04*float32(i%5), G: 0.26 + 0.05*float32((i+2)%5), B: 0.18 + 0.04*float32((i+4)%5)}
		mats[i] = benchMat(matPath, c)
	}
	return mats
}

// benchMat builds a unit material on the selected combo-matrix path: PBR routes
// to the metallic-roughness physical shader (lit, rough dielectric); unlit routes
// to the standard shader with lighting off (flat base colour). Geometry and draw
// counts are identical across paths — only the shader/cost differs.
func benchMat(matPath string, c math32.Color) material.IMaterial {
	if matPath == matPBR {
		return material.NewPhysical().
			SetBaseColorFactor(&math32.Color4{R: c.R, G: c.G, B: c.B, A: 1}).
			SetMetallicFactor(0).
			SetRoughnessFactor(1)
	}
	m := material.NewStandard(&c)
	m.SetUseLights(material.UseLightNone)
	return m
}

func gridPos(i, columns, total int, spacing, zBase float32) (float32, float32) {
	col, row := i%columns, i/columns
	rows := (total + columns - 1) / columns
	x := (float32(col) - float32(columns-1)/2) * spacing
	z := zBase + (float32(row)-float32(rows)/2)*spacing
	return x, z
}

func validateDump(d *benchDump) {
	if d.Stats.OpaqueDrawCalls != d.ExpectedWorldDraws {
		d.Errors = append(d.Errors, fmt.Sprintf("opaque draw calls = %d, want %d", d.Stats.OpaqueDrawCalls, d.ExpectedWorldDraws))
	}
	if d.Policy.SkinnedInstanced {
		d.Errors = append(d.Errors, "floor policy unexpectedly marked skinned content instanced")
	}
	if len(d.AnimationSamples) < 4 {
		d.Errors = append(d.Errors, "missing animation samples")
	}
	if len(d.DeathTrace) != 3 || d.DeathTrace[1].Time != 0.8 || d.DeathTrace[2].Fade >= 1 {
		d.Errors = append(d.Errors, fmt.Sprintf("death trace invalid: %+v", d.DeathTrace))
	}
	d.OK = len(d.Errors) == 0
}

func stateName(s litrender.SimAnimState) string {
	switch s {
	case litrender.StateMove:
		return "move"
	case litrender.StateAttack:
		return "attack"
	case litrender.StateDead:
		return "dead"
	default:
		return "idle"
	}
}

func clipName(c litrender.ClipID) string {
	switch c {
	case litrender.ClipWalk:
		return "Walk"
	case litrender.ClipAttack:
		return "Attack"
	case litrender.ClipDeath:
		return "Death"
	default:
		return "Idle"
	}
}

func writeJSON(path string, v interface{}) error {
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func screenshot(a *app.Application, path string) error {
	w, h := a.GetFramebufferSize()
	data := a.Gls().ReadPixels(0, 0, w, h, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	row := w * 4
	for y := 0; y < h; y++ {
		copy(img.Pix[y*img.Stride:y*img.Stride+row], data[(h-1-y)*row:(h-y)*row])
	}
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, img)
}
