package hud

import (
	"strconv"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// InfoPanel (#195): the context-sensitive bottom-center info panel. Three modes
// driven by the current selection:
//
//	Single   one unit  -> a stat line (life/mana/armor/attack/level) read from
//	                      sim state (the caller supplies the values; the widget
//	                      never touches the sim).
//	Multi    >1 unit    -> a subgroup-aware selection grid; clicking a cell
//	                      re-emits a selection intent through the input pipeline.
//	Building producer   -> the production queue; a cancel button per slot emits
//	                      an OpCancel CommandRecord (building + slot).
//
// Pure state machine like ResourceBar/CommandCard: fed an InfoPanelState each
// refresh, dirty (one repaint) only when the selection or queue VERSION moves,
// so a steady selection costs nothing. Zero alloc steady state — cells/queue
// land in fixed arrays, the stat line formats into a fixed TextBuffer.
const (
	InfoGridCap  = 12 // selection icons shown (WC3 selection cap)
	InfoQueueCap = 8  // production queue slots shown
)

type InfoPanelMode uint8

const (
	InfoEmpty InfoPanelMode = iota
	InfoSingle
	InfoMulti
	InfoBuilding
)

// InfoPanelStrings are the localized stat labels (D-17). Short tokens keep the
// formatted line compact; the caller pulls them from the locale table.
type InfoPanelStrings struct {
	Life   string // e.g. "HP"
	Mana   string // e.g. "MP"
	Armor  string // e.g. "AR"
	Attack string // e.g. "AT"
	Level  string // e.g. "Lv"
}

type InfoUnitStats struct {
	Name                 string
	Level                int
	Life, MaxLife        int
	Mana, MaxMana        int
	Armor                int
	AttackMin, AttackMax int
}

// InfoSelCell is one multi-select grid cell. Subgroup keys the subgroup-aware
// highlight (same typeID = same subgroup) so the active subgroup stands out.
type InfoSelCell struct {
	ID       sim.EntityID `json:"id"`
	Icon     string       `json:"icon,omitempty"`
	Subgroup uint16       `json:"subgroup"`
}

// InfoQueueSlot is one production-queue entry. Progress (0-100) is non-zero only
// for the head slot that is actively training.
type InfoQueueSlot struct {
	Slot     int    `json:"slot"`
	Icon     string `json:"icon,omitempty"`
	Label    string `json:"label,omitempty"`
	Progress uint8  `json:"progress"`
}

// InfoPanelState is the fed snapshot. SelectionVersion/QueueVersion drive the
// dirty flag — bump them when the selection set or a queue changes.
type InfoPanelState struct {
	SelectionVersion uint32
	QueueVersion     uint32
	Mode             InfoPanelMode
	Stats            InfoUnitStats   // Single
	Cells            []InfoSelCell   // Multi (truncated to InfoGridCap)
	ActiveSubgroup   uint16          // Multi: highlighted subgroup
	Building         sim.EntityID    // Building: the producer
	Queue            []InfoQueueSlot // Building (truncated to InfoQueueCap)
}

type InfoPanelUpdate struct {
	Dirty    bool          `json:"dirty"`
	Repaints int           `json:"repaints"`
	Mode     InfoPanelMode `json:"mode"`
	Visible  bool          `json:"visible"`
	Cells    int           `json:"cells"`
	Queue    int           `json:"queue"`
}

// InfoCellClick is the result of clicking a multi-select grid cell: the unit to
// re-select and its subgroup (the caller turns this into a selection command).
type InfoCellClick struct {
	Accepted bool         `json:"accepted"`
	ID       sim.EntityID `json:"id"`
	Subgroup uint16       `json:"subgroup"`
}

type InfoPanel struct {
	Text   *TextBuffer
	Labels InfoPanelStrings

	player uint8
	seq    uint16

	mode        InfoPanelMode
	cells       [InfoGridCap]InfoSelCell
	cellCount   int
	activeSub   uint16
	queue       [InfoQueueCap]InfoQueueSlot
	queueCount  int
	building    sim.EntityID
	records     [InfoQueueCap]sim.CommandRecord
	recordCount int

	state       InfoPanelState
	initialized bool
}

func NewInfoPanel(text *TextBuffer, labels InfoPanelStrings, player uint8) InfoPanel {
	return InfoPanel{Text: text, Labels: labels, player: player}
}

// Cells returns the live multi-select grid (valid until next Update).
func (p *InfoPanel) Cells() []InfoSelCell { return p.cells[:p.cellCount] }

// Queue returns the live production queue (valid until next Update).
func (p *InfoPanel) Queue() []InfoQueueSlot { return p.queue[:p.queueCount] }

// Mode reports the current panel mode.
func (p *InfoPanel) Mode() InfoPanelMode { return p.mode }

// Update folds the latest selection snapshot. Dirty (one repaint) only when the
// mode or a version changed; otherwise a cheap no-op.
func (p *InfoPanel) Update(s InfoPanelState) InfoPanelUpdate {
	dirty := !p.initialized ||
		s.Mode != p.state.Mode ||
		s.SelectionVersion != p.state.SelectionVersion ||
		s.QueueVersion != p.state.QueueVersion
	if !dirty {
		return InfoPanelUpdate{Mode: p.mode, Visible: p.mode != InfoEmpty, Cells: p.cellCount, Queue: p.queueCount}
	}

	p.mode = s.Mode
	p.activeSub = s.ActiveSubgroup
	p.building = s.Building

	// Copy fed slices into the fixed arrays (bounded, no alloc).
	p.cellCount = 0
	for i := 0; i < len(s.Cells) && p.cellCount < InfoGridCap; i++ {
		p.cells[p.cellCount] = s.Cells[i]
		p.cellCount++
	}
	p.queueCount = 0
	for i := 0; i < len(s.Queue) && p.queueCount < InfoQueueCap; i++ {
		p.queue[p.queueCount] = s.Queue[i]
		p.queueCount++
	}

	p.setText(s)
	p.state = s
	p.initialized = true
	return InfoPanelUpdate{Dirty: true, Repaints: 1, Mode: p.mode, Visible: p.mode != InfoEmpty, Cells: p.cellCount, Queue: p.queueCount}
}

// setText renders the single-unit stat line into the fixed buffer (zero alloc).
// Other modes use the cell/queue models, not the text line, so the buffer is
// cleared.
func (p *InfoPanel) setText(s InfoPanelState) {
	if p.Text == nil {
		return
	}
	buf := p.Text.reset()
	if s.Mode == InfoSingle {
		buf = appendLabelled(buf, p.Labels.Life, s.Stats.Life, s.Stats.MaxLife)
		buf = appendLabelled(append(buf, ' ', ' '), p.Labels.Mana, s.Stats.Mana, s.Stats.MaxMana)
		buf = append(buf, ' ', ' ')
		buf = append(buf, p.Labels.Armor...)
		buf = append(buf, ' ')
		buf = strconv.AppendInt(buf, int64(s.Stats.Armor), 10)
		buf = append(buf, ' ', ' ')
		buf = append(buf, p.Labels.Attack...)
		buf = append(buf, ' ')
		buf = strconv.AppendInt(buf, int64(s.Stats.AttackMin), 10)
		buf = append(buf, '-')
		buf = strconv.AppendInt(buf, int64(s.Stats.AttackMax), 10)
		if s.Stats.Level > 0 {
			buf = append(buf, ' ', ' ')
			buf = append(buf, p.Labels.Level...)
			buf = append(buf, ' ')
			buf = strconv.AppendInt(buf, int64(s.Stats.Level), 10)
		}
	}
	p.Text.commit(buf)
}

// appendLabelled writes `LABEL cur/max` without allocating.
func appendLabelled(buf []byte, label string, cur, max int) []byte {
	buf = append(buf, label...)
	buf = append(buf, ' ')
	buf = strconv.AppendInt(buf, int64(cur), 10)
	buf = append(buf, '/')
	buf = strconv.AppendInt(buf, int64(max), 10)
	return buf
}

// ClickCell re-selects the unit at grid index i (Multi mode), reporting its
// subgroup so the caller can drive subgroup-aware selection.
func (p *InfoPanel) ClickCell(i int) InfoCellClick {
	if p.mode != InfoMulti || i < 0 || i >= p.cellCount {
		return InfoCellClick{}
	}
	c := p.cells[i]
	return InfoCellClick{Accepted: true, ID: c.ID, Subgroup: c.Subgroup}
}

// CancelSlot emits an OpCancel command for the producer's queue slot i (Building
// mode). The record carries the building in Units[0] and the slot in Data — the
// sim's CancelTrain(building, slot) contract.
func (p *InfoPanel) CancelSlot(i int) (sim.CommandRecord, bool) {
	if p.mode != InfoBuilding || i < 0 || i >= p.queueCount || p.recordCount >= InfoQueueCap {
		return sim.CommandRecord{}, false
	}
	r := sim.CommandRecord{
		Version:   sim.CommandVersion,
		Player:    p.player,
		Seq:       p.seq,
		Opcode:    sim.OpCancel,
		UnitCount: 1,
		Data:      uint16(p.queue[i].Slot),
	}
	r.Units[0] = p.building
	p.seq++
	p.records[p.recordCount] = r
	p.recordCount++
	return r, true
}

// Records returns the cancel command records emitted this frame.
func (p *InfoPanel) Records() []sim.CommandRecord { return p.records[:p.recordCount] }

// ClearRecords drops the emitted records once drained into the command stream.
func (p *InfoPanel) ClearRecords() { p.recordCount = 0 }
