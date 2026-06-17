package mapdata

import (
	"os"
	"strings"
	"testing"
	"testing/fstest"
)

func TestLoadTest64FSV(t *testing.T) {
	m, err := Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	again, err := Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV mapdata test64 dims=%dx%d pathing=%dx%d biome=%s fp1=%016x fp2=%016x starts=%+v doodads=%+v",
		m.Width, m.Height, m.PathingWidth, m.PathingHeight, m.Biome, m.Fingerprint, again.Fingerprint, m.Starts(), m.Doodads())
	if m.Width != 64 || m.Height != 64 || m.PathingWidth != 256 || m.PathingHeight != 256 {
		t.Fatalf("dimensions wrong: %+v", m)
	}
	if m.Fingerprint == 0 || m.Fingerprint != again.Fingerprint {
		t.Fatalf("fingerprint unstable: %016x vs %016x", m.Fingerprint, again.Fingerprint)
	}
	checkFlag(t, m, 10, 10, PathWalkable|PathBuildable)
	checkFlag(t, m, 64, 124, PathWater)
	checkCliff(t, m, 10, 10, Cliff{Level: 0})
	checkCliff(t, m, 126, 128, Cliff{Level: 0, Ramp: true})
	checkCliff(t, m, 130, 128, Cliff{Level: 1})
	checkHeight(t, m, 31, 4, 0)
	checkHeight(t, m, 32, 4, 256)
	checkHeight(t, m, 33, 4, 512)
	checkSplat(t, m, 0, 0, SplatWeight{A: 255})
	checkSplat(t, m, 16, 31, SplatWeight{C: 255})
	starts := m.Starts()
	if len(starts) != 2 || starts[0].Player != 0 || starts[0].X != 32 || starts[1].Player != 1 || starts[1].X != 224 {
		t.Fatalf("start locations wrong: %+v", starts)
	}
	if len(m.Doodads()) != 3 {
		t.Fatalf("doodad count wrong: %+v", m.Doodads())
	}
}

func TestMapDataRejectsMissingDoodadAssetFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/doodads.toml"] = &fstest.MapFile{Data: []byte(`[[doodad]]
id = 1
asset = "kaykit-hexagon/missing.glb"
cell = [1, 1]
rotation = 0
destructible = true
`)}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV missing doodad asset err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "missing.glb") || !strings.Contains(err.Error(), "doodad 1") {
		t.Fatalf("missing asset error must name asset and doodad: %v", err)
	}
}

func TestMapDataRejectsBadRampFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/cliff.txt"] = &fstest.MapFile{Data: []byte("0*4\n0 r0 0*2\n0*4\n0*4\n")}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV bad ramp err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "ramp at (1,1)") || !strings.Contains(err.Error(), "both levels 0 and 1") {
		t.Fatalf("bad ramp error must name cell and levels: %v", err)
	}
}

func TestMapDataRejectsUnbuildableStartFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/pathing.txt"] = &fstest.MapFile{Data: []byte("1 3*3\n3*4\n3*4\n3*4\n")}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV unbuildable start err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "start player 0") || !strings.Contains(err.Error(), "not buildable ground") {
		t.Fatalf("unbuildable start error must name player/cell: %v", err)
	}
}

func TestMapDataRejectsTruncatedHeightFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/height.txt"] = &fstest.MapFile{Data: []byte("0\n0*2\n")}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV truncated height err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "height.txt") || !strings.Contains(err.Error(), "row 0 got 1 values, want 2") {
		t.Fatalf("truncated height error must name file and row width: %v", err)
	}
}

func TestMapDataFingerprintSensitivityFSV(t *testing.T) {
	base, err := Load(tinyMapFS(), "data/maps/tiny")
	if err != nil {
		t.Fatal(err)
	}
	changedFS := tinyMapFS()
	changedFS["data/maps/tiny/height.txt"] = &fstest.MapFile{Data: []byte("0 1\n0*2\n")}
	changed, err := Load(changedFS, "data/maps/tiny")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV mapdata fingerprint base=%016x changed-height=%016x", base.Fingerprint, changed.Fingerprint)
	if base.Fingerprint == changed.Fingerprint {
		t.Fatalf("height change did not change fingerprint")
	}
}

func TestLoadFixtureBeaconsFSV(t *testing.T) {
	m, err := Load(os.DirFS("../../.."), "data/maps/_fixture")
	if err != nil {
		t.Fatal(err)
	}
	beacons := m.Beacons()
	t.Logf("FSV _fixture dims=%dx%d pathing=%dx%d starts=%+v beacons=%+v",
		m.Width, m.Height, m.PathingWidth, m.PathingHeight, m.Starts(), beacons)
	if m.Width != 8 || m.Height != 8 || m.PathingWidth != 32 || m.PathingHeight != 32 {
		t.Fatalf("fixture dimensions wrong: %+v", m)
	}
	// Authored: id 1 neutral @ (16,16), id 2 owner 0 @ (16,8); returned sorted by id.
	want := []Beacon{
		{ID: 1, X: 16, Y: 16, Owner: BeaconNeutral},
		{ID: 2, X: 16, Y: 8, Owner: 0},
	}
	if len(beacons) != len(want) {
		t.Fatalf("beacon count=%d want %d: %+v", len(beacons), len(want), beacons)
	}
	for i, w := range want {
		if beacons[i] != w {
			t.Fatalf("beacon[%d]=%+v want %+v", i, beacons[i], w)
		}
	}
}

func TestMapDataBeaconNeutralDefaultFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/terrain.toml"] = &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]

[[beacon]]
id = 7
cell = [2, 3]
`)}
	m, err := Load(fsys, "data/maps/tiny")
	if err != nil {
		t.Fatal(err)
	}
	b := m.Beacons()
	t.Logf("FSV beacon neutral-default beacons=%+v BeaconNeutral=%d", b, BeaconNeutral)
	if len(b) != 1 || b[0] != (Beacon{ID: 7, X: 2, Y: 3, Owner: BeaconNeutral}) {
		t.Fatalf("omitted owner must default to BeaconNeutral: %+v", b)
	}
}

func TestMapDataRejectsOutOfBoundsBeaconFSV(t *testing.T) {
	fsys := tinyMapFS()
	// 1x1 tiles => 4x4 pathing grid, valid cell indices [0,3]; (4,4) is off-grid.
	fsys["data/maps/tiny/terrain.toml"] = &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]

[[beacon]]
id = 1
cell = [4, 4]
`)}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV out-of-bounds beacon err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "beacon 1") || !strings.Contains(err.Error(), "out of bounds") {
		t.Fatalf("out-of-bounds beacon error must name beacon and bounds: %v", err)
	}
}

func TestMapDataRejectsBadBeaconOwnerFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/terrain.toml"] = &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]

[[beacon]]
id = 1
cell = [1, 1]
owner = 16
`)}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV bad beacon owner err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "beacon 1") || !strings.Contains(err.Error(), "owner 16 out of range [0,15]") {
		t.Fatalf("bad owner error must name beacon and range: %v", err)
	}
}

func TestMapDataRejectsDuplicateBeaconFSV(t *testing.T) {
	fsys := tinyMapFS()
	fsys["data/maps/tiny/terrain.toml"] = &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]

[[beacon]]
id = 1
cell = [0, 0]

[[beacon]]
id = 1
cell = [1, 1]
`)}
	_, err := Load(fsys, "data/maps/tiny")
	t.Logf("FSV duplicate beacon err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "duplicate beacon id 1") {
		t.Fatalf("duplicate beacon error must name id: %v", err)
	}
}

func TestMapDataBeaconFingerprintSensitivityFSV(t *testing.T) {
	withBeacon := func(owner string) fstest.MapFS {
		fsys := tinyMapFS()
		fsys["data/maps/tiny/terrain.toml"] = &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]

[[beacon]]
id = 1
cell = [1, 1]
` + owner)}
		return fsys
	}
	neutral, err := Load(withBeacon(""), "data/maps/tiny")
	if err != nil {
		t.Fatal(err)
	}
	owned, err := Load(withBeacon("owner = 0\n"), "data/maps/tiny")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV beacon fingerprint neutral=%016x owned=%016x", neutral.Fingerprint, owned.Fingerprint)
	if neutral.Fingerprint == owned.Fingerprint {
		t.Fatalf("beacon owner change did not change fingerprint (beacons absent from map identity hash)")
	}
}

func checkFlag(t *testing.T, m *Map, x, y int, want PathFlags) {
	t.Helper()
	got, ok := m.PathingAt(x, y)
	t.Logf("pathing(%d,%d)=%03b want=%03b", x, y, got, want)
	if !ok || got != want {
		t.Fatalf("pathing(%d,%d)=%v ok=%v want %v", x, y, got, ok, want)
	}
}

func checkCliff(t *testing.T, m *Map, x, y int, want Cliff) {
	t.Helper()
	got, ok := m.CliffAt(x, y)
	t.Logf("cliff(%d,%d)=%+v want=%+v", x, y, got, want)
	if !ok || got != want {
		t.Fatalf("cliff(%d,%d)=%+v ok=%v want %+v", x, y, got, ok, want)
	}
}

func checkHeight(t *testing.T, m *Map, x, y int, want int32) {
	t.Helper()
	got, ok := m.HeightAtVertex(x, y)
	t.Logf("height(%d,%d)=%d want=%d", x, y, got, want)
	if !ok || got != want {
		t.Fatalf("height(%d,%d)=%d ok=%v want %d", x, y, got, ok, want)
	}
}

func checkSplat(t *testing.T, m *Map, x, y int, want SplatWeight) {
	t.Helper()
	got, ok := m.SplatAt(x, y)
	t.Logf("splat(%d,%d)=%+v want=%+v", x, y, got, want)
	if !ok || got != want {
		t.Fatalf("splat(%d,%d)=%+v ok=%v want %+v", x, y, got, ok, want)
	}
}

func tinyMapFS() fstest.MapFS {
	return fstest.MapFS{
		"assets/kaykit-hexagon/tree_single_A.glb": &fstest.MapFile{Data: []byte("stub")},
		"data/maps/tiny/terrain.toml": &fstest.MapFile{Data: []byte(`version = 1
width = 1
height = 1
biome = "tiny"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]
`)},
		"data/maps/tiny/pathing.txt": &fstest.MapFile{Data: []byte("3*4\n3*4\n3*4\n3*4\n")},
		"data/maps/tiny/cliff.txt":   &fstest.MapFile{Data: []byte("0*4\n0*4\n0*4\n0*4\n")},
		"data/maps/tiny/height.txt":  &fstest.MapFile{Data: []byte("0*2\n0*2\n")},
		"data/maps/tiny/splat.txt":   &fstest.MapFile{Data: []byte("255,0,0,0\n")},
		"data/maps/tiny/doodads.toml": &fstest.MapFile{Data: []byte(`[[doodad]]
id = 1
asset = "kaykit-hexagon/tree_single_A.glb"
cell = [1, 1]
rotation = 0
destructible = true
`)},
	}
}
