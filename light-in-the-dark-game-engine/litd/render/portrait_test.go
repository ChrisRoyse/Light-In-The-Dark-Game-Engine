package render

// #193 portrait coverage analysis FSV. SoT = the PortraitCoverage computed from a
// synthetic RGBA buffer with a KNOWN subject/background split (X+X=Y): a buffer that
// is N% subject pixels of a known color must yield Coverage==N% and MeanRGB==that
// color. Pure (no GL), so the classification the EGL harness relies on is verified
// independently of any framebuffer.

import (
	"testing"

	"github.com/g3n/engine/math32"
)

func solidRGBA(n int, r, g, b uint8) []byte {
	buf := make([]byte, n*4)
	for i := 0; i < n; i++ {
		buf[i*4], buf[i*4+1], buf[i*4+2], buf[i*4+3] = r, g, b, 255
	}
	return buf
}

func TestAnalyzePortraitFSV(t *testing.T) {
	bg := math32.Color4{R: 0, G: 0, B: 0, A: 1} // black background

	// Case 1 — all background: zero coverage, never a fabricated subject.
	allBg := solidRGBA(100, 0, 0, 0)
	c1 := AnalyzePortrait(allBg, bg, 8)
	t.Logf("FSV all-bg: total=%d subject=%d coverage=%.2f", c1.Total, c1.Subject, c1.Coverage)
	if c1.Subject != 0 || c1.Coverage != 0 {
		t.Fatalf("all-bg coverage=%.2f subject=%d, want 0/0", c1.Coverage, c1.Subject)
	}

	// Case 2 — exactly half subject (a known red block), half background. SoT:
	// coverage 0.5, mean color of the subject pixels == red.
	buf := make([]byte, 0, 200*4)
	buf = append(buf, solidRGBA(100, 200, 30, 30)...) // 100 red subject pixels
	buf = append(buf, solidRGBA(100, 0, 0, 0)...)     // 100 black background pixels
	c2 := AnalyzePortrait(buf, bg, 8)
	t.Logf("FSV half-red: coverage=%.3f mean=(%.2f,%.2f,%.2f)", c2.Coverage, c2.MeanR, c2.MeanG, c2.MeanB)
	if c2.Total != 200 || c2.Subject != 100 || c2.Coverage != 0.5 {
		t.Fatalf("half: total=%d subject=%d coverage=%.3f, want 200/100/0.5", c2.Total, c2.Subject, c2.Coverage)
	}
	if !approx(c2.MeanR, 200.0/255) || !approx(c2.MeanG, 30.0/255) || !approx(c2.MeanB, 30.0/255) {
		t.Fatalf("mean=(%.3f,%.3f,%.3f), want red ~ (0.784,0.118,0.118)", c2.MeanR, c2.MeanG, c2.MeanB)
	}

	// Case 3 — tolerance edge: a pixel within tol of bg is background, just beyond
	// is subject. bg=black, tol=8 => value 8 is background (diff==8 not >8), 9 is
	// subject.
	edge := append(solidRGBA(1, 8, 0, 0), solidRGBA(1, 9, 0, 0)...)
	c3 := AnalyzePortrait(edge, bg, 8)
	if c3.Subject != 1 {
		t.Fatalf("tolerance edge: subject=%d, want 1 (value 8 within tol, 9 beyond)", c3.Subject)
	}
	t.Logf("FSV tolerance: value 8 -> background, value 9 -> subject (subject=%d)", c3.Subject)

	// Case 4 — empty buffer: zero everything, no panic.
	c4 := AnalyzePortrait(nil, bg, 8)
	if c4.Total != 0 || c4.Coverage != 0 {
		t.Fatalf("empty: total=%d coverage=%.2f, want 0/0", c4.Total, c4.Coverage)
	}
	t.Log("FSV empty buffer: total=0 coverage=0, no panic")
}

func approx(a, b float64) bool {
	d := a - b
	if d < 0 {
		d = -d
	}
	return d < 0.005
}
