package sim

// Trigger event index (ADR #451, issue #458). The inverted dispatch
// index maps an event kind to the triggers registered on it, in
// deterministic fire order, so an emitted event fires exactly the
// matching triggers without polling every trigger and without iterating
// a map in the tick path (R-SIM-2).
//
// Derivation, not duplication. The index is a pure function of the
// trigger slab: it is rebuilt from the slots whenever a structural
// change (create / destroy / add-event) sets the store's dirty bit, and
// is otherwise untouched. In steady state — events firing, no triggers
// being created — the lookup path allocates nothing (R-GC-1) and the
// index never serializes (it is reconstructed at load from the restored
// slab).
//
// Fire order is the trigger's slot order (slots are assigned in
// registration order), then event-add order within a trigger. That order
// is deterministic and reproducible from the slab, so a reloaded match
// fires triggers in exactly the same order.
//
// Scope. A trigger may register an event globally (scope key 0 — fires
// for every event of that kind) or scoped to a small integer key (a unit
// id, player id, or region id assigned by the registration API). An
// emitted event carries a scope key; the lookup returns the global
// triggers plus those whose scope key equals the event's, merged in slot
// order. The index does not interpret the integer — the dispatcher (#459)
// assigns its meaning per event kind.

// triggerIndexEntry is one (kind-bucketed) registration: the trigger's
// slot (the fire-order key), its scope key, and the handle.
type triggerIndexEntry struct {
	slot    uint32
	scope   uint32
	trigger TriggerID
}

// triggerIndexBucket holds one event kind's registrations, sorted by
// slot (ascending) — i.e. fire order.
type triggerIndexBucket struct {
	kind    uint16
	entries []triggerIndexEntry
}

// triggerIndex is the per-world inverted index. buckets are sorted by
// kind for a binary-search lookup; scratch is the reused result buffer.
type triggerIndex struct {
	buckets []triggerIndexBucket
	scratch []TriggerID
}

// GlobalScope is the scope key that fires for every event of a kind.
const GlobalScope uint32 = 0

// ensureTriggerIndex rebuilds the index from the slab if a structural
// change marked it dirty. Cold relative to the tick: in steady state the
// dirty bit is clear and this is a single branch.
func (w *World) ensureTriggerIndex() {
	if w.Triggers.dirty {
		w.rebuildTriggerIndex()
		w.Triggers.dirty = false
	}
}

// rebuildTriggerIndex reconstructs the whole index by scanning the slab
// in slot order. Reuses the bucket and entry backing arrays so a rebuild
// during play does not churn the heap beyond first growth.
func (w *World) rebuildTriggerIndex() {
	idx := &w.trigIndex
	// reset every existing bucket's entries to empty, keep capacity.
	for i := range idx.buckets {
		idx.buckets[i].entries = idx.buckets[i].entries[:0]
	}
	used := 0 // number of buckets currently carrying entries

	ts := w.Triggers
	for slot := range ts.slots {
		sl := &ts.slots[slot]
		if !sl.alive {
			continue
		}
		tid := makeTriggerID(uint32(slot), sl.gen)
		for _, ev := range sl.events {
			e := triggerIndexEntry{slot: uint32(slot), scope: uint32(ev.Scope), trigger: tid}
			bi := idx.bucketIndexUpTo(ev.Kind, used)
			if bi < used && idx.buckets[bi].kind == ev.Kind {
				idx.buckets[bi].entries = append(idx.buckets[bi].entries, e)
				continue
			}
			// insert a new bucket at bi, keeping buckets sorted by kind.
			used++
			if used > len(idx.buckets) {
				idx.buckets = append(idx.buckets, triggerIndexBucket{})
			}
			// Take the freed tail slot's backing for the new bucket BEFORE the
			// shift. The shift below moves buckets[bi:used-1] up by one, which
			// duplicates the slice header now at buckets[bi] into buckets[bi+1];
			// reusing buckets[bi].entries here would alias the inserted bucket
			// onto its right neighbour's entries and corrupt a kind registered
			// out of ascending order (e.g. OnEvent kind 23 then 22). The tail
			// slot's old backing is detached by the shift, so it is safe.
			spare := idx.buckets[used-1].entries[:0]
			copy(idx.buckets[bi+1:used], idx.buckets[bi:used-1])
			idx.buckets[bi] = triggerIndexBucket{kind: ev.Kind, entries: append(spare, e)}
		}
	}
	// drop any buckets beyond the used prefix (stale from a prior build).
	idx.buckets = idx.buckets[:used]
}

// bucketIndexUpTo binary-searches the first `n` buckets for the insertion
// point of kind (buckets[:n] are sorted by kind).
func (idx *triggerIndex) bucketIndexUpTo(kind uint16, n int) int {
	lo, hi := 0, n
	for lo < hi {
		mid := (lo + hi) / 2
		if idx.buckets[mid].kind < kind {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return lo
}

// bucketIndex binary-searches all buckets for kind.
func (idx *triggerIndex) bucketIndex(kind uint16) (int, bool) {
	i := idx.bucketIndexUpTo(kind, len(idx.buckets))
	return i, i < len(idx.buckets) && idx.buckets[i].kind == kind
}

// triggersFor returns the triggers registered on kind whose scope is
// global (GlobalScope) or equals scopeKey, in fire order. The result
// aliases the index's reused scratch buffer — consume it before the next
// call. Zero-alloc once scratch has grown.
func (idx *triggerIndex) triggersFor(kind uint16, scopeKey uint32) []TriggerID {
	idx.scratch = idx.scratch[:0]
	bi, ok := idx.bucketIndex(kind)
	if !ok {
		return idx.scratch
	}
	for _, e := range idx.buckets[bi].entries {
		if e.scope == GlobalScope || e.scope == scopeKey {
			idx.scratch = append(idx.scratch, e.trigger)
		}
	}
	return idx.scratch
}
