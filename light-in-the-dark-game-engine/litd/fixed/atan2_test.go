package fixed

import (
	"math"
	"testing"
)

// Golden axes and diagonals: exact BAM values.
func TestAtan2Golden(t *testing.T) {
	cases := []struct {
		y, x F64
		want Angle
		name string
	}{
		{0, One, 0x0000, "+X east"},
		{One, One, 0x2000, "NE diagonal"},
		{One, 0, 0x4000, "+Y north"},
		{One, -One, 0x6000, "NW diagonal"},
		{0, -One, 0x8000, "-X west"},
		{-One, -One, 0xA000, "SW diagonal"},
		{-One, 0, 0xC000, "-Y south"},
		{-One, One, 0xE000, "SE diagonal"},
		{0, 0, 0x0000, "zero vector convention"},
	}
	for _, c := range cases {
		got := Atan2(c.y, c.x)
		t.Logf("%-22s Atan2(%d,%d) = %#04x (want %#04x)", c.name, c.y, c.x, uint16(got), uint16(c.want))
		if got != c.want {
			t.Fatalf("%s: got %#04x want %#04x", c.name, uint16(got), uint16(c.want))
		}
	}
}

// Round trip: angle → (Cos, Sin) → Atan2 recovers the angle within
// 2 BAM units across the whole circle.
func TestAtan2RoundTrip(t *testing.T) {
	worst := 0
	for a := 0; a < 65536; a += 17 { // coprime stride covers the circle
		ang := Angle(a)
		back := Atan2(ang.Sin(), ang.Cos())
		d := int(uint16(back - ang))
		if d > 32768 {
			d = 65536 - d
		}
		if d > worst {
			worst = d
		}
	}
	t.Logf("worst round-trip error across 3,856 sampled angles: %d BAM units (tolerance 2)", worst)
	if worst > 2 {
		t.Fatalf("round-trip error %d BAM > 2", worst)
	}
}

// Cross-check against math.Atan2 (test-only float reference).
func TestAtan2FloatReference(t *testing.T) {
	worst := 0.0
	for i := 0; i < 1000; i++ {
		x := F64((int64(i)*2654435761 + 12345) % (1 << 40))
		y := F64((int64(i)*40503*65537 + 99991) % (1 << 40))
		if i%3 == 0 {
			x = -x
		}
		if i%5 == 0 {
			y = -y
		}
		got := float64(uint16(Atan2(y, x))) / 65536 * 2 * math.Pi
		want := math.Atan2(float64(y), float64(x))
		if want < 0 {
			want += 2 * math.Pi
		}
		d := math.Abs(got - want)
		if d > math.Pi {
			d = 2*math.Pi - d
		}
		if d > worst {
			worst = d
		}
	}
	t.Logf("worst |Atan2 - math.Atan2| over 1,000 pseudo-random vectors: %.6f rad (tolerance 0.0002)", worst)
	if worst > 0.0002 {
		t.Fatalf("error vs float reference too large: %v", worst)
	}
}

// TurnToward: shortest arc both ways, clamp, exact-arrival, and the
// 180° counterclockwise tie rule.
func TestTurnTowardShortestArc(t *testing.T) {
	cases := []struct {
		cur, want, rate, expect Angle
		name                    string
	}{
		{0x0000, 0x2000, 0x1000, 0x1000, "ccw clamped"},
		{0x2000, 0x0000, 0x1000, 0x1000, "cw clamped"},
		{0x0000, 0x0800, 0x1000, 0x0800, "reaches inside rate"},
		{0xF000, 0x1000, 0x1000, 0x0000, "ccw across wrap"},
		{0x1000, 0xF000, 0x1000, 0x0000, "cw across wrap"},
		{0x0000, 0x8000, 0x1000, 0x1000, "180 tie -> ccw"},
		{0x4000, 0x4000, 0x1000, 0x4000, "already there"},
	}
	for _, c := range cases {
		got := TurnToward(c.cur, c.want, c.rate)
		t.Logf("%-20s TurnToward(%#04x -> %#04x, rate %#04x) = %#04x", c.name, uint16(c.cur), uint16(c.want), uint16(c.rate), uint16(got))
		if got != c.expect {
			t.Fatalf("%s: got %#04x want %#04x", c.name, uint16(got), uint16(c.expect))
		}
	}
}
