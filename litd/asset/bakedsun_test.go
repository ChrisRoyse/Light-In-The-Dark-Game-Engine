package asset

import "testing"

func TestBakedSunScalarFSV(t *testing.T) {
	cfg := DefaultBakedSunConfig()
	up, err := BakedSunScalar([3]float32{0, 4, 0}, cfg)
	if err != nil {
		t.Fatal(err)
	}
	side, err := BakedSunScalar([3]float32{1, 0, 0}, cfg)
	if err != nil {
		t.Fatal(err)
	}
	down, err := BakedSunScalar([3]float32{0, -1, 0}, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV baked sun scalar BEFORE cfg=%+v AFTER up=%0.3f side=%0.3f down=%0.3f", cfg, up, side, down)
	if up != 1 || side != cfg.MinScalar || down != cfg.MinScalar {
		t.Fatalf("unexpected scalars up=%g side=%g down=%g cfg=%+v", up, side, down, cfg)
	}
}

func TestBakedSunScalarEdgesFSV(t *testing.T) {
	cfg := DefaultBakedSunConfig()
	badDir := cfg
	badDir.Direction = [3]float32{}
	if _, err := BakedSunScalar([3]float32{0, 1, 0}, badDir); err == nil {
		t.Fatal("zero sun direction accepted")
	} else {
		t.Logf("FSV baked sun zero direction BEFORE cfg=%+v AFTER err=%v", badDir, err)
	}

	badBounds := cfg
	badBounds.MinScalar = 0.8
	badBounds.MaxScalar = 0.2
	if _, err := BakedSunScalar([3]float32{0, 1, 0}, badBounds); err == nil {
		t.Fatal("inverted scalar bounds accepted")
	} else {
		t.Logf("FSV baked sun bad bounds BEFORE cfg=%+v AFTER err=%v", badBounds, err)
	}

	if _, err := BakedSunScalar([3]float32{}, cfg); err == nil {
		t.Fatal("zero normal accepted")
	} else {
		t.Logf("FSV baked sun zero normal BEFORE cfg=%+v AFTER err=%v", cfg, err)
	}
}
