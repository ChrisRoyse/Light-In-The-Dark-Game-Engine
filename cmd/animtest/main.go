// Command animtest is the R1 risk smoke test (PRD §8, FSV protocol):
// load a GLB, enumerate its animations and skins, play a clip, and capture
// screenshots at staggered times so an agent can verify skinned animation
// actually renders (model visible, pose changes between frames).
//
// Usage:
//
//	animtest -glb path/to/model.glb [-anim Walk] [-out artifacts/anim]
//
// Output: structured "anim:" / "skin:" / "event:" lines + PNG per sample.
// Exit 0 = loaded and rendered; 1 = load/render error.
package main

import (
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"time"

	"github.com/g3n/engine/animation"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/loader/gltf"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/window"
)

func main() {
	glbPath := flag.String("glb", "", "path to .glb file (required)")
	animName := flag.String("anim", "", "animation clip name (default: first clip)")
	outPrefix := flag.String("out", "artifacts/animtest", "screenshot path prefix")
	flag.Parse()
	if *glbPath == "" {
		fmt.Fprintln(os.Stderr, "usage: animtest -glb model.glb [-anim Name] [-out prefix]")
		os.Exit(1)
	}

	a := app.App(960, 720, "LitD animtest")
	scene := core.NewNode()

	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.8))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.8)
	sun.SetPosition(5, 10, 10)
	scene.Add(sun)

	cam := camera.New(960.0 / 720.0)
	cam.SetPosition(0, 1.5, 4)
	cam.LookAt(&math32.Vector3{X: 0, Y: 1, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	scene.Add(cam)

	doc, err := gltf.ParseBin(*glbPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: ParseBin: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("event: parsed %s — %d meshes, %d skins, %d animations, %d materials\n",
		filepath.Base(*glbPath), len(doc.Meshes), len(doc.Skins), len(doc.Animations), len(doc.Materials))
	for i, an := range doc.Animations {
		fmt.Printf("anim: [%d] %q (%d channels)\n", i, an.Name, len(an.Channels))
	}
	for i, sk := range doc.Skins {
		fmt.Printf("skin: [%d] %q (%d joints)\n", i, sk.Name, len(sk.Joints))
	}

	sceneIdx := 0
	if doc.Scene != nil {
		sceneIdx = *doc.Scene
	}
	model, err := doc.LoadScene(sceneIdx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: LoadScene: %v\n", err)
		os.Exit(1)
	}
	scene.Add(model)

	// Auto-frame: models range from 1-unit hex tiles to multi-unit castles
	// and are not all origin-centered. Fixed framing rendered off-screen
	// models as black frames — a false "does not render" signal.
	model.UpdateMatrixWorld() // world matrices are stale until first render; bbox needs them
	bb := model.BoundingBox()
	center := math32.Vector3{}
	bb.Center(&center)
	size := math32.Vector3{}
	bb.Size(&size)
	extent := size.X
	if size.Y > extent {
		extent = size.Y
	}
	if size.Z > extent {
		extent = size.Z
	}
	if extent < 0.001 || extent > 1e6 {
		fmt.Printf("event: degenerate bounding box (extent %v), keeping default camera\n", extent)
	} else {
		dist := extent * 1.6
		cam.SetPosition(center.X+dist*0.6, center.Y+dist*0.6, center.Z+dist)
		cam.LookAt(&center, &math32.Vector3{X: 0, Y: 1, Z: 0})
		fmt.Printf("event: framed bbox center=(%.2f,%.2f,%.2f) extent=%.2f\n", center.X, center.Y, center.Z, extent)
	}

	var anim *animation.Animation
	if len(doc.Animations) > 0 {
		if *animName != "" {
			anim, err = doc.LoadAnimationByName(*animName)
		} else {
			anim, err = doc.LoadAnimation(0)
			*animName = doc.Animations[0].Name
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: LoadAnimation: %v\n", err)
			os.Exit(1)
		}
		anim.SetLoop(true)
		fmt.Printf("event: playing clip %q\n", *animName)
	} else {
		fmt.Println("event: no animations in file (static model)")
	}

	// sample screenshots spread across the clip
	sampleAt := []time.Duration{300 * time.Millisecond, 700 * time.Millisecond, 1100 * time.Millisecond}
	sampleIdx := 0
	start := time.Now()

	a.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := a.GetSize()
		a.Gls().Viewport(0, 0, int32(w), int32(h))
		cam.SetAspect(float32(w) / float32(h))
	})
	w0, h0 := a.GetSize()
	a.Gls().Viewport(0, 0, int32(w0), int32(h0))

	a.Run(func(rend *renderer.Renderer, dt time.Duration) {
		if anim != nil {
			anim.Update(float32(dt.Seconds()))
		}
		a.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
		if err := rend.Render(scene, cam); err != nil {
			fmt.Fprintf(os.Stderr, "error: render: %v\n", err)
			os.Exit(1)
		}
		if sampleIdx < len(sampleAt) && time.Since(start) >= sampleAt[sampleIdx] {
			path := fmt.Sprintf("%s-%d.png", *outPrefix, sampleIdx)
			if err := screenshot(a, path); err != nil {
				fmt.Fprintf(os.Stderr, "error: screenshot: %v\n", err)
				os.Exit(1)
			}
			fmt.Printf("event: screenshot %s at t=%v\n", path, sampleAt[sampleIdx])
			sampleIdx++
			if sampleIdx == len(sampleAt) {
				os.Exit(0)
			}
		}
	})
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
