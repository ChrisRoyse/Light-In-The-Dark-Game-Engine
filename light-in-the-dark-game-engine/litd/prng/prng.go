// Package prng is the simulation's deterministic random source: PCG32
// (XSH-RR 64/32, O'Neill, pcg-random.org), in-repo per determinism.md §4.
// All gameplay randomness flows through here (R-SIM-2) — never math/rand.
//
// The package imports nothing but math/bits: no math/rand, no
// crypto/rand, no time. Seeding comes from the match payload; sub-streams
// are split from the master seed at match start (one per system, each
// single-threaded and tick-owned).
package prng

import "math/bits"

const multiplier = 6364136223846793005

// Stream is a single PCG32 stream. Draws have strict ordering; a Stream
// must only ever be used from the simulation goroutine that owns it.
type Stream struct {
	state uint64
	inc   uint64 // odd; encodes the stream sequence
}

// New returns a Stream seeded per the PCG reference srandom: the seed
// is the match payload seed, seq selects the stream.
func New(seed, seq uint64) *Stream {
	s := &Stream{state: 0, inc: seq<<1 | 1}
	s.Uint32()
	s.state += seed
	s.Uint32()
	return s
}

// Split derives the i-th sub-stream from a master match seed. Children
// use the same seed with sequence i+1; the master stream itself is
// New(seed, 0). The derivation is part of the determinism contract —
// changing it breaks replays.
func Split(masterSeed uint64, i uint64) *Stream {
	return New(masterSeed, i+1)
}

// Uint32 advances the stream and returns the next output (PCG32 XSH-RR).
func (s *Stream) Uint32() uint32 {
	old := s.state
	s.state = old*multiplier + s.inc
	xorshifted := uint32(((old >> 18) ^ old) >> 27)
	rot := int(old >> 59)
	return bits.RotateLeft32(xorshifted, -rot)
}

// Cursor is the serializable position of a Stream (R-SIM-6). It is the
// complete state: restoring it resumes the exact draw sequence.
type Cursor struct {
	State uint64
	Inc   uint64
}

// Cursor exports the stream position.
func (s *Stream) Cursor() Cursor { return Cursor{State: s.state, Inc: s.inc} }

// Restore returns a Stream positioned at the cursor.
func Restore(c Cursor) *Stream { return &Stream{state: c.State, inc: c.Inc} }
