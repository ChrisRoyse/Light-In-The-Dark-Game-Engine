package render

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/input"
)

// SelectionAckDriver is the HUD/input-layer bridge for the #313 ack category.
// Selection is client presentation state, not sim state, so it cannot ride the
// non-hashing sim render-event stream. The caller feeds it the local selection
// Result plus the selectable roster that produced that result.
type SelectionAckDriver struct {
	trig        *SoundTrigger
	codeOfType  map[uint16]string
	localPlayer uint8
}

// NewSelectionAckDriver maps input selection TypeID values to sound-set unit
// codes. The sound trigger still owns table lookup, domain routing, and ack
// cancel-previous behavior.
func NewSelectionAckDriver(trig *SoundTrigger, localPlayer uint8, codeOfType map[uint16]string) *SelectionAckDriver {
	m := make(map[uint16]string, len(codeOfType))
	for k, v := range codeOfType {
		m[k] = v
	}
	return &SelectionAckDriver{trig: trig, codeOfType: m, localPlayer: localPlayer}
}

// FireSelection routes one selection acknowledgement for the primary selected
// local-owned unit. Enemy, neutral, building-only, empty, or unmapped selections
// produce no sound.
func (d *SelectionAckDriver) FireSelection(res input.Result, items []input.Selectable, tick uint32) TriggerOutcome {
	if d == nil || d.trig == nil {
		return CueNoSet
	}
	it, ok := primaryAckSelectable(res.Selection, items, d.localPlayer)
	if !ok {
		return CueFiltered
	}
	code, ok := d.codeOfType[it.TypeID]
	if !ok || code == "" {
		return CueNoSet
	}
	return d.trig.Fire(AudioCue{
		Category: audio.CatAck,
		UnitType: code,
		Unit:     uint32(it.ID),
		Tick:     tick,
	})
}

func primaryAckSelectable(sel input.Selection, items []input.Selectable, localPlayer uint8) (input.Selectable, bool) {
	if sel.Count == 0 {
		return input.Selectable{}, false
	}
	if sel.ActiveSubgroupTypeID != 0 {
		if it, ok := selectedItemByType(sel, items, localPlayer, sel.ActiveSubgroupTypeID); ok {
			return it, true
		}
	}
	for i := 0; i < int(sel.Count); i++ {
		if it, ok := selectableByID(items, uint32(sel.IDs[i])); ok && ackSelectable(it, localPlayer) {
			return it, true
		}
	}
	return input.Selectable{}, false
}

func selectedItemByType(sel input.Selection, items []input.Selectable, localPlayer uint8, typeID uint16) (input.Selectable, bool) {
	for i := 0; i < int(sel.Count); i++ {
		it, ok := selectableByID(items, uint32(sel.IDs[i]))
		if ok && it.TypeID == typeID && ackSelectable(it, localPlayer) {
			return it, true
		}
	}
	return input.Selectable{}, false
}

func selectableByID(items []input.Selectable, id uint32) (input.Selectable, bool) {
	for i := range items {
		if uint32(items[i].ID) == id {
			return items[i], true
		}
	}
	return input.Selectable{}, false
}

func ackSelectable(it input.Selectable, localPlayer uint8) bool {
	return it.ID != 0 && it.OwnerPlayer == localPlayer && it.Class == input.SelectUnit
}
