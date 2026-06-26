package asset

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"image"
	"image/draw"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"os"
	"strings"
)

const (
	AuthoredAtlasSize = 1024
)

type AtlasPreset string

const (
	AtlasPresetHigh   AtlasPreset = "high"
	AtlasPresetMedium AtlasPreset = "medium"
	AtlasPresetLow    AtlasPreset = "low"
)

type AtlasSource struct {
	Name   string
	Image  *image.RGBA
	Width  int
	Height int
	SHA256 string
}

type AtlasUpload struct {
	Name         string      `json:"name"`
	Preset       AtlasPreset `json:"preset"`
	SourceWidth  int         `json:"sourceWidth"`
	SourceHeight int         `json:"sourceHeight"`
	Width        int         `json:"width"`
	Height       int         `json:"height"`
	SHA256       string      `json:"sha256"`
}

func ParseAtlasPreset(text string) (AtlasPreset, error) {
	switch AtlasPreset(strings.ToLower(strings.TrimSpace(text))) {
	case AtlasPresetHigh:
		return AtlasPresetHigh, nil
	case AtlasPresetMedium:
		return AtlasPresetMedium, nil
	case AtlasPresetLow:
		return AtlasPresetLow, nil
	default:
		return "", fmt.Errorf("unknown atlas preset %q", text)
	}
}

func (p AtlasPreset) Size() (int, error) {
	switch p {
	case AtlasPresetHigh:
		return AuthoredAtlasSize, nil
	case AtlasPresetMedium:
		return AuthoredAtlasSize / 2, nil
	case AtlasPresetLow:
		return AuthoredAtlasSize / 4, nil
	default:
		return 0, fmt.Errorf("unknown atlas preset %q", p)
	}
}

func LoadAtlas(path string) (*AtlasSource, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	return DecodeAtlas(f, path)
}

func DecodeAtlas(r io.Reader, name string) (*AtlasSource, error) {
	img, _, err := image.Decode(r)
	if err != nil {
		return nil, fmt.Errorf("%s: decode atlas: %w", name, err)
	}
	return NewAtlasSource(name, img)
}

func NewAtlasSource(name string, img image.Image) (*AtlasSource, error) {
	if img == nil {
		return nil, fmt.Errorf("%s: atlas image is nil", atlasName(name))
	}
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	if w != AuthoredAtlasSize || h != AuthoredAtlasSize {
		return nil, fmt.Errorf("%s: atlas is %dx%d, want authored %dx%d", atlasName(name), w, h, AuthoredAtlasSize, AuthoredAtlasSize)
	}
	if !powerOfTwo(w) || !powerOfTwo(h) {
		return nil, fmt.Errorf("%s: atlas dimensions must be power-of-two, got %dx%d", atlasName(name), w, h)
	}
	rgba := image.NewRGBA(image.Rect(0, 0, w, h))
	draw.Draw(rgba, rgba.Bounds(), img, b.Min, draw.Src)
	sum := sha256.Sum256(rgba.Pix)
	return &AtlasSource{
		Name:   atlasName(name),
		Image:  rgba,
		Width:  w,
		Height: h,
		SHA256: hex.EncodeToString(sum[:]),
	}, nil
}

func BuildAtlasUpload(src *AtlasSource, preset AtlasPreset) (*image.RGBA, AtlasUpload, error) {
	if src == nil || src.Image == nil {
		return nil, AtlasUpload{}, fmt.Errorf("atlas source is nil")
	}
	size, err := preset.Size()
	if err != nil {
		return nil, AtlasUpload{}, err
	}
	img, err := downsampleBox(src.Image, size)
	if err != nil {
		return nil, AtlasUpload{}, fmt.Errorf("%s: %w", src.Name, err)
	}
	sum := sha256.Sum256(img.Pix)
	upload := AtlasUpload{
		Name:         src.Name,
		Preset:       preset,
		SourceWidth:  src.Width,
		SourceHeight: src.Height,
		Width:        img.Bounds().Dx(),
		Height:       img.Bounds().Dy(),
		SHA256:       hex.EncodeToString(sum[:]),
	}
	return img, upload, nil
}

func downsampleBox(src *image.RGBA, size int) (*image.RGBA, error) {
	b := src.Bounds()
	w, h := b.Dx(), b.Dy()
	if size <= 0 {
		return nil, fmt.Errorf("target atlas size must be positive")
	}
	if w != h || w%size != 0 || h%size != 0 {
		return nil, fmt.Errorf("cannot downsample %dx%d to %dx%d by integral box filter", w, h, size, size)
	}
	if w == size && h == size {
		cp := image.NewRGBA(image.Rect(0, 0, size, size))
		copy(cp.Pix, src.Pix)
		return cp, nil
	}
	factor := w / size
	dst := image.NewRGBA(image.Rect(0, 0, size, size))
	area := uint32(factor * factor)
	for y := 0; y < size; y++ {
		for x := 0; x < size; x++ {
			var r, g, b, a uint32
			for by := 0; by < factor; by++ {
				si := src.PixOffset(x*factor, y*factor+by)
				for bx := 0; bx < factor; bx++ {
					r += uint32(src.Pix[si+0])
					g += uint32(src.Pix[si+1])
					b += uint32(src.Pix[si+2])
					a += uint32(src.Pix[si+3])
					si += 4
				}
			}
			di := dst.PixOffset(x, y)
			dst.Pix[di+0] = uint8(r / area)
			dst.Pix[di+1] = uint8(g / area)
			dst.Pix[di+2] = uint8(b / area)
			dst.Pix[di+3] = uint8(a / area)
		}
	}
	return dst, nil
}

func powerOfTwo(v int) bool {
	return v > 0 && v&(v-1) == 0
}

func atlasName(name string) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return "<atlas>"
	}
	return name
}
