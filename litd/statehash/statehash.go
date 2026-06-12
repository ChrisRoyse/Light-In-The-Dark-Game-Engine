// Package statehash is the determinism oracle (determinism.md §5,
// R-FSV-2): a streaming xxHash64 implemented in-repo, fed field by field
// through an explicit writer API. State is NEVER hashed as raw struct
// memory — no unsafe, no reflection — so padding bytes and GOARCH layout
// cannot leak into the hash.
package statehash

import (
	"encoding/binary"
	"math/bits"
)

const (
	prime1 uint64 = 0x9E3779B185EBCA87
	prime2 uint64 = 0xC2B2AE3D27D4EB4F
	prime3 uint64 = 0x165667B19E3779F9
	prime4 uint64 = 0x85EBCA77C2B2AE63
	prime5 uint64 = 0x27D4EB2F165667C5
)

// Hasher is a streaming xxHash64 (seed 0). The zero value is NOT ready;
// use New or Reset.
type Hasher struct {
	v1, v2, v3, v4 uint64
	total          uint64
	buf            [32]byte
	n              int
	scratch        [8]byte
}

// New returns a ready Hasher.
func New() *Hasher {
	h := &Hasher{}
	h.Reset()
	return h
}

// Reset returns the Hasher to the initial (seed 0) state.
func (h *Hasher) Reset() {
	h.v1 = prime1
	h.v1 += prime2 // wraps; Go rejects the overflowing constant expression
	h.v2 = prime2
	h.v3 = 0
	h.v4 = 0
	h.v4 -= prime1
	h.total = 0
	h.n = 0
}

func round(acc, input uint64) uint64 {
	acc += input * prime2
	acc = bits.RotateLeft64(acc, 31)
	return acc * prime1
}

func mergeRound(h, v uint64) uint64 {
	h ^= round(0, v)
	return h*prime1 + prime4
}

// WriteBytes feeds raw bytes. Gameplay code prefers the typed writers;
// this exists for strings/blobs already in canonical byte form.
func (h *Hasher) WriteBytes(b []byte) {
	h.total += uint64(len(b))

	if h.n > 0 {
		c := copy(h.buf[h.n:], b)
		h.n += c
		b = b[c:]
		if h.n < 32 {
			return
		}
		h.consumeBuf()
	}

	for len(b) >= 32 {
		h.v1 = round(h.v1, binary.LittleEndian.Uint64(b[0:8]))
		h.v2 = round(h.v2, binary.LittleEndian.Uint64(b[8:16]))
		h.v3 = round(h.v3, binary.LittleEndian.Uint64(b[16:24]))
		h.v4 = round(h.v4, binary.LittleEndian.Uint64(b[24:32]))
		b = b[32:]
	}
	h.n = copy(h.buf[:], b)
}

func (h *Hasher) consumeBuf() {
	h.v1 = round(h.v1, binary.LittleEndian.Uint64(h.buf[0:8]))
	h.v2 = round(h.v2, binary.LittleEndian.Uint64(h.buf[8:16]))
	h.v3 = round(h.v3, binary.LittleEndian.Uint64(h.buf[16:24]))
	h.v4 = round(h.v4, binary.LittleEndian.Uint64(h.buf[24:32]))
	h.n = 0
}

// WriteU64 feeds one uint64 (little-endian canonical form).
func (h *Hasher) WriteU64(v uint64) {
	binary.LittleEndian.PutUint64(h.scratch[:8], v)
	h.WriteBytes(h.scratch[:8])
}

// WriteI64 feeds one int64.
func (h *Hasher) WriteI64(v int64) { h.WriteU64(uint64(v)) }

// WriteU32 feeds one uint32.
func (h *Hasher) WriteU32(v uint32) {
	binary.LittleEndian.PutUint32(h.scratch[:4], v)
	h.WriteBytes(h.scratch[:4])
}

// WriteU16 feeds one uint16.
func (h *Hasher) WriteU16(v uint16) {
	binary.LittleEndian.PutUint16(h.scratch[:2], v)
	h.WriteBytes(h.scratch[:2])
}

// WriteU8 feeds one byte.
func (h *Hasher) WriteU8(v uint8) {
	h.scratch[0] = v
	h.WriteBytes(h.scratch[:1])
}

// WriteBool feeds a bool as one byte (1/0).
func (h *Hasher) WriteBool(v bool) {
	if v {
		h.WriteU8(1)
	} else {
		h.WriteU8(0)
	}
}

// Sum64 returns the hash of everything written so far. The stream may
// continue to be written afterwards.
func (h *Hasher) Sum64() uint64 {
	var acc uint64
	if h.total >= 32 {
		acc = bits.RotateLeft64(h.v1, 1) + bits.RotateLeft64(h.v2, 7) +
			bits.RotateLeft64(h.v3, 12) + bits.RotateLeft64(h.v4, 18)
		acc = mergeRound(acc, h.v1)
		acc = mergeRound(acc, h.v2)
		acc = mergeRound(acc, h.v3)
		acc = mergeRound(acc, h.v4)
	} else {
		acc = prime5 // seed 0 + prime5
	}
	acc += h.total

	b := h.buf[:h.n]
	for len(b) >= 8 {
		acc ^= round(0, binary.LittleEndian.Uint64(b[:8]))
		acc = bits.RotateLeft64(acc, 27)*prime1 + prime4
		b = b[8:]
	}
	if len(b) >= 4 {
		acc ^= uint64(binary.LittleEndian.Uint32(b[:4])) * prime1
		acc = bits.RotateLeft64(acc, 23)*prime2 + prime3
		b = b[4:]
	}
	for _, c := range b {
		acc ^= uint64(c) * prime5
		acc = bits.RotateLeft64(acc, 11) * prime1
	}

	acc ^= acc >> 33
	acc *= prime2
	acc ^= acc >> 29
	acc *= prime3
	acc ^= acc >> 32
	return acc
}
