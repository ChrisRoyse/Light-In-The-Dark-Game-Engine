package main

import (
	"encoding/json"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	return filepath.Clean(filepath.Join(wd, "..", ".."))
}

func firstFlamePath(t *testing.T) string {
	t.Helper()
	return filepath.Join(repoRoot(t), "data", "maps", "firstflame")
}

func TestAnalyzeFirstFlameFSV(t *testing.T) {
	m, rel, err := loadMap(firstFlamePath(t))
	if err != nil {
		t.Fatalf("load firstflame: %v", err)
	}
	got, err := analyze(m, rel, 16)
	if err != nil {
		t.Fatalf("analyze: %v", err)
	}
	if got.Map.Width != 64 || got.Map.Height != 64 || got.Map.PathingWidth != 256 || got.Map.PathingHeight != 256 {
		t.Fatalf("dims = %+v, want 64x64 / 256x256", got.Map)
	}
	if got.Counts.WalkableCells != 256*256 || got.Counts.BuildableCells != 256*256 {
		t.Fatalf("pathing counts = %+v, want all cells walkable+buildable", got.Counts)
	}
	if got.Counts.RampCells != 36 || got.Counts.HighCells == 0 {
		t.Fatalf("cliff counts = %+v, want 36 ramps and some high ground", got.Counts)
	}
	if len(got.Starts) != 2 || len(got.Beacons) != 3 {
		t.Fatalf("starts/beacons = %d/%d, want 2/3", len(got.Starts), len(got.Beacons))
	}
	if !got.Symmetry.StartMirror || !got.Symmetry.BeaconMirror || len(got.Symmetry.CenterBeacons) != 1 {
		t.Fatalf("symmetry = %+v, want mirrored starts/flanks and one center beacon", got.Symmetry)
	}
	if len(got.Paths) != 2 || got.Paths[0].Steps != 176 || got.Paths[1].Steps != 176 || !got.Paths[0].Reachable || !got.Paths[1].Reachable {
		t.Fatalf("paths = %+v, want 176 steps both directions", got.Paths)
	}
	for _, fp := range got.Footprints {
		if !fp.Clear || fp.Blocked != 0 || fp.Size != 16 {
			t.Fatalf("footprint = %+v, want clear 16x16 base site", fp)
		}
	}
	t.Logf("FSV #174 map analysis: fp=%s starts=%+v beacons=%+v symmetry=%+v paths=%+v footprints=%+v counts=%+v",
		got.Map.Fingerprint, got.Starts, got.Beacons, got.Symmetry, got.Paths, got.Footprints, got.Counts)
}

func TestRunWritesInspectableArtifactsFSV(t *testing.T) {
	out := t.TempDir()
	if err := run([]string{"-map", firstFlamePath(t), "-out", out, "-scale", "2", "-crop-radius", "32", "-footprint", "16"}); err != nil {
		t.Fatalf("run mapshot: %v", err)
	}
	overview := filepath.Join(out, "firstflame-overhead.png")
	start0 := filepath.Join(out, "firstflame-start-p0.png")
	start1 := filepath.Join(out, "firstflame-start-p1.png")
	summaryPath := filepath.Join(out, "firstflame-summary.json")

	assertPNG(t, overview, 512, 512)
	assertPNG(t, start0, 128, 128)
	assertPNG(t, start1, 128, 128)
	img := decodePNG(t, overview)
	if r, g, b, _ := img.At(128*2, 128*2).RGBA(); r < 0xeeee || g < 0xcccc || b > 0x9999 {
		t.Fatalf("central beacon pixel does not show bright marker: rgba16=(%x,%x,%x)", r, g, b)
	}

	body, err := os.ReadFile(summaryPath)
	if err != nil {
		t.Fatalf("read summary: %v", err)
	}
	var got summary
	if err := json.Unmarshal(body, &got); err != nil {
		t.Fatalf("decode summary: %v", err)
	}
	if len(got.Images) != 3 {
		t.Fatalf("summary images = %d, want 3", len(got.Images))
	}
	for _, img := range got.Images {
		if img.Bytes <= 0 || img.SHA256 == "" {
			t.Fatalf("bad image record: %+v", img)
		}
	}
	t.Logf("FSV #174 artifacts: overview=%s starts=[%s,%s] summary=%s hashes=%+v",
		overview, start0, start1, summaryPath, got.Images)
}

func TestRunInputEdgesFSV(t *testing.T) {
	cases := []struct {
		name string
		args []string
		want string
	}{
		{name: "empty-map", args: []string{"-map", ""}, want: "empty -map"},
		{name: "bad-scale-low", args: []string{"-map", firstFlamePath(t), "-scale", "0"}, want: "-scale must be"},
		{name: "bad-scale-high", args: []string{"-map", firstFlamePath(t), "-scale", "17"}, want: "-scale must be"},
		{name: "missing-map", args: []string{"-map", filepath.Join(t.TempDir(), "missing")}, want: "read"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := run(tc.args)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("run error = %v, want containing %q", err, tc.want)
			}
			t.Logf("FSV edge %s refused: %v", tc.name, err)
		})
	}
}

func assertPNG(t *testing.T, path string, wantW, wantH int) {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer f.Close()
	cfg, err := png.DecodeConfig(f)
	if err != nil {
		t.Fatalf("decode config %s: %v", path, err)
	}
	if cfg.Width != wantW || cfg.Height != wantH {
		t.Fatalf("%s dims = %dx%d, want %dx%d", path, cfg.Width, cfg.Height, wantW, wantH)
	}
}

func decodePNG(t *testing.T, path string) image.Image {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer f.Close()
	img, err := png.Decode(f)
	if err != nil {
		t.Fatalf("decode %s: %v", path, err)
	}
	return img
}
