package hud

// Portrait panel (#193, ui-and-hud.md §2-3). The animated portrait of the primary
// selected unit. Like the other HUD widgets this is a pure-logic state machine: it
// folds a per-frame selection snapshot and decides WHAT the portrait row shows —
// the clip to play, the fallback icon, the name/level labels — while the renderer
// drives the actual offscreen render-to-texture pass (litd/render.PortraitTarget,
// the verified RTT primitive) from this decision. No GL, no allocs at steady state,
// headless-testable.
//
// The render budget cares about the offscreen pass, so the panel reports RTTPass:
// it is true ONLY in the Animated mode. An empty selection or a model that lacks a
// Portrait clip (R-AST-3) runs no offscreen pass at all — the fallback is a static
// icon, never a T-pose render of an un-animated head.

import (
	"strconv"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// PortraitMode is the portrait row's display state.
type PortraitMode uint8

const (
	// PortraitEmpty: nothing selected — the row is blank and NO offscreen pass runs.
	PortraitEmpty PortraitMode = iota
	// PortraitAnimated: the selected unit's head model plays its Portrait clip in
	// the offscreen pass (RTTPass true).
	PortraitAnimated
	// PortraitFallback: the model has no Portrait clip — a static icon is shown and
	// NO offscreen pass runs (never a T-pose).
	PortraitFallback
)

// PortraitStrings are the localized label fragments (locale/byte-buffer rules).
type PortraitStrings struct {
	Level string // e.g. "Lv"
}

// PortraitState is the per-frame selection snapshot fed to the panel. The caller
// resolves the primary subgroup member (and, on its death, advances to the next
// member deterministically) before feeding it here — the panel simply reflects the
// entity it is given, so death/sub-advance is a pure entity change.
type PortraitState struct {
	SelectionVersion uint32       // bumps whenever the selection or active subgroup changes
	Entity           sim.EntityID // primary portrait subject; 0 == empty selection
	Name             string       // display name (empty allowed)
	Level            int          // hero level; <=0 suppresses the level label
	HasPortraitClip  bool         // R-AST-3: model exposes a Portrait clip
	Alive            bool         // false drops to empty (dead and no successor fed)
}

// PortraitUpdate is the per-frame decision the renderer consumes.
type PortraitUpdate struct {
	Dirty       bool         `json:"dirty"`       // mode/subject changed → one repaint
	Repaints    int          `json:"repaints"`    // 1 when Dirty, else 0
	Mode        PortraitMode `json:"mode"`        // resolved display state
	Visible     bool         `json:"visible"`     // row shows anything (Animated or Fallback)
	RTTPass     bool         `json:"rttPass"`     // the offscreen render-to-texture pass runs this frame
	Fallback    bool         `json:"fallback"`    // static-icon fallback (no Portrait clip)
	ClipPlaying bool         `json:"clipPlaying"` // the Portrait clip advances (Animated only)
	Entity      sim.EntityID `json:"entity"`      // the resolved subject (0 when empty)
}

// PortraitPanel is the portrait-row widget. Construct with NewPortraitPanel.
type PortraitPanel struct {
	Text   *TextBuffer
	Labels PortraitStrings

	mode   PortraitMode
	entity sim.EntityID

	state       PortraitState
	initialized bool
}

// NewPortraitPanel returns a portrait panel writing its name/level labels into the
// supplied TextBuffer (may be nil if the caller renders labels itself).
func NewPortraitPanel(text *TextBuffer, labels PortraitStrings) PortraitPanel {
	return PortraitPanel{Text: text, Labels: labels}
}

// Mode reports the current display state. Entity reports the current subject.
func (p *PortraitPanel) Mode() PortraitMode    { return p.mode }
func (p *PortraitPanel) Entity() sim.EntityID  { return p.entity }

// resolveMode maps a snapshot to a display state.
func resolvePortraitMode(s PortraitState) PortraitMode {
	if s.Entity == 0 || !s.Alive {
		return PortraitEmpty
	}
	if s.HasPortraitClip {
		return PortraitAnimated
	}
	return PortraitFallback
}

// Update folds the latest selection snapshot. It repaints (Dirty) only when the
// resolved mode or the subject entity changed — a steady selection playing its clip
// is a cheap no-op that still reports RTTPass so the offscreen pass keeps animating.
func (p *PortraitPanel) Update(s PortraitState) PortraitUpdate {
	mode := resolvePortraitMode(s)
	dirty := !p.initialized ||
		s.SelectionVersion != p.state.SelectionVersion ||
		s.Entity != p.entity ||
		mode != p.mode

	if dirty {
		p.mode = mode
		p.entity = s.Entity
		if mode == PortraitEmpty {
			p.entity = 0
		}
		p.setLabels(s, mode)
		p.state = s
		p.initialized = true
	}

	return PortraitUpdate{
		Dirty:       dirty,
		Repaints:    boolToInt(dirty),
		Mode:        p.mode,
		Visible:     p.mode != PortraitEmpty,
		RTTPass:     p.mode == PortraitAnimated,
		Fallback:    p.mode == PortraitFallback,
		ClipPlaying: p.mode == PortraitAnimated,
		Entity:      p.entity,
	}
}

// setLabels renders the name + optional level into the fixed buffer (zero alloc).
// Cleared on the empty state.
func (p *PortraitPanel) setLabels(s PortraitState, mode PortraitMode) {
	if p.Text == nil {
		return
	}
	buf := p.Text.reset()
	if mode != PortraitEmpty {
		buf = append(buf, s.Name...)
		if s.Level > 0 {
			buf = append(buf, ' ', ' ')
			buf = append(buf, p.Labels.Level...)
			buf = append(buf, ' ')
			buf = strconv.AppendInt(buf, int64(s.Level), 10)
		}
	}
	p.Text.commit(buf)
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}
