package fixed

// SqrtU64 must return the EXACT floor square root for every input: the value
// feeds the sim state hash, so the contract is bit-exactness, not approximation.
// These tests lock that — the defining invariant r*r <= v < (r+1)*(r+1), the
// edge/boundary cases, and equality against an independent digit-by-digit
// reference over the full uint64 range. A future faster rewrite stays honest
// only if it still passes here.

import (
	"math/rand"
	"testing"
)

// digitByDigitSqrt is the prior implementation, kept here as an independent
// oracle: a replacement is determinism-safe only if it matches this exactly.
func digitByDigitSqrt(v uint64) uint32 {
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

func TestSqrtU64Invariant(t *testing.T) {
	check := func(v uint64) {
		r := uint64(SqrtU64(v))
		if r*r > v || (r+1)*(r+1) <= v && r != 1<<32-1 {
			t.Fatalf("SqrtU64(%d) = %d violates r*r <= v < (r+1)^2", v, r)
		}
	}
	for v := uint64(0); v < 1<<16; v++ {
		check(v)
	}
	for _, v := range []uint64{
		0, 1, 2, 3, 4, 8, 15, 16, 17, 24, 25, 26,
		1<<31 - 1, 1 << 31, 1<<31 + 1,
		1<<32 - 1, 1 << 32, 1<<32 + 1,
		1 << 62, 1 << 63, 1<<63 + 1, ^uint64(0),
	} {
		check(v)
	}
	t.Log("FSV invariant: r*r <= v < (r+1)^2 holds across dense small + edge inputs incl. max uint64")
}

func TestSqrtU64MatchesReferenceFSV(t *testing.T) {
	mism := 0
	for v := uint64(0); v < 1<<18; v++ { // dense
		if SqrtU64(v) != digitByDigitSqrt(v) {
			mism++
		}
	}
	for k := uint64(0); k < 1<<21; k += 433 { // perfect-square boundaries
		for _, v := range []uint64{k * k, k*k + 1, k*k - 1} {
			if SqrtU64(v) != digitByDigitSqrt(v) {
				mism++
			}
		}
	}
	r := rand.New(rand.NewSource(0xC0FFEE)) // fixed seed: reproducible
	n := 1_000_000
	for i := 0; i < n; i++ {
		for _, v := range []uint64{r.Uint64(), r.Uint64() >> uint(r.Intn(64))} {
			if SqrtU64(v) != digitByDigitSqrt(v) {
				mism++
			}
		}
	}
	if mism != 0 {
		t.Fatalf("SqrtU64 diverged from the digit-by-digit reference in %d cases", mism)
	}
	t.Logf("FSV determinism: SqrtU64 bit-identical to the reference over dense + boundary + %dM random uint64", 2)
}
