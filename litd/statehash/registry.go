package statehash

// Registry holds one named sub-hash per simulation system. Systems
// register once at sim construction in a fixed order; that order is part
// of the determinism contract (changing it changes every TopHash).
//
// Each tick every system writes its state into its own Hasher; TopHash
// combines (name, sub-hash) pairs in registration order. When two runs
// diverge, comparing snapshots bisects the offender: FirstDivergence
// names the first system whose sub-hash differs.
type Registry struct {
	names []string
	// one heap Hasher per system: Register hands the pointer out, so the
	// Hasher must not live inside a slice that a later append relocates
	hashes []*Hasher
	top    Hasher
}

// NewRegistry returns an empty registry.
func NewRegistry() *Registry {
	return &Registry{}
}

// Register adds a named system and returns its Hasher. Panics on a
// duplicate name — two systems sharing a sub-hash would mask divergence.
// Call only during sim construction, never mid-match.
func (r *Registry) Register(name string) *Hasher {
	for _, n := range r.names {
		if n == name {
			panic("statehash: duplicate system name: " + name)
		}
	}
	h := New()
	r.names = append(r.names, name)
	r.hashes = append(r.hashes, h)
	return h
}

// Reset resets every system Hasher for the next tick. Registration
// survives; only stream state clears.
func (r *Registry) Reset() {
	for i := range r.hashes {
		r.hashes[i].Reset()
	}
}

// Snapshot is the per-tick result: sub-hashes in registration order plus
// the combined top hash. Names live in the Registry; Subs[i] belongs to
// the i-th registered system.
type Snapshot struct {
	Subs []uint64
	Top  uint64
}

// Sum fills dst with the current sub-hashes and top hash and returns it.
// dst.Subs is reused when capacity allows, so a caller-retained Snapshot
// makes the per-tick path allocation-free (R-GC-1).
//
// The top hash binds each system's name and sub-hash in registration
// order: an empty contribution still occupies its slot, so reordering
// systems — even no-op ones — changes the top hash.
func (r *Registry) Sum(dst *Snapshot) *Snapshot {
	if cap(dst.Subs) < len(r.hashes) {
		dst.Subs = make([]uint64, len(r.hashes))
	}
	dst.Subs = dst.Subs[:len(r.hashes)]

	r.top.Reset()
	for i := range r.hashes {
		sub := r.hashes[i].Sum64()
		dst.Subs[i] = sub
		name := r.names[i]
		r.top.WriteU32(uint32(len(name)))
		for j := 0; j < len(name); j++ {
			r.top.WriteU8(name[j])
		}
		r.top.WriteU64(sub)
	}
	dst.Top = r.top.Sum64()
	return dst
}

// Names returns the registered system names in registration order. The
// slice is owned by the registry — do not mutate.
func (r *Registry) Names() []string { return r.names }

// FirstDivergence compares two snapshots from registries with identical
// registration and returns the name of the first system whose sub-hash
// differs, and true. Returns "", false when no sub-hash differs (top
// hashes then match too) or when the snapshots have mismatched lengths
// (not comparable — caller registered different systems).
func (r *Registry) FirstDivergence(a, b *Snapshot) (string, bool) {
	if len(a.Subs) != len(b.Subs) || len(a.Subs) != len(r.names) {
		return "", false
	}
	for i := range a.Subs {
		if a.Subs[i] != b.Subs[i] {
			return r.names[i], true
		}
	}
	return "", false
}
