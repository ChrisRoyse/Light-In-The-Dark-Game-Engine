package main

// The GLB container parser and the core-profile catalog now live in the shared,
// importable litd/asset/assetcatalog package (#411) so the in-engine archive
// read path enforces the same rules with one implementation (no drift). These
// aliases keep the assetcheck call sites unchanged.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/assetcatalog"
)

type glbInfo = assetcatalog.GLBInfo
type glbImage = assetcatalog.GLBImage

var (
	parseGLB      = assetcatalog.ParseGLB
	parseGLBBytes = assetcatalog.ParseGLBBytes
)

// glTF container magic (immutable spec constants) — retained here for the GLB
// builders in the assetcheck test helpers; the parser proper lives in assetcatalog.
const (
	glbMagic     = 0x46546C67 // "glTF"
	glbChunkJSON = 0x4E4F534A // "JSON"
	glbChunkBIN  = 0x004E4942 // "BIN\0"
)
