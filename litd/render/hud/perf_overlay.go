package hud

import (
	"errors"
	"image"
	"strconv"
)

const (
	DefaultPerfOverlayWidth  = 384
	DefaultPerfOverlayHeight = 176
	PerfOverlayDrawCallCap   = 5
	PerfOverlayHistory       = 64

	perfOverlayMinWidth  = 300
	perfOverlayMinHeight = 140
	perfOverlayRows      = 7
	perfOverlayPhases    = 7
	perfTextBytes        = 96
)

var ErrPerfOverlaySize = errors.New("hud: unsupported perf overlay size")

type PerfInput struct {
	Tick  uint32
	Frame uint32

	TickNS  int64
	PhaseNS [perfOverlayPhases]int64
	FrameNS int64
	FPS     int64

	DrawCalls int64
	Batches   int64
	Instances int64

	AllocsFrame int64
	AllocsTick  int64
	HeapBytes   int64

	Units    int64
	Missiles int64
	Buffs    int64
}

type PerfOverlay struct {
	img     *image.RGBA
	visible bool
	toggles int

	tick  uint32
	frame uint32

	current [perfOverlayRows]int64
	worst   [perfOverlayRows]int64
	budget  [perfOverlayRows]int64
	text    [perfOverlayRows]perfTextBuffer
	phaseNS [perfOverlayPhases]int64

	history     [PerfOverlayHistory][perfOverlayRows]int64
	historyHead int
	historyLen  int
}

type PerfOverlayDump struct {
	Visible        bool                     `json:"visible"`
	Width          int                      `json:"width"`
	Height         int                      `json:"height"`
	DrawCalls      int                      `json:"drawCalls"`
	DrawCallBudget int                      `json:"drawCallBudget"`
	Toggles        int                      `json:"toggles"`
	Samples        int                      `json:"samples"`
	Tick           uint32                   `json:"tick"`
	Frame          uint32                   `json:"frame"`
	Rows           []PerfOverlayRowDump     `json:"rows"`
	PhaseNS        [perfOverlayPhases]int64 `json:"phaseNS"`
}

type PerfOverlayRowDump struct {
	Name    string `json:"name"`
	Unit    string `json:"unit"`
	Current int64  `json:"current"`
	Worst   int64  `json:"worst"`
	Budget  int64  `json:"budget"`
	Text    string `json:"text"`
}

type perfTextBuffer struct {
	buf [perfTextBytes]byte
	n   int
}

type perfColor struct {
	r uint8
	g uint8
	b uint8
	a uint8
}

var perfRowNames = [perfOverlayRows]string{"TICK", "PHASE", "FRAME", "DRAW", "ALLOC", "HEAP", "ENT"}
var perfRowUnits = [perfOverlayRows]string{"ms_x100", "ms_x100", "ms_x100", "count", "count", "mb_x10", "count"}
var perfTitleText = [...]byte{'L', 'I', 'T', 'D', ' ', 'P', 'E', 'R', 'F'}
var perfHotkeyText = [...]byte{'F', '1', '1'}

var (
	perfBg       = perfColor{8, 12, 18, 210}
	perfPanel    = perfColor{20, 28, 38, 235}
	perfBorder   = perfColor{118, 174, 196, 255}
	perfText     = perfColor{222, 238, 231, 255}
	perfMuted    = perfColor{95, 118, 128, 255}
	perfWarn     = perfColor{232, 176, 82, 255}
	perfGood     = perfColor{86, 196, 138, 255}
	perfGraph    = perfColor{124, 194, 255, 255}
	perfGraphDim = perfColor{42, 72, 96, 255}
)

func NewDefaultPerfOverlay() *PerfOverlay {
	o, err := NewPerfOverlay(DefaultPerfOverlayWidth, DefaultPerfOverlayHeight)
	if err != nil {
		panic(err)
	}
	return o
}

func NewPerfOverlay(width, height int) (*PerfOverlay, error) {
	if width < perfOverlayMinWidth || height < perfOverlayMinHeight {
		return nil, ErrPerfOverlaySize
	}
	o := &PerfOverlay{
		img: image.NewRGBA(image.Rect(0, 0, width, height)),
		budget: [perfOverlayRows]int64{
			1000,  // tick: 10.00 ms
			500,   // phase: 5.00 ms
			1667,  // frame: 16.67 ms
			300,   // draw calls
			0,     // allocs/frame+tick
			15360, // heap: 1536.0 MiB
			1000,  // active entities
		},
	}
	o.formatRows(PerfInput{})
	o.paint()
	return o, nil
}

func (o *PerfOverlay) Image() *image.RGBA { return o.img }

func (o *PerfOverlay) Visible() bool { return o.visible }

func (o *PerfOverlay) SetVisible(visible bool) {
	if o.visible == visible {
		return
	}
	o.visible = visible
	o.toggles++
	if visible {
		o.paint()
	}
}

func (o *PerfOverlay) Toggle() {
	o.SetVisible(!o.visible)
}

func (o *PerfOverlay) ToggleCount() int { return o.toggles }

func (o *PerfOverlay) DrawCalls() int {
	if !o.visible {
		return 0
	}
	return 1
}

func (o *PerfOverlay) Update(in PerfInput) {
	o.tick = in.Tick
	o.frame = in.Frame
	o.phaseNS = in.PhaseNS
	values := perfValues(in)
	for i := 0; i < perfOverlayRows; i++ {
		o.current[i] = values[i]
		if o.historyLen == 0 || values[i] > o.worst[i] {
			o.worst[i] = values[i]
		}
	}
	o.history[o.historyHead] = values
	o.historyHead = (o.historyHead + 1) % PerfOverlayHistory
	if o.historyLen < PerfOverlayHistory {
		o.historyLen++
	}
	o.formatRows(in)
	if o.visible {
		o.paint()
	}
}

func (o *PerfOverlay) Snapshot() PerfOverlayDump {
	rows := make([]PerfOverlayRowDump, perfOverlayRows)
	for i := 0; i < perfOverlayRows; i++ {
		rows[i] = PerfOverlayRowDump{
			Name:    perfRowNames[i],
			Unit:    perfRowUnits[i],
			Current: o.current[i],
			Worst:   o.worst[i],
			Budget:  o.budget[i],
			Text:    o.text[i].String(),
		}
	}
	return PerfOverlayDump{
		Visible:        o.visible,
		Width:          o.img.Rect.Dx(),
		Height:         o.img.Rect.Dy(),
		DrawCalls:      o.DrawCalls(),
		DrawCallBudget: PerfOverlayDrawCallCap,
		Toggles:        o.toggles,
		Samples:        o.historyLen,
		Tick:           o.tick,
		Frame:          o.frame,
		Rows:           rows,
		PhaseNS:        o.phaseNS,
	}
}

func perfValues(in PerfInput) [perfOverlayRows]int64 {
	return [perfOverlayRows]int64{
		nsHundredthMS(in.TickNS),
		nsHundredthMS(maxPhaseNS(in.PhaseNS)),
		nsHundredthMS(in.FrameNS),
		clampNonNegative(in.DrawCalls),
		clampNonNegative(in.AllocsFrame + in.AllocsTick),
		heapTenthMiB(in.HeapBytes),
		clampNonNegative(in.Units + in.Missiles + in.Buffs),
	}
}

func maxPhaseNS(phases [perfOverlayPhases]int64) int64 {
	var max int64
	for i := 0; i < perfOverlayPhases; i++ {
		if phases[i] > max {
			max = phases[i]
		}
	}
	return max
}

func nsHundredthMS(ns int64) int64 {
	if ns <= 0 {
		return 0
	}
	return (ns + 5000) / 10000
}

func heapTenthMiB(bytes int64) int64 {
	if bytes <= 0 {
		return 0
	}
	return (bytes*10 + (1 << 19)) / (1 << 20)
}

func clampNonNegative(v int64) int64 {
	if v < 0 {
		return 0
	}
	return v
}

func (o *PerfOverlay) formatRows(in PerfInput) {
	o.formatDurationRow(0)
	o.formatDurationRow(1)
	o.formatFrameRow(in.FPS)
	o.formatCountRow(3)
	o.formatCountRow(4)
	o.formatHeapRow(5)
	o.formatEntityRow(in)
}

func (o *PerfOverlay) formatDurationRow(row int) {
	b := o.text[row].reset()
	b = appendLabel(b, perfRowNames[row])
	b = appendFixed2(b, o.current[row])
	b = append(b, 'M', 'S', ' ', 'W', ' ')
	b = appendFixed2(b, o.worst[row])
	o.text[row].commit(b)
}

func (o *PerfOverlay) formatCountRow(row int) {
	b := o.text[row].reset()
	b = appendLabel(b, perfRowNames[row])
	b = strconv.AppendInt(b, o.current[row], 10)
	b = append(b, ' ', 'W', ' ')
	b = strconv.AppendInt(b, o.worst[row], 10)
	o.text[row].commit(b)
}

func (o *PerfOverlay) formatFrameRow(fps int64) {
	b := o.text[2].reset()
	b = appendLabel(b, perfRowNames[2])
	b = appendFixed2(b, o.current[2])
	b = append(b, 'M', 'S', ' ')
	b = strconv.AppendInt(b, clampNonNegative(fps), 10)
	b = append(b, 'F', 'P', 'S', ' ', 'W', ' ')
	b = appendFixed2(b, o.worst[2])
	o.text[2].commit(b)
}

func (o *PerfOverlay) formatHeapRow(row int) {
	b := o.text[row].reset()
	b = appendLabel(b, perfRowNames[row])
	b = appendFixed1(b, o.current[row])
	b = append(b, 'M', 'B', ' ', 'W', ' ')
	b = appendFixed1(b, o.worst[row])
	o.text[row].commit(b)
}

func (o *PerfOverlay) formatEntityRow(in PerfInput) {
	b := o.text[6].reset()
	b = appendLabel(b, perfRowNames[6])
	b = strconv.AppendInt(b, clampNonNegative(in.Units), 10)
	b = append(b, '/')
	b = strconv.AppendInt(b, clampNonNegative(in.Missiles), 10)
	b = append(b, '/')
	b = strconv.AppendInt(b, clampNonNegative(in.Buffs), 10)
	b = append(b, ' ', 'W', ' ')
	b = strconv.AppendInt(b, o.worst[6], 10)
	o.text[6].commit(b)
}

func appendLabel(dst []byte, label string) []byte {
	for i := 0; i < len(label); i++ {
		dst = append(dst, label[i])
	}
	return append(dst, ' ')
}

func appendFixed2(dst []byte, hundredths int64) []byte {
	if hundredths < 0 {
		dst = append(dst, '-')
		hundredths = -hundredths
	}
	whole := hundredths / 100
	frac := hundredths % 100
	dst = strconv.AppendInt(dst, whole, 10)
	dst = append(dst, '.')
	if frac < 10 {
		dst = append(dst, '0')
	}
	return strconv.AppendInt(dst, frac, 10)
}

func appendFixed1(dst []byte, tenths int64) []byte {
	if tenths < 0 {
		dst = append(dst, '-')
		tenths = -tenths
	}
	whole := tenths / 10
	frac := tenths % 10
	dst = strconv.AppendInt(dst, whole, 10)
	dst = append(dst, '.')
	return strconv.AppendInt(dst, frac, 10)
}

func (b *perfTextBuffer) reset() []byte {
	b.n = 0
	return b.buf[:0]
}

func (b *perfTextBuffer) commit(p []byte) {
	b.n = len(p)
}

func (b *perfTextBuffer) Bytes() []byte {
	return b.buf[:b.n]
}

func (b *perfTextBuffer) String() string {
	return string(b.Bytes())
}

func (o *PerfOverlay) paint() {
	o.clear()
	w := o.img.Rect.Dx()
	h := o.img.Rect.Dy()
	fillRect(o.img, 0, 0, w, h, perfBg)
	fillRect(o.img, 1, 1, w-2, h-2, perfPanel)
	fillRect(o.img, 0, 0, w, 1, perfBorder)
	fillRect(o.img, 0, h-1, w, 1, perfBorder)
	fillRect(o.img, 0, 0, 1, h, perfBorder)
	fillRect(o.img, w-1, 0, 1, h, perfBorder)
	drawText(o.img, 12, 8, perfTitleText[:], 2, perfGood)
	drawText(o.img, w-118, 8, perfHotkeyText[:], 2, perfMuted)

	for row := 0; row < perfOverlayRows; row++ {
		y := 28 + row*20
		drawText(o.img, 12, y, o.text[row].Bytes(), 2, perfText)
		o.drawSparkline(row, w-126, y-1, 108, 13)
	}
	o.drawPhaseBars(w-126, 51, 108, 8)
}

func (o *PerfOverlay) clear() {
	pix := o.img.Pix
	for i := 0; i < len(pix); i += 4 {
		pix[i] = 0
		pix[i+1] = 0
		pix[i+2] = 0
		pix[i+3] = 0
	}
}

func (o *PerfOverlay) drawSparkline(row, x, y, w, h int) {
	fillRect(o.img, x, y, w, h, perfGraphDim)
	if o.historyLen == 0 {
		return
	}
	maxv := o.worst[row]
	if o.budget[row] > maxv {
		maxv = o.budget[row]
	}
	if maxv <= 0 {
		maxv = 1
	}
	if o.budget[row] > 0 {
		by := y + h - 1 - int(o.budget[row]*int64(h-2)/maxv)
		fillRect(o.img, x, by, w, 1, perfWarn)
	}
	count := o.historyLen
	if count > w {
		count = w
	}
	start := o.historyHead - o.historyLen
	for i := 0; i < count; i++ {
		idx := (start + i + PerfOverlayHistory) % PerfOverlayHistory
		v := o.history[idx][row]
		bh := int(v * int64(h-2) / maxv)
		if bh < 1 && v > 0 {
			bh = 1
		}
		if bh > h-2 {
			bh = h - 2
		}
		px := x + i*w/count
		fillRect(o.img, px, y+h-1-bh, 1, bh, perfGraph)
	}
}

func (o *PerfOverlay) drawPhaseBars(x, y, w, h int) {
	maxv := maxPhaseNS(o.phaseNS)
	if maxv <= 0 {
		maxv = 1
	}
	barW := w/perfOverlayPhases - 2
	if barW < 1 {
		barW = 1
	}
	for i := 0; i < perfOverlayPhases; i++ {
		bh := int(o.phaseNS[i] * int64(h) / maxv)
		if bh < 1 && o.phaseNS[i] > 0 {
			bh = 1
		}
		px := x + i*(barW+2)
		fillRect(o.img, px, y, barW, h, perfGraphDim)
		fillRect(o.img, px, y+h-bh, barW, bh, perfGood)
	}
}

func fillRect(img *image.RGBA, x, y, w, h int, c perfColor) {
	if w <= 0 || h <= 0 {
		return
	}
	minX := img.Rect.Min.X
	minY := img.Rect.Min.Y
	maxX := img.Rect.Max.X
	maxY := img.Rect.Max.Y
	if x < minX {
		w -= minX - x
		x = minX
	}
	if y < minY {
		h -= minY - y
		y = minY
	}
	if x+w > maxX {
		w = maxX - x
	}
	if y+h > maxY {
		h = maxY - y
	}
	if w <= 0 || h <= 0 {
		return
	}
	for py := y; py < y+h; py++ {
		off := img.PixOffset(x, py)
		for px := 0; px < w; px++ {
			img.Pix[off] = c.r
			img.Pix[off+1] = c.g
			img.Pix[off+2] = c.b
			img.Pix[off+3] = c.a
			off += 4
		}
	}
}

func drawText(img *image.RGBA, x, y int, text []byte, scale int, c perfColor) {
	if scale <= 0 {
		scale = 1
	}
	advance := 4*scale + 1
	for i := 0; i < len(text); i++ {
		drawGlyph(img, x+i*advance, y, text[i], scale, c)
	}
}

func drawGlyph(img *image.RGBA, x, y int, ch byte, scale int, c perfColor) {
	g := glyph3x5(ch)
	for gy := 0; gy < 5; gy++ {
		row := g[gy]
		for gx := 0; gx < 3; gx++ {
			if row&(1<<uint(2-gx)) == 0 {
				continue
			}
			fillRect(img, x+gx*scale, y+gy*scale, scale, scale, c)
		}
	}
}

func glyph3x5(ch byte) [5]byte {
	switch ch {
	case '0':
		return [5]byte{7, 5, 5, 5, 7}
	case '1':
		return [5]byte{2, 6, 2, 2, 7}
	case '2':
		return [5]byte{7, 1, 7, 4, 7}
	case '3':
		return [5]byte{7, 1, 7, 1, 7}
	case '4':
		return [5]byte{5, 5, 7, 1, 1}
	case '5':
		return [5]byte{7, 4, 7, 1, 7}
	case '6':
		return [5]byte{7, 4, 7, 5, 7}
	case '7':
		return [5]byte{7, 1, 2, 2, 2}
	case '8':
		return [5]byte{7, 5, 7, 5, 7}
	case '9':
		return [5]byte{7, 5, 7, 1, 7}
	case 'A':
		return [5]byte{2, 5, 7, 5, 5}
	case 'B':
		return [5]byte{6, 5, 6, 5, 6}
	case 'C':
		return [5]byte{7, 4, 4, 4, 7}
	case 'D':
		return [5]byte{6, 5, 5, 5, 6}
	case 'E':
		return [5]byte{7, 4, 6, 4, 7}
	case 'F':
		return [5]byte{7, 4, 6, 4, 4}
	case 'G':
		return [5]byte{7, 4, 5, 5, 7}
	case 'H':
		return [5]byte{5, 5, 7, 5, 5}
	case 'I':
		return [5]byte{7, 2, 2, 2, 7}
	case 'K':
		return [5]byte{5, 5, 6, 5, 5}
	case 'L':
		return [5]byte{4, 4, 4, 4, 7}
	case 'M':
		return [5]byte{5, 7, 7, 5, 5}
	case 'N':
		return [5]byte{5, 7, 7, 7, 5}
	case 'O':
		return [5]byte{7, 5, 5, 5, 7}
	case 'P':
		return [5]byte{6, 5, 6, 4, 4}
	case 'R':
		return [5]byte{6, 5, 6, 5, 5}
	case 'S':
		return [5]byte{7, 4, 7, 1, 7}
	case 'T':
		return [5]byte{7, 2, 2, 2, 2}
	case 'U':
		return [5]byte{5, 5, 5, 5, 7}
	case 'W':
		return [5]byte{5, 5, 7, 7, 5}
	case 'Y':
		return [5]byte{5, 5, 2, 2, 2}
	case '.':
		return [5]byte{0, 0, 0, 0, 2}
	case '/':
		return [5]byte{1, 1, 2, 4, 4}
	case '-':
		return [5]byte{0, 0, 7, 0, 0}
	case ':':
		return [5]byte{0, 2, 0, 2, 0}
	default:
		return [5]byte{}
	}
}
