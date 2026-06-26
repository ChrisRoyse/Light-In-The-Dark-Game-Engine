package hud

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/BurntSushi/toml"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const (
	CommandCardCols       = 4
	CommandCardRows       = 3
	CommandCardSlots      = CommandCardCols * CommandCardRows
	CommandCardSubgroups  = 4
	CommandCardMaxRecords = 16
	commandCardTextBytes  = 256
)

const DefaultCommandCardPath = "hud/command-card.toml"

type CommandCardTable struct {
	Path        string
	GridHotkeys [CommandCardSlots]string
	Groups      []CommandCardGroup
}

type CommandCardGroup struct {
	ID       string
	LabelKey string
	Slots    [CommandCardSlots]CommandDefinition
}

type CommandDefinition struct {
	ID             string
	Row            uint8
	Col            uint8
	Index          uint8
	OpcodeName     string
	Opcode         uint8
	Data           uint16
	Icon           string
	LabelKey       string
	TooltipKey     string
	RequiresTarget bool
	GoldCost       int
	LumberCost     int
}

type CommandCardState struct {
	Player              uint8
	OwnSelection        bool
	SelectionLabel      string
	Subgroups           [CommandCardSubgroups]string
	SubgroupCount       uint8
	ActiveSubgroupIndex uint8
	Units               [sim.MaxCommandUnits]sim.EntityID
	UnitCount           uint8
	Gold                int
	Lumber              int
	Cooldown            [CommandCardSlots]uint16
}

type CommandCardSlotState struct {
	Visible        bool   `json:"visible"`
	Enabled        bool   `json:"enabled"`
	DisabledReason string `json:"disabledReason,omitempty"`
	Index          uint8  `json:"index"`
	Row            uint8  `json:"row"`
	Col            uint8  `json:"col"`
	ID             string `json:"id,omitempty"`
	Opcode         uint8  `json:"opcode"`
	OpcodeName     string `json:"opcodeName,omitempty"`
	Data           uint16 `json:"data,omitempty"`
	Icon           string `json:"icon,omitempty"`
	Hotkey         string `json:"hotkey,omitempty"`
	Label          string `json:"label,omitempty"`
	Tooltip        string `json:"tooltip,omitempty"`
	RequiresTarget bool   `json:"requiresTarget,omitempty"`
}

type CommandCardUpdate struct {
	DirtySlots int  `json:"dirtySlots"`
	Repaints   int  `json:"repaints"`
	Visible    bool `json:"visible"`
}

type CommandCardClick struct {
	Accepted      bool   `json:"accepted"`
	PendingTarget bool   `json:"pendingTarget"`
	Slot          uint8  `json:"slot"`
	CommandID     string `json:"commandID,omitempty"`
	Reason        string `json:"reason,omitempty"`
}

type CommandTextBuffer struct {
	buf [commandCardTextBytes]byte
	n   int
}

func (b *CommandTextBuffer) Bytes() []byte  { return b.buf[:b.n] }
func (b *CommandTextBuffer) String() string { return string(b.Bytes()) }
func (b *CommandTextBuffer) reset() []byte {
	b.n = 0
	return b.buf[:0]
}
func (b *CommandTextBuffer) commit(p []byte) { b.n = len(p) }

type CommandCard struct {
	table          *CommandCardTable
	locale         *locale.Table
	Slots          [CommandCardSlots]CommandCardSlotState
	Summary        CommandTextBuffer
	Visible        bool
	SelectionLabel string
	ActiveSubgroup string

	player      uint8
	units       [sim.MaxCommandUnits]sim.EntityID
	unitCount   uint8
	pending     int8
	nextSeq     uint16
	records     [CommandCardMaxRecords]sim.CommandRecord
	recordCount int
}

type rawCommandCardTable struct {
	GridHotkeys []string       `toml:"grid-hotkeys"`
	Groups      []rawCardGroup `toml:"group"`
}

type rawCardGroup struct {
	ID       string        `toml:"id"`
	LabelKey string        `toml:"label-key"`
	Slots    []rawCardSlot `toml:"slot"`
}

type rawCardSlot struct {
	Row            int    `toml:"row"`
	Col            int    `toml:"col"`
	ID             string `toml:"id"`
	Opcode         string `toml:"opcode"`
	Data           uint16 `toml:"data"`
	Icon           string `toml:"icon"`
	LabelKey       string `toml:"label-key"`
	TooltipKey     string `toml:"tooltip-key"`
	RequiresTarget bool   `toml:"requires-target"`
	GoldCost       int    `toml:"gold-cost"`
	LumberCost     int    `toml:"lumber-cost"`
}

func LoadCommandCardTable(fsys fs.FS) (*CommandCardTable, error) {
	return ReadCommandCardTable(fsys, DefaultCommandCardPath)
}

func ReadCommandCardTable(fsys fs.FS, file string) (*CommandCardTable, error) {
	blob, err := fs.ReadFile(fsys, file)
	if err != nil {
		return nil, fmt.Errorf("commandcard: read %s: %w", file, err)
	}
	var raw rawCommandCardTable
	md, err := toml.Decode(string(blob), &raw)
	if err != nil {
		return nil, fmt.Errorf("commandcard: %s: %w", file, err)
	}
	for _, un := range md.Undecoded() {
		return nil, fmt.Errorf("commandcard: %s: unknown field %q", file, un.String())
	}
	table, err := compileCommandCardTable(file, raw)
	if err != nil {
		return nil, err
	}
	return table, nil
}

func compileCommandCardTable(file string, raw rawCommandCardTable) (*CommandCardTable, error) {
	if len(raw.GridHotkeys) != CommandCardSlots {
		return nil, fmt.Errorf("commandcard: %s: grid-hotkeys must contain %d entries", file, CommandCardSlots)
	}
	out := &CommandCardTable{Path: file}
	for i, key := range raw.GridHotkeys {
		if key == "" {
			return nil, fmt.Errorf("commandcard: %s: grid-hotkeys[%d] is empty", file, i)
		}
		out.GridHotkeys[i] = key
	}
	if len(raw.Groups) == 0 {
		return nil, fmt.Errorf("commandcard: %s: at least one [[group]] is required", file)
	}
	seenGroups := map[string]bool{}
	for _, rg := range raw.Groups {
		if rg.ID == "" {
			return nil, fmt.Errorf("commandcard: %s: group id is empty", file)
		}
		if seenGroups[rg.ID] {
			return nil, fmt.Errorf("commandcard: %s: duplicate group %q", file, rg.ID)
		}
		seenGroups[rg.ID] = true
		if rg.LabelKey == "" {
			return nil, fmt.Errorf("commandcard: %s: group %q missing label-key", file, rg.ID)
		}
		var group CommandCardGroup
		group.ID = rg.ID
		group.LabelKey = rg.LabelKey
		seenSlots := [CommandCardSlots]bool{}
		for _, rs := range rg.Slots {
			def, err := compileCommandSlot(file, rg.ID, rs)
			if err != nil {
				return nil, err
			}
			if seenSlots[def.Index] {
				return nil, fmt.Errorf("commandcard: %s: group %q duplicate slot row=%d col=%d", file, rg.ID, def.Row, def.Col)
			}
			seenSlots[def.Index] = true
			group.Slots[def.Index] = def
		}
		out.Groups = append(out.Groups, group)
	}
	return out, nil
}

func compileCommandSlot(file, group string, raw rawCardSlot) (CommandDefinition, error) {
	if raw.Row < 0 || raw.Row >= CommandCardRows || raw.Col < 0 || raw.Col >= CommandCardCols {
		return CommandDefinition{}, fmt.Errorf("commandcard: %s: group %q slot %q outside 4x3 grid row=%d col=%d", file, group, raw.ID, raw.Row, raw.Col)
	}
	if raw.ID == "" || raw.Icon == "" || raw.LabelKey == "" || raw.TooltipKey == "" {
		return CommandDefinition{}, fmt.Errorf("commandcard: %s: group %q slot at row=%d col=%d missing id/icon/locale key", file, group, raw.Row, raw.Col)
	}
	op, ok := commandOpcode(raw.Opcode)
	if !ok {
		return CommandDefinition{}, fmt.Errorf("commandcard: %s: group %q slot %q unknown opcode %q", file, group, raw.ID, raw.Opcode)
	}
	if raw.GoldCost < 0 || raw.LumberCost < 0 {
		return CommandDefinition{}, fmt.Errorf("commandcard: %s: group %q slot %q has negative cost", file, group, raw.ID)
	}
	idx := raw.Row*CommandCardCols + raw.Col
	return CommandDefinition{
		ID:             raw.ID,
		Row:            uint8(raw.Row),
		Col:            uint8(raw.Col),
		Index:          uint8(idx),
		OpcodeName:     raw.Opcode,
		Opcode:         op,
		Data:           raw.Data,
		Icon:           raw.Icon,
		LabelKey:       raw.LabelKey,
		TooltipKey:     raw.TooltipKey,
		RequiresTarget: raw.RequiresTarget,
		GoldCost:       raw.GoldCost,
		LumberCost:     raw.LumberCost,
	}, nil
}

func commandOpcode(name string) (uint8, bool) {
	switch name {
	case "move":
		return sim.OpMove, true
	case "attack":
		return sim.OpAttack, true
	case "stop":
		return sim.OpStop, true
	case "hold":
		return sim.OpHold, true
	case "patrol":
		return sim.OpPatrol, true
	case "cast":
		return sim.OpCastAbility, true
	case "train":
		return sim.OpTrain, true
	case "rally":
		return sim.OpRally, true
	default:
		return 0, false
	}
}

func (t *CommandCardTable) Group(id string) (CommandCardGroup, bool) {
	if t == nil {
		return CommandCardGroup{}, false
	}
	for _, g := range t.Groups {
		if g.ID == id {
			return g, true
		}
	}
	return CommandCardGroup{}, false
}

func (t *CommandCardTable) LocaleKeys() []string {
	if t == nil {
		return nil
	}
	keys := make([]string, 0, len(t.Groups)*(1+CommandCardSlots*2))
	seen := map[string]bool{}
	add := func(key string) {
		if key != "" && !seen[key] {
			seen[key] = true
			keys = append(keys, key)
		}
	}
	for _, group := range t.Groups {
		add(group.LabelKey)
		for _, slot := range group.Slots {
			if slot.ID == "" {
				continue
			}
			add(slot.LabelKey)
			add(slot.TooltipKey)
		}
	}
	sort.Strings(keys)
	return keys
}

func NewCommandCard(table *CommandCardTable, strings *locale.Table) CommandCard {
	return CommandCard{table: table, locale: strings, pending: -1}
}

func (c *CommandCard) Refresh(state CommandCardState) CommandCardUpdate {
	prev := c.Slots
	prevVisible := c.Visible
	c.SelectionLabel = state.SelectionLabel
	c.ActiveSubgroup = ""
	c.Visible = false
	c.player = state.Player
	c.unitCount = state.UnitCount
	c.units = state.Units
	c.pending = -1
	for i := range c.Slots {
		c.Slots[i] = CommandCardSlotState{Index: uint8(i), Row: uint8(i / CommandCardCols), Col: uint8(i % CommandCardCols)}
	}

	if state.OwnSelection && state.UnitCount > 0 && state.SubgroupCount > 0 {
		idx := state.ActiveSubgroupIndex
		if idx >= state.SubgroupCount {
			idx = 0
		}
		groupID := state.Subgroups[idx]
		if group, ok := c.table.Group(groupID); ok {
			c.Visible = true
			c.ActiveSubgroup = group.ID
			c.fillSlots(group, state)
		}
	}
	c.buildSummary()

	var dirty int
	for i := range c.Slots {
		if c.Slots[i] != prev[i] {
			dirty++
		}
	}
	if c.Visible != prevVisible {
		dirty++
	}
	return CommandCardUpdate{DirtySlots: dirty, Repaints: dirty, Visible: c.Visible}
}

func (c *CommandCard) fillSlots(group CommandCardGroup, state CommandCardState) {
	for i, def := range group.Slots {
		if def.ID == "" {
			continue
		}
		slot := CommandCardSlotState{
			Visible:        true,
			Enabled:        true,
			Index:          uint8(i),
			Row:            def.Row,
			Col:            def.Col,
			ID:             def.ID,
			Opcode:         def.Opcode,
			OpcodeName:     def.OpcodeName,
			Data:           def.Data,
			Icon:           def.Icon,
			Hotkey:         c.table.GridHotkeys[i],
			Label:          c.locale.Must(locale.Key(def.LabelKey)),
			Tooltip:        c.locale.Must(locale.Key(def.TooltipKey)),
			RequiresTarget: def.RequiresTarget,
		}
		if state.Cooldown[i] > 0 {
			slot.Enabled = false
			slot.DisabledReason = "cooldown"
		} else if state.Gold < def.GoldCost || state.Lumber < def.LumberCost {
			slot.Enabled = false
			slot.DisabledReason = "unaffordable"
		}
		c.Slots[i] = slot
	}
}

func (c *CommandCard) buildSummary() {
	p := c.Summary.reset()
	if !c.Visible {
		c.Summary.commit(p)
		return
	}
	first := true
	for i := range c.Slots {
		slot := c.Slots[i]
		if !slot.Visible {
			continue
		}
		if !first {
			p = appendCommandByte(p, ' ')
		}
		first = false
		p = appendCommandText(p, slot.Hotkey)
		p = appendCommandByte(p, ':')
		if !slot.Enabled {
			p = appendCommandByte(p, '!')
		}
		p = appendCommandText(p, slot.Label)
	}
	c.Summary.commit(p)
}

func appendCommandByte(p []byte, b byte) []byte {
	if len(p) == cap(p) {
		return p
	}
	return append(p, b)
}

func appendCommandText(p []byte, text string) []byte {
	available := cap(p) - len(p)
	if available <= 0 {
		return p
	}
	if len(text) > available {
		text = text[:available]
	}
	return append(p, text...)
}

func (c *CommandCard) ClickSlot(index uint8, queued bool) CommandCardClick {
	if index >= CommandCardSlots {
		return CommandCardClick{Slot: index, Reason: "slot out of range"}
	}
	slot := c.Slots[index]
	if !c.Visible || !slot.Visible {
		return CommandCardClick{Slot: index, Reason: "slot empty"}
	}
	if !slot.Enabled {
		return CommandCardClick{Slot: index, CommandID: slot.ID, Reason: slot.DisabledReason}
	}
	if slot.RequiresTarget {
		c.pending = int8(index)
		return CommandCardClick{Accepted: true, PendingTarget: true, Slot: index, CommandID: slot.ID}
	}
	_, ok := c.emit(slot, fixed.Vec2{}, 0, queued)
	return CommandCardClick{Accepted: ok, Slot: index, CommandID: slot.ID}
}

func (c *CommandCard) ConfirmTarget(point fixed.Vec2, target sim.EntityID, queued bool) (sim.CommandRecord, bool) {
	if c.pending < 0 {
		return sim.CommandRecord{}, false
	}
	slot := c.Slots[c.pending]
	c.pending = -1
	if !slot.Visible || !slot.Enabled {
		return sim.CommandRecord{}, false
	}
	return c.emit(slot, point, target, queued)
}

func (c *CommandCard) emit(slot CommandCardSlotState, point fixed.Vec2, target sim.EntityID, queued bool) (sim.CommandRecord, bool) {
	if c.unitCount == 0 || c.recordCount >= CommandCardMaxRecords {
		return sim.CommandRecord{}, false
	}
	r := sim.CommandRecord{
		Version:   sim.CommandVersion,
		Player:    c.player,
		Seq:       c.nextSeq,
		Opcode:    slot.Opcode,
		UnitCount: c.unitCount,
		Target:    target,
		Point:     point,
		Data:      slot.Data,
	}
	if queued {
		r.Flags = sim.CmdFlagQueued
	}
	for i := uint8(0); i < c.unitCount; i++ {
		r.Units[i] = c.units[i]
	}
	c.nextSeq++
	c.records[c.recordCount] = r
	c.recordCount++
	return r, true
}

func (c *CommandCard) Records() []sim.CommandRecord {
	return c.records[:c.recordCount]
}

func CycleCommandSubgroup(state *CommandCardState) bool {
	if state.SubgroupCount < 2 {
		return false
	}
	state.ActiveSubgroupIndex = (state.ActiveSubgroupIndex + 1) % state.SubgroupCount
	return true
}
