package data

// Smart-order resolution table (combat-and-orders.md §2.2, input.md
// §4): right-click resolution is a PURE function of (target class ×
// unit capability class) → one concrete order opcode. The table is
// data (R-AST-1); the loader validates it against the closed v1
// opcode-name registry of input.md §8 — an unknown order name or a
// missing row is a LOAD error, never a runtime fallback. The sim
// never re-runs smart resolution: the resolved opcode is what enters
// the command stream, the replay, and the state hash.

import (
	"fmt"
	"io/fs"
)

// OpcodeByName is the input.md §8 v1 wire registry (frozen per
// encoding version). litd/sim cross-checks these values against its
// Op* constants in a test — the two lists must never drift.
var OpcodeByName = map[string]uint8{
	"move":         0,
	"attack":       1,
	"stop":         2,
	"hold":         3,
	"patrol":       4,
	"cast-ability": 5,
	"train":        6,
	"build":        7,
	"cancel":       8,
	"rally":        9,
	"harvest":      10,
	"repair":       11,
	"board":        12,
	"unload":       13,
}

// Target classes — the row vocabulary, fixed in code (the classifier
// lives in sim state; the table only maps classes to orders).
const (
	TCGroundPoint uint8 = iota
	TCEnemy
	TCAlly
	TCOwnBuilding
	TCResource
	TCItem
	TargetClassCount
)

// TargetClassNames index the rows of SmartTable.Rules.
var TargetClassNames = [TargetClassCount]string{
	"ground-point", "enemy", "ally", "own-building", "resource", "item",
}

// SmartTable is the loaded resolution table.
type SmartTable struct {
	UnitClasses []string  // capability-class names, column order
	Rules       [][]uint8 // [targetClass][unitClass] = opcode
}

type rawSmartTable struct {
	UnitClasses []string            `toml:"unit-classes" json:"unit-classes"`
	Rules       map[string][]string `toml:"rules" json:"rules"`
}

// ClassIndex returns the column of a unit-class name, or -1.
func (s *SmartTable) ClassIndex(name string) int {
	return indexOf(s.UnitClasses, name)
}

// LoadSmart reads orders/smart.{toml|json} from fsys. Every target
// class must be mapped for every unit class; every order name must be
// in the v1 registry.
func LoadSmart(fsys fs.FS) (*SmartTable, error) {
	file, blob, err := readOne(fsys, "orders", "smart")
	if err != nil {
		return nil, err
	}
	var raw rawSmartTable
	if err := decodeStrict(file, blob, &raw); err != nil {
		return nil, err
	}
	if len(raw.UnitClasses) == 0 {
		return nil, fmt.Errorf("data: %s: unit-classes must be non-empty", file)
	}
	seen := map[string]bool{}
	for _, c := range raw.UnitClasses {
		if seen[c] {
			return nil, fmt.Errorf("data: %s: duplicate unit class %q", file, c)
		}
		seen[c] = true
	}
	if len(raw.Rules) != int(TargetClassCount) {
		return nil, fmt.Errorf("data: %s: rules has %d rows, want one per target class (%d: %v)",
			file, len(raw.Rules), TargetClassCount, TargetClassNames)
	}
	t := &SmartTable{
		UnitClasses: raw.UnitClasses,
		Rules:       make([][]uint8, TargetClassCount),
	}
	for tc, tcName := range TargetClassNames { // fixed order: deterministic
		row, ok := raw.Rules[tcName]
		if !ok {
			return nil, fmt.Errorf("data: %s: rules missing target class %q", file, tcName)
		}
		if len(row) != len(raw.UnitClasses) {
			return nil, fmt.Errorf("data: %s: row %q has %d orders, want %d (one per unit class %v)",
				file, tcName, len(row), len(raw.UnitClasses), raw.UnitClasses)
		}
		t.Rules[tc] = make([]uint8, len(row))
		for uc, name := range row {
			op, ok := OpcodeByName[name]
			if !ok {
				return nil, fmt.Errorf("data: %s: row %q unit class %q: order %q is not in the v1 opcode registry (input.md §8)",
					file, tcName, raw.UnitClasses[uc], name)
			}
			t.Rules[tc][uc] = op
		}
	}
	return t, nil
}

// hashInto folds the table into a fingerprint stream.
func (s *SmartTable) hashInto(h interface {
	WriteU8(uint8)
	WriteU32(uint32)
	WriteBytes([]byte)
}) {
	h.WriteU32(uint32(len(s.UnitClasses)))
	for _, c := range s.UnitClasses {
		h.WriteU32(uint32(len(c)))
		h.WriteBytes([]byte(c))
	}
	for _, row := range s.Rules {
		for _, op := range row {
			h.WriteU8(op)
		}
	}
}
