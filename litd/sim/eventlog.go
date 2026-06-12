package sim

// Structured event log (#203, R-FSV-3): one JSONL record per
// dispatched event, written from the phase-6 flush in dispatch order
// — which is deterministic (emission order × registration order), so
// two identical runs produce byte-identical logs. String formatting
// happens only when a log is attached (R-GC-3: the disabled path does
// zero work inside the tick).

import (
	"fmt"
	"io"
)

// eventKindNames maps the built-in kinds to log names; unknown kinds
// log their number.
func eventKindName(k uint16) string {
	switch k {
	case EvUnitDeath:
		return "unit-death"
	case EvMoveDone:
		return "move-done"
	case EvRepathNeeded:
		return "repath-needed"
	case EvOrderIssued:
		return "order-issued"
	case EvOrderDone:
		return "order-done"
	case EvOrderDropped:
		return "order-dropped"
	case EvUnitDamaged:
		return "unit-damaged"
	case EvBuffExpired:
		return "buff-expired"
	}
	return ""
}

// AttachEventLog streams every dispatched event to wr as JSONL.
// Pass nil to detach. The write happens inside the phase-6 flush, so
// a slow writer slows the tick — use a buffered writer and flush
// between ticks (cmd/headless does).
func (w *World) AttachEventLog(wr io.Writer) { w.eventLog = wr }

// logEvent writes one record. Errors are remembered (first one wins)
// and surfaced by EventLogErr — a broken log must not silently pass
// FSV (fail closed: the run reports it).
func (w *World) logEvent(e Event) {
	name := eventKindName(e.Kind)
	var err error
	if name == "" {
		_, err = fmt.Fprintf(w.eventLog,
			"{\"tick\":%d,\"kind\":%d,\"src\":%d,\"dst\":%d,\"arg\":%d}\n",
			w.tick, e.Kind, uint32(e.Src), uint32(e.Dst), e.Arg)
	} else {
		_, err = fmt.Fprintf(w.eventLog,
			"{\"tick\":%d,\"kind\":%d,\"name\":%q,\"src\":%d,\"dst\":%d,\"arg\":%d}\n",
			w.tick, e.Kind, name, uint32(e.Src), uint32(e.Dst), e.Arg)
	}
	if err != nil && w.eventLogErr == nil {
		w.eventLogErr = err
	}
}

// EventLogErr returns the first write error of the attached log, or
// nil.
func (w *World) EventLogErr() error { return w.eventLogErr }
