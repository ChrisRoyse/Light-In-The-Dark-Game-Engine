// Command renderevidence is the cheap render-evidence path (#516 leg 2): it
// turns a full-resolution deterministic render (e.g. the PNG written by
// `firstlight -autotest -shot`) into three small, fast-to-read artifacts so an
// agent can verify a render WITHOUT vision-reading a 1280x720 PNG every cycle:
//
//   1. Checksum   — sha256 of the raw RGBA pixels. O(1) exact compare: identical
//                   render => identical checksum; one changed pixel => different.
//   2. Grid       — an NxN mean-color histogram. Localizes WHERE the image
//                   changed (top hash says "different", the grid says "cell 3,5
//                   went dark"), the render analogue of per-system sub-hashes.
//   3. Thumbnail  — a tiny downscaled PNG the agent vision-reads cheaply, only
//                   escalating to the full-res PNG on a checksum/grid mismatch.
//
// It NEVER decides pass/fail — the agent reads the verdict (doctrine §4). It only
// makes the evidence cheap. Pure Go (image/png), no GL: it consumes a rendered
// PNG, it does not render.
package main

import (
	"crypto/sha256"
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/draw"
	"image/png"
	"os"
)

// Region is one grid cell's mean color and the fraction of its pixels that are
// non-black (any channel > blackCut) — the "is anything drawn here" signal.
type Region struct {
	Row        int     `json:"row"`
	Col        int     `json:"col"`
	R, G, B, A uint8   `json:"-"`
	Hex        string  `json:"hex"`
	NonBlack   float64 `json:"nonBlack"`
}

// Evidence is the structured SoT an agent reads instead of the full PNG.
type Evidence struct {
	In            string   `json:"in"`
	Width         int      `json:"width"`
	Height        int      `json:"height"`
	Checksum      string   `json:"checksum"`     // sha256 of raw RGBA pixels
	NonBlackFrac  float64  `json:"nonBlackFrac"` // whole-image: 0 => black frame
	Grid          int      `json:"grid"`
	Regions       []Region `json:"regions"`
	Thumb         string   `json:"thumb,omitempty"`
	ThumbW        int      `json:"thumbW,omitempty"`
	ThumbH        int      `json:"thumbH,omitempty"`
}

const blackCut = 16 // a channel above this counts as "lit"

func main() {
	in := flag.String("in", "", "input PNG (a deterministic render, e.g. firstlight -autotest -shot)")
	thumb := flag.String("thumb", "", "optional thumbnail PNG output path")
	grid := flag.Int("grid", 8, "grid dimension N for the NxN mean-color histogram")
	thumbW := flag.Int("thumbw", 128, "thumbnail width in pixels (height keeps aspect)")
	out := flag.String("out", "", "optional evidence JSON output path (default stdout)")
	flag.Parse()

	if *in == "" {
		fmt.Fprintln(os.Stderr, "renderevidence: -in is required")
		os.Exit(2)
	}
	if *grid < 1 {
		fmt.Fprintln(os.Stderr, "renderevidence: -grid must be >= 1")
		os.Exit(2)
	}

	rgba, err := loadRGBA(*in)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderevidence: %v\n", err)
		os.Exit(1)
	}

	ev := analyze(rgba, *grid)
	ev.In = *in

	if *thumb != "" {
		tw, th, err := writeThumb(rgba, *thumbW, *thumb)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderevidence: thumbnail: %v\n", err)
			os.Exit(1)
		}
		ev.Thumb, ev.ThumbW, ev.ThumbH = *thumb, tw, th
	}

	b, _ := json.MarshalIndent(ev, "", "  ")
	if *out != "" {
		if err := os.WriteFile(*out, b, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "renderevidence: write %s: %v\n", *out, err)
			os.Exit(1)
		}
	}
	fmt.Println(string(b))
}

// loadRGBA decodes a PNG into a tightly-packed *image.RGBA (so Pix is a stable,
// stride-free byte order for checksumming).
func loadRGBA(path string) (*image.RGBA, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	img, err := png.Decode(f)
	if err != nil {
		return nil, fmt.Errorf("decode %s: %w", path, err)
	}
	b := img.Bounds()
	rgba := image.NewRGBA(image.Rect(0, 0, b.Dx(), b.Dy()))
	draw.Draw(rgba, rgba.Bounds(), img, b.Min, draw.Src)
	return rgba, nil
}

// analyze computes the checksum, whole-image non-black fraction, and the NxN
// mean-color grid. Deterministic for a given image.
func analyze(rgba *image.RGBA, grid int) Evidence {
	w, h := rgba.Rect.Dx(), rgba.Rect.Dy()
	sum := sha256.Sum256(rgba.Pix)

	ev := Evidence{
		Width:    w,
		Height:   h,
		Checksum: fmt.Sprintf("%x", sum[:]),
		Grid:     grid,
		Regions:  make([]Region, 0, grid*grid),
	}

	var litTotal uint64
	for gr := 0; gr < grid; gr++ {
		y0, y1 := gr*h/grid, (gr+1)*h/grid
		for gc := 0; gc < grid; gc++ {
			x0, x1 := gc*w/grid, (gc+1)*w/grid
			var sr, sg, sb, sa, lit, n uint64
			for y := y0; y < y1; y++ {
				row := rgba.Pix[y*rgba.Stride:]
				for x := x0; x < x1; x++ {
					p := row[x*4:]
					r, g, b, a := p[0], p[1], p[2], p[3]
					sr += uint64(r)
					sg += uint64(g)
					sb += uint64(b)
					sa += uint64(a)
					if r > blackCut || g > blackCut || b > blackCut {
						lit++
						litTotal++
					}
					n++
				}
			}
			if n == 0 {
				n = 1
			}
			mr, mg, mb, ma := uint8(sr/n), uint8(sg/n), uint8(sb/n), uint8(sa/n)
			ev.Regions = append(ev.Regions, Region{
				Row: gr, Col: gc, R: mr, G: mg, B: mb, A: ma,
				Hex:      fmt.Sprintf("#%02x%02x%02x", mr, mg, mb),
				NonBlack: float64(lit) / float64(n),
			})
		}
	}
	if w > 0 && h > 0 {
		ev.NonBlackFrac = float64(litTotal) / float64(uint64(w)*uint64(h))
	}
	return ev
}

// writeThumb box-downscales rgba to thumbW (aspect-preserved height) and writes
// it as a PNG. Box averaging is deterministic and dependency-free.
func writeThumb(rgba *image.RGBA, thumbW int, path string) (int, int, error) {
	w, h := rgba.Rect.Dx(), rgba.Rect.Dy()
	if thumbW < 1 {
		thumbW = 1
	}
	if thumbW > w {
		thumbW = w
	}
	thumbH := h * thumbW / w
	if thumbH < 1 {
		thumbH = 1
	}
	dst := image.NewRGBA(image.Rect(0, 0, thumbW, thumbH))
	for ty := 0; ty < thumbH; ty++ {
		y0, y1 := ty*h/thumbH, (ty+1)*h/thumbH
		if y1 <= y0 {
			y1 = y0 + 1
		}
		for tx := 0; tx < thumbW; tx++ {
			x0, x1 := tx*w/thumbW, (tx+1)*w/thumbW
			if x1 <= x0 {
				x1 = x0 + 1
			}
			var sr, sg, sb, sa, n uint64
			for y := y0; y < y1; y++ {
				row := rgba.Pix[y*rgba.Stride:]
				for x := x0; x < x1; x++ {
					p := row[x*4:]
					sr += uint64(p[0])
					sg += uint64(p[1])
					sb += uint64(p[2])
					sa += uint64(p[3])
					n++
				}
			}
			if n == 0 {
				n = 1
			}
			o := dst.Pix[ty*dst.Stride+tx*4:]
			o[0], o[1], o[2], o[3] = uint8(sr/n), uint8(sg/n), uint8(sb/n), uint8(sa/n)
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return 0, 0, err
	}
	defer f.Close()
	if err := png.Encode(f, dst); err != nil {
		return 0, 0, err
	}
	return thumbW, thumbH, nil
}
