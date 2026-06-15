package main

// Texture/atlas usage gate (#32; tooling.md §3.2 "Texture/atlas" row, R-RND-2).
// Two rules over embedded GLB textures:
//
//   - ATLAS-SIZE: no texture may exceed 1024×1024 (boundary inclusive) on
//     either edge — the one-shared-atlas budget.
//   - ATLAS-UNIQUE: the one-shared-atlas pattern wants models to reference a
//     shared faction/biome atlas, not bake their own. When shared atlases
//     exist in the asset tree (identical texture bytes referenced by ≥2
//     models), any model that instead embeds a *non-shared* texture is
//     flagged. With no shared atlas anywhere, nothing is flagged — sharing
//     cannot be expected of a lone model.
//
// Texture dimensions are decoded from the raw image bytes (PNG IHDR / JPEG
// SOF), never trusted from metadata.

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"sort"
)

const maxAtlasEdge = 1024

// imageDims decodes the pixel dimensions of a PNG or JPEG from its leading
// bytes. It returns an error for anything it cannot decode — a malformed or
// unsupported embedded texture fails the gate rather than being skipped.
func imageDims(b []byte) (w, h int, err error) {
	switch {
	case len(b) >= 24 && b[0] == 0x89 && b[1] == 0x50 && b[2] == 0x4E && b[3] == 0x47:
		// PNG: 8-byte signature, then IHDR chunk (len,4 + "IHDR",4 + w,4 + h,4).
		if string(b[12:16]) != "IHDR" {
			return 0, 0, fmt.Errorf("PNG: first chunk is not IHDR")
		}
		w = int(binary.BigEndian.Uint32(b[16:20]))
		h = int(binary.BigEndian.Uint32(b[20:24]))
		return w, h, nil
	case len(b) >= 2 && b[0] == 0xFF && b[1] == 0xD8:
		return jpegDims(b)
	default:
		return 0, 0, fmt.Errorf("unrecognized image format (not PNG or JPEG)")
	}
}

// jpegDims walks JPEG markers to the first start-of-frame (SOF0..SOF3,
// SOF5..SOF7, SOF9..SOF11, SOF13..SOF15) and reads its frame dimensions.
func jpegDims(b []byte) (int, int, error) {
	i := 2
	for i+1 < len(b) {
		if b[i] != 0xFF {
			i++
			continue
		}
		marker := b[i+1]
		i += 2
		switch {
		case marker == 0xD8 || marker == 0xD9 || (marker >= 0xD0 && marker <= 0xD7):
			continue // standalone markers, no length
		case isJPEGSOF(marker):
			if i+7 > len(b) {
				return 0, 0, fmt.Errorf("JPEG: truncated SOF segment")
			}
			h := int(binary.BigEndian.Uint16(b[i+3 : i+5]))
			w := int(binary.BigEndian.Uint16(b[i+5 : i+7]))
			return w, h, nil
		default:
			if i+2 > len(b) {
				return 0, 0, fmt.Errorf("JPEG: truncated segment length")
			}
			seg := int(binary.BigEndian.Uint16(b[i : i+2]))
			i += seg
		}
	}
	return 0, 0, fmt.Errorf("JPEG: no start-of-frame marker found")
}

func isJPEGSOF(m byte) bool {
	switch m {
	case 0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF:
		return true
	}
	return false
}

// textureInfo is one decoded embedded texture (for findings + census).
type textureInfo struct {
	Name   string
	Width  int
	Height int
	Hash   string // sha256 of the image bytes; identical bytes ⇒ shared atlas
	Err    error  // non-nil if dimensions could not be decoded
}

// decodeTextures decodes every embedded image of a parsed GLB.
func decodeTextures(info *glbInfo) []textureInfo {
	var out []textureInfo
	for _, im := range info.Images {
		sum := sha256.Sum256(im.Data)
		ti := textureInfo{Name: im.Name, Hash: hex.EncodeToString(sum[:])}
		ti.Width, ti.Height, ti.Err = imageDims(im.Data)
		out = append(out, ti)
	}
	return out
}

// checkAtlas applies the size and uniqueness rules across all models. textures
// maps file path → its decoded embedded textures. It returns findings plus
// informational per-model texture-count notes for the run summary. Both are
// deterministic.
func checkAtlas(textures map[string][]textureInfo) (findings []finding, notes []string) {
	add := func(path, rule, msg string) { findings = append(findings, finding{path, rule, msg}) }

	// Count, per content hash, how many distinct models reference it. A hash
	// shared by ≥2 models is a shared atlas.
	models := map[string]map[string]bool{}
	for rel, tis := range textures {
		for _, ti := range tis {
			if ti.Err != nil {
				continue
			}
			if models[ti.Hash] == nil {
				models[ti.Hash] = map[string]bool{}
			}
			models[ti.Hash][rel] = true
		}
	}
	sharedExists := false
	for _, m := range models {
		if len(m) >= 2 {
			sharedExists = true
			break
		}
	}

	paths := make([]string, 0, len(textures))
	for p := range textures {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	for _, rel := range paths {
		if n := len(textures[rel]); n > 0 {
			dims := make([]string, 0, n)
			for _, ti := range textures[rel] {
				if ti.Err != nil {
					dims = append(dims, "?x?")
				} else {
					dims = append(dims, fmt.Sprintf("%dx%d", ti.Width, ti.Height))
				}
			}
			notes = append(notes, fmt.Sprintf("textures %s: %d (%v)", rel, n, dims))
		}
		for i, ti := range textures[rel] {
			label := ti.Name
			if label == "" {
				label = fmt.Sprintf("image[%d]", i)
			}
			if ti.Err != nil {
				add(rel, "ATLAS-DECODE", fmt.Sprintf("texture %q: %v", label, ti.Err))
				continue
			}
			if ti.Width > maxAtlasEdge || ti.Height > maxAtlasEdge {
				add(rel, "ATLAS-SIZE", fmt.Sprintf("texture %q is %dx%d, exceeds the %dx%d atlas budget (R-RND-2)", label, ti.Width, ti.Height, maxAtlasEdge, maxAtlasEdge))
				continue
			}
			if sharedExists && len(models[ti.Hash]) < 2 {
				add(rel, "ATLAS-UNIQUE", fmt.Sprintf("texture %q (%dx%d) is unique to this model; use a shared faction/biome atlas", label, ti.Width, ti.Height))
			}
		}
	}
	return findings, notes
}
