// Command portraitdemo is the EGL FSV harness for the unit-portrait render-to-
// texture capability (#193). It renders a lit subject mesh into an offscreen
// litrender.PortraitTarget (an FBO with a color texture + depth renderbuffer),
// reads the color buffer back, classifies subject-vs-background coverage, writes
// the portrait PNG, and emits a JSON dump. Run headless under EGL:
//
//	DISPLAY=:0 G3N_EGL=1 ./bin/portraitdemo -shot artifacts/portrait.png -dump artifacts/portrait.json
//
// FSV: read the PNG (a shaded sphere on a dark field) and the dump (coverage in a
// sane band, mean color near the subject's gold). The offscreen render is the
// source of truth — never the exit code alone.
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
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
)

type portraitDump struct {
	Size     int                        `json:"size"`
	Subject  string                     `json:"subject"`
	BG       [4]float32                 `json:"bg"`
	Coverage litrender.PortraitCoverage `json:"coverage"`
	OK       bool                       `json:"ok"`
	Errors   []string                   `json:"errors,omitempty"`
}

func main() {
	size := flag.Int("size", 256, "portrait target edge in pixels")
	shotPath := flag.String("shot", "", "write the portrait PNG")
	dumpPath := flag.String("dump", "", "write the portrait JSON dump")
	flag.Parse()

	// A window-backed GL context (the offscreen target still needs a live context).
	a := app.App(512, 512, "LitD portrait demo")

	// The portrait subject: a lit gold sphere at the origin — a recognizable shaded
	// 3D body, so a correct RTT yields a gradient-shaded disc, a broken one a flat
	// field. (A real unit GLB would be asset-gated; the RTT capability is the
	// deliverable, the subject is any mesh.)
	subjectColor := math32.Color{R: 0.90, G: 0.75, B: 0.30}
	scene := core.NewNode()
	sphereGeom := geometry.NewSphere(1.0, 32, 24)
	mat := material.NewStandard(&subjectColor)
	sphere := graphic.NewMesh(sphereGeom, mat)
	scene.Add(sphere)
	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.35))
	key := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.9)
	key.SetPosition(2, 3, 4)
	scene.Add(key)

	cam := camera.New(1) // square aspect for a square target
	cam.SetPosition(0, 0.2, 3.2)
	cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	scene.Add(cam)

	bg := math32.Color4{R: 0.05, G: 0.05, B: 0.08, A: 1}

	a.Run(func(rend *renderer.Renderer, _ time.Duration) {
		target, err := litrender.NewPortraitTarget(a.Gls(), int32(*size), int32(*size))
		if err != nil {
			fail("create portrait target: %v", err)
		}
		defer target.Dispose()

		if err := target.Render(rend, scene, cam, bg); err != nil {
			fail("render portrait: %v", err)
		}
		pixels := target.ReadPixels()
		cov := litrender.AnalyzePortrait(pixels, bg, 12)

		// Fail-closed acceptance band: the sphere must cover a meaningful slice of
		// the frame (not empty, not the whole field), and the subject must be the
		// gold body (red channel dominant, brighter than green/blue).
		dump := portraitDump{
			Size:     *size,
			Subject:  "gold-sphere",
			BG:       [4]float32{bg.R, bg.G, bg.B, bg.A},
			Coverage: cov,
		}
		if cov.Coverage < 0.10 {
			dump.Errors = append(dump.Errors, fmt.Sprintf("coverage %.3f too low — subject did not render", cov.Coverage))
		}
		if cov.Coverage > 0.95 {
			dump.Errors = append(dump.Errors, fmt.Sprintf("coverage %.3f too high — background not distinct", cov.Coverage))
		}
		if !(cov.MeanR > cov.MeanG && cov.MeanR > cov.MeanB) {
			dump.Errors = append(dump.Errors, fmt.Sprintf("subject mean (%.2f,%.2f,%.2f) is not gold-dominant", cov.MeanR, cov.MeanG, cov.MeanB))
		}
		dump.OK = len(dump.Errors) == 0

		if *shotPath != "" {
			if err := writePortraitPNG(*shotPath, pixels, *size); err != nil {
				fail("write png: %v", err)
			}
		}
		if *dumpPath != "" {
			if err := writeJSON(*dumpPath, dump); err != nil {
				fail("write dump: %v", err)
			}
		}
		fmt.Printf("portraitdemo: size=%d coverage=%.3f mean=(%.2f,%.2f,%.2f) ok=%v shot=%s dump=%s\n",
			dump.Size, cov.Coverage, cov.MeanR, cov.MeanG, cov.MeanB, dump.OK, *shotPath, *dumpPath)
		if !dump.OK {
			os.Exit(3)
		}
		os.Exit(0)
	})
}

// writePortraitPNG flips GL's bottom-up rows to image top-down (as the bench
// screenshot does) and encodes the readback.
func writePortraitPNG(path string, rgba []byte, size int) error {
	img := image.NewRGBA(image.Rect(0, 0, size, size))
	row := size * 4
	for y := 0; y < size; y++ {
		src := (size - 1 - y) * row
		copy(img.Pix[y*img.Stride:y*img.Stride+row], rgba[src:src+row])
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

func writeJSON(path string, v interface{}) error {
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	b, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, b, 0o644)
}

func fail(format string, a ...interface{}) {
	fmt.Fprintf(os.Stderr, "portraitdemo: "+format+"\n", a...)
	os.Exit(1)
}
