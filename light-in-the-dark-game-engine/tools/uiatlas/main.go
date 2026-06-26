// uiatlas generates the default HUD UI atlas declared in assets/MANIFEST.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
)

const outputPath = "assets/ui/litd-default-ui.atlas.png"

type atlasRegion struct {
	rect   image.Rectangle
	fill   color.RGBA
	stroke color.RGBA
}

func main() {
	if err := os.MkdirAll("assets/ui", 0o755); err != nil {
		fatal(err)
	}
	img := buildAtlas()
	f, err := os.Create(outputPath)
	if err != nil {
		fatal(err)
	}
	if err := png.Encode(f, img); err != nil {
		_ = f.Close()
		fatal(err)
	}
	if err := f.Close(); err != nil {
		fatal(err)
	}
	sum, err := fileSHA256(outputPath)
	if err != nil {
		fatal(err)
	}
	fmt.Printf("%s  %s\n", sum, outputPath)
}

func buildAtlas() *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, 128, 128))
	fill(img, img.Bounds(), color.RGBA{R: 18, G: 22, B: 28, A: 70})
	regions := []atlasRegion{
		{image.Rect(0, 0, 32, 32), color.RGBA{44, 75, 108, 75}, color.RGBA{145, 190, 235, 130}},
		{image.Rect(32, 0, 64, 32), color.RGBA{86, 55, 118, 75}, color.RGBA{190, 160, 235, 130}},
		{image.Rect(64, 0, 96, 32), color.RGBA{102, 73, 30, 75}, color.RGBA{230, 188, 92, 130}},
		{image.Rect(96, 0, 128, 32), color.RGBA{116, 41, 45, 75}, color.RGBA{232, 116, 120, 130}},
		{image.Rect(0, 32, 64, 64), color.RGBA{39, 82, 55, 75}, color.RGBA{123, 216, 145, 130}},
		{image.Rect(64, 32, 128, 64), color.RGBA{31, 68, 96, 75}, color.RGBA{112, 185, 230, 130}},
		{image.Rect(0, 64, 64, 96), color.RGBA{43, 96, 48, 85}, color.RGBA{144, 230, 100, 135}},
		{image.Rect(64, 64, 128, 96), color.RGBA{31, 52, 120, 85}, color.RGBA{116, 150, 245, 135}},
		{image.Rect(0, 96, 128, 128), color.RGBA{38, 43, 55, 75}, color.RGBA{180, 190, 210, 130}},
	}
	for _, reg := range regions {
		fill(img, reg.rect, reg.fill)
		border(img, reg.rect, reg.stroke)
	}
	for i := 0; i < 12; i++ {
		x := 8 + (i%6)*18
		y := 102 + (i/6)*12
		rect := image.Rect(x, y, x+8, y+8)
		fill(img, rect, color.RGBA{214, 184, 92, 135})
		border(img, rect, color.RGBA{255, 235, 150, 170})
	}
	return img
}

func fill(img *image.RGBA, rect image.Rectangle, c color.RGBA) {
	for y := rect.Min.Y; y < rect.Max.Y; y++ {
		for x := rect.Min.X; x < rect.Max.X; x++ {
			img.SetRGBA(x, y, c)
		}
	}
}

func border(img *image.RGBA, rect image.Rectangle, c color.RGBA) {
	for x := rect.Min.X; x < rect.Max.X; x++ {
		img.SetRGBA(x, rect.Min.Y, c)
		img.SetRGBA(x, rect.Max.Y-1, c)
	}
	for y := rect.Min.Y; y < rect.Max.Y; y++ {
		img.SetRGBA(rect.Min.X, y, c)
		img.SetRGBA(rect.Max.X-1, y, c)
	}
}

func fileSHA256(path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), nil
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "uiatlas:", err)
	os.Exit(1)
}
