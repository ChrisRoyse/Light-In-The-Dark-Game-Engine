package sim

// On-demand state dump (#203, R-FSV-2): the full authoritative state
// as JSON, readable by an agent doing Full State Verification. The
// dump path allocates freely (R-GC-3: it is off the steady-state
// gate) but NEVER mutates the world — hashing before and after a dump
// is bit-identical, proven by TestDumpReadOnly.

import (
	"encoding/json"
	"fmt"
	"io"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// dumpFixed renders a 32.32 value as both raw bits (the authoritative
// form) and a human decimal string (derived, for reading). The
// decimal is built with integer math only — floats are banned from
// this package (#335, hazard §2.3-1), and a string avoids any
// JSON-encoder rounding drift.
type dumpFixed struct {
	Raw int64  `json:"raw"`
	Dec string `json:"dec"`
}

func df(v fixed.F64) dumpFixed {
	return dumpFixed{Raw: int64(v), Dec: fixedDecString(int64(v))}
}

// fixedDecString formats a raw 32.32 value with six fractional
// digits, round-half-up, pure integer math.
func fixedDecString(raw int64) string {
	u := uint64(raw)
	if raw < 0 {
		u = uint64(-raw) // two's complement: correct even for MinInt64
	}
	ip := u >> 32
	dec := ((u&0xFFFFFFFF)*1_000_000 + (1 << 31)) >> 32
	if dec == 1_000_000 {
		ip++
		dec = 0
	}
	if raw < 0 {
		return fmt.Sprintf("-%d.%06d", ip, dec)
	}
	return fmt.Sprintf("%d.%06d", ip, dec)
}

type dumpEntity struct {
	ID     uint32    `json:"id"`
	Index  uint32    `json:"index"`
	Gen    uint8     `json:"gen"`
	Kind   string    `json:"kind"` // unit | missile
	PosX   dumpFixed `json:"posX"`
	PosY   dumpFixed `json:"posY"`
	Facing uint16    `json:"facing"`

	Life       *dumpFixed `json:"life,omitempty"`
	MaxLife    *dumpFixed `json:"maxLife,omitempty"`
	ArmorValue *int16     `json:"armorValue,omitempty"`
	ArmorType  *uint8     `json:"armorType,omitempty"`

	Player *uint8 `json:"player,omitempty"`
	Team   *uint8 `json:"team,omitempty"`

	OrderKind   *uint8  `json:"orderKind,omitempty"`
	OrderTarget *uint32 `json:"orderTarget,omitempty"`
	OrderData   *uint16 `json:"orderData,omitempty"`
	QueueDepth  *int    `json:"queueDepth,omitempty"`

	Mana *dumpFixed `json:"mana,omitempty"`
}

type dumpBuff struct {
	Slot           int32  `json:"slot"`
	BuffID         uint16 `json:"buffId"`
	Stacks         uint8  `json:"stacks"`
	Flags          uint8  `json:"flags"`
	Target         uint32 `json:"target"`
	Source         uint32 `json:"source"`
	RemainingTicks uint32 `json:"remainingTicks"`
	PeriodicClock  uint32 `json:"periodicClock"`
}

type dumpState struct {
	Tick       uint32            `json:"tick"`
	Hash       string            `json:"hash"` // top hash, hex
	Subs       map[string]string `json:"subs"`
	PRNGState  uint64            `json:"prngState"`
	PRNGInc    uint64            `json:"prngInc"`
	UnitCount  int               `json:"unitCount"`
	BuffsLive  int               `json:"buffsLive"`
	EventsDrop uint64            `json:"eventsDropped"`
	DmgDrop    uint32            `json:"damageDropped"`
	Entities   []dumpEntity      `json:"entities"`
	Buffs      []dumpBuff        `json:"buffs"`
}

// DumpState writes the full state JSON to wr. Read-only; returns the
// encoder error if any. The hash inside the dump is computed from the
// same state the dump describes.
func (w *World) DumpState(wr io.Writer) error {
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)

	d := dumpState{
		Tick:       w.tick,
		Hash:       fmt.Sprintf("%016x", snap.Top),
		Subs:       make(map[string]string, len(HashSystems)),
		UnitCount:  w.unitCount,
		BuffsLive:  w.Buffs.Live(),
		EventsDrop: w.eventsDropped,
		DmgDrop:    w.dmgDropped,
	}
	cur := w.rng.Cursor()
	d.PRNGState, d.PRNGInc = cur.State, cur.Inc
	for i, n := range HashSystems {
		d.Subs[n] = fmt.Sprintf("%016x", snap.Subs[i])
	}

	// every entity holds a Transform; row order is the canonical order
	for i := int32(0); i < w.Transforms.Count(); i++ {
		id := w.Transforms.Entity[i]
		e := dumpEntity{
			ID:     uint32(id),
			Index:  id.Index(),
			Gen:    id.Generation(),
			Kind:   "unit",
			PosX:   df(w.Transforms.Pos[i].X),
			PosY:   df(w.Transforms.Pos[i].Y),
			Facing: uint16(w.Transforms.Facing[i]),
		}
		if w.Missiles.Row(id) != -1 {
			e.Kind = "missile"
		}
		if hr := w.Healths.Row(id); hr != -1 {
			l, ml := df(w.Healths.Life[hr]), df(w.Healths.MaxLife[hr])
			av, at := w.Healths.ArmorValue[hr], w.Healths.ArmorType[hr]
			e.Life, e.MaxLife, e.ArmorValue, e.ArmorType = &l, &ml, &av, &at
		}
		if or := w.Owners.Row(id); or != -1 {
			e.Player, e.Team = &w.Owners.Player[or], &w.Owners.Team[or]
		}
		if r := w.Orders.Row(id); r != -1 {
			k, tgt, dt := w.Orders.Kind[r], uint32(w.Orders.Target[r]), w.Orders.Data[r]
			depth := 0
			for q := w.Orders.QueueHead[r]; q != NoOrderEntry; q = w.orderPool[q].next {
				depth++
			}
			e.OrderKind, e.OrderTarget, e.OrderData, e.QueueDepth = &k, &tgt, &dt, &depth
		}
		if ar := w.Abilities.Row(id); ar != -1 {
			mn := df(w.Abilities.Mana[ar])
			e.Mana = &mn
		}
		d.Entities = append(d.Entities, e)
	}
	for i := int32(0); int(i) < w.Buffs.Cap(); i++ {
		if !w.Buffs.live[i] {
			continue
		}
		r := &w.Buffs.rows[i]
		d.Buffs = append(d.Buffs, dumpBuff{
			Slot: i, BuffID: r.BuffID, Stacks: r.Stacks, Flags: r.Flags,
			Target: uint32(r.Target), Source: uint32(r.Source),
			RemainingTicks: r.RemainingTicks, PeriodicClock: r.PeriodicClock,
		})
	}

	enc := json.NewEncoder(wr)
	enc.SetIndent("", "  ")
	return enc.Encode(&d)
}
