package minimap

import "testing"

func TestMapperRectangularRoundTripFSV(t *testing.T) {
	m := NewMapper(128, 64, 0, 0, 256, 64)
	if !m.Valid() {
		t.Fatal("mapper should be valid")
	}
	cases := []struct {
		x, z           float32
		wantPX, wantPY int
	}{
		{0, 0, 0, 0},
		{128, 32, 64, 32},
		{255, 63, 127, 63},
	}
	for _, c := range cases {
		px, py := m.WorldToPixel(c.x, c.z)
		t.Logf("FSV mapper world(%.0f,%.0f)->px(%d,%d) want(%d,%d)", c.x, c.z, px, py, c.wantPX, c.wantPY)
		if px != c.wantPX || py != c.wantPY {
			t.Fatalf("world(%.0f,%.0f)->(%d,%d), want (%d,%d)", c.x, c.z, px, py, c.wantPX, c.wantPY)
		}
	}
	for py := 0; py < m.Height(); py += 7 {
		for px := 0; px < m.Width(); px += 11 {
			wx, wz := m.PixelToWorld(px, py)
			rpx, rpy := m.WorldToPixel(wx, wz)
			if rpx != px || rpy != py {
				t.Fatalf("round-trip px(%d,%d)->world(%.2f,%.2f)->px(%d,%d)", px, py, wx, wz, rpx, rpy)
			}
		}
	}
	t.Logf("FSV rectangular mapper pixel/world round-trip exact over sampled grid")
}

func TestMapperInvalidFailsClosedFSV(t *testing.T) {
	for _, m := range []Mapper{
		NewMapper(0, 64, 0, 0, 1, 1),
		NewMapper(64, 0, 0, 0, 1, 1),
		NewMapper(64, 64, 1, 0, 1, 1),
		NewMapper(64, 64, 0, 1, 1, 1),
	} {
		if m.Valid() {
			t.Fatalf("invalid mapper reported valid: %+v", m)
		}
		px, py := m.WorldToPixel(10, 10)
		wx, wz := m.PixelToWorld(10, 10)
		t.Logf("FSV invalid mapper state=%+v world->px=%d,%d px->world=%.1f,%.1f", m, px, py, wx, wz)
		if px != 0 || py != 0 || wx != 0 || wz != 0 {
			t.Fatalf("invalid mapper should map to zero, got px=%d,%d world=%.1f,%.1f", px, py, wx, wz)
		}
	}
}
