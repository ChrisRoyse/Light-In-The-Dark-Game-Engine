package hud

import "strconv"

// GroupBar (#197): the control-group bar. One labelled badge per NON-EMPTY
// control group (0-9), mirroring the input layer's per-group live counts
// (input.ControlGroups.counts). Pure state machine like ResourceBar — feed it
// the counts each refresh; it rebuilds the ordered visible-badge list and the
// "group:count" text, marking dirty ONLY when the visible state changes (counts
// prune as members die, so an emptied group drops off the bar). Click routing
// (single = recall, double = recall + center camera) stays in the input
// pipeline; this widget owns layout + text, never commands. Zero alloc steady.
const GroupBarSlots = 10 // control groups 0-9 (input.ControlGroupCount)

type GroupBarState struct {
	Counts [GroupBarSlots]uint8 // live members per group; 0 = empty = hidden
}

// GroupBadge is one visible group's button model: the group/hotkey digit (0-9,
// the index IS the key) and its live unit count.
type GroupBadge struct {
	Group int   `json:"group"`
	Count uint8 `json:"count"`
}

type GroupBarUpdate struct {
	Dirty    bool `json:"dirty"`
	Repaints int  `json:"repaints"`
	Visible  int  `json:"visible"` // number of non-empty groups shown
}

type GroupBar struct {
	Text *TextBuffer

	badges      [GroupBarSlots]GroupBadge
	visible     int
	state       GroupBarState
	initialized bool
}

func NewGroupBar(text *TextBuffer) GroupBar { return GroupBar{Text: text} }

// Badges returns the ordered visible badges (groups ascending). Backed by the
// widget's fixed array — valid until the next Update; never allocates.
func (b *GroupBar) Badges() []GroupBadge { return b.badges[:b.visible] }

// Update folds the latest per-group counts. Dirty (one repaint) only when the
// visible composition changed; otherwise a cheap no-op carrying the count.
func (b *GroupBar) Update(s GroupBarState) GroupBarUpdate {
	if b.initialized && s.Counts == b.state.Counts {
		return GroupBarUpdate{Visible: b.visible}
	}
	n := 0
	for g := 0; g < GroupBarSlots; g++ {
		if s.Counts[g] == 0 {
			continue // emptied group: pruned off the bar
		}
		b.badges[n] = GroupBadge{Group: g, Count: s.Counts[g]}
		n++
	}
	b.visible = n
	b.setText(n)
	b.state = s
	b.initialized = true
	return GroupBarUpdate{Dirty: true, Repaints: 1, Visible: n}
}

// setText writes "g:count" badges joined by two spaces into the fixed buffer,
// without allocating (strconv.AppendInt into the reset slice).
func (b *GroupBar) setText(n int) {
	if b.Text == nil {
		return
	}
	buf := b.Text.reset()
	for i := 0; i < n; i++ {
		if i > 0 {
			buf = append(buf, ' ', ' ')
		}
		buf = strconv.AppendInt(buf, int64(b.badges[i].Group), 10)
		buf = append(buf, ':')
		buf = strconv.AppendInt(buf, int64(b.badges[i].Count), 10)
	}
	b.Text.commit(buf)
}
