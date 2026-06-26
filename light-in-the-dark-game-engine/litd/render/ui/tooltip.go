// Package ui holds presentation-only HUD overlays that react to game state and
// the published render-event stream — tooltips, minimap pings, screen-edge
// alerts (#315). They never touch the sim hashing path: they read fed state and
// emit input-pipeline intents (camera jumps, selection), nothing more.
package ui

import "strconv"

// Tooltip (#315) is the single pooled hover tooltip. Hovering one target (a
// command-card slot, item, or build button — keyed by an opaque uint64 the
// caller supplies) for at least DelayMS shows it; moving off or onto a new
// target hides/restarts it; only ONE tooltip is ever live. Content is fed
// (localized title + a cost line + a requirement line shown greyed when the
// entry is tech-gated). Pure state machine: fed (key, content, nowMS) each
// frame; zero alloc steady state (lines format into fixed buffers).
type Tooltip struct {
	DelayMS uint32

	Title TextBuffer // localized name
	Cost  TextBuffer // "G g  W w  F f" (omitted parts are zero-skipped)
	Req   TextBuffer // requirement reason, only when gated

	Labels TooltipStrings

	target  uint64
	since   uint32
	visible bool
	gated   bool
}

// TooltipStrings are the localized cost tokens (D-17).
type TooltipStrings struct {
	Gold string // "G"
	Wood string // "W"
	Food string // "F"
}

// TooltipContent is the fed payload for the currently hovered target.
type TooltipContent struct {
	Title       string
	Gold        int
	Wood        int
	Food        int
	Gated       bool   // requirement unmet -> Req line shown greyed
	Requirement string // reason text, shown only when Gated
}

// TooltipUpdate is the per-frame result.
type TooltipUpdate struct {
	Visible bool `json:"visible"`
	Shown   bool `json:"shown"` // true on the frame it first appears
	Gated   bool `json:"gated"` // requirement line is greyed
}

func NewTooltip(delayMS uint32, labels TooltipStrings) Tooltip {
	return Tooltip{DelayMS: delayMS, Labels: labels}
}

// Update folds the current hover. key==0 means "hovering nothing" → hidden.
// A new key restarts the delay timer; the same key past DelayMS reveals the
// tooltip and paints its lines once.
func (t *Tooltip) Update(key uint64, content TooltipContent, nowMS uint32) TooltipUpdate {
	if key == 0 {
		t.target, t.visible = 0, false
		return TooltipUpdate{}
	}
	if key != t.target {
		// Moved onto a new target: restart the hover timer. Fall through to the
		// delay check so a zero delay reveals on this same frame.
		t.target, t.since, t.visible = key, nowMS, false
	}
	if t.visible {
		return TooltipUpdate{Visible: true, Gated: t.gated}
	}
	if nowMS-t.since < t.DelayMS {
		return TooltipUpdate{}
	}
	// Delay elapsed on a stable hover: reveal + paint the lines once.
	t.visible = true
	t.gated = content.Gated
	t.paint(content)
	return TooltipUpdate{Visible: true, Shown: true, Gated: content.Gated}
}

func (t *Tooltip) paint(c TooltipContent) {
	tb := t.Title.reset()
	tb = append(tb, c.Title...)
	t.Title.commit(tb)

	cb := t.Cost.reset()
	cb = appendCost(cb, t.Labels.Gold, c.Gold, false)
	cb = appendCost(cb, t.Labels.Wood, c.Wood, len(cb) > 0)
	cb = appendCost(cb, t.Labels.Food, c.Food, len(cb) > 0)
	t.Cost.commit(cb)

	rb := t.Req.reset()
	if c.Gated {
		rb = append(rb, c.Requirement...)
	}
	t.Req.commit(rb)
}

// appendCost writes "LABEL n" for a non-zero amount, with a two-space separator
// when something precedes it. Zero amounts are skipped (no clutter).
func appendCost(buf []byte, label string, amount int, sep bool) []byte {
	if amount == 0 {
		return buf
	}
	if sep {
		buf = append(buf, ' ', ' ')
	}
	buf = append(buf, label...)
	buf = append(buf, ' ')
	buf = strconv.AppendInt(buf, int64(amount), 10)
	return buf
}
