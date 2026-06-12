package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
)

// glbInfo is what the gate needs from a .glb: declared extensions and
// animation clip names. Parsing is strict — any container or JSON
// malformation is an error, never a skipped check.
type glbInfo struct {
	ExtensionsUsed     []string
	ExtensionsRequired []string
	Clips              []string
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
	return info, nil
}
