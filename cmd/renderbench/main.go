package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"time"

	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/window"
)

const (
	variantBaseline = "baseline"
	variantFloor    = "floor"
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
	VisualDescription  string                             `json:"visualDescription"`
	OK                 bool                               `json:"ok"`
	Errors             []string                           `json:"errors,omitempty"`
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
	shotPath := flag.String("shot", "", "write screenshot PNG")
	dumpPath := flag.String("dump", "", "write benchmark JSON")
	flag.Parse()

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

	a := app.App(1024, 576, "LitD renderbench")
	scene := core.NewNode()
	cam := camera.New(1024.0 / 576.0)
	cam.SetPosition(0, 1350, 920)
	cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	cam.SetNear(250)
	cam.SetFar(3800)
	scene.Add(cam)
	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.85))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.45)
	sun.SetPosition(-300, 800, 600)
	scene.Add(sun)

	dump, err := buildScene(scene, sc, v)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderbench: %v\n", err)
		os.Exit(1)
	}

	a.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := a.GetSize()
		a.Gls().Viewport(0, 0, int32(w), int32(h))
		cam.SetAspect(float32(w) / float32(h))
	})
	w, h := a.GetSize()
	a.Gls().Viewport(0, 0, int32(w), int32(h))

	rendered := false
	a.Run(func(rend *renderer.Renderer, dt time.Duration) {
		if rendered {
			os.Exit(0)
		}
		rendered = true
		frameStart := time.Now()
		a.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
		submitStart := time.Now()
		if err := rend.Render(scene, cam); err != nil {
			fmt.Fprintf(os.Stderr, "renderbench: render: %v\n", err)
			os.Exit(1)
		}
		dump.SubmissionMS = float64(time.Since(submitStart).Nanoseconds()) / 1e6
		dump.FrameMS = float64(time.Since(frameStart).Nanoseconds()) / 1e6
		dump.Stats = litrender.ReadFrameStats(rend)
		validateDump(dump)
		if *shotPath != "" {
			if err := screenshot(a, *shotPath); err != nil {
				fmt.Fprintf(os.Stderr, "renderbench: screenshot: %v\n", err)
				os.Exit(1)
			}
		}
		if *dumpPath != "" {
			if err := writeJSON(*dumpPath, dump); err != nil {
				fmt.Fprintf(os.Stderr, "renderbench: dump: %v\n", err)
				os.Exit(1)
			}
		}
		fmt.Printf("renderbench: scene=%s variant=%s stats=%+v expectedWorldDraws=%d frameMs=%.3f submissionMs=%.3f ok=%v shot=%s dump=%s\n",
			dump.Scene, dump.Variant, dump.Stats, dump.ExpectedWorldDraws, dump.FrameMS, dump.SubmissionMS, dump.OK, *shotPath, *dumpPath)
		os.Exit(0)
	})
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

func buildScene(scene *core.Node, sc benchScenario, variant string) (*benchDump, error) {
	if variant != variantBaseline && variant != variantFloor {
		return nil, fmt.Errorf("unknown variant %q", variant)
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

	groundMat := material.NewStandard(&math32.Color{R: 0.09, G: 0.12, B: 0.10})
	ground := graphic.NewMesh(geometry.NewPlane(1700, 980), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	if variant == variantBaseline {
		addRigidBaseline(scene, sc)
	} else {
		if err := addRigidFloor(scene, sc); err != nil {
			return nil, err
		}
	}
	dump.AnimationSamples, dump.DeathTrace = addSkinnedPerDraw(scene, sc)
	addSpellStormLights(scene, sc.Lights)
	return dump, nil
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

func addRigidBaseline(scene *core.Node, sc benchScenario) {
	geom := geometry.NewBox(22, 36, 22)
	mats := rigidMaterials(sc.RigidModelTypes)
	for i := 0; i < sc.RigidInstances; i++ {
		x, z := gridPos(i, sc.Columns, sc.RigidInstances, 34, -460)
		mesh := graphic.NewMesh(geom, mats[i%len(mats)])
		mesh.SetPosition(x, 18, z)
		scene.Add(mesh)
	}
}

func addRigidFloor(scene *core.Node, sc benchScenario) error {
	mats := rigidMaterials(sc.RigidModelTypes)
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

func addSkinnedPerDraw(scene *core.Node, sc benchScenario) ([]animationSample, []animationSample) {
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
	mats := []*material.Standard{
		material.NewStandard(&math32.Color{R: 0.26, G: 0.45, B: 0.95}),
		material.NewStandard(&math32.Color{R: 0.26, G: 0.78, B: 0.40}),
		material.NewStandard(&math32.Color{R: 0.96, G: 0.62, B: 0.20}),
		material.NewStandard(&math32.Color{R: 0.50, G: 0.50, B: 0.50}),
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

func rigidMaterials(n int) []*material.Standard {
	mats := make([]*material.Standard, n)
	for i := range mats {
		c := math32.Color{R: 0.35 + 0.04*float32(i%5), G: 0.26 + 0.05*float32((i+2)%5), B: 0.18 + 0.04*float32((i+4)%5)}
		mats[i] = material.NewStandard(&c)
	}
	return mats
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
