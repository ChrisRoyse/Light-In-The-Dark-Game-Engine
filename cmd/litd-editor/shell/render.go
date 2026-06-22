package shell

import (
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"os"
	"path/filepath"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
	"golang.org/x/image/font"
	"golang.org/x/image/font/basicfont"
	"golang.org/x/image/math/fixed"
)

const (
	ShotWidth        = 1280
	ShotHeight       = 720
	terrainGridX     = 240
	terrainGridY     = 210
	terrainGridStepX = 44
	terrainGridStepY = 34
	terrainGridCellW = 34
	terrainGridCellH = 24
)

var (
	ink        = color.RGBA{R: 230, G: 234, B: 226, A: 255}
	muted      = color.RGBA{R: 151, G: 162, B: 160, A: 255}
	graphite   = color.RGBA{R: 24, G: 28, B: 30, A: 255}
	rail       = color.RGBA{R: 34, G: 39, B: 41, A: 255}
	panel      = color.RGBA{R: 43, G: 49, B: 51, A: 255}
	panelAlt   = color.RGBA{R: 53, G: 61, B: 59, A: 255}
	brass      = color.RGBA{R: 200, G: 156, B: 83, A: 255}
	green      = color.RGBA{R: 103, G: 153, B: 105, A: 255}
	errorColor = color.RGBA{R: 189, G: 82, B: 76, A: 255}
)

func RenderPNG(path string, snap Snapshot) error {
	img := RenderImage(snap)
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

func RenderImage(snap Snapshot) *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, ShotWidth, ShotHeight))
	draw.Draw(img, img.Bounds(), &image.Uniform{graphite}, image.Point{}, draw.Src)
	fill(img, 0, 0, ShotWidth, 62, rail)
	fill(img, 0, 62, 190, ShotHeight-62, color.RGBA{R: 29, G: 33, B: 34, A: 255})
	fill(img, 210, 92, 1040, 550, panel)
	fill(img, 210, 662, 1040, 34, rail)
	line(img, 0, 62, ShotWidth, 62, brass)
	line(img, 190, 62, 190, ShotHeight, brass)

	textFit(img, 24, 38, 920, snap.Title, ink)
	text(img, 1060, 38, snap.DirtyLabel, dirtyColor(snap.Dirty))
	text(img, 24, 100, strings.ToUpper(snap.ModeLabel), muted)
	modeButton(img, 22, 124, snap.Labels["terrain"], snap.Mode == ModeTerrain)
	modeButton(img, 22, 176, snap.Labels["objects"], snap.Mode == ModeObjects)
	modeButton(img, 22, 228, snap.Labels["metadata"], snap.Mode == ModeMetadata)
	text(img, 24, 330, snap.Labels["new"]+"   "+snap.Labels["open"], muted)
	text(img, 24, 358, snap.Labels["save"]+"   "+snap.Labels["export"], muted)

	switch snap.Mode {
	case ModeTerrain:
		drawTerrain(img, snap)
	case ModeObjects:
		drawObjects(img, snap)
	case ModeMetadata:
		drawMetadata(img, snap)
	}
	if snap.Error != "" {
		fill(img, 210, 612, 1040, 42, color.RGBA{R: 83, G: 40, B: 38, A: 255})
		textFit(img, 226, 638, 1000, snap.Error, ink)
	}
	if snap.Confirm != nil {
		fill(img, 390, 220, 520, 170, color.RGBA{R: 57, G: 48, B: 39, A: 255})
		line(img, 390, 220, 910, 220, brass)
		text(img, 420, 262, snap.Confirm.Title, ink)
		text(img, 420, 294, snap.Confirm.Body, muted)
		text(img, 420, 344, snap.Labels["proceed"]+" / "+snap.Labels["cancel"], brass)
	}
	textFit(img, 226, 686, 1000, snap.Labels["statusPrefix"]+": "+snap.Status, muted)
	return img
}

func drawTerrain(img *image.RGBA, snap Snapshot) {
	text(img, 236, 132, snap.Labels["panelTerrain"], ink)
	text(img, 236, 166, fmt.Sprintf("%dx%d", snap.World.Width, snap.World.Height), muted)
	rows := snap.World.HeightRows
	if len(rows) == 0 {
		rows = make([][]int, min(8, snap.World.Height))
		for y := range rows {
			rows[y] = make([]int, min(8, snap.World.Width))
		}
	}
	flagCells := cliffFlagCells(snap.CliffFlags)
	for y := 0; y < len(rows) && y < 8; y++ {
		for x := 0; x < len(rows[y]) && x < 8; x++ {
			h := rows[y][x]
			c := heightColor(h)
			if y < len(snap.World.SplatRows) && x < len(snap.World.SplatRows[y]) {
				if sc, ok := splatColor(snap.World.SplatRows[y][x]); ok {
					c = sc
				}
			}
			fill(img, terrainGridX+x*terrainGridStepX, terrainGridY+y*terrainGridStepY, terrainGridCellW, terrainGridCellH, c)
			if y < len(snap.World.CliffRows) && x < len(snap.World.CliffRows[y]) && strings.HasPrefix(snap.World.CliffRows[y][x], "r") {
				fill(img, terrainGridX+x*terrainGridStepX+terrainGridCellW-7, terrainGridY+y*terrainGridStepY, 7, terrainGridCellH, brass)
			}
			if kind, ok := flagCells[[2]int{x, y}]; ok {
				drawCliffFlagMarker(img, terrainGridX+x*terrainGridStepX, terrainGridY+y*terrainGridStepY, kind)
			}
		}
	}
	text(img, 640, 226, snap.Labels["fieldCell"]+"[1,1]="+fmt.Sprint(snap.World.HeightCell), brass)
	text(img, 640, 258, snap.Labels["fieldCliff"]+"[1,1]="+snap.World.CliffCell, muted)
	text(img, 640, 290, snap.Labels["fieldSplat"]+"[1,1]="+snap.World.SplatCell, muted)
	text(img, 640, 338, snap.Labels["fieldTool"]+": "+string(snap.TerrainTool), brass)
	text(img, 640, 370, snap.Labels["fieldBrush"]+": "+snap.Brush.Label, ink)
	text(img, 640, 402, snap.Labels["fieldPaint"]+": "+snap.Paint.Label, ink)
	textFit(img, 640, 434, 520, snap.Labels["fieldFlags"]+": "+cliffFlagsLabel(snap.CliffFlags), flagTextColor(snap.CliffFlags))
	for i, layer := range snap.Paint.Palette {
		x := 640 + i*72
		y := 474
		fill(img, x, y, 42, 24, paintLayerColor(layer.Layer))
		if layer.Active {
			fill(img, x, y+26, 42, 3, brass)
		}
		text(img, x+50, y+18, string(rune('A'+layer.Layer)), muted)
	}
	text(img, 640, 532, snap.Labels["hintTerrain"], muted)
}

func drawObjects(img *image.RGBA, snap Snapshot) {
	text(img, 236, 132, snap.Labels["panelObjects"], ink)
	text(img, 236, 166, snap.Labels["hintObjects"], muted)
	fill(img, 238, 208, 890, 42, panelAlt)
	text(img, 258, 235, fmt.Sprintf("%s: %d   %s: %d", snap.Labels["fieldEntities"], snap.World.Entities, snap.Labels["fieldDoodads"], snap.World.Doodads), ink)
	fill(img, 238, 264, 890, 42, panelAlt)
	selection := snap.Objects.Selection
	textFit(img, 258, 291, 820, fmt.Sprintf("%s: %s %s owner=%d rot=%d scale=%d", snap.Labels["fieldSelection"], selection.Kind, shortObjectType(selection.Type), selection.Owner, selection.Rotation, selection.Scale), brass)
	fill(img, 238, 320, 890, 42, panelAlt)
	textFit(img, 258, 347, 820, fmt.Sprintf("%s: %v", snap.Labels["fieldOverride"], selection.OverrideWalkability), muted)
	fill(img, 238, 376, 890, 42, panelAlt)
	textFit(img, 258, 403, 820, snap.Labels["fieldPalette"]+": "+paletteLabel(snap.Objects.Palette), ink)
	y := 458
	for i, ent := range snap.Objects.Units {
		if i >= 4 {
			break
		}
		fill(img, 238, y+i*36, 430, 28, panelAlt)
		textFit(img, 258, y+20+i*36, 390, fmt.Sprintf("U#%d p%d %s @%s r%d s%d", ent.ID, ent.Player, shortObjectType(ent.Type), objectCellLabel(ent.Pos), ent.Rotation, ent.Scale), ink)
	}
	for i, d := range snap.Objects.Doodads {
		if i >= 5 {
			break
		}
		fill(img, 690, y+i*36, 430, 28, panelAlt)
		textFit(img, 710, y+20+i*36, 390, fmt.Sprintf("D#%d %s @%s r%d s%d", d.ID, shortObjectType(d.Type), objectCellLabel(d.Pos), d.Rotation, d.Scale), ink)
	}
}

func drawMetadata(img *image.RGBA, snap Snapshot) {
	text(img, 236, 132, snap.Labels["panelMetadata"], ink)
	text(img, 236, 166, snap.Labels["hintMetadata"], muted)
	drawMetadataStartGrid(img, snap)
	displayPath := snap.ProjectPath
	if snap.ArchivePath != "" {
		displayPath = snap.ArchivePath
		if snap.ArchiveReadOnly {
			displayPath += " (read-only)"
		}
	}
	rows := []string{
		snap.Labels["fieldID"] + ": " + snap.World.ID,
		snap.Labels["fieldName"] + ": " + snap.World.Name,
		snap.Labels["fieldDescription"] + ": " + snap.World.Description,
		snap.Labels["fieldEngine"] + ": " + snap.World.EngineRange,
		fmt.Sprintf("%s: min %d / suggested %d / max %d", snap.Labels["fieldPlayers"], snap.World.Players.Min, snap.World.Players.Suggested, snap.World.Players.Max),
		snap.Labels["fieldTileset"] + ": " + snap.World.Tileset,
		snap.Labels["fieldSplatSet"] + ": " + snap.World.SplatSet,
		snap.Labels["fieldStarts"] + ": " + startsLabel(snap.World.Starts),
		snap.Labels["fieldSeedPolicy"] + ": " + snap.World.SeedPolicy,
		snap.Labels["fieldPath"] + ": " + displayPath,
	}
	for i, row := range rows {
		fill(img, 640, 208+i*36, 470, 28, panelAlt)
		textFit(img, 658, 228+i*36, 430, row, ink)
	}
}

func drawMetadataStartGrid(img *image.RGBA, snap Snapshot) {
	starts := map[[2]int]int{}
	for _, start := range snap.World.Starts {
		starts[start.Cell] = start.Player
	}
	rows := snap.World.HeightRows
	if len(rows) == 0 {
		rows = make([][]int, min(8, snap.World.Height))
		for y := range rows {
			rows[y] = make([]int, min(8, snap.World.Width))
		}
	}
	for y := 0; y < len(rows) && y < 8; y++ {
		for x := 0; x < len(rows[y]) && x < 8; x++ {
			c := heightColor(rows[y][x])
			if y < len(snap.World.SplatRows) && x < len(snap.World.SplatRows[y]) {
				if sc, ok := splatColor(snap.World.SplatRows[y][x]); ok {
					c = sc
				}
			}
			gx := terrainGridX + x*terrainGridStepX
			gy := terrainGridY + y*terrainGridStepY
			fill(img, gx, gy, terrainGridCellW, terrainGridCellH, c)
			if player, ok := starts[[2]int{x, y}]; ok {
				drawStartMarker(img, gx, gy, player)
			}
		}
	}
}

func drawStartMarker(img *image.RGBA, x, y, player int) {
	fill(img, x+3, y+3, terrainGridCellW-6, terrainGridCellH-6, brass)
	text(img, x+8, y+19, fmt.Sprintf("P%d", player), graphite)
}

func startsLabel(starts []sourceform.StartLocation) string {
	if len(starts) == 0 {
		return "none"
	}
	parts := make([]string, len(starts))
	for i, start := range starts {
		parts[i] = fmt.Sprintf("P%d@%d,%d", start.Player, start.Cell[0], start.Cell[1])
	}
	return strings.Join(parts, " ")
}

func modeButton(img *image.RGBA, x, y int, label string, active bool) {
	c := panel
	fg := muted
	if active {
		c = color.RGBA{R: 65, G: 77, B: 65, A: 255}
		fg = ink
	}
	fill(img, x, y, 144, 34, c)
	if active {
		fill(img, x, y, 5, 34, brass)
	}
	text(img, x+16, y+23, label, fg)
}

func heightColor(height int) color.RGBA {
	if height == 0 {
		return panelAlt
	}
	if height < 0 {
		v := 72 + clampInt(-height*12, 0, 96)
		return color.RGBA{R: 48, G: uint8(v), B: 142, A: 255}
	}
	v := 84 + clampInt(height*12, 0, 132)
	return color.RGBA{R: 70, G: uint8(v), B: 83, A: 255}
}

func splatColor(weights []int) (color.RGBA, bool) {
	if len(weights) != 4 {
		return color.RGBA{}, false
	}
	if weights[0] == 255 && weights[1] == 0 && weights[2] == 0 && weights[3] == 0 {
		return color.RGBA{}, false
	}
	best := 0
	for i := 1; i < len(weights); i++ {
		if weights[i] > weights[best] {
			best = i
		}
	}
	return paintLayerColor(best), true
}

func paintLayerColor(layer int) color.RGBA {
	switch layer {
	case 0:
		return color.RGBA{R: 74, G: 138, B: 83, A: 255}
	case 1:
		return color.RGBA{R: 143, G: 125, B: 79, A: 255}
	case 2:
		return color.RGBA{R: 82, G: 124, B: 140, A: 255}
	case 3:
		return color.RGBA{R: 154, G: 93, B: 79, A: 255}
	default:
		return panelAlt
	}
}

func cliffFlagCells(flags []CliffFlagSnapshot) map[[2]int]CliffFlagKind {
	out := make(map[[2]int]CliffFlagKind, len(flags))
	for _, flag := range flags {
		key := [2]int{flag.X, flag.Y}
		if flag.Kind == CliffFlagRampInvalidated {
			out[key] = flag.Kind
			continue
		}
		if _, ok := out[key]; !ok {
			out[key] = flag.Kind
		}
	}
	return out
}

func drawCliffFlagMarker(img *image.RGBA, x, y int, kind CliffFlagKind) {
	c := brass
	if kind == CliffFlagRampInvalidated {
		c = errorColor
	}
	fill(img, x+2, y+2, terrainGridCellW-4, 3, c)
	fill(img, x+2, y+2, 3, terrainGridCellH-4, c)
	fill(img, x+terrainGridCellW-5, y+2, 3, terrainGridCellH-4, c)
	fill(img, x+2, y+terrainGridCellH-5, terrainGridCellW-4, 3, c)
}

func cliffFlagsLabel(flags []CliffFlagSnapshot) string {
	if len(flags) == 0 {
		return "none"
	}
	limit := len(flags)
	if limit > 3 {
		limit = 3
	}
	parts := make([]string, 0, limit+1)
	for i := 0; i < limit; i++ {
		flag := flags[i]
		label := fmt.Sprintf("%s[%d,%d]", flag.Kind, flag.X, flag.Y)
		if flag.EntityID != 0 {
			label += fmt.Sprintf("#%d", flag.EntityID)
		}
		parts = append(parts, label)
	}
	if len(flags) > limit {
		parts = append(parts, fmt.Sprintf("+%d more", len(flags)-limit))
	}
	return strings.Join(parts, " ")
}

func flagTextColor(flags []CliffFlagSnapshot) color.RGBA {
	if len(flags) == 0 {
		return muted
	}
	return errorColor
}

func paletteLabel(items []ObjectPaletteItem) string {
	if len(items) == 0 {
		return "not loaded"
	}
	limit := len(items)
	if limit > 5 {
		limit = 5
	}
	parts := make([]string, 0, limit+1)
	for i := 0; i < limit; i++ {
		prefix := string(items[i].Kind)
		if items[i].Active {
			prefix += "*"
		}
		parts = append(parts, prefix+":"+shortObjectType(items[i].Type))
	}
	if len(items) > limit {
		parts = append(parts, fmt.Sprintf("+%d", len(items)-limit))
	}
	return strings.Join(parts, " ")
}

func objectCellLabel(pos [2]int) string {
	return fmt.Sprintf("%d,%d", pos[0]/editorTerrainCellWorldUnit, pos[1]/editorTerrainCellWorldUnit)
}

func shortObjectType(s string) string {
	if s == "" {
		return "none"
	}
	if len(s) <= 28 {
		return s
	}
	return "..." + s[len(s)-25:]
}

func clampInt(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func fill(img *image.RGBA, x, y, w, h int, c color.RGBA) {
	draw.Draw(img, image.Rect(x, y, x+w, y+h), &image.Uniform{c}, image.Point{}, draw.Src)
}

func line(img *image.RGBA, x1, y1, x2, y2 int, c color.RGBA) {
	if y1 == y2 {
		fill(img, x1, y1, x2-x1, 1, c)
		return
	}
	if x1 == x2 {
		fill(img, x1, y1, 1, y2-y1, c)
	}
}

func text(img *image.RGBA, x, y int, s string, c color.RGBA) {
	d := &font.Drawer{Dst: img, Src: &image.Uniform{c}, Face: basicfont.Face7x13, Dot: fixed.P(x, y)}
	d.DrawString(s)
}

func textFit(img *image.RGBA, x, y, maxWidth int, s string, c color.RGBA) {
	text(img, x, y, clipText(s, maxWidth), c)
}

func clipText(s string, maxWidth int) string {
	if maxWidth <= 0 {
		return ""
	}
	maxChars := maxWidth / 7
	if len(s) <= maxChars {
		return s
	}
	if maxChars <= 3 {
		return s[:maxChars]
	}
	return s[:maxChars-3] + "..."
}

func dirtyColor(dirty bool) color.RGBA {
	if dirty {
		return brass
	}
	return green
}
