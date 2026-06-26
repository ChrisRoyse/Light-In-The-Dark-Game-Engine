package input

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

const (
	ControlGroupCount         = 10
	DefaultDoubleTapMS uint32 = 300
)

type GroupConfig struct {
	MaxMembers  int
	DoubleTapMS uint32
}

type GroupEntity struct {
	ID sim.EntityID `json:"id"`
	X  float32      `json:"x"`
	Z  float32      `json:"z"`
}

type GroupResult struct {
	Group                 uint8     `json:"group"`
	Selection             Selection `json:"selection"`
	GroupCount            uint8     `json:"groupCount"`
	Pruned                uint8     `json:"pruned"`
	CenterRequested       bool      `json:"centerRequested"`
	CenterX               float32   `json:"centerX,omitempty"`
	CenterZ               float32   `json:"centerZ,omitempty"`
	CommandRecordsEmitted uint16    `json:"commandRecordsEmitted"`
}

type ControlGroups struct {
	cfg       GroupConfig
	ids       [ControlGroupCount][MaxSelection]sim.EntityID
	counts    [ControlGroupCount]uint8
	lastGroup uint8
	lastMS    uint32
	lastValid bool
}

func DefaultGroupConfig() GroupConfig {
	return GroupConfig{MaxMembers: MaxSelection, DoubleTapMS: DefaultDoubleTapMS}
}

func NewControlGroups(cfg GroupConfig) ControlGroups {
	if cfg.MaxMembers < 0 || cfg.MaxMembers > MaxSelection {
		cfg.MaxMembers = MaxSelection
	}
	if cfg.DoubleTapMS == 0 {
		cfg.DoubleTapMS = DefaultDoubleTapMS
	}
	return ControlGroups{cfg: cfg}
}

func (g *ControlGroups) Assign(group int, selection Selection) GroupResult {
	if !validGroup(group) {
		return GroupResult{}
	}
	next := [MaxSelection]sim.EntityID{}
	n := g.copySelection(&next, 0, selection)
	g.ids[group] = next
	g.counts[group] = uint8(n)
	g.clearTap()
	return g.resultWithSelection(group, selection, 0, false, 0, 0)
}

func (g *ControlGroups) Add(group int, selection Selection) GroupResult {
	if !validGroup(group) {
		return GroupResult{}
	}
	next := g.ids[group]
	n := int(g.counts[group])
	n = g.copySelection(&next, n, selection)
	g.ids[group] = next
	g.counts[group] = uint8(n)
	g.clearTap()
	return g.resultWithSelection(group, selection, 0, false, 0, 0)
}

func (g *ControlGroups) Recall(group int, live []GroupEntity, nowMS uint32) GroupResult {
	if !validGroup(group) {
		return GroupResult{}
	}
	center := g.lastValid && g.lastGroup == uint8(group) && nowMS-g.lastMS <= g.cfg.DoubleTapMS
	pruned, cx, cz := g.prune(group, live)
	g.lastGroup = uint8(group)
	g.lastMS = nowMS
	g.lastValid = true
	if g.counts[group] == 0 {
		center = false
		cx, cz = 0, 0
	}
	return g.result(group, pruned, center, cx, cz)
}

func (g *ControlGroups) IDs(group int) ([]sim.EntityID, bool) {
	if !validGroup(group) {
		return nil, false
	}
	return g.ids[group][:g.counts[group]], true
}

func (g *ControlGroups) Count(group int) uint8 {
	if !validGroup(group) {
		return 0
	}
	return g.counts[group]
}

func (g *ControlGroups) copySelection(dst *[MaxSelection]sim.EntityID, n int, selection Selection) int {
	limit := g.maxMembers()
	for i := 0; i < int(selection.Count) && n < limit; i++ {
		id := selection.IDs[i]
		if id == 0 || containsID(dst[:], n, id) {
			continue
		}
		dst[n] = id
		n++
	}
	return n
}

func (g *ControlGroups) prune(group int, live []GroupEntity) (uint8, float32, float32) {
	oldCount := int(g.counts[group])
	next := [MaxSelection]sim.EntityID{}
	n := 0
	var sumX, sumZ float32
	for i := 0; i < oldCount; i++ {
		id := g.ids[group][i]
		if ent, ok := liveEntity(live, id); ok {
			next[n] = id
			n++
			sumX += ent.X
			sumZ += ent.Z
		}
	}
	g.ids[group] = next
	g.counts[group] = uint8(n)
	if n == 0 {
		return uint8(oldCount), 0, 0
	}
	return uint8(oldCount - n), sumX / float32(n), sumZ / float32(n)
}

func (g *ControlGroups) result(group int, pruned uint8, center bool, cx, cz float32) GroupResult {
	var sel Selection
	count := g.counts[group]
	sel.Count = count
	copy(sel.IDs[:], g.ids[group][:])
	return g.resultWithSelection(group, sel, pruned, center, cx, cz)
}

func (g *ControlGroups) resultWithSelection(group int, selection Selection, pruned uint8, center bool, cx, cz float32) GroupResult {
	out := GroupResult{
		Group:           uint8(group),
		Selection:       selection,
		GroupCount:      g.counts[group],
		Pruned:          pruned,
		CenterRequested: center,
	}
	if center {
		out.CenterX = cx
		out.CenterZ = cz
	}
	return out
}

func (g *ControlGroups) maxMembers() int {
	if g.cfg.MaxMembers <= 0 || g.cfg.MaxMembers > MaxSelection {
		return MaxSelection
	}
	return g.cfg.MaxMembers
}

func (g *ControlGroups) clearTap() {
	g.lastGroup = 0
	g.lastMS = 0
	g.lastValid = false
}

func validGroup(group int) bool {
	return group >= 0 && group < ControlGroupCount
}

func liveEntity(live []GroupEntity, id sim.EntityID) (GroupEntity, bool) {
	for i := range live {
		if live[i].ID == id {
			return live[i], true
		}
	}
	return GroupEntity{}, false
}
