package main

import (
	"os"
	"path/filepath"
	"testing"

	litlocale "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	litinput "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/input"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/g3n/engine/core"
)

func TestResolutionFlagSetFSV(t *testing.T) {
	var r resolutionFlag
	if err := r.Set("1920x1080"); err != nil {
		t.Fatalf("valid resolution rejected: %v", err)
	}
	t.Logf("FSV resolution valid BEFORE empty AFTER %+v", r)
	if r.W != 1920 || r.H != 1080 || !r.set {
		t.Fatalf("valid resolution parsed incorrectly: %+v", r)
	}

	before := r
	invalid := []string{
		"",
		"1920",
		"1920x",
		"x1080",
		"1920x1080extra",
		"1920x1080x1",
		"0x1080",
		"1920x-1",
		"1920X1080",
	}
	for _, input := range invalid {
		if err := r.Set(input); err == nil {
			t.Fatalf("invalid resolution %q accepted: %+v", input, r)
		}
		t.Logf("FSV resolution invalid input=%q BEFORE %+v AFTER %+v", input, before, r)
		if r != before {
			t.Fatalf("invalid resolution %q mutated state: got %+v want %+v", input, r, before)
		}
	}
}

func TestCameraZoomRequestFSV(t *testing.T) {
	cfg := litrender.DefaultRTSCameraConfig(16.0 / 9.0)
	cases := []struct {
		input string
		want  float32
	}{
		{input: "", want: cfg.Zoom},
		{input: "default", want: cfg.Zoom},
		{input: "min", want: cfg.ZoomMin},
		{input: "zmin", want: cfg.ZoomMin},
		{input: "max", want: cfg.ZoomMax},
		{input: "zmax", want: cfg.ZoomMax},
		{input: "below-min", want: cfg.ZoomMin * 0.5},
		{input: "above-max", want: cfg.ZoomMax * 2},
		{input: "1700", want: 1700},
	}
	for _, tc := range cases {
		got, err := cameraZoomRequest(tc.input, cfg)
		t.Logf("FSV camera zoom request input=%q got=%.3f err=%v", tc.input, got, err)
		if err != nil || got != tc.want {
			t.Fatalf("cameraZoomRequest(%q) = %.3f, %v; want %.3f nil", tc.input, got, err, tc.want)
		}
	}
	if got, err := cameraZoomRequest("bogus", cfg); err == nil {
		t.Fatalf("invalid zoom accepted: got %.3f", got)
	} else {
		t.Logf("FSV camera invalid zoom input=%q err=%v", "bogus", err)
	}
}

func TestBuildCameraProjectionModeFSV(t *testing.T) {
	persp, err := buildCamera(960, 540, "default", "persp")
	if err != nil {
		t.Fatalf("perspective camera rejected: %v", err)
	}
	ortho, err := buildCamera(960, 540, "above-max", "ortho")
	if err != nil {
		t.Fatalf("orthographic camera rejected: %v", err)
	}
	t.Logf("FSV renderdemo camera persp=%+v", persp.Snapshot())
	t.Logf("FSV renderdemo camera ortho=%+v", ortho.Snapshot())

	if persp.Snapshot().Projection != "perspective" {
		t.Fatalf("perspective camera flag produced wrong projection: %+v", persp.Snapshot())
	}
	orthoSnap := ortho.Snapshot()
	if orthoSnap.Projection != "orthographic" || orthoSnap.Zoom != orthoSnap.ZoomMax || !litrenderClose32(orthoSnap.OrthoSize, orthoSnap.OrthoSizeMax) {
		t.Fatalf("orthographic camera flag did not clamp zoom to Size_max: %+v", orthoSnap)
	}
	if _, err := buildCamera(960, 540, "default", "isometric"); err == nil {
		t.Fatalf("invalid camera projection accepted")
	} else {
		t.Logf("FSV renderdemo invalid camera projection err=%v", err)
	}
}

func TestBuildGroupFSVFSV(t *testing.T) {
	rig, err := buildCamera(960, 540, "default", "persp")
	if err != nil {
		t.Fatalf("camera rejected: %v", err)
	}
	dump := buildGroupFSV(core.NewNode(), rig)
	t.Logf("FSV renderdemo groups ok=%v current=%s selection=%v center=(%.1f,%.1f) cameraAnchor=%+v",
		dump.OK, dump.Current.Name, dump.Current.Selection, dump.Current.CenterX, dump.Current.CenterZ, rig.Snapshot().Anchor)
	if !dump.OK || dump.Current.Name != "doubletap-299" || !dump.Current.CenterRequested || dump.Current.CenterX != 120 || dump.Current.CenterZ != 80 {
		t.Fatalf("group FSV current mismatch: %+v", dump.Current)
	}
	if rig.Snapshot().Anchor.X != 120 || rig.Snapshot().Anchor.Z != 80 {
		t.Fatalf("double-tap did not center camera: %+v", rig.Snapshot().Anchor)
	}
	seen := map[string]groupCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
		if !c.OK || c.CommandRecordsEmitted != 0 {
			t.Fatalf("case %s failed or emitted commands: %+v", c.Name, c)
		}
	}
	if seen["recall-pruned"].Pruned != 2 || seen["doubletap-350"].CenterRequested || seen["generation-reuse"].RecycledID != 0x01000007 {
		t.Fatalf("group FSV edge cases missing: recall=%+v late=%+v gen=%+v",
			seen["recall-pruned"], seen["doubletap-350"], seen["generation-reuse"])
	}
}

func TestBuildSmartOrderFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dump, err := buildSmartOrderFSV(core.NewNode())
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo smart orders ok=%v current=%+v cases=%+v", dump.OK, dump.Current, dump.Cases)
	if !dump.OK || dump.Current.Name != "harvest-split" || len(dump.Current.Records) != 2 {
		t.Fatalf("smart-order fixture current mismatch: %+v", dump)
	}
	if dump.Current.Records[0].Opcode != sim.OpHarvest || dump.Current.Records[1].Opcode != sim.OpMove {
		t.Fatalf("harvest split should emit harvest then move: %+v", dump.Current.Records)
	}
	if len(dump.Current.Records[0].Units) != 2 || len(dump.Current.Records[1].Units) != 1 {
		t.Fatalf("harvest split unit groups wrong: %+v", dump.Current.Records)
	}
	seen := map[string]orderCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
	}
	if seen["hidden-enemy"].Feedback != litinput.SmartFeedbackHiddenTarget.String() || len(seen["hidden-enemy"].Records) != 0 {
		t.Fatalf("hidden target edge wrong: %+v", seen["hidden-enemy"])
	}
	if seen["dead-target"].Feedback != litinput.SmartFeedbackDeadTarget.String() || len(seen["dead-target"].Records) != 0 {
		t.Fatalf("dead target edge wrong: %+v", seen["dead-target"])
	}
}

func TestBuildQueueFSV(t *testing.T) {
	dump, err := buildQueueFSV(core.NewNode())
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo queue ok=%v flag=%02x replay=%+v screenshot=%+v final=%+v cases=%+v",
		dump.OK, dump.QueuedFlagByte, dump.Replay, dump.ScreenshotState, dump.FinalState, dump.Cases)
	if !dump.OK {
		t.Fatalf("queue fixture failed: %+v", dump.Errors)
	}
	if dump.QueuedFlagByte != sim.CmdFlagQueued || dump.QueuedFlagHex == "" {
		t.Fatalf("queued flag hexdump missing/wrong: byte=%02x hex=%q", dump.QueuedFlagByte, dump.QueuedFlagHex)
	}
	if len(dump.Trace) < 2 || dump.Trace[1].QueueDepth != 4 {
		t.Fatalf("queue did not grow to four shifted entries: trace=%+v", dump.Trace)
	}
	if dump.ScreenshotState.QueueDepth > 2 || dump.ScreenshotState.MoveState != sim.MoveFollowing || dump.ScreenshotState.Pos == dump.ScreenshotState.Current.Point {
		t.Fatalf("screenshot state is not mid-route after queued drain: %+v", dump.ScreenshotState)
	}
	if !dump.Replay.Equal || dump.Replay.FirstHash == "" || dump.Replay.FirstHash != dump.Replay.SecondHash {
		t.Fatalf("replay hash equality missing: %+v", dump.Replay)
	}
	if dump.FinalState.Pos != dump.SecondSequence[0] {
		t.Fatalf("final position should be second sequence target: got %+v want %+v", dump.FinalState.Pos, dump.SecondSequence[0])
	}
	seen := map[string]queueCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
		if !c.OK {
			t.Fatalf("queue case %s failed: %+v", c.Name, c)
		}
	}
	if seen["overflow-20-shift-orders"].After.QueueDepth != sim.MaxOrderQueue || len(seen["overflow-20-shift-orders"].Drops) != 4 {
		t.Fatalf("overflow edge wrong: %+v", seen["overflow-20-shift-orders"])
	}
	if seen["unmodified-collapse"].After.QueueDepth != 0 || seen["unmodified-collapse"].After.TotalOrders != 1 {
		t.Fatalf("collapse edge wrong: %+v", seen["unmodified-collapse"])
	}
	if seen["dead-unit-cleanup"].After.Alive || seen["dead-unit-cleanup"].After.TotalOrders != 0 ||
		seen["dead-unit-cleanup"].After.OrderPoolFree != seen["dead-unit-cleanup"].PoolFreeBase {
		t.Fatalf("dead cleanup edge wrong: %+v", seen["dead-unit-cleanup"])
	}
}

func TestBuildCommandCardKeymapFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dir := t.TempDir()
	path := filepath.Join(dir, "custom.toml")
	if err := os.WriteFile(path, []byte("profile = \"grid\"\n[game]\n\"card.slot.0\" = [\"T\"]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	localeTable := mustRenderDemoLocale(t)
	dump, display, err := buildCommandCardFSV(localeTable, "unit", path)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo keymap profile=%s summary=%q keypresses=%+v", dump.KeymapProfile, display.Summary.String(), dump.KeyPresses)
	if display.Slots[0].Hotkey != "T" || len(dump.KeyPresses) != 2 {
		t.Fatalf("custom keymap did not relabel slot0 or emit keypresses: hotkey=%q presses=%+v", display.Slots[0].Hotkey, dump.KeyPresses)
	}
	if dump.KeyPresses[0].Key != "T" || !dump.KeyPresses[0].Accepted || dump.KeyPresses[0].Emitted == nil || dump.KeyPresses[0].Emitted.Opcode != sim.OpMove {
		t.Fatalf("T did not emit slot0 command: %+v", dump.KeyPresses[0])
	}
	if dump.KeyPresses[1].Key != "Q" || dump.KeyPresses[1].Accepted || dump.KeyPresses[1].Reason != "unbound" {
		t.Fatalf("Q should be unbound after Q->T rebind: %+v", dump.KeyPresses[1])
	}
}

func TestBuildMapDataDumpFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dump, err := buildMapDataDump("data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo mapdata fp=%s counts=%+v samples=%+v", dump.Fingerprint, dump.Counts, dump.PathingSamples)
	if !dump.OK || dump.Width != 64 || dump.Height != 64 || dump.PathingWidth != 256 || dump.Counts.Water != 512 {
		t.Fatalf("map dump metadata/counts wrong: %+v", dump)
	}
	if len(dump.PathingSamples) < 5 || dump.PathingSamples[1].Flags != 4 || dump.PathingSamples[2].CliffText != "r0" || dump.PathingSamples[3].CliffText != "1" {
		t.Fatalf("map dump samples wrong: %+v", dump.PathingSamples)
	}
	if len(dump.HeightSamples) < 3 || dump.HeightSamples[0].Height != 0 || dump.HeightSamples[1].Height != 256 || dump.HeightSamples[2].Height != 512 {
		t.Fatalf("map dump height samples wrong: %+v", dump.HeightSamples)
	}
	if len(dump.SplatSamples) < 2 || dump.SplatSamples[1].Weight.C != 255 {
		t.Fatalf("map dump splat samples wrong: %+v", dump.SplatSamples)
	}
}

func TestBuildTerrainFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	scene := core.NewNode()
	spec, dump, err := buildTerrainFSV(scene, "terrain-units", true)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo terrain spec=%+v triangles=%d maxDiff=%d inverted=%d border=%+v units=%+v",
		spec, dump.TriangleCount, dump.MaxHeightDiff, dump.InvertedTriangles, dump.BorderVertices, dump.Units)
	if !dump.OK || dump.VertexCount != 4225 || dump.TriangleCount != 8192 || dump.MaxHeightDiff != 0 || dump.InvertedTriangles != 0 {
		t.Fatalf("terrain dump wrong: %+v", dump)
	}
	if len(dump.HeightSamples) != 100 || len(dump.BorderVertices) != 4 || len(dump.Units) != 4 {
		t.Fatalf("terrain FSV coverage wrong: samples=%d border=%d units=%d", len(dump.HeightSamples), len(dump.BorderVertices), len(dump.Units))
	}
	if spec.expected.VisibleGraphics != 5 || spec.expected.OpaqueDrawCalls != 5 {
		t.Fatalf("terrain-units expected stats wrong: %+v", spec.expected)
	}
}

func TestBuildTerrainChunksFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	scene := core.NewNode()
	spec, dump, err := buildTerrainChunksFSV(scene, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo terrain-chunks spec=%+v count=%d cols=%d rows=%d maxTris=%d totalTris=%d seams=%d border=%+v",
		spec.name, dump.ChunkCount, dump.ChunkCols, dump.ChunkRows, dump.MaxChunkTris, dump.TriangleCount, dump.SeamMismatches, dump.BorderVertices)
	if !dump.OK || !dump.Chunked {
		t.Fatalf("chunk dump not OK: %+v", dump)
	}
	if dump.ChunkCount != 16 || dump.ChunkCols != 4 || dump.ChunkRows != 4 {
		t.Fatalf("chunk grid wrong: count=%d cols=%d rows=%d", dump.ChunkCount, dump.ChunkCols, dump.ChunkRows)
	}
	if dump.MaxChunkTris != 512 || dump.TriangleCount != 8192 {
		t.Fatalf("chunk tris wrong: max=%d total=%d", dump.MaxChunkTris, dump.TriangleCount)
	}
	if dump.SeamMismatches != 0 {
		t.Fatalf("seam mismatches=%d, want 0 (cracks)", dump.SeamMismatches)
	}
	if len(dump.BorderVertices) != 4 || len(dump.ChunkTris) != 16 {
		t.Fatalf("coverage wrong: border=%d chunkTris=%d", len(dump.BorderVertices), len(dump.ChunkTris))
	}
}

func TestBuildSpellstormFSV(t *testing.T) {
	scene := core.NewNode()
	_, dump, err := buildSpellstormFSV(scene, false)
	if err != nil {
		t.Fatal(err)
	}
	last := dump.Events[len(dump.Events)-1].Decision
	t.Logf("FSV spellstorm maxActive=%d finalActive=%d events=%d lastEvict=%+v OK=%v",
		dump.MaxActive, dump.FinalActive, len(dump.Events), last, dump.OK)
	if !dump.OK || dump.MaxActive != 8 || dump.FinalActive != 8 || len(dump.Events) != 9 {
		t.Fatalf("spellstorm dump wrong: %+v", dump)
	}
	if !last.Granted || last.Victim < 0 || last.Reason != "evict:lower-priority" {
		t.Fatalf("9th request must evict a lower-priority light: %+v", last)
	}

	sceneLow := core.NewNode()
	_, low, err := buildSpellstormFSV(sceneLow, true)
	if err != nil {
		t.Fatal(err)
	}
	lastLow := low.Events[len(low.Events)-1].Decision
	t.Logf("FSV spellstorm low-preset finalActive=%d lastReason=%s OK=%v", low.FinalActive, lastLow.Reason, low.OK)
	if !low.OK || low.FinalActive != 0 || lastLow.Reason != "denied:low-preset" {
		t.Fatalf("low preset must bind no lights: %+v", low)
	}
}

func chdirRepoRoot(t *testing.T) func() {
	t.Helper()
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir("../.."); err != nil {
		t.Fatal(err)
	}
	return func() {
		if err := os.Chdir(cwd); err != nil {
			t.Fatal(err)
		}
	}
}

func mustRenderDemoLocale(t *testing.T) *litlocale.Table {
	t.Helper()
	table, err := litlocale.Load(os.DirFS("data"), "en")
	if err != nil {
		t.Fatal(err)
	}
	return table
}

func litrenderClose32(got, want float32) bool {
	d := got - want
	if d < 0 {
		d = -d
	}
	return d <= 0.001
}
