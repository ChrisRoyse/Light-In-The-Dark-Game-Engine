package hud

import (
	"strconv"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

const (
	DefaultAtlasPath       = "assets/ui/litd-default-ui.atlas.png"
	DefaultHUDDrawCallCap  = 20
	DefaultHUDWidgetCount  = 10
	DefaultHUDLabelCount   = 8
	defaultTextBufferBytes = 96
)

type WidgetKind uint8

const (
	WidgetNineSlice WidgetKind = iota
	WidgetIconGrid
	WidgetProgressBar
)

func (k WidgetKind) String() string {
	switch k {
	case WidgetNineSlice:
		return "nine-slice"
	case WidgetIconGrid:
		return "icon-grid"
	case WidgetProgressBar:
		return "progress-bar"
	default:
		return "unknown"
	}
}

type WidgetSpec struct {
	Name        string
	Kind        WidgetKind
	Anchor      Anchor
	Ref         RefRect
	AtlasRegion string
	Parent      string
	CellsX      int
	CellsY      int
}

type Widget struct {
	WidgetSpec
	Rect Rect
}

type TextBuffer struct {
	buf [defaultTextBufferBytes]byte
	n   int
}

func (b *TextBuffer) Bytes() []byte {
	return b.buf[:b.n]
}

func (b *TextBuffer) String() string {
	return string(b.Bytes())
}

func (b *TextBuffer) reset() []byte {
	b.n = 0
	return b.buf[:0]
}

func (b *TextBuffer) commit(p []byte) {
	b.n = len(p)
}

type HUDState struct {
	Tick                uint32
	Gold                int
	Lumber              int
	FoodUsed            int
	FoodCap             int
	Upkeep              int
	Life                int
	LifeMax             int
	Mana                int
	ManaMax             int
	SelectionVersion    int
	QueueVersion        int
	ControlGroupVersion int
}

type UpdateStats struct {
	DirtyLabels       int `json:"dirtyLabels"`
	Repaints          int `json:"repaints"`
	ResourceRepaints  int `json:"resourceRepaints"`
	VitalRepaints     int `json:"vitalRepaints"`
	SelectionRebuilds int `json:"selectionRebuilds"`
	QueueRebuilds     int `json:"queueRebuilds"`
	GroupRebuilds     int `json:"groupRebuilds"`
}

type ScenarioStats struct {
	Frames            int   `json:"frames"`
	Repaints          int   `json:"repaints"`
	DirtyLabels       int   `json:"dirtyLabels"`
	ResourceRepaints  int   `json:"resourceRepaints"`
	VitalRepaints     int   `json:"vitalRepaints"`
	SelectionRebuilds int   `json:"selectionRebuilds"`
	QueueRebuilds     int   `json:"queueRebuilds"`
	GroupRebuilds     int   `json:"groupRebuilds"`
	UpdateMicros      int64 `json:"updateMicros"`
}

type FSVScenarios struct {
	Initial        UpdateStats   `json:"initial"`
	Static100      ScenarioStats `json:"static100"`
	ResourceChurn  ScenarioStats `json:"resourceChurn"`
	SelectionChurn ScenarioStats `json:"selectionChurn500"`
}

type DefaultHUD struct {
	Canvas      Canvas
	Labels      HUDStrings
	widgets     [DefaultHUDWidgetCount]Widget
	Resource    TextBuffer
	ResourceBar ResourceBar
	Vitals      TextBuffer
	Selection   TextBuffer
	Queue       TextBuffer
	Groups      TextBuffer
	state       HUDState
	initialized bool
}

type HUDStrings struct {
	ResourceGold    string
	ResourceLumber  string
	ResourceFood    string
	ResourceUpkeep  string
	VitalLife       string
	VitalMana       string
	SelectionPrefix string
	QueuePrefix     string
	GroupsPrefix    string
	MenuOKTrue      string
	MenuOKFalse     string
	IdleWorker      string
	Minimap         string
}

var defaultWidgetSpecs = [DefaultHUDWidgetCount]WidgetSpec{
	{Name: "menu-cluster", Kind: WidgetNineSlice, Anchor: AnchorTopLeft, Ref: RefRect{X: 16, Y: 16, W: 220, H: 36}, AtlasRegion: "panel-small"},
	{Name: "resource-bar", Kind: WidgetNineSlice, Anchor: AnchorTopRight, Ref: RefRect{X: 906, Y: 16, W: 358, H: 36}, AtlasRegion: "panel-small"},
	{Name: "idle-worker", Kind: WidgetNineSlice, Anchor: AnchorBottomLeft, Ref: RefRect{X: 16, Y: 504, W: 150, H: 36}, AtlasRegion: "button"},
	{Name: "control-groups", Kind: WidgetIconGrid, Anchor: AnchorBottomLeft, Ref: RefRect{X: 16, Y: 540, W: 500, H: 36}, AtlasRegion: "button", CellsX: 10, CellsY: 1},
	{Name: "minimap", Kind: WidgetNineSlice, Anchor: AnchorBottomLeft, Ref: RefRect{X: 16, Y: 584, W: 220, H: 120}, AtlasRegion: "panel-large"},
	{Name: "portrait", Kind: WidgetNineSlice, Anchor: AnchorBottomLeft, Ref: RefRect{X: 252, Y: 584, W: 180, H: 120}, AtlasRegion: "panel-large"},
	{Name: "life-bar", Kind: WidgetProgressBar, Anchor: AnchorBottomLeft, Ref: RefRect{X: 266, Y: 666, W: 152, H: 12}, AtlasRegion: "bar-life", Parent: "portrait"},
	{Name: "mana-bar", Kind: WidgetProgressBar, Anchor: AnchorBottomLeft, Ref: RefRect{X: 266, Y: 682, W: 152, H: 10}, AtlasRegion: "bar-mana", Parent: "portrait"},
	{Name: "info-panel", Kind: WidgetNineSlice, Anchor: AnchorBottom, Ref: RefRect{X: 448, Y: 584, W: 360, H: 120}, AtlasRegion: "panel-large"},
	{Name: "command-card", Kind: WidgetIconGrid, Anchor: AnchorBottomRight, Ref: RefRect{X: 872, Y: 548, W: 392, H: 156}, AtlasRegion: "button", CellsX: 4, CellsY: 3},
}

func DefaultHUDState() HUDState {
	return HUDState{
		Gold:                725,
		Lumber:              240,
		FoodUsed:            18,
		FoodCap:             30,
		Upkeep:              0,
		Life:                420,
		LifeMax:             500,
		Mana:                110,
		ManaMax:             180,
		SelectionVersion:    1,
		QueueVersion:        1,
		ControlGroupVersion: 1,
	}
}

func NewDefaultHUDWithStrings(canvas Canvas, labels HUDStrings) DefaultHUD {
	var h DefaultHUD
	h.Labels = labels
	h.ResourceBar = NewResourceBar(&h.Resource, ResourceBarStringsFromHUD(labels))
	h.Layout(canvas)
	h.Update(DefaultHUDState())
	return h
}

func HUDStringsFromLocale(table *locale.Table) HUDStrings {
	return HUDStrings{
		ResourceGold:    table.Must(locale.HUDResourceGold),
		ResourceLumber:  table.Must(locale.HUDResourceLumber),
		ResourceFood:    table.Must(locale.HUDResourceFood),
		ResourceUpkeep:  table.Must(locale.HUDResourceUpkeep),
		VitalLife:       table.Must(locale.HUDVitalLife),
		VitalMana:       table.Must(locale.HUDVitalMana),
		SelectionPrefix: table.Must(locale.HUDSelectionPrefix),
		QueuePrefix:     table.Must(locale.HUDQueuePrefix),
		GroupsPrefix:    table.Must(locale.HUDGroupsPrefix),
		MenuOKTrue:      table.Must(locale.HUDMenuOKTrue),
		MenuOKFalse:     table.Must(locale.HUDMenuOKFalse),
		IdleWorker:      table.Must(locale.HUDIdleWorker),
		Minimap:         table.Must(locale.HUDMinimap),
	}
}

func (h *DefaultHUD) Layout(canvas Canvas) {
	h.Canvas = canvas
	for i, spec := range defaultWidgetSpecs {
		h.widgets[i] = Widget{WidgetSpec: spec, Rect: canvas.Place(spec.Anchor, spec.Ref)}
	}
}

func (h *DefaultHUD) Widgets() []Widget {
	return h.widgets[:]
}

func (h *DefaultHUD) PanelDrawCalls() int {
	return len(h.widgets)
}

func (h *DefaultHUD) LabelDrawCalls() int {
	return DefaultHUDLabelCount
}

func (h *DefaultHUD) ExpectedGUIDrawCalls() int {
	return h.PanelDrawCalls() + h.LabelDrawCalls()
}

func (h *DefaultHUD) Update(s HUDState) UpdateStats {
	var stats UpdateStats
	first := !h.initialized

	if h.ResourceBar.Text == nil {
		h.ResourceBar = NewResourceBar(&h.Resource, ResourceBarStringsFromHUD(h.Labels))
	}
	h.ResourceBar.Text = &h.Resource
	resource := h.ResourceBar.Update(ResourceBarState{Gold: s.Gold, Lumber: s.Lumber, FoodUsed: s.FoodUsed, FoodCap: s.FoodCap, Upkeep: s.Upkeep, Tick: s.Tick})
	if first || resource.Dirty {
		stats.DirtyLabels++
		stats.ResourceRepaints++
	}
	if first || s.Life != h.state.Life || s.LifeMax != h.state.LifeMax || s.Mana != h.state.Mana || s.ManaMax != h.state.ManaMax {
		h.setVitalsText(s)
		stats.DirtyLabels++
		stats.VitalRepaints++
	}
	if first || s.SelectionVersion != h.state.SelectionVersion {
		h.setSelectionText(s.SelectionVersion)
		stats.DirtyLabels++
		stats.SelectionRebuilds++
	}
	if first || s.QueueVersion != h.state.QueueVersion {
		h.setQueueText(s.QueueVersion)
		stats.DirtyLabels++
		stats.QueueRebuilds++
	}
	if first || s.ControlGroupVersion != h.state.ControlGroupVersion {
		h.setGroupsText(s.ControlGroupVersion)
		stats.DirtyLabels++
		stats.GroupRebuilds++
	}

	stats.Repaints = stats.DirtyLabels
	h.state = s
	h.initialized = true
	return stats
}

func (h *DefaultHUD) RunFSVScenarios() FSVScenarios {
	state := DefaultHUDState()
	h.initialized = false
	initial := h.Update(state)
	static := h.runScenario(100, state, func(_ int, s *HUDState) {})
	h.initialized = false
	h.Update(state)
	resource := h.runScenario(60, state, churnResources)
	h.initialized = false
	h.Update(state)
	selection := h.runScenario(500, state, churnSelection)
	return FSVScenarios{
		Initial:        initial,
		Static100:      static,
		ResourceChurn:  resource,
		SelectionChurn: selection,
	}
}

func (h *DefaultHUD) runScenario(frames int, state HUDState, mutate func(int, *HUDState)) ScenarioStats {
	var out ScenarioStats
	out.Frames = frames
	start := time.Now()
	for i := 0; i < frames; i++ {
		state.Tick++
		mutate(i, &state)
		stats := h.Update(state)
		out.Repaints += stats.Repaints
		out.DirtyLabels += stats.DirtyLabels
		out.ResourceRepaints += stats.ResourceRepaints
		out.VitalRepaints += stats.VitalRepaints
		out.SelectionRebuilds += stats.SelectionRebuilds
		out.QueueRebuilds += stats.QueueRebuilds
		out.GroupRebuilds += stats.GroupRebuilds
	}
	out.UpdateMicros = time.Since(start).Microseconds()
	return out
}

func churnResources(i int, s *HUDState) {
	s.Gold += 3 + i%5
	s.Lumber += 1 + i%3
	s.FoodUsed = 18 + i%7
}

func churnSelection(i int, s *HUDState) {
	s.SelectionVersion++
	if i%8 == 0 {
		s.QueueVersion++
	}
}

func (h *DefaultHUD) setVitalsText(s HUDState) {
	p := h.Vitals.reset()
	p = append(p, h.Labels.VitalLife...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.Life), 10)
	p = append(p, '/')
	p = strconv.AppendInt(p, int64(s.LifeMax), 10)
	p = append(p, ' ', ' ')
	p = append(p, h.Labels.VitalMana...)
	p = append(p, ' ')
	p = strconv.AppendInt(p, int64(s.Mana), 10)
	p = append(p, '/')
	p = strconv.AppendInt(p, int64(s.ManaMax), 10)
	h.Vitals.commit(p)
}

func (h *DefaultHUD) setSelectionText(version int) {
	p := h.Selection.reset()
	p = append(p, h.Labels.SelectionPrefix...)
	p = strconv.AppendInt(p, int64(version), 10)
	h.Selection.commit(p)
}

func (h *DefaultHUD) setQueueText(version int) {
	p := h.Queue.reset()
	p = append(p, h.Labels.QueuePrefix...)
	p = strconv.AppendInt(p, int64(version), 10)
	h.Queue.commit(p)
}

func (h *DefaultHUD) setGroupsText(version int) {
	p := h.Groups.reset()
	p = append(p, h.Labels.GroupsPrefix...)
	p = strconv.AppendInt(p, int64(version), 10)
	h.Groups.commit(p)
}

type LayoutIssue struct {
	Widget string `json:"widget"`
	Rule   string `json:"rule"`
	Msg    string `json:"msg"`
}

func ValidateWidgets(widgets []Widget, width, height int) []LayoutIssue {
	var issues []LayoutIssue
	for i, w := range widgets {
		if !w.Rect.Inside(width, height) {
			issues = append(issues, LayoutIssue{Widget: w.Name, Rule: "offscreen", Msg: "widget rect leaves the canvas"})
		}
		if w.Parent != "" {
			parent, ok := findWidget(widgets, w.Parent)
			if !ok || !w.Rect.InsideRect(parent.Rect) {
				issues = append(issues, LayoutIssue{Widget: w.Name, Rule: "parent", Msg: "child widget is not inside its parent"})
			}
			continue
		}
		for j := 0; j < i; j++ {
			prev := widgets[j]
			if prev.Parent != "" || w.Rect.Overlaps(prev.Rect) {
				if prev.Parent == w.Name || w.Parent == prev.Name {
					continue
				}
			}
			if prev.Parent == "" && w.Rect.Overlaps(prev.Rect) {
				issues = append(issues, LayoutIssue{Widget: w.Name, Rule: "overlap", Msg: "top-level widgets overlap"})
			}
		}
	}
	return issues
}

func findWidget(widgets []Widget, name string) (Widget, bool) {
	for _, w := range widgets {
		if w.Name == name {
			return w, true
		}
	}
	return Widget{}, false
}

func (r Rect) InsideRect(parent Rect) bool {
	return r.X >= parent.X && r.Y >= parent.Y && r.Right() <= parent.Right() && r.Bottom() <= parent.Bottom()
}
