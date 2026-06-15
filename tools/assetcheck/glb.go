package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

// glbInfo is what the gate needs from a .glb: declared extensions and
// animation clip names. Parsing is strict — any container or JSON
// malformation is an error, never a skipped check.
type glbInfo struct {
	ExtensionsUsed     []string
	ExtensionsRequired []string
	Clips              []string
	ExternalURIs       []string // non-data: buffer/image URIs — break self-containment
	Triangles          int      // summed over all mesh primitives (triangle topologies only)
}

// trianglesForMode returns the triangle count a primitive of the given glTF
// primitive mode contributes for `count` vertices/indices. Non-triangle
// topologies (POINTS/LINES/...) contribute zero — they are not budgeted geometry.
func trianglesForMode(mode, count int) int {
	switch mode {
	case 4: // TRIANGLES
		return count / 3
	case 5, 6: // TRIANGLE_STRIP, TRIANGLE_FAN
		if count >= 3 {
			return count - 2
		}
		return 0
	default: // 0 POINTS, 1 LINES, 2 LINE_LOOP, 3 LINE_STRIP
		return 0
	}
}

const (
	glbMagic     = 0x46546C67 // "glTF"
	glbChunkJSON = 0x4E4F534A // "JSON"
)

// parseGLB reads the GLB container header and the JSON chunk.
func parseGLB(path string) (*glbInfo, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if len(data) < 20 {
		return nil, fmt.Errorf("file too short for GLB header (%d bytes)", len(data))
	}
	if magic := binary.LittleEndian.Uint32(data[0:4]); magic != glbMagic {
		return nil, fmt.Errorf("bad magic 0x%08X, want 0x%08X (\"glTF\")", magic, uint32(glbMagic))
	}
	if version := binary.LittleEndian.Uint32(data[4:8]); version != 2 {
		return nil, fmt.Errorf("glTF version %d, core profile requires 2", version)
	}
	declared := binary.LittleEndian.Uint32(data[8:12])
	if int(declared) > len(data) {
		return nil, fmt.Errorf("header declares %d bytes but file has %d", declared, len(data))
	}
	chunkLen := binary.LittleEndian.Uint32(data[12:16])
	chunkType := binary.LittleEndian.Uint32(data[16:20])
	if chunkType != glbChunkJSON {
		return nil, fmt.Errorf("first chunk type 0x%08X, want JSON chunk first per spec", chunkType)
	}
	if 20+int(chunkLen) > len(data) {
		return nil, fmt.Errorf("JSON chunk declares %d bytes, only %d available", chunkLen, len(data)-20)
	}

	var doc struct {
		Asset struct {
			Version string `json:"version"`
		} `json:"asset"`
		ExtensionsUsed     []string `json:"extensionsUsed"`
		ExtensionsRequired []string `json:"extensionsRequired"`
		Animations         []struct {
			Name string `json:"name"`
		} `json:"animations"`
		Buffers []struct {
			URI string `json:"uri"`
		} `json:"buffers"`
		Images []struct {
			URI string `json:"uri"`
		} `json:"images"`
		Meshes []struct {
			Primitives []struct {
				Mode       *int           `json:"mode"`
				Indices    *int           `json:"indices"`
				Attributes map[string]int `json:"attributes"`
			} `json:"primitives"`
		} `json:"meshes"`
		Accessors []struct {
			Count int `json:"count"`
		} `json:"accessors"`
	}
	if err := json.Unmarshal(data[20:20+chunkLen], &doc); err != nil {
		return nil, fmt.Errorf("JSON chunk: %w", err)
	}
	if doc.Asset.Version != "2.0" {
		return nil, fmt.Errorf("asset.version %q, core profile requires \"2.0\"", doc.Asset.Version)
	}
	info := &glbInfo{
		ExtensionsUsed:     doc.ExtensionsUsed,
		ExtensionsRequired: doc.ExtensionsRequired,
	}
	for _, a := range doc.Animations {
		info.Clips = append(info.Clips, a.Name)
	}
	for _, b := range doc.Buffers {
		if b.URI != "" && !strings.HasPrefix(b.URI, "data:") {
			info.ExternalURIs = append(info.ExternalURIs, "buffer: "+b.URI)
		}
	}
	for _, im := range doc.Images {
		if im.URI != "" && !strings.HasPrefix(im.URI, "data:") {
			info.ExternalURIs = append(info.ExternalURIs, "image: "+im.URI)
		}
	}
	nAcc := len(doc.Accessors)
	for mi, m := range doc.Meshes {
		for pi, prim := range m.Primitives {
			mode := 4 // glTF default primitive mode is TRIANGLES
			if prim.Mode != nil {
				mode = *prim.Mode
			}
			var count int
			if prim.Indices != nil { // indexed: triangles come from the index accessor
				idx := *prim.Indices
				if idx < 0 || idx >= nAcc {
					return nil, fmt.Errorf("mesh %d primitive %d: indices accessor %d out of range [0,%d)", mi, pi, idx, nAcc)
				}
				count = doc.Accessors[idx].Count
			} else { // non-indexed: every POSITION is a vertex
				pos, ok := prim.Attributes["POSITION"]
				if !ok {
					continue // no POSITION and no indices — nothing to count
				}
				if pos < 0 || pos >= nAcc {
					return nil, fmt.Errorf("mesh %d primitive %d: POSITION accessor %d out of range [0,%d)", mi, pi, pos, nAcc)
				}
				count = doc.Accessors[pos].Count
			}
			info.Triangles += trianglesForMode(mode, count)
		}
	}
	return info, nil
}
