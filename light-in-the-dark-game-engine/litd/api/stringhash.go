package litd

// StringHash is the deterministic survivor of the WC3 StringHash native.
//
// WC3's StringHash (SStrHash2) exists only as a hashtable key function,
// and LitD tombstones hashtables (jass-mapping/math-strings-conversion.md
// §"StringHash compat"; deduplication-policy.md §7). The M2 decision is
// therefore to NOT reproduce SStrHash2 but to use FNV-1a/32 — a small,
// well-specified, byte-exact, platform-stable hash with public test
// vectors (recorded as an ADR alongside this commit). Callers needing a
// stable string→int key get one; nobody depends on WC3 bit-compat.
//
// JASS: StringHash, StringHashBJ
func StringHash(s string) int32 {
	const (
		offset uint32 = 2166136261 // FNV-1a 32-bit offset basis
		prime  uint32 = 16777619   // FNV-1a 32-bit prime
	)
	h := offset
	for i := 0; i < len(s); i++ {
		h ^= uint32(s[i])
		h *= prime
	}
	return int32(h)
}
