package sched

import (
	"encoding/binary"
	"errors"
	"fmt"
	"sort"
)

// Save format v1 (R-SIM-6) — all integers little-endian, fixed width:
//
//	[8]  magic   "LITDSCHD"
//	[2]  version 1
//	[4]  now
//	[4]  nextSeq
//	[4]  sleepCount
//	     sleepCount × { [4] wakeTick, [4] seq, [4] cont, [32] state (4×i64) }
//	     in strictly ascending (wakeTick, seq) order — canonical
//	[4]  eventCount (events with ≥1 waiter only)
//	     eventCount × { [4] ev, [4] waiterCount,
//	                    waiterCount × { [4] seq, [4] cont, [32] state } }
//	     events strictly ascending by ev; waiters strictly ascending by seq
//
// Canonical ordering everywhere means the same logical state always
// produces byte-identical blobs, and Load can reject any blob that
// isn't in canonical form (fail-closed against tampering/corruption).
//
// The continuation registry is code, not state: blobs reference
// continuations by stable ContID and Load refuses any ID the live
// registry doesn't know.

var saveMagic = [8]byte{'L', 'I', 'T', 'D', 'S', 'C', 'H', 'D'}

const saveVersion uint16 = 1

const (
	recordSize = 4 + 4 + 4 + 32 // wakeTick, seq, cont, state
	waiterSize = 4 + 4 + 32     // seq, cont, state
)

// SaveSize returns the exact byte length Save will produce. Transient
// (save-unsafe) sleep records are excluded — Save drops them (#557) — so
// this must count only persistable records to stay exactly in sync.
func (s *Scheduler) SaveSize() int {
	persist := len(s.sleep) - s.PendingTransient()
	n := 8 + 2 + 4 + 4 + 4 + persist*recordSize + 4
	for i := range s.waiters {
		if len(s.waiters[i].list) > 0 {
			n += 4 + 4 + len(s.waiters[i].list)*waiterSize
		}
	}
	return n
}

// Save appends the canonical encoding of the full scheduler state to
// dst and returns the result. Deterministic: no map is iterated; the
// sleep heap is emitted in sorted (wakeTick, seq) order.
func (s *Scheduler) Save(dst []byte) []byte {
	dst = append(dst, saveMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, saveVersion)
	dst = binary.LittleEndian.AppendUint32(dst, s.now)
	dst = binary.LittleEndian.AppendUint32(dst, s.nextSeq)

	// Persist only non-transient records: a transient (save-unsafe) cont
	// — e.g. the Go-closure timer trampoline — is dropped on save (#557),
	// so its records never reach the blob and cannot fail Load.
	sorted := make([]record, 0, len(s.sleep))
	for i := range s.sleep {
		if !s.isTransient(s.sleep[i].cont) {
			sorted = append(sorted, s.sleep[i])
		}
	}
	sort.Slice(sorted, func(i, j int) bool { return recordLess(sorted[i], sorted[j]) })
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(sorted)))
	for i := range sorted {
		dst = binary.LittleEndian.AppendUint32(dst, sorted[i].wakeTick)
		dst = binary.LittleEndian.AppendUint32(dst, sorted[i].seq)
		dst = binary.LittleEndian.AppendUint32(dst, uint32(sorted[i].cont))
		dst = appendState(dst, sorted[i].state)
	}

	events := 0
	for i := range s.waiters {
		if len(s.waiters[i].list) > 0 {
			events++
		}
	}
	dst = binary.LittleEndian.AppendUint32(dst, uint32(events))
	for i := range s.waiters { // already sorted ascending by ev
		w := &s.waiters[i]
		if len(w.list) == 0 {
			continue
		}
		dst = binary.LittleEndian.AppendUint32(dst, uint32(w.ev))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(len(w.list)))
		for j := range w.list {
			dst = binary.LittleEndian.AppendUint32(dst, w.list[j].seq)
			dst = binary.LittleEndian.AppendUint32(dst, uint32(w.list[j].cont))
			dst = appendState(dst, w.list[j].state)
		}
	}
	return dst
}

func appendState(dst []byte, st State) []byte {
	for _, v := range st {
		dst = binary.LittleEndian.AppendUint64(dst, uint64(v))
	}
	return dst
}

// reader is a bounds-checked cursor over the blob. Any short read
// flips err and every later read returns zero — Load checks err once
// at the end of each structural section.
type reader struct {
	b   []byte
	off int
	err error
}

func (r *reader) u16() uint16 {
	if r.err != nil || r.off+2 > len(r.b) {
		r.fail()
		return 0
	}
	v := binary.LittleEndian.Uint16(r.b[r.off:])
	r.off += 2
	return v
}

func (r *reader) u32() uint32 {
	if r.err != nil || r.off+4 > len(r.b) {
		r.fail()
		return 0
	}
	v := binary.LittleEndian.Uint32(r.b[r.off:])
	r.off += 4
	return v
}

func (r *reader) u64() uint64 {
	if r.err != nil || r.off+8 > len(r.b) {
		r.fail()
		return 0
	}
	v := binary.LittleEndian.Uint64(r.b[r.off:])
	r.off += 8
	return v
}

func (r *reader) state() State {
	var st State
	for i := range st {
		st[i] = int64(r.u64())
	}
	return st
}

func (r *reader) fail() {
	if r.err == nil {
		r.err = fmt.Errorf("sched: truncated save blob at offset %d (len %d)", r.off, len(r.b))
	}
}

var (
	errMagic   = errors.New("sched: bad save magic")
	errVersion = errors.New("sched: unsupported save version")
)

// Load replaces the scheduler's suspended state with the blob's.
// The continuation registry must already be populated — Load rejects
// any ContID it doesn't know. On any error the scheduler is left
// completely untouched: decode builds into locals and state is
// swapped only after the entire blob has validated (fail-closed,
// zero partial application).
func (s *Scheduler) Load(blob []byte) error {
	r := &reader{b: blob}
	if len(blob) < len(saveMagic) {
		return fmt.Errorf("sched: blob too short for magic (%d bytes)", len(blob))
	}
	for i := range saveMagic {
		if blob[i] != saveMagic[i] {
			return errMagic
		}
	}
	r.off = 8
	if v := r.u16(); r.err == nil && v != saveVersion {
		return fmt.Errorf("%w: %d (want %d)", errVersion, v, saveVersion)
	}
	now := r.u32()
	nextSeq := r.u32()

	sleepCount := r.u32()
	if r.err != nil {
		return r.err
	}
	if int(sleepCount) > (len(blob)-r.off)/recordSize {
		return fmt.Errorf("sched: sleep count %d exceeds blob size", sleepCount)
	}
	sleep := make([]record, 0, sleepCount)
	var prev record
	for i := uint32(0); i < sleepCount; i++ {
		rec := record{
			wakeTick: r.u32(),
			seq:      r.u32(),
			cont:     ContID(r.u32()),
			state:    r.state(),
		}
		if r.err != nil {
			return r.err
		}
		if i > 0 && !recordLess(prev, rec) {
			return fmt.Errorf("sched: sleep queue not in canonical (wakeTick,seq) order at entry %d", i)
		}
		if _, ok := s.conts[rec.cont]; !ok {
			return fmt.Errorf("sched: save references unregistered ContID %d", rec.cont)
		}
		if rec.seq > nextSeq {
			return fmt.Errorf("sched: sleep entry seq %d exceeds nextSeq %d", rec.seq, nextSeq)
		}
		prev = rec
		sleep = append(sleep, rec)
	}

	eventCount := r.u32()
	if r.err != nil {
		return r.err
	}
	waiters := make([]eventWaiters, 0, eventCount)
	for i := uint32(0); i < eventCount; i++ {
		ev := EventID(r.u32())
		wn := r.u32()
		if r.err != nil {
			return r.err
		}
		if len(waiters) > 0 && waiters[len(waiters)-1].ev >= ev {
			return fmt.Errorf("sched: events not in canonical ascending order at %d", ev)
		}
		if wn == 0 {
			return fmt.Errorf("sched: canonical form forbids empty waiter list for event %d", ev)
		}
		if int(wn) > (len(blob)-r.off)/waiterSize {
			return fmt.Errorf("sched: waiter count %d exceeds blob size", wn)
		}
		list := make([]waiter, 0, wn)
		for j := uint32(0); j < wn; j++ {
			w := waiter{seq: r.u32(), cont: ContID(r.u32()), state: r.state()}
			if r.err != nil {
				return r.err
			}
			if j > 0 && list[j-1].seq >= w.seq {
				return fmt.Errorf("sched: waiters for event %d not in canonical seq order", ev)
			}
			if _, ok := s.conts[w.cont]; !ok {
				return fmt.Errorf("sched: save references unregistered ContID %d", w.cont)
			}
			if w.seq > nextSeq {
				return fmt.Errorf("sched: waiter seq %d exceeds nextSeq %d", w.seq, nextSeq)
			}
			list = append(list, w)
		}
		waiters = append(waiters, eventWaiters{ev: ev, list: list})
	}
	if r.err != nil {
		return r.err
	}
	if r.off != len(blob) {
		return fmt.Errorf("sched: %d trailing bytes after save data", len(blob)-r.off)
	}

	// Whole blob validated — commit. sleep was decoded in ascending
	// (wakeTick, seq) order, which is a valid binary min-heap layout.
	s.now = now
	s.nextSeq = nextSeq
	s.sleep = sleep
	s.waiters = waiters
	return nil
}
