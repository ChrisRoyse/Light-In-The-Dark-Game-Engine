// Package assetcatalog is the shared glTF/GLB asset catalog: the strict GLB
// container parser plus the core-profile rules (extension allowlist, compression
// rejection, self-containment, an absolute geometry ceiling). It is extracted
// from tools/assetcheck (package main, unimportable) so BOTH the CI validator
// AND the in-engine archive read path (litd/asset/worldarchive) enforce ONE rule
// set with one implementation — no drift (#411; mirrors the lualint extraction in
// commit 45a2184). Per-CATEGORY triangle budgets stay in assetcheck: they need
// each asset's MANIFEST category, which a .litdworld archive entry does not carry.
package assetcatalog

import (
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

// AllowedGLTFExtensions is the closed allowlist of glTF extensions the core
// profile permits (R-FMT-1). Anything else is a GLTF-EXT finding.
var AllowedGLTFExtensions = map[string]bool{"KHR_materials_unlit": true}

// CompressionExtensions maps a forbidden compression extension to its display
// name. G3N cannot decode these (R-FMT-3), so they are a GLTF-COMPRESS finding.
var CompressionExtensions = map[string]string{
	"KHR_draco_mesh_compression": "Draco",
	"EXT_meshopt_compression":    "Meshopt",
}

// MaxArchiveTriangles is the absolute geometry ceiling enforced on an embedded
// archive GLB at load (#411 defense-in-depth). It equals the largest per-category
// budget (building, 4,000 tris): an uncategorized archive model over this is a
// geometry bomb regardless of category. Per-category budgets (unit ≤ 1,500) are
// still enforced in CI/assetcheck against the MANIFEST, which carries categories.
const MaxArchiveTriangles = 4000

// GLBInfo is what the gate needs from a .glb: declared extensions and
// animation clip names. Parsing is strict — any container or JSON
// malformation is an error, never a skipped check.
type GLBInfo struct {
	ExtensionsUsed     []string
	ExtensionsRequired []string
	Clips              []string
	ExternalURIs       []string   // non-data: buffer/image URIs — break self-containment
	Triangles          int        // summed over all mesh primitives (triangle topologies only)
	Images             []GLBImage // embedded textures (bufferView- or data:-resolved)
}

// GLBImage is one embedded texture: its declared name and resolved bytes. The
// atlas gate decodes dimensions and content-hashes these. Images referenced by
// an external (non-data) URI carry no bytes — they are reported as URI
// violations by the self-containment rule instead.
type GLBImage struct {
	Name string
	Data []byte
}

// decodeDataURI decodes a base64 data: URI payload (data:[mime][;base64],<data>).
func decodeDataURI(uri string) ([]byte, error) {
	comma := strings.IndexByte(uri, ',')
	if comma < 0 {
		return nil, fmt.Errorf("malformed data URI (no comma)")
	}
	meta, payload := uri[:comma], uri[comma+1:]
	if !strings.Contains(meta, ";base64") {
		return nil, fmt.Errorf("data URI is not base64-encoded")
	}
	b, err := base64.StdEncoding.DecodeString(payload)
	if err != nil {
		return nil, fmt.Errorf("base64: %w", err)
	}
	return b, nil
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
	glbChunkBIN  = 0x004E4942 // "BIN\0"
)

// ParseGLB reads a GLB file from disk and parses it.
func ParseGLB(path string) (*GLBInfo, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return ParseGLBBytes(data)
}

// ParseGLBBytes parses an in-memory GLB container (header + JSON chunk + BIN).
func ParseGLBBytes(data []byte) (*GLBInfo, error) {
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
			URI        string `json:"uri"`
			MimeType   string `json:"mimeType"`
			Name       string `json:"name"`
			BufferView *int   `json:"bufferView"`
		} `json:"images"`
		BufferViews []struct {
			Buffer     int `json:"buffer"`
			ByteOffset int `json:"byteOffset"`
			ByteLength int `json:"byteLength"`
		} `json:"bufferViews"`
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
	info := &GLBInfo{
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
	// Locate the optional BIN chunk (immediately after the JSON chunk) so we can
	// resolve bufferView-backed embedded images.
	var bin []byte
	if pos := 20 + int(chunkLen); pos+8 <= len(data) {
		binLen := binary.LittleEndian.Uint32(data[pos : pos+4])
		binType := binary.LittleEndian.Uint32(data[pos+4 : pos+8])
		if binType == glbChunkBIN && pos+8+int(binLen) <= len(data) {
			bin = data[pos+8 : pos+8+int(binLen)]
		}
	}
	for _, im := range doc.Images {
		switch {
		case im.URI != "" && !strings.HasPrefix(im.URI, "data:"):
			info.ExternalURIs = append(info.ExternalURIs, "image: "+im.URI)
		case im.URI != "": // data: URI
			payload, derr := decodeDataURI(im.URI)
			if derr != nil {
				return nil, fmt.Errorf("image %q: %w", im.Name, derr)
			}
			info.Images = append(info.Images, GLBImage{Name: im.Name, Data: payload})
		case im.BufferView != nil: // embedded in the BIN chunk
			bv := *im.BufferView
			if bv < 0 || bv >= len(doc.BufferViews) {
				return nil, fmt.Errorf("image %q: bufferView %d out of range [0,%d)", im.Name, bv, len(doc.BufferViews))
			}
			view := doc.BufferViews[bv]
			if view.Buffer != 0 {
				return nil, fmt.Errorf("image %q: bufferView references buffer %d, only the GLB BIN buffer (0) is supported", im.Name, view.Buffer)
			}
			end := view.ByteOffset + view.ByteLength
			if view.ByteOffset < 0 || end > len(bin) {
				return nil, fmt.Errorf("image %q: bufferView [%d,%d) exceeds BIN chunk of %d bytes", im.Name, view.ByteOffset, end, len(bin))
			}
			info.Images = append(info.Images, GLBImage{Name: im.Name, Data: bin[view.ByteOffset:end]})
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

// CheckGLB runs the category-independent glTF catalog on an in-memory GLB and
// returns one finding string per violation ("CODE: message"), empty when clean.
// This is the load-time defense-in-depth gate (#411): a hash-valid hand-crafted
// archive carrying a malformed, Draco-compressed, non-allowlisted-extension,
// externally-referenced, or geometry-bomb GLB is refused. Per-category triangle
// budgets are NOT checked here (no category at load) — the absolute ceiling is.
func CheckGLB(data []byte) []string {
	info, err := ParseGLBBytes(data)
	if err != nil {
		return []string{"GLTF-CORE: " + err.Error()}
	}
	var hits []string
	for _, u := range info.ExternalURIs {
		hits = append(hits, fmt.Sprintf("GLTF-URI: external resource reference (%s) — archives must be self-contained", u))
	}
	for _, e := range append(append([]string{}, info.ExtensionsUsed...), info.ExtensionsRequired...) {
		if cn, bad := CompressionExtensions[e]; bad {
			hits = append(hits, fmt.Sprintf("GLTF-COMPRESS: %s compression (%s) — G3N cannot decode (R-FMT-3)", cn, e))
		} else if !AllowedGLTFExtensions[e] {
			hits = append(hits, fmt.Sprintf("GLTF-EXT: extension %q not permitted; core profile allows only KHR_materials_unlit (R-FMT-1)", e))
		}
	}
	if info.Triangles > MaxArchiveTriangles {
		hits = append(hits, fmt.Sprintf("GEO-MAX: %d triangles exceeds the absolute archive ceiling of %d (R-RND-2; per-category budgets enforced in CI)", info.Triangles, MaxArchiveTriangles))
	}
	return hits
}
