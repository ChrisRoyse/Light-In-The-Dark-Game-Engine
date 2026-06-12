// Package fixtures is the determlint test corpus: every hazard the
// linter must catch, plus clean constructs it must NOT flag. Each
// violation line carries a `// want:` annotation consumed by
// determlint_test.go. This package compiles but is never imported.
package fixtures

import (
	cryptorand "crypto/rand" // want: crypto/rand
	"math"                   // want: import "math" banned
	"math/bits"              // clean: the one allowed math package
	mathrand "math/rand"     // want: import "math/rand" banned
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// FloatField has a float in a struct declaration.
type FloatField struct {
	X float64 // want: float in gameplay declaration
	N int64   // clean
}

// Hazards exercises every banned construct.
func Hazards(m map[string]int, a, b fixed.F64) int64 {
	total := 0
	for k := range m { // want: range over map
		total += m[k]
	}

	now := time.Now()       // want: time.Now
	_ = time.Since(now)     // want: time.Since
	go func() { total++ }() // want: go statement
	select {                // want: select statement
	case <-time.After(0):
	default:
	}

	_ = math.Sqrt(100)        // (flagged via the math import above)
	_, hi := bits.Mul64(3, 5) // clean: math/bits allowed
	_ = hi
	_ = mathrand.Int() // (flagged via the math/rand import above)
	buf := make([]byte, 1)
	_, _ = cryptorand.Read(buf) // (flagged via the crypto/rand import above)

	var f float32 // want: float in gameplay declaration
	_ = f
	g := 1.5 // want: float in gameplay declaration (inferred float64)
	_ = g

	c := a * b // want: raw * on fixed.F64
	c += a     // want: raw += on fixed.F64
	d := a + b // want: raw + on fixed.F64
	_ = d
	e := fixed.One / 2 // clean: constant expression, folded at compile time
	_ = e
	h := a.Mul(b).Add(a) // clean: method calls
	_ = h
	_ = c

	return int64(total)
}
