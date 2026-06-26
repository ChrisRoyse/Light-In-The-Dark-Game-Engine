package input

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// SmartFeedback is the client-side order feedback class. A non-none
// value means no command record crossed the determinism boundary.
type SmartFeedback uint8

const (
	SmartFeedbackNone SmartFeedback = iota
	SmartFeedbackEmptySelection
	SmartFeedbackHiddenTarget
	SmartFeedbackDeadTarget
	SmartFeedbackNoResolution
	SmartFeedbackEncodeFailed
)

func (f SmartFeedback) String() string {
	switch f {
	case SmartFeedbackNone:
		return "none"
	case SmartFeedbackEmptySelection:
		return "empty-selection"
	case SmartFeedbackHiddenTarget:
		return "hidden-target"
	case SmartFeedbackDeadTarget:
		return "dead-target"
	case SmartFeedbackNoResolution:
		return "no-resolution"
	case SmartFeedbackEncodeFailed:
		return "encode-failed"
	default:
		return "unknown"
	}
}

// SmartTarget is the presentation-layer click target. ClassSet lets
// render/UI hit-testing select classes the sim cannot infer yet
// (resource nodes, transport-capacity targets, damaged structures).
type SmartTarget struct {
	Entity   sim.EntityID
	Point    fixed.Vec2
	Class    uint8
	ClassSet bool
	Hidden   bool
	Minimap  bool
}

type SmartOrderRequest struct {
	Player    uint8
	Team      uint8
	Seq       uint16
	Selection Selection
	Target    SmartTarget
	Queued    bool
}

type ExplicitOrderRequest struct {
	Player    uint8
	Seq       uint16
	Opcode    uint8
	Selection Selection
	Target    SmartTarget
	Queued    bool
	Data      uint16
}

type SmartOrderResult struct {
	Records      [MaxSelection]sim.CommandRecord
	Count        uint8
	TargetClass  uint8
	Feedback     SmartFeedback
	EncodedBytes int
}

func ResolveRightClick(w *sim.World, req SmartOrderRequest, encoded []byte, out *SmartOrderResult) ([]byte, bool) {
	if out == nil || w == nil {
		return encoded, false
	}
	*out = SmartOrderResult{}
	if req.Selection.Count == 0 {
		out.Feedback = SmartFeedbackEmptySelection
		return encoded, false
	}
	if req.Target.Hidden {
		out.Feedback = SmartFeedbackHiddenTarget
		return encoded, false
	}
	if req.Target.Entity != 0 && !w.Ents.Alive(req.Target.Entity) {
		out.Feedback = SmartFeedbackDeadTarget
		return encoded, false
	}
	n, tc, ok := w.ResolveSmartRecords(sim.SmartRecordRequest{
		Player:         req.Player,
		Team:           req.Team,
		Seq:            req.Seq,
		Target:         req.Target.Entity,
		Point:          req.Target.Point,
		TargetClass:    req.Target.Class,
		TargetClassSet: req.Target.ClassSet,
		Queued:         req.Queued,
	}, req.Selection.IDs[:req.Selection.Count], out.Records[:])
	out.TargetClass = tc
	if !ok {
		out.Feedback = SmartFeedbackNoResolution
		return encoded, false
	}
	out.Count = uint8(n)
	return encodeRecords(encoded, out)
}

func ResolveExplicitOrder(w *sim.World, req ExplicitOrderRequest, encoded []byte, out *SmartOrderResult) ([]byte, bool) {
	if out == nil {
		return encoded, false
	}
	*out = SmartOrderResult{}
	if req.Selection.Count == 0 {
		out.Feedback = SmartFeedbackEmptySelection
		return encoded, false
	}
	if req.Target.Hidden {
		out.Feedback = SmartFeedbackHiddenTarget
		return encoded, false
	}
	if req.Target.Entity != 0 && w != nil && !w.Ents.Alive(req.Target.Entity) {
		out.Feedback = SmartFeedbackDeadTarget
		return encoded, false
	}
	flags := uint8(0)
	if req.Queued {
		flags = sim.CmdFlagQueued
	}
	rec := &out.Records[0]
	*rec = sim.CommandRecord{
		Version: sim.CommandVersion,
		Player:  req.Player,
		Seq:     req.Seq,
		Opcode:  req.Opcode,
		Flags:   flags,
		Target:  req.Target.Entity,
		Point:   req.Target.Point,
		Data:    req.Data,
	}
	for i := uint8(0); i < req.Selection.Count; i++ {
		id := req.Selection.IDs[i]
		if id == 0 || (w != nil && !w.Ents.Alive(id)) {
			continue
		}
		rec.Units[rec.UnitCount] = id
		rec.UnitCount++
	}
	if rec.UnitCount == 0 {
		out.Feedback = SmartFeedbackEmptySelection
		return encoded, false
	}
	out.Count = 1
	return encodeRecords(encoded, out)
}

func encodeRecords(encoded []byte, out *SmartOrderResult) ([]byte, bool) {
	base := len(encoded)
	for i := uint8(0); i < out.Count; i++ {
		var ok bool
		encoded, ok = sim.AppendEncode(encoded, &out.Records[i])
		if !ok {
			encoded = encoded[:base]
			out.Feedback = SmartFeedbackEncodeFailed
			out.EncodedBytes = 0
			return encoded, false
		}
	}
	out.Feedback = SmartFeedbackNone
	out.EncodedBytes = len(encoded) - base
	return encoded, true
}
