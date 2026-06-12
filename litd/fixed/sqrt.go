package fixed

// SqrtU64 returns the exact floor square root of v:
// the unique r with r*r <= v < (r+1)*(r+1).
//
// Digit-by-digit (restoring) method — pure integer ops, no convergence
// subtleties, bit-identical on every architecture.
func SqrtU64(v uint64) uint32 {
	var r uint64
	bit := uint64(1) << 62
	for bit > v {
		bit >>= 2
	}
	for bit != 0 {
		if v >= r+bit {
			v -= r + bit
			r = r>>1 + bit
		} else {
			r >>= 1
		}
		bit >>= 2
	}
	return uint32(r)
}
